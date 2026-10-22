//! The command surface the desktop shell exposes.
//!
//! One method per command the interface can call, named after it, so the
//! adapter's job is a mapping and nothing else: it turns a request into a
//! method call and the result into a reply. Everything here is headless and
//! testable, which is the point — the shell is not where queue rules belong.

use std::sync::Arc;

use serde::Serialize;

use crate::error::HostError;
use crate::events::TaskView;
use crate::manager::{AddRequest, ManagerHandle};
use crate::now_ms;
use crate::store::schedules::{self, NewSchedule, ScheduleKind, ScheduleRow};
use crate::store::{Store, settings, tasks};

/// Every setting, in the shape the frontend reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub max_parallel: i64,
    pub max_connections_total: i64,
    pub global_speed_limit: Option<i64>,
    pub resume_on_start: bool,
    pub retry_max_attempts: i64,
    pub retry_base_ms: i64,
    pub retry_cap_ms: i64,
    pub default_dest_dir: String,
}

impl Settings {
    /// Reads every setting, falling back to the seeded defaults when a key is
    /// missing. A value that cannot be read is an error rather than a silent
    /// default: a corrupt limit should be visible, not quietly replaced.
    pub async fn load(pool: &sqlx::SqlitePool) -> Result<Self, HostError> {
        Ok(Self {
            max_parallel: settings::get_i64(pool, settings::MAX_PARALLEL, 3).await?,
            max_connections_total: settings::get_i64(pool, settings::MAX_CONNECTIONS_TOTAL, 48)
                .await?,
            global_speed_limit: settings::get_optional_i64(pool, settings::GLOBAL_SPEED_LIMIT)
                .await?,
            resume_on_start: settings::get_bool(pool, settings::RESUME_ON_START, true).await?,
            retry_max_attempts: settings::get_i64(pool, settings::RETRY_MAX_ATTEMPTS, 3).await?,
            retry_base_ms: settings::get_i64(pool, settings::RETRY_BASE_MS, 5_000).await?,
            retry_cap_ms: settings::get_i64(pool, settings::RETRY_CAP_MS, 300_000).await?,
            default_dest_dir: settings::get_string(pool, settings::DEFAULT_DEST_DIR, "").await?,
        })
    }
}

/// A start window as the frontend reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleView {
    pub id: i64,
    pub label: String,
    pub kind: String,
    pub start_at: i64,
    pub window_minutes: i64,
    pub weekdays: i64,
    pub enabled: bool,
}

impl ScheduleView {
    pub fn from_row(row: &ScheduleRow) -> Self {
        Self {
            id: row.id,
            label: row.label.clone(),
            kind: row.kind.clone(),
            start_at: row.start_at,
            window_minutes: row.window_minutes,
            weekdays: row.weekdays,
            enabled: row.is_enabled(),
        }
    }
}

/// What a caller supplies to create a start window.
#[derive(Debug, Clone)]
pub struct ScheduleInput {
    pub label: String,
    pub kind: ScheduleKind,
    pub start_at: i64,
    /// Length of the open window in minutes; 0 means it never closes.
    pub window_minutes: i64,
    /// Seven-bit mask with bit 0 as Monday; only read for a weekly window.
    pub weekdays: i64,
    pub enabled: bool,
}

/// The commands the shell can issue.
pub struct Host {
    store: Arc<Store>,
    manager: ManagerHandle,
}

impl Host {
    pub fn new(store: Arc<Store>, manager: ManagerHandle) -> Self {
        Self { store, manager }
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    // --- tasks ----------------------------------------------------------

    pub async fn tasks_list(&self) -> Result<Vec<TaskView>, HostError> {
        self.manager.list().await
    }

    pub async fn task_add(&self, request: AddRequest) -> Result<TaskView, HostError> {
        self.manager.add(request).await
    }

    pub async fn task_pause(&self, id: i64) -> Result<(), HostError> {
        self.manager.pause(id).await
    }

    pub async fn task_resume(&self, id: i64) -> Result<(), HostError> {
        self.manager.resume(id).await
    }

    pub async fn task_retry(&self, id: i64) -> Result<(), HostError> {
        self.manager.retry(id).await
    }

    pub async fn task_cancel(&self, id: i64) -> Result<(), HostError> {
        self.manager.cancel(id).await
    }

    pub async fn task_delete(&self, id: i64, remove_file: bool) -> Result<(), HostError> {
        self.manager.delete(id, remove_file).await
    }

    pub async fn task_reorder(&self, id: i64, priority: i64) -> Result<(), HostError> {
        self.manager.reorder(id, priority).await
    }

    // --- queue ----------------------------------------------------------

    /// Stops starting new work. Running downloads keep going: the queue is
    /// what is paused, not the transfers already under way.
    pub async fn queue_pause(&self) -> Result<(), HostError> {
        self.manager.pause_queue().await
    }

    pub async fn queue_resume(&self) -> Result<(), HostError> {
        self.manager.resume_queue().await
    }

    pub async fn queue_paused(&self) -> Result<bool, HostError> {
        self.manager.queue_paused().await
    }

    // --- settings -------------------------------------------------------

    pub async fn settings_get(&self) -> Result<Settings, HostError> {
        Settings::load(self.store.pool()).await
    }

    /// Writes one setting after checking its type and range, then lets the
    /// manager re-read the ones that change what it does next.
    pub async fn settings_update(
        &self,
        name: &str,
        value: &serde_json::Value,
    ) -> Result<Settings, HostError> {
        let Some(setting) = settings::Setting::parse(name) else {
            return Err(HostError::invalid(format!(
                "there is no setting called {name}"
            )));
        };
        let encoded = setting.encode(value)?;
        settings::set(self.store.pool(), setting.key(), &encoded, now_ms()).await?;
        self.manager.refresh().await?;
        Settings::load(self.store.pool()).await
    }

    // --- schedules ------------------------------------------------------

    pub async fn schedules_list(&self) -> Result<Vec<ScheduleView>, HostError> {
        Ok(schedules::list(self.store.pool())
            .await?
            .iter()
            .map(ScheduleView::from_row)
            .collect())
    }

    pub async fn schedule_create(&self, input: ScheduleInput) -> Result<ScheduleView, HostError> {
        if input.label.trim().is_empty() {
            return Err(HostError::invalid("a schedule needs a name"));
        }
        if input.window_minutes < 0 {
            return Err(HostError::invalid("a window cannot have negative length"));
        }
        let id = schedules::insert(
            self.store.pool(),
            &NewSchedule {
                label: input.label.trim().to_string(),
                kind: input.kind,
                start_at: input.start_at,
                window_minutes: input.window_minutes,
                weekdays: input.weekdays,
                enabled: input.enabled,
                created_at: now_ms(),
            },
        )
        .await?;
        let row = self.schedule(id).await?;
        // A window that opens immediately should not wait for the next beat.
        self.manager.refresh().await?;
        Ok(ScheduleView::from_row(&row))
    }

    pub async fn schedule_set_enabled(&self, id: i64, enabled: bool) -> Result<(), HostError> {
        schedules::set_enabled(self.store.pool(), id, enabled).await?;
        self.manager.refresh().await
    }

    pub async fn schedule_delete(&self, id: i64) -> Result<(), HostError> {
        if !schedules::delete(self.store.pool(), id).await? {
            return Err(HostError::NotFound {
                entity: "schedule",
                id,
            });
        }
        // Tasks that pointed at the schedule keep their work; the schema
        // clears the link, so they simply stop being gated by a window.
        self.manager.refresh().await
    }

    async fn schedule(&self, id: i64) -> Result<ScheduleRow, HostError> {
        schedules::get(self.store.pool(), id)
            .await?
            .ok_or(HostError::NotFound {
                entity: "schedule",
                id,
            })
    }

    /// How many tasks sit in each state, for a queue summary.
    pub async fn queue_counts(&self) -> Result<QueueCounts, HostError> {
        let counts = tasks::counts(self.store.pool()).await?;
        Ok(QueueCounts::from_counts(&counts))
    }
}

/// How many tasks are in each state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueCounts {
    pub queued: i64,
    pub scheduled: i64,
    pub downloading: i64,
    pub paused: i64,
    pub completed: i64,
    pub failed: i64,
}

impl QueueCounts {
    /// Builds the summary from a grouped count. A status with no rows is
    /// simply absent from the query, so it reads as zero.
    pub fn from_counts(counts: &[(String, i64)]) -> Self {
        let of = |status: &str| {
            counts
                .iter()
                .find(|(name, _)| name == status)
                .map(|(_, total)| *total)
                .unwrap_or(0)
        };
        Self {
            queued: of("queued"),
            scheduled: of("scheduled"),
            downloading: of("downloading"),
            paused: of("paused"),
            completed: of("completed"),
            failed: of("error"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_setting_name_round_trips_through_parse() {
        for setting in settings::Setting::ALL {
            assert_eq!(settings::Setting::parse(setting.key()), Some(setting));
        }
    }
}
