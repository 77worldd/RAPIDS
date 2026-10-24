//! Header profiles, the probe ladder, and per-request header application.
//!
//! The ladder is the engine's answer to servers that refuse specific
//! request shapes: send the full profile, drop cookies, then drop all
//! identity, and keep whichever rung answered.

use std::time::Duration;

use rapids_engine::headers::{HeaderProfile, RequestHeaders};
use rapids_engine::{DownloadSpec, EngineConfig, EngineEvent, probe_into, spawn_download};
use rapids_test_support::{TestServer, expected_sha256, options};

fn headers_with_identity() -> RequestHeaders {
    RequestHeaders {
        referer: Some("https://example.test/watch".to_string()),
        cookies: Some("session=abc123; theme=dark".to_string()),
        user_agent: Some("rapids-engine/test".to_string()),
        basic_auth: None,
        extra: Vec::new(),
    }
}

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

#[tokio::test]
async fn probe_keeps_the_full_profile_when_the_server_accepts_it() {
    let server = TestServer::start(options(1024));
    let url = server.url("/payload.bin");

    let outcome = probe_into(
        &reqwest::Client::new(),
        &url,
        &headers_with_identity(),
        EngineConfig::default(),
    )
    .await;

    assert_eq!(outcome.result.headers_used, HeaderProfile::Full);
    let requests = server.requests();
    assert!(
        requests.iter().any(|record| record.cookie.is_some()),
        "the full profile must send the cookies: {requests:?}"
    );
    assert!(
        requests.iter().any(|record| record.referer.is_some()),
        "the full profile must send the referer"
    );
}

#[tokio::test]
async fn probe_ladder_drops_cookies_when_the_server_refuses_them() {
    let mut server_options = options(1024);
    server_options.reject_cookies = true;
    let server = TestServer::start(server_options);
    let url = server.url("/payload.bin");

    let outcome = probe_into(
        &reqwest::Client::new(),
        &url,
        &headers_with_identity(),
        EngineConfig::default(),
    )
    .await;

    assert_eq!(
        outcome.result.headers_used,
        HeaderProfile::NoCookies,
        "the ladder must stop at the rung the server accepts"
    );
    let requests = server.requests();
    assert!(
        requests.iter().all(|record| record.cookie.is_none()),
        "no rung may carry cookies once the server refused them"
    );
    assert!(
        requests
            .iter()
            .any(|record| record.referer.is_some() || record.user_agent.is_some()),
        "only the cookies were the problem; identity stays"
    );
}

#[tokio::test]
async fn probe_ladder_drops_all_identity_when_cookies_are_not_the_problem() {
    let mut server_options = options(1024);
    server_options.reject_identity = true;
    let server = TestServer::start(server_options);
    let url = server.url("/payload.bin");

    let outcome = probe_into(
        &reqwest::Client::new(),
        &url,
        &headers_with_identity(),
        EngineConfig::default(),
    )
    .await;

    assert_eq!(
        outcome.result.headers_used,
        HeaderProfile::NoIdentity,
        "a server that refuses every identity header needs the last rung"
    );
    let requests = server.requests();
    assert!(
        requests.iter().all(|record| {
            record.cookie.is_none() && record.referer.is_none() && record.user_agent.is_none()
        }),
        "the accepted rung carries no identity at all"
    );
}

#[tokio::test]
async fn download_completes_through_a_cookie_rejecting_server() {
    let total = 2 * 1024 * 1024u64;
    let mut server_options = options(total);
    server_options.reject_cookies = true;
    let server = TestServer::start(server_options);
    let dir = tempfile::tempdir().expect("temp dir");
    let dest = dir.path().join("payload.bin");
    let expected = expected_sha256(total);

    let spec = DownloadSpec {
        url: server.url("/payload.bin"),
        dest: Some(dest.clone()),
        expected_sha256: Some(expected.clone()),
        connections: Some(2),
        overwrite: false,
        headers: headers_with_identity(),
    };
    let (handle, mut rx) = spawn_download(spec, EngineConfig::default()).expect("spawn");

    let completed = next_event(&mut rx, 30, "completion").await;
    drop(handle);

    match completed {
        EngineEvent::Completed { sha256, .. } => {
            assert_eq!(sha256.as_deref(), Some(expected.as_str()));
        }
        other => panic!("expected completion, got {other}"),
    }
    assert_eq!(
        rapids_engine::sha256_file(&dest).expect("hash"),
        expected,
        "the transferred bytes must be intact"
    );

    assert!(server.wait_idle(Duration::from_secs(10)), "server idle");
    let requests = server.requests();
    assert!(
        requests.iter().all(|record| record.cookie.is_none()),
        "the transfer must run on the rung the ladder accepted"
    );
}

/// A task that carries no identity at all never leaves the first rung.
#[tokio::test]
async fn an_identity_free_task_probes_once() {
    let total = 4096u64;
    let mut server_options = options(total);
    server_options.reject_identity = true;
    let server = TestServer::start(server_options);
    let url = server.url("/payload.bin");

    let outcome = probe_into(
        &reqwest::Client::new(),
        &url,
        &RequestHeaders::default(),
        EngineConfig::default(),
    )
    .await;

    assert_eq!(outcome.result.headers_used, HeaderProfile::Full);
    let requests = server.requests();
    assert_eq!(
        requests.len(),
        2,
        "HEAD plus one ranged GET; the ladder must not walk further"
    );
}

/// The resume gate must reject a restart whose profile is weaker, because
/// the bytes already fetched were requested with different headers.
#[tokio::test]
async fn a_weaker_profile_does_not_resume_a_manifest() {
    let total = 4096u64;
    let dir = tempfile::tempdir().expect("temp dir");
    let dest = dir.path().join("payload.bin");

    let full = RequestHeaders {
        cookies: Some("session=abc".to_string()),
        ..RequestHeaders::default()
    };
    let manifest = rapids_engine::Manifest {
        v: rapids_engine::manifest::MANIFEST_VERSION,
        url: "https://example.test/payload.bin".to_string(),
        final_url: "https://example.test/payload.bin".to_string(),
        dest: dest.clone(),
        total_bytes: Some(total),
        accepts_ranges: true,
        etag: Some("\"v1\"".to_string()),
        last_modified: None,
        content_type: None,
        filename: Some("payload.bin".to_string()),
        headers: full.clone(),
        header_profile: HeaderProfile::Full,
        expected_sha256: None,
        observed_sha256: None,
        created_at: rapids_engine::manifest::unix_timestamp(),
        segments: Vec::new(),
        completed: false,
    };
    manifest
        .save_atomic(dir.path())
        .await
        .expect("write manifest");
    let loaded = rapids_engine::Manifest::load(dir.path())
        .await
        .expect("read manifest");

    assert!(
        !loaded.can_resume(
            Some(total),
            Some("\"v1\""),
            None,
            HeaderProfile::NoCookies,
            &RequestHeaders::default(),
        ),
        "a resume with different headers must not trust the existing parts"
    );
    assert!(
        !loaded.can_resume(
            Some(total),
            Some("\"v1\""),
            None,
            HeaderProfile::Full,
            &RequestHeaders::default(),
        ),
        "the same ladder step with a different header set must not resume"
    );
    assert!(
        loaded.can_resume(
            Some(total),
            Some("\"v1\""),
            None,
            HeaderProfile::Full,
            &full
        ),
        "the identical profile and headers must resume"
    );
}
