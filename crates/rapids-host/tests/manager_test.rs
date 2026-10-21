use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rapids_engine::{Manifest, Workspace};
use rapids_host::events::{EventSink, HostEvent, RecordingSink};
use rapids_host::store::{Store, TaskCategory, TaskRow, TaskStatus, history, settings, tasks};
use rapids_host::{AddRequest, Clock, HostError, ManagerHandle, now_ms};
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
        let (manager, join) =
            rapids_host::manager::spawn(Arc::clone(&store), sink_port, Clock::utc());
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
        .wait_for(view.id, "progress before pause", |row| {
            row.downloaded > 64 * 1024
        })
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
    let (manager, join) = rapids_host::manager::spawn(Arc::clone(&store), sink_port, Clock::utc());

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

#[tokio::test]
async fn shutdown_unlocks_a_running_download_and_restarts_from_its_sidecar() {
    let total = 16 * 1024 * 1024u64;
    let mut server_options = options(total);
    server_options.throttle_bps = Some(512 * 1024);
    let server = TestServer::start(server_options);
    let harness = Harness::start().await;

    let view = harness
        .manager
        .add(harness.request(&server.url("/payload.bin")))
        .await
        .expect("add");
    let running = harness
        .wait_for(view.id, "progress before shutdown", |row| {
            row.downloaded >= 2 * 1024 * 1024
        })
        .await;
    assert_eq!(running.status_enum(), Some(TaskStatus::Downloading));

    let Harness {
        store,
        manager,
        sink: _,
        join,
        downloads,
        _dir,
    } = harness;
    tokio::time::timeout(Duration::from_secs(15), manager.shutdown())
        .await
        .expect("shutdown completed");
    join.await.expect("first manager stopped");

    let interrupted = tasks::get(store.pool(), view.id)
        .await
        .expect("read task")
        .expect("task survives shutdown");
    assert_eq!(interrupted.status_enum(), Some(TaskStatus::Downloading));
    assert!(
        history::recent(store.pool(), 10)
            .await
            .expect("history")
            .is_empty(),
        "stopping does not record a failed or cancelled outcome"
    );
    let dest = downloads.join("payload.bin");
    let workspace = Workspace::for_dest(&dest);
    let saved = Manifest::load(&workspace.dir)
        .await
        .expect("shutdown checkpoint");
    let confirmed: u64 = saved.segments.iter().map(|segment| segment.written).sum();
    assert!(
        confirmed >= u64::try_from(interrupted.downloaded).expect("non-negative progress")
            && confirmed < total,
        "checkpoint saved {confirmed} of {total} bytes"
    );
    assert!(workspace.part_path.exists());
    assert!(!dest.exists());

    // This is still the same process. The manager's acknowledgement must
    // mean the engine has released the workspace, not merely paused it.
    let mut claimant = Workspace::for_dest(&dest);
    claimant
        .create_and_lock()
        .await
        .expect("workspace unlocked");
    drop(claimant);
    drop(manager);
    Arc::try_unwrap(store)
        .unwrap_or_else(|_| panic!("the first manager still holds the store"))
        .close()
        .await;

    assert!(
        server.wait_idle(Duration::from_secs(10)),
        "old requests settled"
    );
    server.clear_requests();
    let store = Arc::new(
        Store::open(&_dir.path().join("rapids.db"))
            .await
            .expect("reopen store"),
    );
    let (restarted, join) = rapids_host::manager::spawn(
        Arc::clone(&store),
        Arc::new(RecordingSink::new()),
        Clock::utc(),
    );
    let deadline = Instant::now() + WAIT;
    let done = loop {
        let row = tasks::get(store.pool(), view.id)
            .await
            .expect("read restarted task")
            .expect("task remains present");
        if row.status_enum() == Some(TaskStatus::Completed) {
            break row;
        }
        assert_ne!(
            row.status_enum(),
            Some(TaskStatus::Error),
            "restart failed: {:?}",
            row.error_message.as_deref()
        );
        if Instant::now() >= deadline {
            panic!("restart stalled in {}", row.status);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    };
    let expected = expected_sha256(total);
    assert_eq!(done.observed_sha256.as_deref(), Some(expected.as_str()));
    assert_eq!(rapids_engine::sha256_file(&dest).expect("hash"), expected);
    assert!(!workspace.dir.exists(), "completion removes the sidecar");
    restarted.list().await.expect("manager caught up");
    let entries = history::recent(store.pool(), 10).await.expect("history");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].outcome, "completed");
    assert_eq!(entries[0].task_id, view.id);
    assert!(
        server.wait_idle(Duration::from_secs(10)),
        "resumed requests settled"
    );
    let refetched: u64 = server
        .requests()
        .iter()
        .filter(|record| record.method == "GET")
        .map(|record| record.body_len)
        .sum();
    assert!(
        refetched < total * 9 / 10,
        "the restart fetched {refetched} of {total} bytes"
    );

    restarted.shutdown().await;
    join.await.expect("restarted manager stopped");
    drop(restarted);
    Arc::try_unwrap(store)
        .unwrap_or_else(|_| panic!("the restarted manager still holds the store"))
        .close()
        .await;
}

#[tokio::test]
async fn ten_queued_downloads_survive_a_store_and_manager_restart() {
    let total = 64 * 1024u64;
    let server = TestServer::start(options(total));
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("rapids.db");
    let downloads = dir.path().join("downloads");
    std::fs::create_dir_all(&downloads).expect("downloads");

    let store = Arc::new(Store::open(&path).await.expect("open store"));
    let (manager, join) = rapids_host::manager::spawn(
        Arc::clone(&store),
        Arc::new(RecordingSink::new()),
        Clock::utc(),
    );
    manager.pause_queue().await.expect("pause queue");

    let mut inserted = Vec::new();
    for index in 0..10 {
        let file_name = format!("payload-{index}.bin");
        let url = server.url(&format!("/{file_name}"));
        let view = manager
            .add(AddRequest {
                url: url.clone(),
                dest_dir: downloads.clone(),
                category: TaskCategory::Apps,
                priority: 0,
                connections: Some(2),
                speed_limit_bps: None,
                expected_sha256: None,
                schedule_id: None,
            })
            .await
            .expect("queue download");
        assert_eq!(view.status, "queued");
        inserted.push((view.id, file_name, url));
    }
    assert_eq!(tasks::count(store.pool()).await.expect("count"), 10);
    assert!(server.requests().is_empty(), "the paused queue did no work");

    manager.shutdown().await;
    join.await.expect("first manager stopped");
    drop(manager);
    Arc::try_unwrap(store)
        .unwrap_or_else(|_| panic!("the first manager still holds the store"))
        .close()
        .await;

    let store = Arc::new(Store::open(&path).await.expect("reopen store"));
    let waiting = tasks::waiting(store.pool()).await.expect("reloaded queue");
    assert_eq!(waiting.len(), 10);
    assert_eq!(
        waiting.iter().map(|row| row.id).collect::<Vec<_>>(),
        inserted.iter().map(|(id, _, _)| *id).collect::<Vec<_>>(),
        "the persisted queue keeps first-in-first-out order"
    );
    for (row, (id, file_name, url)) in waiting.iter().zip(&inserted) {
        assert_eq!(row.id, *id);
        assert_eq!(row.status_enum(), Some(TaskStatus::Queued));
        assert_eq!(row.url.as_str(), url.as_str());
        assert_eq!(row.file_name.as_str(), file_name.as_str());
        assert_eq!(
            row.dest_path,
            downloads.join(file_name).to_string_lossy().into_owned()
        );
        assert_eq!(row.connections, Some(2));
        assert_eq!(row.downloaded, 0);
    }

    // Queue-paused is a process-local choice. On restart, every saved row is
    // eligible to run; the engine still owns the bytes and completed hashes.
    let (restarted, join) = rapids_host::manager::spawn(
        Arc::clone(&store),
        Arc::new(RecordingSink::new()),
        Clock::utc(),
    );
    let deadline = Instant::now() + WAIT;
    let finished = loop {
        let rows = tasks::list(store.pool()).await.expect("list tasks");
        if rows.len() == 10
            && rows
                .iter()
                .all(|row| status(row) == Some(TaskStatus::Completed))
        {
            break rows;
        }
        if Instant::now() >= deadline {
            panic!(
                "the restarted queue did not finish: {:?}",
                rows.iter()
                    .map(|row| (row.id, row.status.as_str(), row.error_message.as_deref()))
                    .collect::<Vec<_>>()
            );
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    };

    let expected = expected_sha256(total);
    for (id, file_name, _) in &inserted {
        let row = finished
            .iter()
            .find(|row| row.id == *id)
            .expect("saved task");
        assert_eq!(row.observed_sha256.as_deref(), Some(expected.as_str()));
        assert_eq!(row.downloaded, total as i64);
        let file = downloads.join(file_name);
        assert_eq!(
            rapids_engine::sha256_file(&file).expect("file hash"),
            expected
        );
    }
    // Completion is written before its history entry within one manager turn.
    // A command reply is the barrier that tells us the last turn has finished.
    restarted.list().await.expect("manager caught up");
    let recorded = history::recent(store.pool(), 10).await.expect("history");
    assert_eq!(recorded.len(), 10);
    assert!(recorded.iter().all(|entry| entry.outcome == "completed"));
    let mut recorded_ids: Vec<_> = recorded.iter().map(|entry| entry.task_id).collect();
    recorded_ids.sort_unstable();
    let mut inserted_ids: Vec<_> = inserted.iter().map(|(id, _, _)| *id).collect();
    inserted_ids.sort_unstable();
    assert_eq!(recorded_ids, inserted_ids);

    restarted.shutdown().await;
    join.await.expect("restarted manager stopped");
    drop(restarted);
    Arc::try_unwrap(store)
        .unwrap_or_else(|_| panic!("the restarted manager still holds the store"))
        .close()
        .await;
}
