use std::fmt;
use std::path::{Path, PathBuf};

/// Every way a download can end badly. Messages are written for the person
/// reading them: they state what happened and what to do next.
#[derive(Debug)]
pub enum EngineError {
    /// The URL could not be parsed.
    InvalidUrl { url: String, reason: String },
    /// The request itself is unusable before any network traffic.
    InvalidSpec { reason: String },
    /// A retried network operation exhausted its attempts.
    Network {
        operation: &'static str,
        reason: String,
        attempts: u32,
    },
    /// A retried operation kept timing out until attempts ran out.
    Timeout {
        operation: &'static str,
        reason: String,
        attempts: u32,
    },
    /// The server answered with a status that cannot be worked around.
    HttpStatus { status: u16, url: String },
    /// The server contradicted the HTTP contract mid-transfer.
    Protocol { reason: String },
    /// Remote content changed; continuing would splice two versions.
    ContentChanged,
    /// Remote size changed; ranges no longer tile the file.
    SizeChanged,
    /// The destination file exists and replacement was not requested.
    DestinationExists { path: PathBuf },
    /// Another live process holds the workspace lock.
    WorkspaceLocked { path: PathBuf },
    /// Local disk interaction failed.
    Io {
        operation: &'static str,
        path: PathBuf,
        reason: String,
    },
    /// Finished bytes did not hash to the expected value.
    VerificationFailed { expected: String, actual: String },
}

impl EngineError {
    pub fn io(operation: &'static str, path: &Path, error: std::io::Error) -> Self {
        Self::Io {
            operation,
            path: path.to_path_buf(),
            reason: error.to_string(),
        }
    }

    pub fn invalid_spec(reason: impl Into<String>) -> Self {
        Self::InvalidSpec {
            reason: reason.into(),
        }
    }

    pub fn protocol(reason: impl Into<String>) -> Self {
        Self::Protocol {
            reason: reason.into(),
        }
    }

    pub fn network(operation: &'static str, reason: impl Into<String>, attempts: u32) -> Self {
        Self::Network {
            operation,
            reason: reason.into(),
            attempts,
        }
    }

    pub fn timeout(operation: &'static str, reason: impl Into<String>, attempts: u32) -> Self {
        Self::Timeout {
            operation,
            reason: reason.into(),
            attempts,
        }
    }
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUrl { url, reason } => write!(f, "invalid download URL {url}: {reason}"),
            Self::InvalidSpec { reason } => write!(f, "invalid download request: {reason}"),
            Self::Network {
                operation,
                reason,
                attempts,
            } => write!(
                f,
                "network problem during {operation} after {attempts} attempt(s): {reason}"
            ),
            Self::Timeout {
                operation,
                reason,
                attempts,
            } => write!(
                f,
                "{operation} timed out after {attempts} attempt(s): {reason}"
            ),
            Self::HttpStatus { status, url } => {
                write!(f, "server responded with HTTP {status} for {url}")
            }
            Self::Protocol { reason } => write!(f, "server spoke HTTP incorrectly: {reason}"),
            Self::ContentChanged => write!(
                f,
                "the file changed on the server during the download; run the download again to fetch the new content"
            ),
            Self::SizeChanged => write!(
                f,
                "the file size on the server changed; run the download again to fetch the current file"
            ),
            Self::DestinationExists { path } => write!(
                f,
                "{} already exists; pass --overwrite to replace it",
                path.display()
            ),
            Self::WorkspaceLocked { path } => write!(
                f,
                "another RAPIDS process is working on {}",
                path.display()
            ),
            Self::Io {
                operation,
                path,
                reason,
            } => write!(f, "{operation} failed for {}: {reason}", path.display()),
            Self::VerificationFailed { expected, actual } => write!(
                f,
                "SHA-256 verification failed: expected {expected}, computed {actual}"
            ),
        }
    }
}

impl std::error::Error for EngineError {}
