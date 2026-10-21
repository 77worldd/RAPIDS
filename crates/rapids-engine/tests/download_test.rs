use std::path::Path;
use std::time::Duration;

use rapids_engine::{DownloadSpec, EngineConfig, EngineEvent, Manifest, Workspace, spawn_download};
use rapids_test_support::{TestServer, expected_sha256, options};

async fn next_event(
    rx: &mut tokio::sync::mpsc::Receiver<EngineEvent>,
    seconds: u64,
    label: &str,
) -> EngineEvent {
    match tokio::time::timeout(Duration::from_secs(seconds), rx.recv()).await {
        Ok(Some(event)) => event,
        Ok(None) => panic!("{label}: event channel closed early"),
        Err(_) => panic!("{label}: no event within {seconds}s"),
    }
}

async fn wait_for<F>(
    rx: &mut tokio::sync::mpsc::Receiver<EngineEvent>,
    seconds: u64,
    label: &str,
    mut predicate: F,
) -> EngineEvent
where
    F: FnMut(&EngineEvent) -> bool,
{
    loop {
        let event = next_event(rx, seconds, label).await;
        if predicate(&event) {
            return event;
        }
        if event.is_terminal() {
            panic!("{label}: unexpected terminal event: {event}");
        }
    }
}

fn spec_for(url: &str, dest: &Path) -> DownloadSpec {
    DownloadSpec {
        url: url.to_string(),
        dest: Some(dest.to_path_buf()),
        expected_sha256: None,
        connections: None,
        overwrite: false,
    }
}

#[tokio::test]
async fn completes_a_segmented_download_and_verifies_sha() {
    let total = 8 * 1024 * 1024u64;
    let server = TestServer::start(options(total));
    let dir = tempfile::tempdir().expect("temp dir");
    let dest = dir.path().join("payload.bin");
    let expected = expected_sha256(total);

    let mut spec = spec_for(&server.url("/payload.bin"), &dest);
    spec.expected_sha256 = Some(expected.clone());
    let (handle, mut rx) = spawn_download(spec, EngineConfig::default()).expect("spawn");

    let mut saw_progress = false;
    let completed = wait_for(&mut rx, 30, "completion", |event| {
        if let EngineEvent::Progress {
            total: reported, ..
        } = event
        {
            assert_eq!(*reported, Some(total), "progress must report the size");
            saw_progress = true;
        }
        matches!(event, EngineEvent::Completed { .. })
    })
    .await;
    drop(handle);

    assert!(saw_progress, "progress events must flow during transfer");
    let bytes = std::fs::read(&dest).expect("final file");
    assert_eq!(bytes.len() as u64, total);
    drop(bytes);
    assert_eq!(rapids_engine::sha256_file(&dest).expect("hash"), expected);
    match completed {
        EngineEvent::Completed { sha256, .. } => {
            assert_eq!(sha256.as_deref(), Some(expected.as_str()));
        }
        other => panic!("expected completion, got {other}"),
    }
    assert!(
        !Workspace::for_dest(&dest).dir.exists(),
        "sidecar workspace must be cleaned up"
    );
}

#[tokio::test]
async fn single_stream_when_the_server_ignores_ranges() {
    let total = 2 * 1024 * 1024u64;
    let mut server_options = options(total);
    server_options.ranges = false;
    let server = TestServer::start(server_options);
    let dir = tempfile::tempdir().expect("temp dir");
    let dest = dir.path().join("payload.bin");

    let spec = spec_for(&server.url("/payload.bin"), &dest);
    let (handle, mut rx) = spawn_download(spec, EngineConfig::default()).expect("spawn");

    let mut exceeded_one_connection = false;
    wait_for(&mut rx, 30, "completion", |event| {
        if let EngineEvent::Progress {
            active_connections, ..
        } = event
        {
            exceeded_one_connection |= *active_connections > 1;
        }
        matches!(event, EngineEvent::Completed { .. })
    })
    .await;
    drop(handle);

    assert!(
        !exceeded_one_connection,
        "range-less downloads must stay on one connection"
    );
    assert!(
        server
            .requests()
            .iter()
            .all(|record| record.range.is_none()),
        "no Range header may be sent when the server does not support ranges"
    );
    assert_eq!(std::fs::read(&dest).expect("file").len() as u64, total);
}

#[tokio::test]
async fn flaky_connections_recover_through_retries() {
    let total = 4 * 1024 * 1024u64;
    let mut server_options = options(total);
    server_options.flaky_every = Some(3);
    server_options.flaky_after = 128 * 1024;
    let server = TestServer::start(server_options);
    let dir = tempfile::tempdir().expect("temp dir");
    let dest = dir.path().join("payload.bin");

    let spec = spec_for(&server.url("/payload.bin"), &dest);
    let (handle, mut rx) = spawn_download(spec, EngineConfig::default()).expect("spawn");

    let mut saw_retry = false;
    wait_for(&mut rx, 60, "completion despite drops", |event| {
        if matches!(event, EngineEvent::Retrying { .. }) {
            saw_retry = true;
        }
        matches!(event, EngineEvent::Completed { .. })
    })
    .await;
    drop(handle);

    assert!(saw_retry, "dropped connections must surface as retries");
    assert_eq!(
        rapids_engine::sha256_file(&dest).expect("hash"),
        expected_sha256(total)
    );
}

#[tokio::test]
async fn pause_then_resume_completes_byte_perfectly() {
    let total = 16 * 1024 * 1024u64;
    let mut server_options = options(total);
    server_options.throttle_bps = Some(1024 * 1024);
    let server = TestServer::start(server_options);
    let dir = tempfile::tempdir().expect("temp dir");
    let dest = dir.path().join("payload.bin");

    let mut spec = spec_for(&server.url("/payload.bin"), &dest);
    spec.connections = Some(4);
    let (handle, mut rx) = spawn_download(spec, EngineConfig::default()).expect("spawn");

    wait_for(&mut rx, 30, "early progress", |event| match event {
        EngineEvent::Progress { downloaded, .. } => *downloaded >= 2 * 1024 * 1024,
        _ => false,
    })
    .await;

    handle.pause().await;
    wait_for(&mut rx, 15, "paused state", |event| {
        matches!(
            event,
            EngineEvent::StateChanged {
                state: rapids_engine::DownloadState::Paused
            }
        )
    })
    .await;
    let workspace = Workspace::for_dest(&dest);
    assert!(workspace.manifest_path.exists(), "manifest survives pause");
    assert!(workspace.part_path.exists(), "part file survives pause");

    handle.resume().await;
    let completed = wait_for(&mut rx, 60, "completion after resume", |event| {
        matches!(event, EngineEvent::Completed { .. })
    })
    .await;
    drop(handle);

    assert_eq!(
        rapids_engine::sha256_file(&dest).expect("hash"),
        expected_sha256(total)
    );
    match completed {
        EngineEvent::Completed { sha256, .. } => {
            assert_eq!(sha256.as_deref(), Some(expected_sha256(total).as_str()));
        }
        other => panic!("expected completion, got {other}"),
    }
}

#[tokio::test]
async fn stopping_releases_the_workspace_for_a_byte_perfect_restart() {
    let total = 16 * 1024 * 1024u64;
    let mut server_options = options(total);
    server_options.throttle_bps = Some(512 * 1024);
    let server = TestServer::start(server_options);
    let dir = tempfile::tempdir().expect("temp dir");
    let dest = dir.path().join("payload.bin");
    let mut spec = spec_for(&server.url("/payload.bin"), &dest);
    spec.connections = Some(4);

    let (first, mut first_rx) =
        spawn_download(spec.clone(), EngineConfig::default()).expect("first run");
    wait_for(&mut first_rx, 30, "progress before stop", |event| {
        matches!(event, EngineEvent::Progress { downloaded, .. } if *downloaded >= 2 * 1024 * 1024)
    })
    .await;

    tokio::time::timeout(Duration::from_secs(15), first.stop())
        .await
        .expect("stop released the coordinator");
    assert_eq!(first.state(), rapids_engine::DownloadState::Stopped);
    let stopped = wait_for(&mut first_rx, 15, "stopped event", |event| {
        matches!(event, EngineEvent::Stopped)
    })
    .await;
    assert!(stopped.is_terminal());
    first.stop().await;

    let workspace = Workspace::for_dest(&dest);
    let saved = Manifest::load(&workspace.dir)
        .await
        .expect("stopped manifest");
    let confirmed: u64 = saved.segments.iter().map(|segment| segment.written).sum();
    assert!(
        confirmed >= 2 * 1024 * 1024 && confirmed < total,
        "stopped after {confirmed} of {total} bytes"
    );
    assert!(workspace.part_path.exists(), "the part file survives stop");
    assert!(!dest.exists(), "a partial file is never published");
    assert!(saved.can_resume(Some(total), saved.etag.as_deref(), saved.last_modified.as_deref()));

    // The first handle is deliberately still alive: stop, not dropping the
    // process or handle, is what must release the operating-system lock.
    let mut available = Workspace::for_dest(&dest);
    available.create_and_lock().await.expect("lock released");
    drop(available);
    assert!(server.wait_idle(Duration::from_secs(10)), "old requests settled");
    server.clear_requests();

    let (second, mut second_rx) =
        spawn_download(spec, EngineConfig::default()).expect("restart");
    let completed = wait_for(&mut second_rx, 60, "resumed completion", |event| {
        matches!(event, EngineEvent::Completed { .. })
    })
    .await;
    drop(second);
    let expected = expected_sha256(total);
    assert_eq!(rapids_engine::sha256_file(&dest).expect("hash"), expected);
    match completed {
        EngineEvent::Completed { sha256, .. } => {
            assert_eq!(sha256.as_deref(), Some(expected.as_str()));
        }
        other => panic!("expected completion, got {other}"),
    }
    assert!(!workspace.dir.exists(), "completed sidecar is removed");
    assert!(server.wait_idle(Duration::from_secs(10)), "resumed requests settled");

    let requests = server.requests();
    let refetched: u64 = requests
        .iter()
        .filter(|record| record.method == "GET")
        .map(|record| record.body_len)
        .sum();
    assert!(
        refetched < total * 9 / 10,
        "the restart fetched {refetched} of {total} bytes"
    );
    assert!(
        requests.iter().filter_map(|record| record.range.as_deref()).any(|range| {
            range
                .strip_prefix("bytes=")
                .and_then(|value| value.split_once('-'))
                .and_then(|(start, _)| start.parse::<u64>().ok())
                .is_some_and(|start| start > 0)
        }),
        "a resumable request starts after byte zero"
    );
}

#[tokio::test]
async fn stopping_a_paused_download_unlocks_without_deleting_bytes() {
    let total = 8 * 1024 * 1024u64;
    let mut server_options = options(total);
    server_options.throttle_bps = Some(512 * 1024);
    let server = TestServer::start(server_options);
    let dir = tempfile::tempdir().expect("temp dir");
    let dest = dir.path().join("payload.bin");
    let mut spec = spec_for(&server.url("/payload.bin"), &dest);
    spec.connections = Some(4);

    let (handle, mut rx) = spawn_download(spec, EngineConfig::default()).expect("spawn");
    wait_for(&mut rx, 30, "early progress", |event| {
        matches!(event, EngineEvent::Progress { downloaded, .. } if *downloaded >= 512 * 1024)
    })
    .await;
    handle.pause().await;
    wait_for(&mut rx, 15, "pause", |event| {
        matches!(event, EngineEvent::StateChanged { state: rapids_engine::DownloadState::Paused })
    })
    .await;

    let workspace = Workspace::for_dest(&dest);
    let before = Manifest::load(&workspace.dir).await.expect("paused manifest");
    let saved_before: u64 = before.segments.iter().map(|segment| segment.written).sum();
    assert!(saved_before > 0);
    let mut claimant = Workspace::for_dest(&dest);
    assert!(
        matches!(
            claimant.create_and_lock().await,
            Err(rapids_engine::EngineError::WorkspaceLocked { .. })
        ),
        "pause does not release the lock"
    );

    tokio::time::timeout(Duration::from_secs(15), handle.stop())
        .await
        .expect("stop while paused");
    assert_eq!(handle.state(), rapids_engine::DownloadState::Stopped);
    wait_for(&mut rx, 15, "stopped", |event| matches!(event, EngineEvent::Stopped)).await;
    claimant.create_and_lock().await.expect("stop released lock");
    drop(claimant);
    let after = Manifest::load(&workspace.dir).await.expect("stopped manifest");
    let saved_after: u64 = after.segments.iter().map(|segment| segment.written).sum();
    assert!(saved_after >= saved_before);
    assert!(workspace.part_path.exists());
    assert!(!dest.exists());
}

#[tokio::test]
async fn stopping_before_the_probe_does_not_start_network_work() {
    let server = TestServer::start(options(4 * 1024 * 1024));
    let dir = tempfile::tempdir().expect("temp dir");
    let dest = dir.path().join("payload.bin");
    let (handle, mut rx) = spawn_download(
        spec_for(&server.url("/payload.bin"), &dest),
        EngineConfig::default(),
    )
    .expect("spawn");

    // No yield between spawning and requesting stop: the coordinator must
    // notice the flag before attempting the first HTTP probe.
    tokio::time::timeout(Duration::from_secs(15), handle.stop())
        .await
        .expect("stop before probe");
    assert_eq!(handle.state(), rapids_engine::DownloadState::Stopped);
    wait_for(&mut rx, 15, "stopped", |event| matches!(event, EngineEvent::Stopped)).await;
    assert!(server.wait_idle(Duration::from_secs(10)), "requests settled");
    assert!(server.requests().is_empty(), "stop prevented a probe");
    assert!(!dest.exists());
    let mut claimant = Workspace::for_dest(&dest);
    claimant.create_and_lock().await.expect("no lock retained");
    drop(claimant);
}

#[tokio::test]
async fn cancel_discards_the_workspace() {
    let total = 16 * 1024 * 1024u64;
    let mut server_options = options(total);
    server_options.throttle_bps = Some(1024 * 1024);
    let server = TestServer::start(server_options);
    let dir = tempfile::tempdir().expect("temp dir");
    let dest = dir.path().join("payload.bin");

    let mut spec = spec_for(&server.url("/payload.bin"), &dest);
    spec.connections = Some(4);
    let (handle, mut rx) = spawn_download(spec, EngineConfig::default()).expect("spawn");

    wait_for(&mut rx, 30, "early progress", |event| match event {
        EngineEvent::Progress { downloaded, .. } => *downloaded >= 512 * 1024,
        _ => false,
    })
    .await;

    handle.cancel().await;
    wait_for(&mut rx, 15, "cancelled state", |event| {
        matches!(event, EngineEvent::Cancelled)
    })
    .await;
    drop(handle);

    assert!(
        !Workspace::for_dest(&dest).dir.exists(),
        "workspace removed"
    );
    assert!(!dest.exists(), "no final file after cancel");
}

#[tokio::test]
async fn second_process_is_rejected_while_the_lock_is_held() {
    let total = 32 * 1024 * 1024u64;
    let mut server_options = options(total);
    server_options.throttle_bps = Some(512 * 1024);
    let server = TestServer::start(server_options);
    let dir = tempfile::tempdir().expect("temp dir");
    let dest = dir.path().join("payload.bin");

    let mut spec = spec_for(&server.url("/payload.bin"), &dest);
    spec.connections = Some(4);
    let (first, mut first_rx) =
        spawn_download(spec.clone(), EngineConfig::default()).expect("spawn first");
    wait_for(&mut first_rx, 30, "first download running", |event| {
        matches!(
            event,
            EngineEvent::StateChanged {
                state: rapids_engine::DownloadState::Downloading
            }
        )
    })
    .await;

    let (second, mut second_rx) =
        spawn_download(spec, EngineConfig::default()).expect("spawn second");
    let failure = wait_for(&mut second_rx, 15, "lock rejection", |event| {
        matches!(event, EngineEvent::Failed { .. })
    })
    .await;
    drop(second);
    match failure {
        EngineEvent::Failed {
            message,
            retryable,
        } => {
            assert!(
                message.contains("working on"),
                "message should identify the busy workspace: {message}"
            );
            assert!(retryable, "a busy workspace is worth another attempt");
        }
        other => panic!("expected rejection, got {other}"),
    }

    first.cancel().await;
    wait_for(&mut first_rx, 15, "first cancelled", |event| {
        matches!(event, EngineEvent::Cancelled)
    })
    .await;
    drop(first);
}

#[tokio::test]
async fn empty_file_downloads_to_an_empty_file() {
    let total = 0u64;
    let server = TestServer::start(options(total));
    let dir = tempfile::tempdir().expect("temp dir");
    let dest = dir.path().join("empty.bin");

    let mut spec = spec_for(&server.url("/empty.bin"), &dest);
    spec.expected_sha256 = Some(expected_sha256(total));
    let (handle, mut rx) = spawn_download(spec, EngineConfig::default()).expect("spawn");

    let completed = wait_for(&mut rx, 15, "empty completion", |event| {
        matches!(event, EngineEvent::Completed { .. })
    })
    .await;
    drop(handle);

    assert_eq!(std::fs::read(&dest).expect("empty file").len(), 0);
    match completed {
        EngineEvent::Completed { sha256, .. } => {
            assert_eq!(sha256.as_deref(), Some(expected_sha256(total).as_str()));
        }
        other => panic!("expected completion, got {other}"),
    }
}

#[tokio::test]
async fn mid_download_validator_change_fails_the_task() {
    let total = 16 * 1024 * 1024u64;
    let mut server_options = options(total);
    // Every body connection drops after 256 KiB. A healthy long-lived
    // connection would finish its segment without ever asking the server
    // again, so the validator flip below would have nothing to intercept;
    // the drops guarantee that a request carries If-Range after the flip.
    server_options.flaky_every = Some(1);
    server_options.flaky_after = 256 * 1024;
    let server = TestServer::start(server_options);
    let dir = tempfile::tempdir().expect("temp dir");
    let dest = dir.path().join("payload.bin");

    let mut spec = spec_for(&server.url("/payload.bin"), &dest);
    spec.connections = Some(4);
    let (handle, mut rx) = spawn_download(spec, EngineConfig::default()).expect("spawn");

    wait_for(&mut rx, 30, "early progress", |event| match event {
        EngineEvent::Progress { downloaded, .. } => *downloaded >= 512 * 1024,
        _ => false,
    })
    .await;
    // The next attempt sends If-Range: "v1"; the server now serves "v2", so
    // the range is refused and the full body arrives instead — which is the
    // signal the engine must refuse to splice.
    server.set_etag("\"v2\"");

    let failure = wait_for(&mut rx, 30, "content-changed failure", |event| {
        matches!(event, EngineEvent::Failed { .. })
    })
    .await;
    drop(handle);

    match failure {
        EngineEvent::Failed {
            message,
            retryable,
        } => {
            assert!(
                message.contains("changed on the server"),
                "failure should explain the content change: {message}"
            );
            assert!(retryable, "replanning against the new bytes can succeed");
        }
        other => panic!("expected failure, got {other}"),
    }
}

#[tokio::test]
async fn mismatched_expected_sha_reports_both_hashes() {
    let total = 1024 * 1024u64;
    let server = TestServer::start(options(total));
    let dir = tempfile::tempdir().expect("temp dir");
    let dest = dir.path().join("payload.bin");
    let actual = expected_sha256(total);
    let wrong = "a".repeat(64);

    let mut spec = spec_for(&server.url("/payload.bin"), &dest);
    spec.expected_sha256 = Some(wrong.clone());
    let (handle, mut rx) = spawn_download(spec, EngineConfig::default()).expect("spawn");

    let failure = wait_for(&mut rx, 30, "verification failure", |event| {
        matches!(event, EngineEvent::Failed { .. })
    })
    .await;
    drop(handle);

    match failure {
        EngineEvent::Failed {
            message,
            retryable,
        } => {
            assert!(message.contains("SHA-256 verification failed"));
            assert!(
                message.contains(&wrong),
                "message must show the expectation"
            );
            assert!(
                message.contains(&actual),
                "message must show what was computed"
            );
            assert!(!retryable, "the same bytes will hash the same way again");
        }
        other => panic!("expected verification failure, got {other}"),
    }

    let workspace = Workspace::for_dest(&dest);
    assert!(
        workspace.part_path.exists(),
        "bytes are kept for inspection"
    );
    let manifest = Manifest::load(&workspace.dir)
        .await
        .expect("manifest present");
    assert_eq!(manifest.observed_sha256.as_deref(), Some(actual.as_str()));
    assert!(!manifest.completed);
}
