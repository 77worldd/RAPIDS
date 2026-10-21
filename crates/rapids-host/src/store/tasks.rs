//! Task rows and the queries the queue runs against them.

use std::fmt;

use sqlx::SqlitePool;

use crate::error::HostError;

/// Where a task sits. Mirrors the `status` CHECK constraint in the schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Queued,
    Scheduled,
    Downloading,
    Paused,
    Completed,
    Error,
}

impl TaskStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Scheduled => "scheduled",
            Self::Downloading => "downloading",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Error => "error",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "queued" => Some(Self::Queued),
            "scheduled" => Some(Self::Scheduled),
            "downloading" => Some(Self::Downloading),
            "paused" => Some(Self::Paused),
            "completed" => Some(Self::Completed),
            "error" => Some(Self::Error),
            _ => None,
        }
    }

    /// Waiting work: the two states the queue projection covers.
    pub fn is_waiting(self) -> bool {
        matches!(self, Self::Queued | Self::Scheduled)
    }

    /// No further transition happens without a person asking.
    pub fn is_finished(self) -> bool {
        matches!(self, Self::Completed | Self::Error)
    }
}

impl fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A destination category. Mirrors the `category` CHECK constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskCategory {
    Apps,
    Movies,
    Music,
    Images,
    Other,
}

impl TaskCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Apps => "apps",
            Self::Movies => "movies",
            Self::Music => "music",
            Self::Images => "images",
            Self::Other => "other",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "apps" => Some(Self::Apps),
            "movies" => Some(Self::Movies),
            "music" => Some(Self::Music),
            "images" => Some(Self::Images),
            "other" => Some(Self::Other),
            _ => None,
        }
    }
}

impl fmt::Display for TaskCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A task as it is stored. Numeric columns are `i64` because that is what
/// SQLite holds; conversions to the sizes the engine speaks happen at the
/// boundary where they are used.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TaskRow {
    pub id: i64,
    pub url: String,
    pub final_url: Option<String>,
    pub dest_path: String,
    pub file_name: String,
    pub category: String,
    pub status: String,
    pub priority: i64,
    pub sequence: i64,
    pub total_bytes: Option<i64>,
    pub downloaded: i64,
    pub speed_bps: i64,
    pub eta_seconds: Option<i64>,
    pub connections: Option<i64>,
    pub speed_limit_bps: Option<i64>,
    pub expected_sha256: Option<String>,
    pub observed_sha256: Option<String>,
    pub error_message: Option<String>,
    pub attempts: i64,
    pub next_attempt_at: Option<i64>,
    pub schedule_id: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
}

impl TaskRow {
    pub fn status_enum(&self) -> Option<TaskStatus> {
        TaskStatus::parse(&self.status)
    }

    pub fn category_enum(&self) -> Option<TaskCategory> {
        TaskCategory::parse(&self.category)
    }

    /// Fraction of the file confirmed so far, or `None` when the size is not
    /// known yet.
    pub fn progress_fraction(&self) -> Option<f64> {
        let total = self.total_bytes?;
        if total <= 0 {
            return None;
        }
        let downloaded = self.downloaded.clamp(0, total);
        Some(downloaded as f64 / total as f64)
    }
}

/// What a caller supplies to enqueue work. The queue position, the timestamps
/// it does not care about, and the counters are filled in by the store.
#[derive(Debug, Clone)]
pub struct NewTask {
    pub url: String,
    pub dest_path: String,
    pub file_name: String,
    pub category: TaskCategory,
    pub status: TaskStatus,
    pub priority: i64,
    pub connections: Option<i64>,
    pub speed_limit_bps: Option<i64>,
    pub expected_sha256: Option<String>,
    pub schedule_id: Option<i64>,
    pub created_at: i64,
}

/// A progress sample. Only display state: the sidecar manifest decides what
/// is actually resumable.
#[derive(Debug, Clone, Copy)]
pub struct Progress {
    pub downloaded: i64,
    pub speed_bps: i64,
    pub eta_seconds: Option<i64>,
    pub total_bytes: Option<i64>,
}

/// Enqueues a task and returns its id.
///
/// The queue position comes from a sub-select inside the same statement, so
/// two inserts can never share one — even if a caller ever forgets that all
/// writes belong to a single owner.
pub async fn insert(pool: &SqlitePool, task: &NewTask) -> Result<i64, HostError> {
    let result = sqlx::query(
        "INSERT INTO tasks (url, dest_path, file_name, category, status, priority, sequence, \
         connections, speed_limit_bps, expected_sha256, schedule_id, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, (SELECT COALESCE(MAX(sequence), 0) + 1 FROM tasks), \
         ?, ?, ?, ?, ?, ?)",
    )
    .bind(task.url.as_str())
    .bind(task.dest_path.as_str())
    .bind(task.file_name.as_str())
    .bind(task.category.as_str())
    .bind(task.status.as_str())
    .bind(task.priority)
    .bind(task.connections)
    .bind(task.speed_limit_bps)
    .bind(task.expected_sha256.as_deref())
    .bind(task.schedule_id)
    .bind(task.created_at)
    .bind(task.created_at)
    .execute(pool)
    .await
    .map_err(|error| HostError::database("insert task", error))?;
    Ok(result.last_insert_rowid())
}

pub async fn get(pool: &SqlitePool, id: i64) -> Result<Option<TaskRow>, HostError> {
    sqlx::query_as::<_, TaskRow>("SELECT * FROM tasks WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|error| HostError::database("read task", error))
}

/// Every task, newest first. This is what the list shows.
pub async fn list(pool: &SqlitePool) -> Result<Vec<TaskRow>, HostError> {
    sqlx::query_as::<_, TaskRow>("SELECT * FROM tasks ORDER BY id DESC")
        .fetch_all(pool)
        .await
        .map_err(|error| HostError::database("list tasks", error))
}

/// The queue projection in consumption order: priority first, arrival second.
pub async fn waiting(pool: &SqlitePool) -> Result<Vec<TaskRow>, HostError> {
    sqlx::query_as::<_, TaskRow>(
        "SELECT * FROM tasks WHERE status IN ('queued', 'scheduled') \
         ORDER BY priority DESC, sequence ASC",
    )
    .fetch_all(pool)
    .await
    .map_err(|error| HostError::database("read queue", error))
}

pub async fn by_status(pool: &SqlitePool, status: TaskStatus) -> Result<Vec<TaskRow>, HostError> {
    sqlx::query_as::<_, TaskRow>("SELECT * FROM tasks WHERE status = ? ORDER BY id DESC")
        .bind(status.as_str())
        .fetch_all(pool)
        .await
        .map_err(|error| HostError::database("read tasks by status", error))
}

pub async fn count(pool: &SqlitePool) -> Result<i64, HostError> {
    sqlx::query_scalar("SELECT COUNT(*) FROM tasks")
        .fetch_one(pool)
        .await
        .map_err(|error| HostError::database("count tasks", error))
}

pub async fn set_status(
    pool: &SqlitePool,
    id: i64,
    status: TaskStatus,
    now: i64,
) -> Result<(), HostError> {
    let result = sqlx::query("UPDATE tasks SET status = ?, updated_at = ? WHERE id = ?")
        .bind(status.as_str())
        .bind(now)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|error| HostError::database("update task status", error))?;
    if result.rows_affected() == 0 {
        return Err(HostError::NotFound {
            entity: "task",
            id,
        });
    }
    Ok(())
}

/// Marks a queued task as running and starts its clock, without overwriting a
/// start time left over from an earlier attempt.
pub async fn mark_started(pool: &SqlitePool, id: i64, now: i64) -> Result<(), HostError> {
    let result = sqlx::query(
        "UPDATE tasks SET status = 'downloading', started_at = COALESCE(started_at, ?), \
         error_message = NULL, speed_bps = 0, next_attempt_at = NULL, updated_at = ? \
         WHERE id = ?",
    )
    .bind(now)
    .bind(now)
    .bind(id)
    .execute(pool)
    .await
    .map_err(|error| HostError::database("start task", error))?;
    if result.rows_affected() == 0 {
        return Err(HostError::NotFound {
            entity: "task",
            id,
        });
    }
    Ok(())
}

/// Records a finished download. The observed hash and size come from the
/// engine, which is the only authority on the bytes.
pub async fn mark_completed(
    pool: &SqlitePool,
    id: i64,
    total_bytes: Option<i64>,
    observed_sha256: Option<&str>,
    now: i64,
) -> Result<(), HostError> {
    let result = sqlx::query(
        "UPDATE tasks SET status = 'completed', completed_at = ?, \
         total_bytes = COALESCE(?, total_bytes), \
         downloaded = COALESCE(?, downloaded), \
         observed_sha256 = COALESCE(?, observed_sha256), \
         speed_bps = 0, error_message = NULL, next_attempt_at = NULL, updated_at = ? \
         WHERE id = ?",
    )
    .bind(now)
    .bind(total_bytes)
    .bind(total_bytes)
    .bind(observed_sha256)
    .bind(now)
    .bind(id)
    .execute(pool)
    .await
    .map_err(|error| HostError::database("complete task", error))?;
    if result.rows_affected() == 0 {
        return Err(HostError::NotFound {
            entity: "task",
            id,
        });
    }
    Ok(())
}

/// Stores a progress sample. A missing row is not an error: a sample can
/// arrive just after the task it describes was deleted.
pub async fn set_progress(
    pool: &SqlitePool,
    id: i64,
    progress: Progress,
    now: i64,
) -> Result<(), HostError> {
    sqlx::query(
        "UPDATE tasks SET downloaded = ?, speed_bps = ?, eta_seconds = ?, \
         total_bytes = COALESCE(?, total_bytes), updated_at = ? WHERE id = ?",
    )
    .bind(progress.downloaded)
    .bind(progress.speed_bps)
    .bind(progress.eta_seconds)
    .bind(progress.total_bytes)
    .bind(now)
    .bind(id)
    .execute(pool)
    .await
    .map_err(|error| HostError::database("store progress", error))?;
    Ok(())
}

/// Records a failure, either as a terminal error or as a scheduled retry.
pub async fn set_failure(
    pool: &SqlitePool,
    id: i64,
    status: TaskStatus,
    message: &str,
    attempts: i64,
    next_attempt_at: Option<i64>,
    now: i64,
) -> Result<(), HostError> {
    let result = sqlx::query(
        "UPDATE tasks SET status = ?, error_message = ?, attempts = ?, \
         next_attempt_at = ?, speed_bps = 0, updated_at = ? WHERE id = ?",
    )
    .bind(status.as_str())
    .bind(message)
    .bind(attempts)
    .bind(next_attempt_at)
    .bind(now)
    .bind(id)
    .execute(pool)
    .await
    .map_err(|error| HostError::database("record failure", error))?;
    if result.rows_affected() == 0 {
        return Err(HostError::NotFound {
            entity: "task",
            id,
        });
    }
    Ok(())
}

/// Reorders one task. Higher priorities run first; the arrival order still
/// breaks ties, so raising a task only moves it ahead of lower priorities.
pub async fn set_priority(
    pool: &SqlitePool,
    id: i64,
    priority: i64,
    now: i64,
) -> Result<(), HostError> {
    let result = sqlx::query("UPDATE tasks SET priority = ?, updated_at = ? WHERE id = ?")
        .bind(priority)
        .bind(now)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|error| HostError::database("update priority", error))?;
    if result.rows_affected() == 0 {
        return Err(HostError::NotFound {
            entity: "task",
            id,
        });
    }
    Ok(())
}

/// Removes a task row. History is written by the caller first, because it is
/// meant to outlive the row.
pub async fn delete(pool: &SqlitePool, id: i64) -> Result<bool, HostError> {
    let result = sqlx::query("DELETE FROM tasks WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .map_err(|error| HostError::database("delete task", error))?;
    Ok(result.rows_affected() > 0)
}

/// Returns every task left running by a previous process to the queue and
/// reports how many rows that was.
///
/// This changes display state only. How much of each download survived is
/// decided by the sidecar manifest when the engine opens it, never here.
pub async fn requeue_interrupted(pool: &SqlitePool, now: i64) -> Result<u64, HostError> {
    let result = sqlx::query(
        "UPDATE tasks SET status = 'queued', speed_bps = 0, updated_at = ? \
         WHERE status = 'downloading'",
    )
    .bind(now)
    .execute(pool)
    .await
    .map_err(|error| HostError::database("requeue interrupted tasks", error))?;
    Ok(result.rows_affected())
}
