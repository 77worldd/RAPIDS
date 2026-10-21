//! What happened to a download, kept after the task row is gone.

use std::fmt;

use sqlx::SqlitePool;

use crate::error::HostError;

/// How a download ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Completed,
    Failed,
    Cancelled,
}

impl Outcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct HistoryRow {
    pub id: i64,
    pub task_id: i64,
    pub url: String,
    pub file_name: String,
    pub dest_path: String,
    pub outcome: String,
    pub total_bytes: Option<i64>,
    pub observed_sha256: Option<String>,
    pub error_message: Option<String>,
    pub started_at: Option<i64>,
    pub ended_at: i64,
}

impl HistoryRow {
    pub fn outcome_enum(&self) -> Option<Outcome> {
        Outcome::parse(&self.outcome)
    }
}

/// One recorded outcome. Deliberately carries no foreign key: the record has
/// to survive the task row being deleted.
#[derive(Debug, Clone)]
pub struct NewHistory {
    pub task_id: i64,
    pub url: String,
    pub file_name: String,
    pub dest_path: String,
    pub outcome: Outcome,
    pub total_bytes: Option<i64>,
    pub observed_sha256: Option<String>,
    pub error_message: Option<String>,
    pub started_at: Option<i64>,
    pub ended_at: i64,
}

pub async fn insert(pool: &SqlitePool, entry: &NewHistory) -> Result<i64, HostError> {
    let result = sqlx::query(
        "INSERT INTO history (task_id, url, file_name, dest_path, outcome, total_bytes, \
         observed_sha256, error_message, started_at, ended_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(entry.task_id)
    .bind(entry.url.as_str())
    .bind(entry.file_name.as_str())
    .bind(entry.dest_path.as_str())
    .bind(entry.outcome.as_str())
    .bind(entry.total_bytes)
    .bind(entry.observed_sha256.as_deref())
    .bind(entry.error_message.as_deref())
    .bind(entry.started_at)
    .bind(entry.ended_at)
    .execute(pool)
    .await
    .map_err(|error| HostError::database("record history", error))?;
    Ok(result.last_insert_rowid())
}

/// The most recent outcomes, newest first.
pub async fn recent(pool: &SqlitePool, limit: i64) -> Result<Vec<HistoryRow>, HostError> {
    if limit <= 0 {
        return Err(HostError::invalid("history limit must be positive"));
    }
    sqlx::query_as::<_, HistoryRow>("SELECT * FROM history ORDER BY ended_at DESC, id DESC LIMIT ?")
        .bind(limit)
        .fetch_all(pool)
        .await
        .map_err(|error| HostError::database("read history", error))
}

pub async fn count(pool: &SqlitePool) -> Result<i64, HostError> {
    sqlx::query_scalar("SELECT COUNT(*) FROM history")
        .fetch_one(pool)
        .await
        .map_err(|error| HostError::database("count history", error))
}
