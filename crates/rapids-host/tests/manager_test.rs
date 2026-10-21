use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rapids_host::events::{EventSink, HostEvent, RecordingSink};
use rapids_host::store::{
    Store, TaskCategory, TaskRow, TaskStatus, history, settings, tasks,
};
use rapids_host::{AddRequest, HostError, ManagerHandle, now_ms};
use rapids_test_support::{TestServer, expected_sha256, options};
use tokio::task::JoinHandle;

const WAIT: Duration = Duration::from_secs(60);

struct Harness {
    store: Arc<Store>,
    manager: ManagerHandle,
    sink: Arc<RecordingSink>,
    join: JoinHandle<()>,
    downloads: PathBuf,
    _dir: tempfile::TempDir,
}

impl Harness {
    async fn start() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let downloads = dir.path().join("downloads");
        std::fs::create_dir_all(&downloads).expect("downloads");
        let store = Arc::new(
            Store::open(&dir.path().join("rapids.db"))
                .await
                .expect("open store"),
        );
        let sink = Arc::new(RecordingSink::new());
        let sink_port: Arc<dyn EventSink> = Arc::clone(&sink) as Arc<dyn EventSink>;
        let (manager, join) = rapids_host::manager::spawn(Arc::clone(&store), sink_port);
        Self {
            store,
            manager,
            sink,
            join,
            downloads,
            _dir: dir,
        }
    }

    fn request(&self, url: &str) -> AddRequest {
        AddRequest {
            url: url.to_string(),
            dest_dir: self.downloads.clone(),
            category: TaskCategory::Apps,
            priority: 0,
            connections: Some(4),
            speed_limit_bps: None,
            expected_sha256: None,
            schedule_id: None,
        }
    }

    fn destination(&self, file_name: &str) -> PathBuf {
        self.downloads.join(file_name)
    }

    async fn row(&self, id: i64) -> TaskRow {
        tasks::get(self.store.pool(), id)
            .await
            .expect("read task")
            .expect("task is present")
    }

    /// Polls the row until `predicate` holds, so timing never decides a test.
    async fn wait_for<F>(&self, id: i64, label: &str, predicate: F) -> TaskRow
    where
        F: Fn(&TaskRow) -> bool,
    {
        let deadline = Instant::now() + WAIT;
        loop {
            let row = self.row(id).await;
            if predicate(&row) {
                return row;
            }
            if Instant::now() >= deadline {
                panic!(
                    "{label}: timed out with status {}, downloaded {}",
                    row.status, row.downloaded
                );
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    /// Parks every download and stops the manager task, so a test never
    /// leaves work running into the next one.
    async fn finish(self) {
        self.manager.shutdown().await;
        let _ = self.join.await;
    }
}

fn status(row: &TaskRow) -> Option<TaskStatus> {
    row.status_enum()
}

#[tokio::test]
async fn an_added_task_downloads_and_records_its_hash() {
    let total = 512 * 1024u64;
    let server = TestServer::start(options(total));
    let harness = Harness::start().await;

    let view = harness
        .manager
        .add(harness.request(&server.url("/payload.bin")))
        .await
        .expect("add");
    assert_eq!(view.status, "queued");
    assert_eq!(view.file_name, "payload.bin");

    let row = harness
        .wait_for(view.id, "completion", |row| {
            status(row) == Some(TaskStatus::Completed)
        })
        .await;

    let expected = expected_sha256(total);
    assert_eq!(row.observed_sha256.as_deref(), Some(expected.as_str()));
    assert_eq!(row.total_bytes, Some(total as i64));
    assert_eq!(row.downloaded, total as i64);
    assert!(row.completed_at.is_some());

    let file = harness.destination("payload.bin");
    assert_eq!(std::fs::metadata(&file).expect("file").len(), total);

    let recorded = history::recent(harness.store.pool(), 10)
        .await
        .expect("history");
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].outcome, "completed");
    assert_eq!(recorded[0].task_id, view.id);

    assert!(
        harness.sink.touched_ids().contains(&view.id),
        "the shell must hear about the task at least once"
    );
    assert!(
        harness
            .sink
            .events()
            .iter()
            .any(|event| matches!(event, HostEvent::Progress { .. })),
        "progress samples reach the shell"
    );

    harness.finish().await;
}

#[tokio::test]
async fn a_paused_task_stops_and_a_resumed_one_finishes() {
    let total = 4 * 1024 * 1024u64;
    let mut server_options = options(total);
    server_options.throttle_bps = Some(512 * 1024);
    let server = TestServer::start(server_options);
    let harness = Harness::start().await;

    let view = harness
        .manager
        .add(harness.request(&server.url("/payload.bin")))
        .await
        .expect("add");

    harness
        .wait_for(view.id, "progress before pause", |row| row.downloaded > 64 * 1024)
        .await;

    harness.manager.pause(view.id).await.expect("pause");
    let paused = harness.row(view.id).await;
    assert_eq!(status(&paused), Some(TaskStatus::Paused));

    let held = paused.downloaded;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let still_held = harness.row(view.id).await.downloaded;
    assert!(
        still_held <= held + 64 * 1024,
        "a paused download must not keep growing: {held} then {still_held}"
    );

    harness.manager.resume(view.id).await.expect("resume");
    let completed = harness
        .wait_for(view.id, "completion after resume", |row| {
            status(row) == Some(TaskStatus::Completed)
        })
        .await;
    assert_eq!(
        completed.observed_sha256.as_deref(),
        Some(expected_sha256(total).as_str())
    );

    harness.finish().await;
}

#[tokio::test]
async fn cancelling_removes_the_row_and_leaves_no_file() {
    let total = 4 * 1024 * 1024u64;
    let mut server_options = options(total);
    server_options.throttle_bps = Some(512 * 1024);
    let server = TestServer::start(server_options);
    let harness = Harness::start().await;

    let view = harness
        .manager
        .add(harness.request(&server.url("/payload.bin")))
        .await
        .expect("add");
    harness
        .wait_for(view.id, "some progress", |row| row.downloaded > 0)
        .await;

    harness.manager.cancel(view.id).await.expect("cancel");

    assert!(
        tasks::get(harness.store.pool(), view.id)
            .await
            .expect("read")
            .is_none(),
        "the row is gone"
    );
    assert!(
        !harness.destination("payload.bin").exists(),
        "no final file appears"
    );
    let recorded = history::recent(harness.store.pool(), 10)
        .await
        .expect("history");
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].outcome, "cancelled");

    harness.finish().await;
}

#[tokio::test]
async fn deleting_with_remove_file_takes_the_download_with_it() {
    let total = 256 * 1024u64;
    let server = TestServer::start(options(total));
    let harness = Harness::start().await;

    let view = harness
        .manager
        .add(harness.request(&server.url("/payload.bin")))
        .await
        .expect("add");
    harness
        .wait_for(view.id, "completion", |row| {
            status(row) == Some(TaskStatus::Completed)
        })
        .await;
    let file = harness.destination("payload.bin");
    assert!(file.exists());

    harness.manager.delete(view.id, true).await.expect("delete");

    assert!(!file.exists(), "the file is deleted on request");
    assert!(
        tasks::get(harness.store.pool(), view.id)
            .await
            .expect("read")
            .is_none()
    );

    harness.finish().await;
}

#[tokio::test]
async fn the_parallel_limit_holds_the_rest_of_the_queue() {
    let total = 2 * 1024 * 1024u64;
    let mut server_options = options(total);
    server_options.throttle_bps = Some(256 * 1024);
    let server = TestServer::start(server_options);
    let harness = Harness::start().await;

    settings::set(harness.store.pool(), settings::MAX_PARALLEL, "2", now_ms())
        .await
        .expect("set parallel limit");

    let mut ids = Vec::new();
    for index in 0..3 {
        let request = harness.request(&server.url(&format!("/payload-{index}.bin")));
        ids.push(harness.manager.add(request).await.expect("add").id);
    }

    let deadline = Instant::now() + WAIT;
    let mut peak = 0;
    loop {
        let mut active = 0;
        let mut waiting = 0;
        for id in &ids {
            match status(&harness.row(*id).await) {
                Some(TaskStatus::Downloading) => active += 1,
                Some(TaskStatus::Queued) => waiting += 1,
                _ => {}
            }
        }
        peak = peak.max(active);
        assert!(peak <= 2, "three tasks ran at once");
        if active == 2 && waiting == 1 {
            break;
        }
        if Instant::now() >= deadline {
            panic!("the queue never settled into two running and one waiting");
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    harness.finish().await;
}

#[tokio::test]
async fn a_failed_download_is_recorded_and_can_be_retried() {
    let total = 128 * 1024u64;
    let server = TestServer::start(options(total));
    let harness = Harness::start().await;

    // A destination that already exists makes the engine refuse the task
    // before any network traffic, which is a deterministic failure.
    let blocked = harness.destination("payload.bin");
    std::fs::write(&blocked, b"already here").expect("blocking file");

    let view = harness
        .manager
        .add(harness.request(&server.url("/payload.bin")))
        .await
        .expect("add");
    let failed = harness
        .wait_for(view.id, "failure", |row| {
            status(row) == Some(TaskStatus::Error)
        })
        .await;
    assert!(
        failed
            .error_message
            .as_deref()
            .is_some_and(|message| message.contains("already exists")),
        "the engine's message is kept verbatim: {:?}",
        failed.error_message
    );
    assert_eq!(failed.attempts, 1);

    std::fs::remove_file(&blocked).expect("clear the obstacle");
    harness.manager.retry(view.id).await.expect("retry");
    harness
        .wait_for(view.id, "completion after retry", |row| {
            status(row) == Some(TaskStatus::Completed)
        })
        .await;

    harness.finish().await;
}

#[tokio::test]
async fn an_unknown_id_is_reported_not_ignored() {
    let harness = Harness::start().await;
    let error = harness.manager.pause(9_999).await.expect_err("no task");
    assert!(matches!(error, HostError::NotFound { .. }));
    harness.finish().await;
}

#[tokio::test]
async fn interrupted_rows_return_to_the_queue_on_startup() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("rapids.db");
    let downloads = dir.path().join("downloads");
    std::fs::create_dir_all(&downloads).expect("downloads");

    // A row left mid-download by a previous process, plus a higher-priority
    // task that will hold the only running slot.
    let store = Store::open(&path).await.expect("open");
    let interrupted = tasks::insert(
        store.pool(),
        &rapids_host::store::NewTask {
            url: "https://example.com/file.bin".to_string(),
            dest_path: downloads.join("file.bin").to_string_lossy().to_string(),
            file_name: "file.bin".to_string(),
            category: TaskCategory::Apps,
            status: TaskStatus::Downloading,
            priority: 0,
            connections: None,
            speed_limit_bps: None,
            expected_sha256: None,
            schedule_id: None,
            created_at: now_ms(),
        },
    )
    .await
    .expect("insert interrupted");
    settings::set(store.pool(), settings::MAX_PARALLEL, "1", now_ms())
        .await
        .expect("parallel limit");
    store.close().await;

    let mut server_options = options(1024 * 1024);
    server_options.throttle_bps = Some(128 * 1024);
    let server = TestServer::start(server_options);

    let store = Arc::new(Store::open(&path).await.expect("reopen"));
    let blocker = tasks::insert(
        store.pool(),
        &rapids_host::store::NewTask {
            url: server.url("/blocker.bin"),
            dest_path: downloads.join("blocker.bin").to_string_lossy().to_string(),
            file_name: "blocker.bin".to_string(),
            category: TaskCategory::Apps,
            status: TaskStatus::Queued,
            priority: 10,
            connections: Some(2),
            speed_limit_bps: None,
            expected_sha256: None,
            schedule_id: None,
            created_at: now_ms(),
        },
    )
    .await
    .expect("insert blocker");

    let sink = Arc::new(RecordingSink::new());
    let sink_port: Arc<dyn EventSink> = Arc::clone(&sink) as Arc<dyn EventSink>;
    let (manager, join) = rapids_host::manager::spawn(Arc::clone(&store), sink_port);

    // The interrupted row is back in the queue, waiting behind the task that
    // took the single slot; nothing about it was resumed from the database.
    let deadline = Instant::now() + WAIT;
    loop {
        let row = tasks::get(store.pool(), interrupted)
            .await
            .expect("read")
            .expect("present");
        if row.status == "queued" {
            break;
        }
        if Instant::now() >= deadline {
            panic!("the interrupted row stayed {}", row.status);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let holder = tasks::get(store.pool(), blocker)
        .await
        .expect("read")
        .expect("present");
    assert_eq!(holder.status, "downloading");

    manager.shutdown().await;
    let _ = join.await;
}
