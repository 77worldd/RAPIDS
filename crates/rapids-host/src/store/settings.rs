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
