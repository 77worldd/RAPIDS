//! Start windows: one-time, daily, and weekly.

use std::fmt;

use sqlx::SqlitePool;

use crate::error::HostError;

/// How often a window opens. Mirrors the `kind` CHECK constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduleKind {
    Once,
    Daily,
    Weekly,
}

impl ScheduleKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Once => "once",
            Self::Daily => "daily",
            Self::Weekly => "weekly",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "once" => Some(Self::Once),
            "daily" => Some(Self::Daily),
            "weekly" => Some(Self::Weekly),
            _ => None,
        }
    }
}

impl fmt::Display for ScheduleKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ScheduleRow {
    pub id: i64,
    pub label: String,
    pub kind: String,
    pub start_at: i64,
    pub window_minutes: i64,
    pub weekdays: i64,
    pub enabled: i64,
    pub created_at: i64,
}

impl ScheduleRow {
    pub fn kind_enum(&self) -> Option<ScheduleKind> {
        ScheduleKind::parse(&self.kind)
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled != 0
    }
}

#[derive(Debug, Clone)]
pub struct NewSchedule {
    pub label: String,
    pub kind: ScheduleKind,
    pub start_at: i64,
    pub window_minutes: i64,
    pub weekdays: i64,
    pub enabled: bool,
    pub created_at: i64,
}

pub async fn insert(pool: &SqlitePool, schedule: &NewSchedule) -> Result<i64, HostError> {
    if schedule.window_minutes < 0 {
        return Err(HostError::invalid("a window cannot have negative length"));
    }
    if !(0..=127).contains(&schedule.weekdays) {
        return Err(HostError::invalid(
            "weekdays must be a seven-bit mask (0-127)",
        ));
    }
    let result = sqlx::query(
        "INSERT INTO schedules (label, kind, start_at, window_minutes, weekdays, enabled, \
         created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(schedule.label.as_str())
    .bind(schedule.kind.as_str())
    .bind(schedule.start_at)
    .bind(schedule.window_minutes)
    .bind(schedule.weekdays)
    .bind(if schedule.enabled { 1_i64 } else { 0_i64 })
    .bind(schedule.created_at)
    .execute(pool)
    .await
    .map_err(|error| HostError::database("insert schedule", error))?;
    Ok(result.last_insert_rowid())
}

pub async fn list(pool: &SqlitePool) -> Result<Vec<ScheduleRow>, HostError> {
    sqlx::query_as::<_, ScheduleRow>("SELECT * FROM schedules ORDER BY start_at ASC, id ASC")
        .fetch_all(pool)
        .await
        .map_err(|error| HostError::database("list schedules", error))
}

pub async fn set_enabled(
    pool: &SqlitePool,
    id: i64,
    enabled: bool,
) -> Result<(), HostError> {
    let result = sqlx::query("UPDATE schedules SET enabled = ? WHERE id = ?")
        .bind(if enabled { 1_i64 } else { 0_i64 })
        .bind(id)
        .execute(pool)
        .await
        .map_err(|error| HostError::database("update schedule", error))?;
    if result.rows_affected() == 0 {
        return Err(HostError::NotFound {
            entity: "schedule",
            id,
        });
    }
    Ok(())
}

/// Removes a schedule. Tasks that pointed at it keep running; their
/// `schedule_id` becomes NULL through the schema's foreign key action.
pub async fn delete(pool: &SqlitePool, id: i64) -> Result<bool, HostError> {
    let result = sqlx::query("DELETE FROM schedules WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .map_err(|error| HostError::database("delete schedule", error))?;
    Ok(result.rows_affected() > 0)
}
