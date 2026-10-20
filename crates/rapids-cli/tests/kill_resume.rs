use std::process::Command;
use std::time::{Duration, Instant};

use rapids_engine::{Manifest, Workspace};
use rapids_test_support::{expected_sha256, options, TestServer};

/// Bytes the engine has confirmed for the stored segments. The part file is
/// preallocated to the full size on purpose, so its length says nothing about
/// progress; the manifest's flushed low-water marks are the honest signal.
fn confirmed_bytes(runtime: &tokio::runtime::Runtime, workspace: &Workspace) -> u64 {
    match runtime.block_on(Manifest::load(&workspace.dir)) {
        Some(manifest) => manifest
            .segments
            .iter()
            .map(|segment| segment.written)
            .sum(),
        None => 0,
    }
}

/// End-to-end acceptance for resume: a 100 MB download is killed at the
/// halfway mark, restarted, and must produce the identical SHA-256 without
/// refetching the whole payload.
#[test]
fn killed_download_resumes_and_hashes_identically() {
    let total = 100 * 1024 * 1024u64;
    let mut server_options = options(total);
    server_options.throttle_bps = Some(3 * 1024 * 1024);
    let server = TestServer::start(server_options);
    let dir = tempfile::tempdir().expect("temp dir");
    let dest = dir.path().join("payload.bin");
    let dest_arg = dest.to_str().expect("utf-8 destination");
    let url = server.url("/payload.bin");
    let expected = expected_sha256(total);
    let binary = env!("CARGO_BIN_EXE_rapids");
    let workspace = Workspace::for_dest(&dest);
    let part = workspace.part_path.clone();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("runtime for manifest reads");

    // --- first run: kill around the halfway point ------------------------
    let mut first = Command::new(binary)
        .args([
            "get",
            url.as_str(),
            "-o",
            dest_arg,
            "--sha256",
            expected.as_str(),
            "--connections",
            "8",
        ])
        .spawn()
        .expect("first run");

    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        if let Some(status) = first.try_wait().expect("first run status") {
            panic!(
                "first run finished before the kill ({status}); lower the throttle so the \
                 halfway point is observable"
            );
        }
        if confirmed_bytes(&runtime, &workspace) >= total / 2 {
            break;
        }
        if Instant::now() >= deadline {
            let _ = first.kill();
            let _ = first.wait();
            panic!("first run never reached half of the payload");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    first.kill().expect("terminate first run");
    let _ = first.wait();

    assert!(part.exists(), "the workspace must survive the kill");
    assert!(
        !dest.exists(),
        "the download completed before the kill could land; lower the throttle"
    );
    let confirmed = confirmed_bytes(&runtime, &workspace);
    assert!(
        confirmed < total,
        "the kill must land before the download is complete"
    );

    // --- second run: resume from the part file ---------------------------
    server.clear_requests();
    let mut second = Command::new(binary)
        .args([
            "get",
            url.as_str(),
            "-o",
            dest_arg,
            "--sha256",
            expected.as_str(),
            "--connections",
            "8",
        ])
        .spawn()
        .expect("second run");

    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        if let Some(status) = second.try_wait().expect("second run status") {
            assert!(status.success(), "resume run failed with {status}");
            break;
        }
        if Instant::now() >= deadline {
            let _ = second.kill();
            panic!("resume run exceeded its time budget");
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    let final_bytes = std::fs::read(&dest).expect("final file");
    assert_eq!(final_bytes.len() as u64, total, "size mismatch");
    drop(final_bytes);
    assert_eq!(
        rapids_engine::sha256_file(&dest).expect("hash"),
        expected,
        "resumed download must hash exactly like the source"
    );

    let refetched: u64 = server
        .requests()
        .iter()
        .filter(|record| record.method == "GET")
        .map(|record| record.body_len)
        .sum();
    assert!(
        refetched < total * 9 / 10,
        "second run fetched {refetched} of {total} bytes; it restarted instead of resuming"
    );
}
