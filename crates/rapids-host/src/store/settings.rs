//! Typed access to the settings table. Values are stored as JSON scalars, so
//! a reader knows whether a value is a number, a boolean, a string, or unset
//! without guessing from the text.

use std::collections::BTreeMap;

use sqlx::SqlitePool;

use crate::error::HostError;

pub const MAX_PARALLEL: &str = "max_parallel";
pub const MAX_CONNECTIONS_TOTAL: &str = "max_connections_total";
pub const GLOBAL_SPEED_LIMIT: &str = "global_speed_limit";
pub const RESUME_ON_START: &str = "resume_on_start";
pub const RETRY_MAX_ATTEMPTS: &str = "retry_max_attempts";
pub const RETRY_BASE_MS: &str = "retry_base_ms";
pub const RETRY_CAP_MS: &str = "retry_cap_ms";
pub const DEFAULT_DEST_DIR: &str = "default_dest_dir";

/// Every setting, keyed by name.
pub async fn all(pool: &SqlitePool) -> Result<BTreeMap<String, String>, HostError> {
    let rows: Vec<(String, String)> = sqlx::query_as("SELECT key, value FROM settings")
        .fetch_all(pool)
        .await
        .map_err(|error| HostError::database("read settings", error))?;
    Ok(rows.into_iter().collect())
}

pub async fn get(pool: &SqlitePool, key: &str) -> Result<Option<String>, HostError> {
    sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await
        .map_err(|error| HostError::database("read setting", error))
}

/// Writes one setting, inserting it when it is new.
pub async fn set(pool: &SqlitePool, key: &str, value: &str, now: i64) -> Result<(), HostError> {
    sqlx::query(
        "INSERT INTO settings (key, value, updated_at) VALUES (?, ?, ?) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(key)
    .bind(value)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|error| HostError::database("write setting", error))?;
    Ok(())
}

/// Reads a number, or `fallback` when the setting is absent.
pub async fn get_i64(pool: &SqlitePool, key: &str, fallback: i64) -> Result<i64, HostError> {
    match get(pool, key).await? {
        None => Ok(fallback),
        Some(value) => value
            .trim()
            .parse::<i64>()
            .map_err(|_| HostError::invalid(format!("{key} is not a number: {value}"))),
    }
}

/// Reads an optional number: `null` in storage means "no value".
pub async fn get_optional_i64(pool: &SqlitePool, key: &str) -> Result<Option<i64>, HostError> {
    match get(pool, key).await? {
        None => Ok(None),
        Some(value) if value.trim() == "null" => Ok(None),
        Some(value) => value
            .trim()
            .parse::<i64>()
            .map(Some)
            .map_err(|_| HostError::invalid(format!("{key} is not a number: {value}"))),
    }
}

/// Reads a boolean, or `fallback` when the setting is absent.
pub async fn get_bool(pool: &SqlitePool, key: &str, fallback: bool) -> Result<bool, HostError> {
    match get(pool, key).await? {
        None => Ok(fallback),
        Some(value) => match value.trim() {
            "true" => Ok(true),
            "false" => Ok(false),
            other => Err(HostError::invalid(format!(
                "{key} is not a boolean: {other}"
            ))),
        },
    }
}

/// Reads a JSON string setting (stored quoted), or `fallback` when absent.
/// Parsing goes through a real JSON reader so escapes (\", \\, \n, \uXXXX)
/// survive the round trip instead of coming back as literal text.
pub async fn get_string(pool: &SqlitePool, key: &str, fallback: &str) -> Result<String, HostError> {
    match get(pool, key).await? {
        None => Ok(fallback.to_string()),
        Some(value) => match serde_json::from_str::<serde_json::Value>(value.trim()) {
            Ok(serde_json::Value::String(text)) => Ok(text),
            _ => Err(HostError::invalid(format!(
                "{key} is not a JSON string: {value}"
            ))),
        },
    }
}

/// Encodes a string the way [`get_string`] expects to read it back: a JSON
/// string literal, escapes included.
pub fn encode_string(value: &str) -> String {
    serde_json::to_string(value).expect("a string always serializes to JSON")
}

/// The settings the host accepts, each with the bounds it is held to.
///
/// A name outside this list is rejected rather than written, so a typo in a
/// frontend cannot quietly add a setting nothing reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    MaxParallel,
    /// How many connections the whole app may hold open at once, across every
    /// running download. It is a budget, not a field of any one request: the
    /// engine refuses a per-task connection count above
    /// [`crate::manager::MAX_CONNECTIONS_PER_TASK`], so the task plan is
    /// derived from this budget and clamped to that ceiling.
    MaxConnectionsTotal,
    GlobalSpeedLimit,
    ResumeOnStart,
    RetryMaxAttempts,
    RetryBaseMs,
    RetryCapMs,
    DefaultDestDir,
}

impl Setting {
    /// Every setting, for a reader that needs the set rather than one entry.
    pub const ALL: [Self; 8] = [
        Self::MaxParallel,
        Self::MaxConnectionsTotal,
        Self::GlobalSpeedLimit,
        Self::ResumeOnStart,
        Self::RetryMaxAttempts,
        Self::RetryBaseMs,
        Self::RetryCapMs,
        Self::DefaultDestDir,
    ];

    pub fn parse(name: &str) -> Option<Self> {
        match name {
            MAX_PARALLEL => Some(Self::MaxParallel),
            MAX_CONNECTIONS_TOTAL => Some(Self::MaxConnectionsTotal),
            GLOBAL_SPEED_LIMIT => Some(Self::GlobalSpeedLimit),
            RESUME_ON_START => Some(Self::ResumeOnStart),
            RETRY_MAX_ATTEMPTS => Some(Self::RetryMaxAttempts),
            RETRY_BASE_MS => Some(Self::RetryBaseMs),
            RETRY_CAP_MS => Some(Self::RetryCapMs),
            DEFAULT_DEST_DIR => Some(Self::DefaultDestDir),
            _ => None,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::MaxParallel => MAX_PARALLEL,
            Self::MaxConnectionsTotal => MAX_CONNECTIONS_TOTAL,
            Self::GlobalSpeedLimit => GLOBAL_SPEED_LIMIT,
            Self::ResumeOnStart => RESUME_ON_START,
            Self::RetryMaxAttempts => RETRY_MAX_ATTEMPTS,
            Self::RetryBaseMs => RETRY_BASE_MS,
            Self::RetryCapMs => RETRY_CAP_MS,
            Self::DefaultDestDir => DEFAULT_DEST_DIR,
        }
    }

    /// Checks a caller's value and returns the text to store.
    ///
    /// The bounds are the ones the rest of the host already enforces or that
    /// keep a value usable: a parallel limit of zero would stall the queue
    /// forever, and a base retry delay of a day is indistinguishable from a
    /// broken app.
    pub fn encode(self, value: &serde_json::Value) -> Result<String, HostError> {
        use serde_json::Value;

        match self {
            Self::MaxParallel => number(value, self, 1, 32),
            Self::MaxConnectionsTotal => number(value, self, 1, 128),
            Self::RetryMaxAttempts => number(value, self, 0, 10),
            Self::RetryBaseMs => number(value, self, 100, 3_600_000),
            Self::RetryCapMs => number(value, self, 100, 86_400_000),
            Self::GlobalSpeedLimit => match value {
                Value::Null => Ok("null".to_string()),
                // Below a kilobyte per second a transfer looks hung.
                _ => number(value, self, 1_024, 1_073_741_824),
            },
            Self::ResumeOnStart => match value {
                Value::Bool(flag) => Ok(flag.to_string()),
                other => Err(HostError::invalid(format!(
                    "{} must be true or false, not {other}",
                    self.key()
                ))),
            },
            Self::DefaultDestDir => match value {
                Value::String(text) => Ok(encode_string(text)),
                other => Err(HostError::invalid(format!(
                    "{} must be a folder path, not {other}",
                    self.key()
                ))),
            },
        }
    }
}

/// Validates an integer setting against an inclusive range.
fn number(
    value: &serde_json::Value,
    setting: Setting,
    lowest: i64,
    highest: i64,
) -> Result<String, HostError> {
    let Some(integer) = value.as_i64() else {
        return Err(HostError::invalid(format!(
            "{} must be a whole number, not {value}",
            setting.key()
        )));
    };
    if !(lowest..=highest).contains(&integer) {
        return Err(HostError::invalid(format!(
            "{} must be between {lowest} and {highest}, not {integer}",
            setting.key()
        )));
    }
    Ok(integer.to_string())
}
