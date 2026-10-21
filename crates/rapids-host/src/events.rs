//! What the host tells its shell, and the trait it tells it through.
//!
//! Nothing here knows what a window is. The desktop adapter implements
//! [`EventSink`] over its own emitter; tests implement it as a recorder. The
//! payloads are serializable so the adapter can forward them untouched.

use serde::Serialize;

use crate::store::TaskRow;

/// One task as the interface shows it. Field names are camelCase because this
/// is the shape the frontend receives.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TaskView {
    pub id: i64,
    pub url: String,
    pub final_url: Option<String>,
    pub file_name: String,
    pub dest_path: String,
    pub category: String,
    pub status: String,
    pub priority: i64,
    pub total_bytes: Option<i64>,
    pub downloaded: i64,
    pub speed_bps: i64,
    pub eta_seconds: Option<i64>,
    pub error_message: Option<String>,
    pub attempts: i64,
    pub schedule_id: Option<i64>,
    pub sha256: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl TaskView {
    pub fn from_row(row: &TaskRow) -> Self {
        Self {
            id: row.id,
            url: row.url.clone(),
            final_url: row.final_url.clone(),
            file_name: row.file_name.clone(),
            dest_path: row.dest_path.clone(),
            category: row.category.clone(),
            status: row.status.clone(),
            priority: row.priority,
            total_bytes: row.total_bytes,
            downloaded: row.downloaded,
            speed_bps: row.speed_bps,
            eta_seconds: row.eta_seconds,
            error_message: row.error_message.clone(),
            attempts: row.attempts,
            schedule_id: row.schedule_id,
            sha256: row.observed_sha256.clone(),
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

/// One download's progress sample, small enough to batch several per message.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProgressUpdate {
    pub id: i64,
    pub downloaded: i64,
    pub speed_bps: i64,
    pub eta_seconds: Option<i64>,
    pub total_bytes: Option<i64>,
}

/// Everything the host pushes at its shell.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum HostEvent {
    /// A task's authoritative row after any transition. The shell upserts it.
    TaskChanged { task: Box<TaskView> },
    /// A task row no longer exists.
    TaskRemoved { id: i64 },
    /// Display-only samples; never used to decide anything.
    Progress { updates: Vec<ProgressUpdate> },
    /// The set of waiting or running work changed.
    QueueChanged,
}

/// Where host events go. Implementations must be cheap and must not block:
/// the manager calls this from its own task.
pub trait EventSink: Send + Sync {
    fn emit(&self, event: HostEvent);
}

/// A sink for callers that do not listen (headless runs, tests that read the
/// database instead).
#[derive(Debug, Default)]
pub struct NullSink;

impl EventSink for NullSink {
    fn emit(&self, _event: HostEvent) {}
}

/// A sink that remembers everything, for tests.
#[derive(Debug, Default)]
pub struct RecordingSink {
    events: std::sync::Mutex<Vec<HostEvent>>,
}

impl RecordingSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn events(&self) -> Vec<HostEvent> {
        self.events
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn clear(&self) {
        self.events
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
    }

    /// Every task id that appeared in a change or removal, in order.
    pub fn touched_ids(&self) -> Vec<i64> {
        self.events()
            .into_iter()
            .filter_map(|event| match event {
                HostEvent::TaskChanged { task } => Some(task.id),
                HostEvent::TaskRemoved { id } => Some(id),
                _ => None,
            })
            .collect()
    }
}

impl EventSink for RecordingSink {
    fn emit(&self, event: HostEvent) {
        self.events
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(event);
    }
}
