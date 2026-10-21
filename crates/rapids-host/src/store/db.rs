use std::path::Path;
use std::time::Duration;

use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};

use crate::error::HostError;

/// Several connections so a progress write cannot queue behind a settings
/// read. All writes still come from one owner, so this is headroom, not
/// concurrency policy.
const MAX_CONNECTIONS: u32 = 4;

/// SQLite gives up waiting for another writer after this long. In practice
/// RAPIDS is the only writer; the timeout exists so a stuck statement fails
/// with a message instead of hanging forever.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// The database file, opened, migrated, and checked once per process.
pub struct Store {
    pool: SqlitePool,
}

impl Store {
    /// Opens the database, creating its directory and the file when needed,
    /// applies migrations, and runs an integrity check.
    ///
    /// A damaged database is reported as an error and left untouched: RAPIDS
    /// would rather refuse to start than write into a file it cannot trust.
    pub async fn open(path: &Path) -> Result<Self, HostError> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|error| HostError::Database {
                    operation: "create database directory",
                    reason: error.to_string(),
                })?;
        }

        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(BUSY_TIMEOUT)
            .foreign_keys(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(MAX_CONNECTIONS)
            .connect_with(options)
            .await
            .map_err(|error| HostError::database("open database", error))?;

        let store = Self { pool };
        store.migrate().await?;
        store.check_integrity().await?;
        Ok(store)
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Closes every connection. Pending writes are flushed first.
    pub async fn close(self) {
        self.pool.close().await;
    }

    async fn migrate(&self) -> Result<(), HostError> {
        sqlx::migrate!("./migrations")
            .run(&self.pool)
            .await
            .map_err(|error| HostError::Migration {
                reason: error.to_string(),
            })
    }

    async fn check_integrity(&self) -> Result<(), HostError> {
        let report: String = sqlx::query_scalar("PRAGMA quick_check")
            .fetch_one(&self.pool)
            .await
            .map_err(|error| HostError::database("integrity check", error))?;
        if report == "ok" {
            Ok(())
        } else {
            Err(HostError::Integrity { reason: report })
        }
    }
}
