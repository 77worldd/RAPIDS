//! RAPIDS host logic: the queue, persistence, scheduling, limits, and the
//! lifecycle of running downloads.
//!
//! The desktop shell is a thin adapter over this crate. Nothing here names a
//! window, a Tauri type, or an app handle: events leave through the
//! [`EventSink`] trait, which the adapter implements and tests record, and
//! every behaviour is exercised headless in this crate's own tests.

pub mod api;
pub mod error;
pub mod events;
pub mod manager;
pub mod progress;
mod runner;
pub mod scheduler;
pub mod store;

pub use api::{Host, QueueCounts, ScheduleInput, ScheduleView, Settings};
pub use error::HostError;
pub use events::{EventSink, HostEvent, NullSink, ProgressUpdate, RecordingSink, TaskView};
pub use manager::{AddRequest, ManagerHandle};
pub use scheduler::{Clock, Window};

/// Milliseconds since the Unix epoch. Callers pass the clock in rather than
/// letting queries read it, so tests can place a task in the past or the
/// future without sleeping.
pub fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}
