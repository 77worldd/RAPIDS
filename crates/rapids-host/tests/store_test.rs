use rapids_host::now_ms;
use rapids_host::store::{
    NewHistory, NewSchedule, NewTask, Outcome, Progress, ScheduleKind, Store, TaskCategory,
    TaskStatus, history, schedules, settings, tasks,
};

fn task(url: &str, priority: i64) -> NewTask {
    NewTask {
        url: url.to_string(),
        dest_path: "/downloads/file.bin".to_string(),
        file_name: "file.bin".to_string(),
        category: TaskCategory::Apps,
        status: TaskStatus::Queued,
        priority,
        connections: Some(8),
        speed_limit_bps: None,
        expected_sha256: None,
        schedule_id: None,
        created_at: now_ms(),
    }
}

#[tokio::test]
async fn a_fresh_database_is_migrated_and_seeded_once() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("rapids.db");

    let store = Store::open(&path).await.expect("open");
    let pool = store.pool();

    assert_eq!(
        settings::get_i64(pool, settings::MAX_PARALLEL, 0)
            .await
            .expect("max parallel"),
        3
    );
    assert_eq!(
        settings::get_i64(pool, settings::MAX_CONNECTIONS_TOTAL, 0)
            .await
            .expect("connection budget"),
        48
    );
    assert!(
        settings::get_bool(pool, settings::RESUME_ON_START, false)
            .await
            .expect("resume on start")
    );
    assert_eq!(
        settings::get_optional_i64(pool, settings::GLOBAL_SPEED_LIMIT)
            .await
            .expect("global limit"),
        None
    );
    assert_eq!(
        settings::get_string(pool, settings::DEFAULT_DEST_DIR, "/fallback")
            .await
            .expect("destination"),
        ""
    );
    assert_eq!(tasks::count(pool).await.expect("task count"), 0);
    store.close().await;

    // Reopening runs the migrator again; it must be a no-op that keeps data.
    let store = Store::open(&path).await.expect("reopen");
    assert_eq!(tasks::count(store.pool()).await.expect("task count"), 0);
    store.close().await;
}

#[tokio::test]
async fn the_queue_orders_by_priority_then_arrival() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = Store::open(&dir.path().join("rapids.db"))
        .await
        .expect("open");
    let pool = store.pool();

    let low = tasks::insert(pool, &task("https://example.com/low.bin", 0))
        .await
        .expect("first task");
    let high = tasks::insert(pool, &task("https://example.com/high.bin", 10))
        .await
        .expect("second task");
    let same = tasks::insert(pool, &task("https://example.com/same.bin", 10))
        .await
        .expect("third task");

    let waiting: Vec<i64> = tasks::waiting(pool)
        .await
        .expect("queue")
        .iter()
        .map(|row| row.id)
        .collect();
    assert_eq!(
        waiting,
        vec![high, same, low],
        "higher priority first, arrival order breaks the tie"
    );

    // The list view is newest first and ignores priority.
    let listed: Vec<i64> = tasks::list(pool)
        .await
        .expect("list")
        .iter()
        .map(|row| row.id)
        .collect();
    assert_eq!(listed, vec![same, high, low]);

    // Raising the oldest task moves it ahead of everything at its old level.
    tasks::set_priority(pool, low, 20, now_ms())
        .await
        .expect("reprioritise");
    let reordered: Vec<i64> = tasks::waiting(pool)
        .await
        .expect("queue")
        .iter()
        .map(|row| row.id)
        .collect();
    assert_eq!(reordered, vec![low, high, same]);

    store.close().await;
}

#[tokio::test]
async fn an_interrupted_download_returns_to_the_queue() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = Store::open(&dir.path().join("rapids.db"))
        .await
        .expect("open");
    let pool = store.pool();
    let now = now_ms();

    let id = tasks::insert(pool, &task("https://example.com/file.bin", 0))
        .await
        .expect("insert");
    tasks::mark_started(pool, id, now).await.expect("start");

    let row = tasks::get(pool, id).await.expect("get").expect("present");
    assert_eq!(row.status_enum(), Some(TaskStatus::Downloading));
    assert!(row.started_at.is_some());

    tasks::set_progress(
        pool,
        id,
        Progress {
            downloaded: 512,
            speed_bps: 1024,
            eta_seconds: Some(9),
            total_bytes: Some(4096),
        },
        now,
    )
    .await
    .expect("progress");

    let row = tasks::get(pool, id).await.expect("get").expect("present");
    assert_eq!(row.downloaded, 512);
    assert_eq!(row.progress_fraction(), Some(0.125));

    let requeued = tasks::requeue_interrupted(pool, now)
        .await
        .expect("requeue");
    assert_eq!(requeued, 1);

    let row = tasks::get(pool, id).await.expect("get").expect("present");
    assert_eq!(row.status_enum(), Some(TaskStatus::Queued));
    assert_eq!(row.speed_bps, 0);
    assert!(
        row.started_at.is_some(),
        "the first start time survives a requeue"
    );

    store.close().await;
}

#[tokio::test]
async fn a_completed_task_records_what_the_engine_reported() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = Store::open(&dir.path().join("rapids.db"))
        .await
        .expect("open");
    let pool = store.pool();
    let now = now_ms();
    let hash = "a".repeat(64);

    let id = tasks::insert(pool, &task("https://example.com/file.bin", 0))
        .await
        .expect("insert");
    tasks::mark_started(pool, id, now).await.expect("start");
    tasks::mark_completed(pool, id, Some(4096), Some(hash.as_str()), now)
        .await
        .expect("complete");

    let row = tasks::get(pool, id).await.expect("get").expect("present");
    assert_eq!(row.status_enum(), Some(TaskStatus::Completed));
    assert!(row.completed_at.is_some());
    assert_eq!(row.observed_sha256.as_deref(), Some(hash.as_str()));
    assert_eq!(row.total_bytes, Some(4096));
    assert_eq!(row.downloaded, 4096);
    assert_eq!(row.error_message, None);

    store.close().await;
}

#[tokio::test]
async fn history_outlives_the_task_row() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = Store::open(&dir.path().join("rapids.db"))
        .await
        .expect("open");
    let pool = store.pool();
    let now = now_ms();

    let id = tasks::insert(pool, &task("https://example.com/file.bin", 0))
        .await
        .expect("insert");
    history::insert(
        pool,
        &NewHistory {
            task_id: id,
            url: "https://example.com/file.bin".to_string(),
            file_name: "file.bin".to_string(),
            dest_path: "/downloads/file.bin".to_string(),
            outcome: Outcome::Completed,
            total_bytes: Some(4096),
            observed_sha256: Some("b".repeat(64)),
            error_message: None,
            started_at: Some(now - 1_000),
            ended_at: now,
        },
    )
    .await
    .expect("record");

    assert!(tasks::delete(pool, id).await.expect("delete"));
    assert_eq!(tasks::count(pool).await.expect("count"), 0);

    let rows = history::recent(pool, 10).await.expect("history");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].task_id, id);
    assert_eq!(rows[0].outcome_enum(), Some(Outcome::Completed));

    assert!(
        history::recent(pool, 0).await.is_err(),
        "a non-positive limit is rejected, not silently emptied"
    );

    store.close().await;
}

#[tokio::test]
async fn schedules_round_trip_and_reject_impossible_windows() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = Store::open(&dir.path().join("rapids.db"))
        .await
        .expect("open");
    let pool = store.pool();
    let now = now_ms();

    let id = schedules::insert(
        pool,
        &NewSchedule {
            label: "Night window".to_string(),
            kind: ScheduleKind::Daily,
            start_at: 1_000,
            window_minutes: 120,
            weekdays: 0,
            enabled: true,
            created_at: now,
        },
    )
    .await
    .expect("insert");

    let rows = schedules::list(pool).await.expect("list");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].kind_enum(), Some(ScheduleKind::Daily));
    assert!(rows[0].is_enabled());

    schedules::set_enabled(pool, id, false)
        .await
        .expect("disable");
    assert!(!schedules::list(pool).await.expect("list")[0].is_enabled());

    let bad_mask = schedules::insert(
        pool,
        &NewSchedule {
            label: "Impossible".to_string(),
            kind: ScheduleKind::Weekly,
            start_at: 1_000,
            window_minutes: 0,
            weekdays: 200,
            enabled: true,
            created_at: now,
        },
    )
    .await;
    assert!(bad_mask.is_err(), "a weekday mask wider than seven bits");

    assert!(schedules::delete(pool, id).await.expect("delete"));
    assert!(schedules::list(pool).await.expect("list").is_empty());

    store.close().await;
}

#[tokio::test]
async fn settings_round_trip_with_types() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = Store::open(&dir.path().join("rapids.db"))
        .await
        .expect("open");
    let pool = store.pool();
    let now = now_ms();

    settings::set(pool, settings::GLOBAL_SPEED_LIMIT, "1048576", now)
        .await
        .expect("write");
    assert_eq!(
        settings::get_optional_i64(pool, settings::GLOBAL_SPEED_LIMIT)
            .await
            .expect("read"),
        Some(1_048_576)
    );

    settings::set(pool, settings::RESUME_ON_START, "false", now)
        .await
        .expect("write");
    assert!(
        !settings::get_bool(pool, settings::RESUME_ON_START, true)
            .await
            .expect("read")
    );

    // Every escape a path can contain has to survive the round trip: quotes,
    // backslashes, newlines, and non-ASCII text.
    for folder in [
        "/downloads/movies \"new\"",
        "C:\\Users\\rapids\\Downloads",
        "/downloads/line\nbreak",
        "/downloads/€uro/文件",
        "",
    ] {
        settings::set(pool, settings::DEFAULT_DEST_DIR, &settings::encode_string(folder), now)
            .await
            .expect("write");
        assert_eq!(
            settings::get_string(pool, settings::DEFAULT_DEST_DIR, "/fallback")
                .await
                .expect("read"),
            folder,
            "stored text must come back unchanged"
        );
    }

    // A stored number is not a string; reading it as one reports the type
    // mismatch instead of returning the digits.
    settings::set(pool, settings::DEFAULT_DEST_DIR, "42", now)
        .await
        .expect("write");
    assert!(
        settings::get_string(pool, settings::DEFAULT_DEST_DIR, "")
            .await
            .is_err(),
        "a number is not a string setting"
    );

    let malformed = settings::set(pool, settings::MAX_PARALLEL, "not a number", now).await;
    assert!(malformed.is_ok(), "writing text is allowed");
    assert!(
        settings::get_i64(pool, settings::MAX_PARALLEL, 3).await.is_err(),
        "reading it back as a number reports the problem instead of guessing"
    );

    store.close().await;
}

#[tokio::test]
async fn the_schema_rejects_statuses_outside_the_contract() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = Store::open(&dir.path().join("rapids.db"))
        .await
        .expect("open");

    let result = sqlx::query(
        "INSERT INTO tasks (url, dest_path, file_name, category, status, sequence, \
         created_at, updated_at) VALUES ('https://example.com/x', '/tmp/x', 'x', 'apps', \
         'bogus', 1, 0, 0)",
    )
    .execute(store.pool())
    .await;
    assert!(
        result.is_err(),
        "an unknown status must not reach the table"
    );

    store.close().await;
}
