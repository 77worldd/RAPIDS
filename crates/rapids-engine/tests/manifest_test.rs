use rapids_engine::{Manifest, SegmentState};

fn sample_manifest() -> Manifest {
    Manifest {
        v: 1,
        url: "https://example.com/payload.bin".to_string(),
        final_url: "https://example.com/payload.bin".to_string(),
        dest: std::path::PathBuf::from("payload.bin"),
        total_bytes: Some(100),
        accepts_ranges: true,
        etag: Some("\"v1\"".to_string()),
        last_modified: Some("Wed, 21 Oct 2026 07:28:00 GMT".to_string()),
        content_type: Some("application/octet-stream".to_string()),
        filename: Some("payload.bin".to_string()),
        expected_sha256: None,
        observed_sha256: None,
        created_at: 1_761_000_000,
        segments: vec![
            SegmentState {
                index: 0,
                start: 0,
                end: 49,
                written: 25,
            },
            SegmentState {
                index: 1,
                start: 50,
                end: 99,
                written: 0,
            },
        ],
        completed: false,
    }
}

#[tokio::test]
async fn manifest_round_trips_through_atomic_save() {
    let dir = tempfile::tempdir().expect("temp dir");
    let manifest = sample_manifest();
    manifest.save_atomic(dir.path()).await.expect("save");

    let loaded = Manifest::load(dir.path()).await.expect("load");
    assert_eq!(loaded, manifest);
    assert!(!dir.path().join("manifest.json.tmp").exists());
}

#[tokio::test]
async fn missing_or_unreadable_manifest_yields_none() {
    let dir = tempfile::tempdir().expect("temp dir");
    assert!(Manifest::load(dir.path()).await.is_none());

    tokio::fs::write(dir.path().join("manifest.json"), b"{ not json")
        .await
        .expect("write garbage");
    assert!(Manifest::load(dir.path()).await.is_none());

    let mut future_version = sample_manifest();
    future_version.v = 99;
    future_version
        .save_atomic(dir.path())
        .await
        .expect("save");
    assert!(Manifest::load(dir.path()).await.is_none());
}

#[tokio::test]
async fn progress_recording_and_completion_check() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut manifest = sample_manifest();
    manifest.record_progress(&[50, 0]);
    assert_eq!(manifest.segments[0].written, 50);
    assert!(!manifest.is_fully_written());

    manifest.record_progress(&[50, 50]);
    assert!(manifest.is_fully_written());
    manifest.save_atomic(dir.path()).await.expect("save");
    let loaded = Manifest::load(dir.path()).await.expect("load");
    assert_eq!(loaded.segments[0].written, 50);

    let mut single = sample_manifest();
    single.accepts_ranges = false;
    single.segments = vec![SegmentState {
        index: 0,
        start: 0,
        end: 99,
        written: 0,
    }];
    assert!(!single.is_fully_written());
    single.record_progress(&[100]);
    assert!(single.is_fully_written());
}

#[test]
fn resume_requires_matching_validator_and_size() {
    let stored = sample_manifest();

    assert!(stored.can_resume(Some(100), Some("\"v1\""), None));
    assert!(stored.can_resume(
        Some(100),
        None,
        Some("Wed, 21 Oct 2026 07:28:00 GMT")
    ));

    assert!(!stored.can_resume(Some(101), Some("\"v1\""), None));
    assert!(!stored.can_resume(Some(100), Some("\"v2\""), None));
    assert!(!stored.can_resume(Some(100), None, None));
    assert!(!stored.can_resume(None, Some("\"v1\""), None));

    let mut no_etag = sample_manifest();
    no_etag.etag = None;
    assert!(no_etag.can_resume(
        Some(100),
        None,
        Some("Wed, 21 Oct 2026 07:28:00 GMT")
    ));
    assert!(!no_etag.can_resume(Some(100), None, Some("different date")));

    let mut no_validators = sample_manifest();
    no_validators.etag = None;
    no_validators.last_modified = None;
    assert!(!no_validators.can_resume(Some(100), Some("\"v1\""), None));

    let mut no_ranges = sample_manifest();
    no_ranges.accepts_ranges = false;
    assert!(!no_ranges.can_resume(Some(100), Some("\"v1\""), None));

    let mut no_size = sample_manifest();
    no_size.total_bytes = None;
    assert!(!no_size.can_resume(None, Some("\"v1\""), None));
}

#[test]
fn zero_length_download_counts_as_written() {
    let mut manifest = sample_manifest();
    manifest.total_bytes = Some(0);
    manifest.segments = Vec::new();
    assert!(manifest.is_fully_written());
}
