use std::fmt;
use std::path::PathBuf;

/// Where a download sits in its lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadState {
    Probing,
    Downloading,
    Paused,
    Verifying,
    Completed,
    Failed,
    Cancelled,
}

impl DownloadState {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

impl fmt::Display for DownloadState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::Probing => "probing",
            Self::Downloading => "downloading",
            Self::Paused => "paused",
            Self::Verifying => "verifying",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        };
        f.write_str(text)
    }
}

/// Everything the engine tells its host while a download runs.
///
/// Progress events are coalescible (a slow reader may miss intermediate
/// ticks); state, retry, and terminal events are delivered reliably while the
/// event receiver is alive.
#[derive(Debug, Clone)]
pub enum EngineEvent {
    StateChanged {
        state: DownloadState,
    },
    Progress {
        downloaded: u64,
        total: Option<u64>,
        speed_bps: u64,
        eta_secs: Option<u64>,
        active_connections: usize,
        segments_done: usize,
        segments_total: usize,
    },
    Retrying {
        segment: u32,
        attempt: u32,
        delay_ms: u64,
        reason: String,
    },
    Completed {
        path: PathBuf,
        sha256: Option<String>,
    },
    Failed {
        message: String,
        /// Whether trying again could succeed. It is decided from the concrete
        /// error where it was raised, so nothing downstream has to read the
        /// message to tell a dead link from a dropped connection.
        retryable: bool,
    },
    Cancelled,
}

impl EngineEvent {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed { .. } | Self::Failed { .. } | Self::Cancelled
        )
    }
}

impl fmt::Display for EngineEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StateChanged { state } => write!(f, "status: {state}"),
            Self::Progress {
                downloaded,
                total,
                speed_bps,
                eta_secs,
                active_connections,
                segments_done,
                segments_total,
            } => {
                let total_text = match total {
                    Some(value) => crate::util::human_bytes(*value),
                    None => "unknown".to_string(),
                };
                let eta_text = match eta_secs {
                    Some(secs) => crate::util::human_duration(*secs),
                    None => "--:--".to_string(),
                };
                write!(
                    f,
                    "{} / {total_text} · {}/s · ETA {eta_text} · {active_connections} connections · segments {segments_done}/{segments_total}",
                    crate::util::human_bytes(*downloaded),
                    crate::util::human_bytes(*speed_bps),
                )
            }
            Self::Retrying {
                segment,
                attempt,
                delay_ms,
                reason,
            } => write!(
                f,
                "segment {segment}: attempt {attempt} failed ({reason}); retrying in {delay_ms}ms"
            ),
            Self::Completed { path, sha256 } => match sha256 {
                Some(hash) => write!(f, "completed: {} · sha256 {hash}", path.display()),
                None => write!(f, "completed: {}", path.display()),
            },
            Self::Failed { message, .. } => write!(f, "failed: {message}"),
            Self::Cancelled => f.write_str("cancelled"),
        }
    }
}
