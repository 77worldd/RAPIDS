use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rapids_engine::{
    ConnectionBudget, DownloadSpec, EngineConfig, EngineEvent, RequestHeaders, sha256_file,
    spawn_download,
};
use rapids_test_support::{TestServer, expected_sha256, options};

fn download(url: &str, dest: &Path) -> DownloadSpec {
    DownloadSpec {
        url: url.to_string(),
        dest: Some(dest.to_path_buf()),
        expected_sha256: None,
        connections: Some(4),
        overwrite: false,
        headers: RequestHeaders::default(),
    }
}

fn record(
    event: Option<EngineEvent>,
    completed: &mut bool,
    saw_transfer_with_lease: &mut bool,
    budget: &ConnectionBudget,
) {
    match event {
        Some(EngineEvent::Completed { .. }) => *completed = true,
        Some(EngineEvent::Progress { downloaded, .. }) => {
            *saw_transfer_with_lease |= downloaded > 0 && budget.in_use() == 1;
        }
        Some(event) if event.is_terminal() => panic!("unexpected terminal event: {event}"),
        Some(_) => {}
        None => panic!("event channel closed before completion"),
    }
}

#[tokio::test]
async fn separate_downloads_share_one_active_http_connection() {
    let total = 2 * 1024 * 1024u64;
    let mut server_options = options(total);
    server_options.throttle_bps = Some(1024 * 1024);
    let server = TestServer::start(server_options);
    let dir = tempfile::tempdir().expect("temp dir");
    let left = dir.path().join("left.bin");
    let right = dir.path().join("right.bin");
    let budget = Arc::new(ConnectionBudget::new(1));
    let config = EngineConfig {
        connection_budget: Some(Arc::clone(&budget)),
        ..EngineConfig::default()
    };

    let (left_handle, mut left_events) =
        spawn_download(download(&server.url("/left.bin"), &left), config.clone())
            .expect("left download");
    let (right_handle, mut right_events) =
        spawn_download(download(&server.url("/right.bin"), &right), config)
            .expect("right download");

    let deadline = Instant::now() + Duration::from_secs(30);
    let (mut left_done, mut right_done, mut saw_transfer_with_lease) = (false, false, false);
    let mut peak = 0;
    while !left_done || !right_done {
        let current = budget.in_use();
        peak = peak.max(current);
        assert!(current <= 1, "app-wide budget allowed {current} requests");
        assert!(Instant::now() < deadline, "downloads exceeded the deadline");
        tokio::select! {
            event = left_events.recv(), if !left_done => {
                record(event, &mut left_done, &mut saw_transfer_with_lease, &budget);
            }
            event = right_events.recv(), if !right_done => {
                record(event, &mut right_done, &mut saw_transfer_with_lease, &budget);
            }
            _ = tokio::time::sleep(Duration::from_millis(10)) => {}
        }
    }
    drop((left_handle, right_handle));
    assert_eq!(peak, 1, "at least one network attempt used the budget");
    assert!(
        saw_transfer_with_lease,
        "the shared lease must cover a transfer body, not just its probe"
    );
    let hash = expected_sha256(total);
    assert_eq!(sha256_file(&left).expect("left file"), hash);
    assert_eq!(sha256_file(&right).expect("right file"), hash);
    assert_eq!(budget.in_use(), 0, "completion returned every lease");
}
