use std::fmt;

/// Everything that can go wrong inside the host layer. Messages are written
/// for the person reading them, and every one says what failed.
#[derive(Debug)]
pub enum HostError {
    /// A query or connection failed.
    Database {
        operation: &'static str,
        reason: String,
    },
    /// The schema could not be brought up to date.
    Migration { reason: String },
    /// SQLite reported a damaged database. RAPIDS refuses to continue rather
    /// than write into a file it cannot trust.
    Integrity { reason: String },
    /// A row the caller named does not exist.
    NotFound { entity: &'static str, id: i64 },
    /// The caller asked for something the store does not allow.
    Invalid { reason: String },
}

impl HostError {
    pub fn database(operation: &'static str, error: sqlx::Error) -> Self {
        Self::Database {
            operation,
            reason: error.to_string(),
        }
    }

    pub fn invalid(reason: impl Into<String>) -> Self {
        Self::Invalid {
            reason: reason.into(),
        }
    }
}

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database { operation, reason } => {
                write!(f, "database operation failed ({operation}): {reason}")
            }
            Self::Migration { reason } => write!(f, "schema update failed: {reason}"),
            Self::Integrity { reason } => write!(
                f,
                "the RAPIDS database is damaged ({reason}); it was left untouched"
            ),
            Self::NotFound { entity, id } => write!(f, "no {entity} with id {id}"),
            Self::Invalid { reason } => write!(f, "rejected: {reason}"),
        }
    }
}

impl std::error::Error for HostError {}

impl From<sqlx::Error> for HostError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database {
            operation: "query",
            reason: error.to_string(),
        }
    }
}
