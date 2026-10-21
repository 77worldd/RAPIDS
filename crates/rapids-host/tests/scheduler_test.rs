//! The scheduler as the manager runs it: start windows release their work,
//! retries wait their backoff, and the queue never starts either early.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rapids_host::events::{EventSink, RecordingSink};
use rapids_host::scheduler::{DAY_MS, MINUTE_MS, window_state};
use rapids_host::store::{
    NewSchedule, NewTask, ScheduleKind, ScheduleRow, Store, TaskCategory, TaskRow, TaskStatus,
    history, schedules, settings, tasks,
};
use rapids_host::{AddRequest, Clock, ManagerHandle, Window, now_ms};
use rapids_test_support::{TestServer, expected_sha256, options};
use tokio::task::JoinHandle;

const WAIT: Duration = Duration::from_secs(60);

/// A database and a downloads folder, before anything is running against them.
/// Tests that need rows in place first build one of these, write, and then
/// start the manager over it.
struct Pending {
    store: Arc<Store>,
    downloads: PathBuf,
    _dir: tempfile::TempDir,
}

impl Pending {
    async fn new() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let downloads = dir.path().join("downloads");
        std::fs::create_dir_all(&downloads).expect("downloads");
        let store = Arc::new(
            Store::open(&dir.path().join("rapids.db"))
                .await
                .expect("open store"),
        );
        Self {
            store,
            downloads,
            _dir: dir,
        }
    }

    fn start(self) -> Harness {
        let sink = Arc::new(RecordingSink::new());
        let sink_port: Arc<dyn EventSink> = Arc::clone(&sink) as Arc<dyn EventSink>;
        let (manager, join) = rapids_host::manager::spawn(
            Arc::clone(&self.store),
            sink_port,
            Clock::utc(),
        );
        Harness {
            store: self.store,
            manager,
            sink,
            join,
            downloads: self.downloads,
            _dir: self._dir,
        }
    }
}

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
        Pending::new().await.start()
    }

    async fn set(&self, key: &str, value: &str) {
        settings::set(self.store.pool(), key, value, now_ms())
            .await
            .expect("write setting");
    }

    fn request(&self, url: &str) -> AddRequest {
        AddRequest {
            url: url.to_string(),
            dest_dir: self.downloads.clone(),
            category: TaskCategory::Apps,
            priority: 0,
            connections: Some(2),
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
                    "{label}: timed out with status {}, attempts {}",
                    row.status, row.attempts
                );
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    async fn finish(self) {
        self.manager.shutdown().await;
        let _ = self.join.await;
    }

    /// End the first manager and pool before reading the same database with
    /// a new manager. The temporary directory stays alive across the gap.
    async fn stop_and_reopen(self) -> Pending {
        let Self {
            store,
            manager,
            sink: _,
            join,
            downloads,
            _dir,
        } = self;
        manager.shutdown().await;
        join.await.expect("manager stopped");
        drop(manager);
        Arc::try_unwrap(store)
            .unwrap_or_else(|_| panic!("the old manager still holds the store"))
            .close()
            .await;
        let store = Arc::new(
            Store::open(&_dir.path().join("rapids.db"))
                .await
                .expect("reopen store"),
        );
        Pending {
            store,
            downloads,
            _dir,
        }
    }
}

#[tokio::test]
async fn a_retryable_failure_waits_its_backoff_and_tries_again() {
    let total = 64 * 1024u64;
    let server = TestServer::start(options(total));
    let harness = Harness::start().await;
    harness.set(settings::RETRY_MAX_ATTEMPTS, "3").await;
    harness.set(settings::RETRY_BASE_MS, "800").await;
    harness.set(settings::RETRY_CAP_MS, "2000").await;

    // Holding the workspace lock from this process is a deterministic,
    // instant failure that the engine classifies as worth retrying.
    let workspace = harness.downloads.join("payload.bin.rapids");
    std::fs::create_dir_all(&workspace).expect("workspace");
    let held = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(workspace.join("lock"))
        .expect("lock file");
    held.try_lock().expect("hold the workspace");

    let view = harness
        .manager
        .add(harness.request(&server.url("/payload.bin")))
        .await
        .expect("add");

    // The first failure goes back to the queue with a due time rather than
    // stopping, and the row says when it will be tried again.
    let waiting = harness
        .wait_for(view.id, "first retry", |row| {
            row.attempts == 1 && row.status_enum() == Some(TaskStatus::Queued)
        })
        .await;
    let due = waiting.next_attempt_at.expect("a retry is scheduled");
    assert_eq!(due - waiting.updated_at, 800, "first wait is the base delay");
    let now = now_ms();
    assert!(
        due > now,
        "the retry must not be due the instant it is written"
    );
    assert!(
        due <= now + 1_000,
        "the first wait is the base delay, not something longer"
    );
    assert!(
        waiting
            .error_message
            .as_deref()
            .is_some_and(|message| message.contains("working on")),
        "the engine's message is kept: {:?}",
        waiting.error_message
    );

    // Nobody asked for this: the second attempt happens on its own, and
    // its wait doubles rather than repeating the first delay.
    let second = harness
        .wait_for(view.id, "second retry", |row| {
            row.attempts == 2 && row.status_enum() == Some(TaskStatus::Queued)
        })
        .await;
    assert_eq!(
        second.next_attempt_at.expect("second retry is scheduled") - second.updated_at,
        1_600,
        "the second backoff is twice the first"
    );

    // The budget runs out at three attempts.
    let failed = harness
        .wait_for(view.id, "final error", |row| {
            row.status_enum() == Some(TaskStatus::Error)
        })
        .await;
    assert_eq!(failed.attempts, 3, "the attempt budget bounds the retries");
    assert!(failed.next_attempt_at.is_none());

    // Only the attempt that ended the task is an outcome; the two that were
    // retried are not.
    let entries = history::recent(harness.store.pool(), 10)
        .await
        .expect("history");
    let for_this_task: Vec<_> = entries
        .iter()
        .filter(|entry| entry.task_id == view.id)
        .collect();
    assert_eq!(for_this_task.len(), 1, "one outcome for three attempts");
    assert_eq!(for_this_task[0].outcome, "failed");

    // Every attempt was reported to the shell, not only the last one.
    assert!(
        harness.sink.touched_ids().contains(&view.id),
        "the shell was told the task changed"
    );

    // Letting go of the workspace lets a hand-started retry finish, and the
    // new run gets a whole budget again.
    drop(held);
    harness.manager.retry(view.id).await.expect("retry");
    let done = harness
        .wait_for(view.id, "completion after retry", |row| {
            row.status_enum() == Some(TaskStatus::Completed)
        })
        .await;
    assert_eq!(
        done.observed_sha256.as_deref(),
        Some(expected_sha256(total).as_str())
    );

    harness.finish().await;
}

#[tokio::test]
async fn a_queued_row_does_not_start_before_it_is_due() {
    let total = 64 * 1024u64;
    let server = TestServer::start(options(total));
    let pending = Pending::new().await;

    let id = tasks::insert(
        pending.store.pool(),
        &NewTask {
            url: server.url("/payload.bin"),
            dest_path: pending
                .downloads
                .join("payload.bin")
                .to_string_lossy()
                .to_string(),
            file_name: "payload.bin".to_string(),
            category: TaskCategory::Apps,
            status: TaskStatus::Queued,
            priority: 0,
            connections: Some(2),
            speed_limit_bps: None,
            expected_sha256: None,
            schedule_id: None,
            created_at: now_ms(),
        },
    )
    .await
    .expect("insert");
    tasks::set_failure(
        pending.store.pool(),
        id,
        TaskStatus::Queued,
        "waiting for a retry",
        1,
        Some(now_ms() + 1_200),
        now_ms(),
    )
    .await
    .expect("schedule the retry");

    let harness = pending.start();

    // Nothing may touch the network while the backoff runs.
    tokio::time::sleep(Duration::from_millis(400)).await;
    let waiting = harness.row(id).await;
    assert_eq!(waiting.status, "queued", "the row is waiting, not running");
    assert!(
        server.requests().is_empty(),
        "the engine was started before the retry was due"
    );
    assert!(!harness.destination("payload.bin").exists());

    let done = harness
        .wait_for(id, "completion after the backoff", |row| {
            row.status_enum() == Some(TaskStatus::Completed)
        })
        .await;
    assert_eq!(
        done.observed_sha256.as_deref(),
        Some(expected_sha256(total).as_str())
    );

    harness.finish().await;
}

#[tokio::test]
async fn a_real_retry_keeps_its_due_time_and_attempt_count_across_restart() {
    let total = 64 * 1024u64;
    let server = TestServer::start(options(total));
    let harness = Harness::start().await;
    harness.set(settings::RETRY_MAX_ATTEMPTS, "3").await;
    harness.set(settings::RETRY_BASE_MS, "2000").await;

    let workspace = harness.downloads.join("payload.bin.rapids");
    std::fs::create_dir_all(&workspace).expect("workspace");
    let held = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(workspace.join("lock"))
        .expect("lock file");
    held.try_lock().expect("hold the workspace");

    let view = harness
        .manager
        .add(harness.request(&server.url("/payload.bin")))
        .await
        .expect("add");
    let waiting = harness
        .wait_for(view.id, "retry waiting", |row| {
            row.attempts == 1 && row.status_enum() == Some(TaskStatus::Queued)
        })
        .await;
    let due = waiting.next_attempt_at.expect("next attempt is scheduled");
    assert_eq!(due - waiting.updated_at, 2_000);

    let pending = harness.stop_and_reopen().await;
    let restored = tasks::get(pending.store.pool(), view.id)
        .await
        .expect("read task")
        .expect("retry survived the restart");
    assert_eq!(restored.status_enum(), Some(TaskStatus::Queued));
    assert_eq!(restored.attempts, 1);
    assert_eq!(restored.next_attempt_at, Some(due));
    assert_eq!(restored.error_message, waiting.error_message);
    assert!(
        tasks::due(pending.store.pool(), due - 1)
            .await
            .expect("not yet due")
            .is_empty(),
        "a persisted retry cannot run even one millisecond early"
    );
    let due_ids: Vec<_> = tasks::due(pending.store.pool(), due)
        .await
        .expect("due work")
        .iter()
        .map(|row| row.id)
        .collect();
    assert_eq!(due_ids, vec![view.id]);

    drop(held);
    let resumed = pending.start();
    let completed = resumed
        .wait_for(view.id, "completion after restart", |row| {
            row.status_enum() == Some(TaskStatus::Completed)
        })
        .await;
    assert_eq!(completed.attempts, 1);
    assert_eq!(completed.next_attempt_at, None);
    assert_eq!(
        completed.observed_sha256.as_deref(),
        Some(expected_sha256(total).as_str())
    );
    resumed.manager.list().await.expect("manager caught up");
    let entries = history::recent(resumed.store.pool(), 10)
        .await
        .expect("history");
    assert_eq!(entries.len(), 1, "the failed attempt was not an outcome");
    assert_eq!(entries[0].outcome, "completed");

    resumed.finish().await;
}

#[tokio::test]
async fn a_one_off_window_releases_its_task_when_it_opens() {
    let total = 64 * 1024u64;
    let server = TestServer::start(options(total));
    let harness = Harness::start().await;

    let schedule = schedules::insert(
        harness.store.pool(),
        &NewSchedule {
            label: "tonight".to_string(),
            kind: ScheduleKind::Once,
            start_at: now_ms() + 900,
            window_minutes: 0,
            weekdays: 0,
            enabled: true,
            created_at: now_ms(),
        },
    )
    .await
    .expect("insert schedule");

    let mut request = harness.request(&server.url("/payload.bin"));
    request.schedule_id = Some(schedule);
    let view = harness.manager.add(request).await.expect("add");
    assert_eq!(
        view.status,
        "scheduled",
        "a windowed task starts waiting"
    );

    // Before the window opens the task must not have touched the network.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(harness.row(view.id).await.status, "scheduled");
    assert!(
        server.requests().is_empty(),
        "a scheduled task started before its window"
    );

    let done = harness
        .wait_for(view.id, "completion inside the window", |row| {
            row.status_enum() == Some(TaskStatus::Completed)
        })
        .await;
    assert_eq!(
        done.observed_sha256.as_deref(),
        Some(expected_sha256(total).as_str())
    );

    harness.finish().await;
}

#[tokio::test]
async fn a_closed_window_takes_its_queued_work_back_out() {
    let total = 64 * 1024u64;
    let server = TestServer::start(options(total));
    let harness = Harness::start().await;

    // A window that is open now and never closes on its own.
    let schedule = schedules::insert(
        harness.store.pool(),
        &NewSchedule {
            label: "always".to_string(),
            kind: ScheduleKind::Once,
            start_at: now_ms() - 60_000,
            window_minutes: 0,
            weekdays: 0,
            enabled: true,
            created_at: now_ms(),
        },
    )
    .await
    .expect("insert schedule");

    // One slot, held by a download that takes a few seconds, so the windowed
    // task is visibly waiting in the queue rather than running.
    harness.set(settings::MAX_PARALLEL, "1").await;
    let mut blocker_options = options(512 * 1024);
    blocker_options.throttle_bps = Some(24 * 1024);
    let blocker = TestServer::start(blocker_options);
    harness
        .manager
        .add(harness.request(&blocker.url("/blocker.bin")))
        .await
        .expect("add blocker");

    let mut request = harness.request(&server.url("/payload.bin"));
    request.schedule_id = Some(schedule);
    let view = harness.manager.add(request).await.expect("add");
    harness
        .wait_for(view.id, "released into the queue", |row| {
            row.status_enum() == Some(TaskStatus::Queued)
        })
        .await;

    // Switching the schedule off takes its waiting work back out of the queue.
    schedules::set_enabled(harness.store.pool(), schedule, false)
        .await
        .expect("disable schedule");
    harness
        .wait_for(view.id, "parked behind a closed window", |row| {
            row.status_enum() == Some(TaskStatus::Scheduled)
        })
        .await;

    // And switching it back on lets the work run.
    schedules::set_enabled(harness.store.pool(), schedule, true)
        .await
        .expect("enable schedule");
    harness
        .wait_for(view.id, "completion after reopening", |row| {
            row.status_enum() == Some(TaskStatus::Completed)
        })
        .await;
    assert!(harness.destination("payload.bin").exists());

    harness.finish().await;
}

#[test]
fn a_weekly_start_window_crosses_midnight_without_opening_on_tuesday() {
    // Monday 1970-01-05 at 23:30 UTC, with Monday as the only selected day.
    let monday = 4 * DAY_MS;
    let start = monday + 23 * 60 * MINUTE_MS + 30 * MINUTE_MS;
    let schedule = ScheduleRow {
        id: 1,
        label: "late Monday".to_string(),
        kind: ScheduleKind::Weekly.as_str().to_string(),
        start_at: start,
        window_minutes: 120,
        weekdays: 1,
        enabled: 1,
        created_at: start,
    };
    let at = |elapsed: i64| window_state(&schedule, start + elapsed, Clock::utc());

    assert_eq!(at(-1), Window::Later);
    assert_eq!(at(0), Window::Open);
    assert_eq!(at(60 * MINUTE_MS), Window::Open);
    assert_eq!(at(120 * MINUTE_MS - 1), Window::Open);
    assert_eq!(at(120 * MINUTE_MS), Window::Later);
    assert_eq!(at(DAY_MS), Window::Later, "Tuesday is not selected");
    assert_eq!(at(7 * DAY_MS), Window::Open, "the next Monday opens");
}
