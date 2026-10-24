use std::future::Future;
use std::path::Path;
use std::sync::Arc;
use std::task::{Context, Waker};
use std::time::{Duration, Instant};

use rapids_engine::{
    DownloadSpec, EngineConfig, EngineEvent, RateLimiter, RateLimits, RequestHeaders, sha256_file,
    spawn_download,
};
use rapids_test_support::{TestServer, expected_sha256, options};

fn spec_for(url: &str, dest: &Path, connections: u8) -> DownloadSpec {
    DownloadSpec {
        url: url.to_string(),
        dest: Some(dest.to_path_buf()),
        expected_sha256: None,
        connections: Some(connections),
        overwrite: false,
        headers: RequestHeaders::default(),
    }
}

fn config_with(task_rate_bps: Option<u64>, global: Option<Arc<RateLimiter>>) -> EngineConfig {
    EngineConfig {
        rate_limit_bps: task_rate_bps,
        shared_limiter: global,
        ..EngineConfig::default()
    }
}

async fn wait_for_completion(rx: &mut tokio::sync::mpsc::Receiver<EngineEvent>, seconds: u64) {
    let deadline = Instant::now() + Duration::from_secs(seconds);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match tokio::time::timeout(remaining, rx.recv()).await {
            Ok(Some(EngineEvent::Completed { .. })) => return,
            Ok(Some(event)) if event.is_terminal() => panic!("unexpected terminal event: {event}"),
            Ok(Some(_)) => {}
            Ok(None) => panic!("event channel closed before completion"),
            Err(_) => panic!("download did not complete within {seconds}s"),
        }
    }
}

#[tokio::test]
async fn a_per_task_limit_caps_the_transfer() {
    let total = 2 * 1024 * 1024u64;
    let server = TestServer::start(options(total));
    let dir = tempfile::tempdir().expect("temp dir");
    let dest = dir.path().join("payload.bin");

    let started = Instant::now();
    let (handle, mut rx) = spawn_download(
        spec_for(&server.url("/payload.bin"), &dest, 8),
        config_with(Some(512 * 1024), None),
    )
    .expect("spawn");
    wait_for_completion(&mut rx, 60).await;
    let elapsed = started.elapsed();
    drop(handle);

    assert_eq!(sha256_file(&dest).expect("hash"), expected_sha256(total));
    // Two megabytes at 512 KiB/s is four seconds. The bucket starts full, so
    // allow the first quarter second of it back; the upper bound only has to
    // catch a limiter that is not throttling at all.
    assert!(
        elapsed >= Duration::from_millis(2_500),
        "limiter let the transfer finish in {elapsed:?}"
    );
    assert!(
        elapsed <= Duration::from_secs(20),
        "transfer took {elapsed:?}, far above the configured ceiling"
    );
}

#[tokio::test]
async fn a_global_limit_binds_a_download_with_no_limit_of_its_own() {
    let total = 1024 * 1024u64;
    let server = TestServer::start(options(total));
    let dir = tempfile::tempdir().expect("temp dir");
    let dest = dir.path().join("payload.bin");
    let global = Arc::new(RateLimiter::new(Some(256 * 1024)));

    let started = Instant::now();
    let (handle, mut rx) = spawn_download(
        spec_for(&server.url("/payload.bin"), &dest, 4),
        config_with(None, Some(Arc::clone(&global))),
    )
    .expect("spawn");
    wait_for_completion(&mut rx, 60).await;
    let elapsed = started.elapsed();
    drop(handle);

    assert_eq!(sha256_file(&dest).expect("hash"), expected_sha256(total));
    assert!(
        elapsed >= Duration::from_millis(2_000),
        "global limiter let the transfer finish in {elapsed:?}"
    );
    assert_eq!(global.rate_bps(), Some(256 * 1024));

    // A live change takes effect without restarting anything.
    global.set_rate(Some(8 * 1024 * 1024));
    assert_eq!(global.rate_bps(), Some(8 * 1024 * 1024));
}

#[tokio::test(start_paused = true)]
async fn two_downloads_debit_one_shared_global_bucket() {
    // A 64 KiB/s bucket holds a 16 KiB burst. Two independently composed
    // downloads cannot both spend that burst at the same instant.
    let global = Arc::new(RateLimiter::new(Some(64 * 1024)));
    let first = RateLimits::compose(Some(Arc::clone(&global)), None);
    let second = RateLimits::compose(Some(Arc::clone(&global)), None);
    let started = tokio::time::Instant::now();
    let (one, two) = tokio::join!(
        first.acquire_while(16 * 1024, || true),
        second.acquire_while(16 * 1024, || true)
    );
    assert!(one && two);
    let elapsed = started.elapsed();
    assert!(
        elapsed >= Duration::from_millis(250),
        "both downloads spent the same global burst in {elapsed:?}"
    );
    assert!(
        elapsed <= Duration::from_millis(500),
        "a 16 KiB refill at 64 KiB/s took {elapsed:?} of paused clock time"
    );
}

#[tokio::test(start_paused = true)]
async fn a_partial_global_payment_is_reserved_under_contention() {
    let global = Arc::new(RateLimiter::new(Some(64 * 1024)));
    let first = RateLimits::compose(Some(Arc::clone(&global)), None);
    let second = RateLimits::compose(Some(Arc::clone(&global)), None);
    assert!(first.acquire_while(16 * 1024, || true).await);
    tokio::time::advance(Duration::from_millis(125)).await;

    let started = tokio::time::Instant::now();
    let mut older = Box::pin(first.acquire_while(16 * 1024, || true));
    let mut context = Context::from_waker(Waker::noop());
    assert!(older.as_mut().poll(&mut context).is_pending());
    let mut newer = Box::pin(second.acquire_while(8 * 1024, || true));
    assert!(
        newer.as_mut().poll(&mut context).is_pending(),
        "the second waiter cannot take the first waiter's partial payment"
    );

    let (one, two) = tokio::join!(older, newer);
    assert!(one && two);
    let elapsed = started.elapsed();
    assert!(
        elapsed >= Duration::from_millis(250),
        "two claims spent more than the shared bucket earned in {elapsed:?}"
    );
    assert!(
        elapsed <= Duration::from_millis(500),
        "shared claims stalled despite sufficient credit in {elapsed:?}"
    );
}

#[tokio::test]
async fn a_throttled_download_still_stops_on_pause() {
    let total = 4 * 1024 * 1024u64;
    let server = TestServer::start(options(total));
    let dir = tempfile::tempdir().expect("temp dir");
    let dest = dir.path().join("payload.bin");

    let (handle, mut rx) = spawn_download(
        spec_for(&server.url("/payload.bin"), &dest, 4),
        config_with(Some(256 * 1024), None),
    )
    .expect("spawn");

    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match tokio::time::timeout(remaining, rx.recv()).await {
            Ok(Some(EngineEvent::Progress { downloaded, .. })) if downloaded >= 128 * 1024 => break,
            Ok(Some(event)) if event.is_terminal() => panic!("terminal event early: {event}"),
            Ok(Some(_)) => {}
            Ok(None) => panic!("event channel closed early"),
            Err(_) => panic!("no throttled progress within the deadline"),
        }
    }

    // The pump is parked inside a bucket wait at this point; the stop
    // callback has to cut that wait short rather than sit out the chunk.
    let pause_started = Instant::now();
    handle.pause().await;
    let pause_took = pause_started.elapsed();
    assert!(
        pause_took < Duration::from_secs(5),
        "pause took {pause_took:?} while the download was throttled"
    );

    handle.cancel().await;
    assert!(
        !dest.exists(),
        "a cancelled download never publishes a file"
    );
}
