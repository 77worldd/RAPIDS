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

    /// Whether attempting the same download again could plausibly succeed
    /// without the person changing anything. Hosts use this to decide
    /// between an automatic retry with backoff and a terminal error row,
    /// so the classification has to be honest: telling someone to wait
    /// for a 404 that will never change is worse than failing at once.
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Network { .. } | Self::Timeout { .. } => true,
            Self::HttpStatus { status, .. } => matches!(*status, 408 | 429 | 500..=599),
            // Another process held the workspace; it may well let go.
            Self::WorkspaceLocked { .. } => true,
            // Both wipe and restart from the sidecar, which is a clean
            // outcome: the next attempt plans against the bytes the server
            // serves now instead of splicing two versions together.
            Self::ContentChanged | Self::SizeChanged => true,
            // A retry cannot repair a request, a server contradicting HTTP,
            // a full disk, an existing destination, or a hash mismatch.
            Self::InvalidUrl { .. }
            | Self::InvalidSpec { .. }
            | Self::Protocol { .. }
            | Self::DestinationExists { .. }
            | Self::Io { .. }
            | Self::VerificationFailed { .. } => false,
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
            Self::WorkspaceLocked { path } => {
                write!(f, "another RAPIDS process is working on {}", path.display())
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn http_status(status: u16) -> EngineError {
        EngineError::HttpStatus {
            status,
            url: "https://example.com/file".to_string(),
        }
    }

    #[test]
    fn transient_network_failures_are_retryable() {
        assert!(EngineError::network("server probe", "connection reset", 5).is_retryable());
        assert!(EngineError::timeout("server probe", "no headers", 5).is_retryable());
        assert!(
            EngineError::WorkspaceLocked {
                path: PathBuf::from("file.bin.rapids")
            }
            .is_retryable()
        );
    }

    #[test]
    fn only_transient_statuses_are_retryable() {
        for status in [408, 429, 500, 502, 503, 504, 599] {
            assert!(http_status(status).is_retryable(), "{status} should retry");
        }
        for status in [400, 401, 403, 404, 410, 451, 416] {
            assert!(
                !http_status(status).is_retryable(),
                "{status} must fail at once"
            );
        }
    }

    #[test]
    fn a_changed_remote_file_restarts_cleanly() {
        assert!(EngineError::ContentChanged.is_retryable());
        assert!(EngineError::SizeChanged.is_retryable());
    }

    #[test]
    fn unrecoverable_local_and_request_errors_are_not_retryable() {
        assert!(
            !EngineError::InvalidUrl {
                url: "not a url".to_string(),
                reason: "relative URL without a base".to_string(),
            }
            .is_retryable()
        );
        assert!(!EngineError::invalid_spec("connections must be 1-32").is_retryable());
        assert!(!EngineError::protocol("206 without Content-Range").is_retryable());
        assert!(
            !EngineError::DestinationExists {
                path: PathBuf::from("file.bin")
            }
            .is_retryable()
        );
        assert!(
            !EngineError::io(
                "write part file",
                Path::new("file.bin.rapids/data.part"),
                std::io::Error::from(std::io::ErrorKind::PermissionDenied)
            )
            .is_retryable()
        );
        assert!(
            !EngineError::VerificationFailed {
                expected: "a".repeat(64),
                actual: "b".repeat(64),
            }
            .is_retryable()
        );
    }
}
