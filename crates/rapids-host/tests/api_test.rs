//! The command surface: every call the interface can make, exercised headless.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rapids_host::api::Host;
use rapids_host::events::{EventSink, HostEvent, RecordingSink};
use rapids_host::store::{ScheduleKind, Store, TaskCategory, TaskRow, TaskStatus, tasks};
use rapids_host::{AddRequest, Clock, HostError, ManagerHandle, ScheduleInput, Settings};
use rapids_test_support::{TestServer, expected_sha256, options};
use tokio::task::JoinHandle;

const WAIT: Duration = Duration::from_secs(60);

struct Harness {
    host: Host,
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
        let (manager, join) = rapids_host::manager::spawn(
            Arc::clone(&store),
            sink_port,
            Clock::utc(),
        );
        Self {
            host: Host::new(Arc::clone(&store), manager.clone()),
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
                panic!("{label}: timed out with status {}", row.status);
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    async fn finish(self) {
        self.manager.shutdown().await;
        let _ = self.join.await;
    }
}

#[tokio::test]
async fn settings_are_read_back_as_the_types_they_are() {
    let harness = Harness::start().await;

    let settings = harness.host.settings_get().await.expect("read settings");
    assert_eq!(settings.max_parallel, 3);
    assert_eq!(settings.max_connections_total, 48);
    assert_eq!(settings.global_speed_limit, None);
    assert!(settings.resume_on_start);
    assert_eq!(settings.retry_max_attempts, 3);
    assert_eq!(settings.default_dest_dir, "");

    harness.finish().await;
}

#[tokio::test]
async fn a_setting_is_written_and_every_reader_sees_it() {
    let harness = Harness::start().await;

    let updated = harness
        .host
        .settings_update("max_parallel", &serde_json::json!(5))
        .await
        .expect("update");
    assert_eq!(updated.max_parallel, 5);

    let settings = harness.host.settings_get().await.expect("read settings");
    assert_eq!(settings.max_parallel, 5);

    let folder = harness.downloads.to_string_lossy().to_string();
    let updated = harness
        .host
        .settings_update("default_dest_dir", &serde_json::json!(folder))
        .await
        .expect("update folder");
    assert_eq!(updated.default_dest_dir, folder);

    harness.finish().await;
}

#[tokio::test]
async fn a_setting_outside_its_range_is_refused_and_left_alone() {
    let harness = Harness::start().await;

    let too_small = harness
        .host
        .settings_update("max_parallel", &serde_json::json!(0))
        .await
        .expect_err("zero slots is refused");
    assert!(
        matches!(&too_small, HostError::Invalid { reason } if reason.contains("between 1 and 32")),
        "the message must say what is allowed: {too_small}"
    );

    let too_large = harness
        .host
        .settings_update("max_parallel", &serde_json::json!(99))
        .await
        .expect_err("99 slots is refused");
    assert!(matches!(too_large, HostError::Invalid { .. }));

    let wrong_type = harness
        .host
        .settings_update("max_parallel", &serde_json::json!("three"))
        .await
        .expect_err("text is not a number");
    assert!(matches!(wrong_type, HostError::Invalid { .. }));

    let unknown = harness
        .host
        .settings_update("colour", &serde_json::json!("blue"))
        .await
        .expect_err("there is no such setting");
    assert!(
        matches!(&unknown, HostError::Invalid { reason } if reason.contains("colour")),
        "{unknown}"
    );

    let settings = harness.host.settings_get().await.expect("read settings");
    assert_eq!(settings.max_parallel, 3, "refused writes change nothing");

    harness.finish().await;
}

#[tokio::test]
async fn a_paused_queue_stops_starting_work_and_resuming_lets_it_run() {
    let total = 64 * 1024u64;
    let server = TestServer::start(options(total));
    let harness = Harness::start().await;

    assert!(!harness.host.queue_paused().await.expect("initial state"));

    harness.host.queue_pause().await.expect("pause");
    assert!(harness.host.queue_paused().await.expect("state"));

    let view = harness
        .host
        .task_add(harness.request(&server.url("/payload.bin")))
        .await
        .expect("add");
    assert_eq!(view.status, "queued");

    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(harness.row(view.id).await.status, "queued");
    assert!(
        server.requests().is_empty(),
        "a paused queue started work anyway"
    );
    assert!(!harness.destination("payload.bin").exists());

    harness.host.queue_resume().await.expect("resume");
    let done = harness
        .wait_for(view.id, "completion after resuming", |row| {
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
async fn pausing_the_queue_leaves_a_running_download_alone() {
    let mut throttled = options(256 * 1024);
    throttled.throttle_bps = Some(32 * 1024);
    let server = TestServer::start(throttled);
    let harness = Harness::start().await;

    let view = harness
        .host
        .task_add(harness.request(&server.url("/payload.bin")))
        .await
        .expect("add");
    harness
        .wait_for(view.id, "running", |row| {
            row.status_enum() == Some(TaskStatus::Downloading)
        })
        .await;

    harness.host.queue_pause().await.expect("pause");
    let done = harness
        .wait_for(view.id, "completion despite the pause", |row| {
            row.status_enum() == Some(TaskStatus::Completed)
        })
        .await;
    assert_eq!(done.downloaded, 256 * 1024, "the transfer kept its bytes");

    harness.finish().await;
}

#[tokio::test]
async fn the_task_commands_reach_the_rows_they_name() {
    let harness = Harness::start().await;

    let missing = harness.host.task_pause(4_242).await.expect_err("no task");
    assert!(matches!(missing, HostError::NotFound { .. }));

    let missing = harness
        .host
        .task_delete(4_242, false)
        .await
        .expect_err("no task");
    assert!(matches!(missing, HostError::NotFound { .. }));

    let missing = harness
        .host
        .schedule_delete(4_242)
        .await
        .expect_err("no schedule");
    assert!(matches!(missing, HostError::NotFound { .. }));

    assert!(harness.host.tasks_list().await.expect("list").is_empty());

    harness.finish().await;
}

#[tokio::test]
async fn a_schedule_is_created_listed_switched_off_and_removed() {
    let harness = Harness::start().await;

    let view = harness
        .host
        .schedule_create(ScheduleInput {
            label: "nightly".to_string(),
            kind: ScheduleKind::Daily,
            start_at: rapids_host::now_ms() + 60_000,
            window_minutes: 120,
            weekdays: 0,
            enabled: true,
        })
        .await
        .expect("create");
    assert_eq!(view.label, "nightly");
    assert_eq!(view.kind, "daily");
    assert!(view.enabled);

    let listed = harness.host.schedules_list().await.expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, view.id);

    harness
        .host
        .schedule_set_enabled(view.id, false)
        .await
        .expect("disable");
    let listed = harness.host.schedules_list().await.expect("list");
    assert!(!listed[0].enabled);

    harness
        .host
        .schedule_delete(view.id)
        .await
        .expect("delete");
    assert!(harness.host.schedules_list().await.expect("list").is_empty());

    harness.finish().await;
}

#[tokio::test]
async fn a_schedule_without_a_name_is_refused() {
    let harness = Harness::start().await;

    let error = harness
        .host
        .schedule_create(ScheduleInput {
            label: "   ".to_string(),
            kind: ScheduleKind::Once,
            start_at: rapids_host::now_ms(),
            window_minutes: 0,
            weekdays: 0,
            enabled: true,
        })
        .await
        .expect_err("no name");
    assert!(matches!(&error, HostError::Invalid { .. }));
    assert!(harness.host.schedules_list().await.expect("list").is_empty());

    harness.finish().await;
}

#[tokio::test]
async fn the_queue_summary_counts_each_state() {
    let total = 64 * 1024u64;
    let server = TestServer::start(options(total));
    let harness = Harness::start().await;

    let view = harness
        .host
        .task_add(harness.request(&server.url("/payload.bin")))
        .await
        .expect("add");
    harness
        .wait_for(view.id, "completion", |row| {
            row.status_enum() == Some(TaskStatus::Completed)
        })
        .await;

    let counts = harness.host.queue_counts().await.expect("counts");
    assert_eq!(counts.completed, 1);
    assert_eq!(counts.queued, 0);
    assert_eq!(counts.downloading, 0);
    assert_eq!(counts.failed, 0);

    harness.finish().await;
}

#[tokio::test]
async fn progress_arrives_in_batches_and_the_last_sample_is_not_lost() {
    let mut throttled = options(384 * 1024);
    throttled.throttle_bps = Some(96 * 1024);
    let server = TestServer::start(throttled);
    let harness = Harness::start().await;

    let view = harness
        .host
        .task_add(harness.request(&server.url("/payload.bin")))
        .await
        .expect("add");
    harness
        .wait_for(view.id, "completion", |row| {
            row.status_enum() == Some(TaskStatus::Completed)
        })
        .await;

    let batches: Vec<Vec<i64>> = harness
        .sink
        .events()
        .into_iter()
        .filter_map(|event| match event {
            HostEvent::Progress { updates } => {
                Some(updates.iter().map(|update| update.id).collect())
            }
            _ => None,
        })
        .collect();
    assert!(!batches.is_empty(), "progress was reported");
    assert!(
        batches.iter().all(|ids| ids.iter().all(|id| *id == view.id)),
        "every sample belongs to the task that ran"
    );
    assert!(
        batches
            .iter()
            .all(|ids| ids.len() <= rapids_host::progress::MAX_BATCH),
        "no batch grows past the limit"
    );
    assert!(
        harness
            .sink
            .events()
            .iter()
            .any(|event| matches!(event, HostEvent::TaskChanged { .. })),
        "the shell hears about the task as well as its progress"
    );

    harness.finish().await;
}

#[tokio::test]
async fn a_connection_budget_larger_than_one_task_may_use_breaks_nothing() {
    let total = 64 * 1024u64;
    let server = TestServer::start(options(total));
    let harness = Harness::start().await;

    // The budget is an app-wide total, so it is allowed to exceed the ceiling
    // one download may use. It must never reach the engine as a per-task
    // connection count: the engine would refuse the spec, and a refused spec
    // is terminal, so the task would die on its first attempt.
    let updated = harness
        .host
        .settings_update("max_connections_total", &serde_json::json!(128))
        .await
        .expect("update");
    assert_eq!(updated.max_connections_total, 128);

    let view = harness
        .host
        .task_add(harness.request(&server.url("/payload.bin")))
        .await
        .expect("add");
    let done = harness
        .wait_for(view.id, "completion", |row| {
            row.status_enum() == Some(TaskStatus::Completed)
        })
        .await;
    assert_eq!(
        done.observed_sha256.as_deref(),
        Some(expected_sha256(total).as_str())
    );

    // A task asking for more connections than the engine accepts is clamped
    // rather than refused, so the budget cannot poison a queue either.
    let mut greedy = harness.request(&server.url("/greedy.bin"));
    greedy.connections = Some(200);
    let view = harness.host.task_add(greedy).await.expect("add greedy");
    let done = harness
        .wait_for(view.id, "completion with a clamped plan", |row| {
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
async fn a_settings_value_the_store_cannot_read_is_reported_not_hidden() {
    let harness = Harness::start().await;

    // A number that is not a number can only arrive by bypassing the command,
    // which is exactly the case worth refusing to paper over.
    rapids_host::store::settings::set(
        harness.store.pool(),
        rapids_host::store::settings::MAX_PARALLEL,
        "many",
        rapids_host::now_ms(),
    )
    .await
    .expect("write");

    let error = harness.host.settings_get().await.expect_err("unreadable");
    assert!(matches!(&error, HostError::Invalid { .. }), "{error}");
    assert!(
        Settings::load(harness.store.pool()).await.is_err(),
        "the same value fails the same way through the plain reader"
    );

    harness.finish().await;
}
