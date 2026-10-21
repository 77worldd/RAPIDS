//! One running download: it starts the engine and forwards what the engine
//! says. The runner owns no state of its own — every decision belongs to the
//! manager, so there is exactly one writer.

use rapids_engine::{spawn_download, DownloadHandle, DownloadSpec, EngineConfig, EngineEvent};
use tokio::sync::mpsc;

use crate::error::HostError;
use crate::events::ProgressUpdate;
use crate::store::Progress;

/// What a running download tells the manager.
#[derive(Debug)]
pub(crate) enum Report {
    /// A display-only sample. The manager persists it and forwards it.
    Progress {
        id: i64,
        progress: Progress,
        update: ProgressUpdate,
    },
    Completed {
        id: i64,
        sha256: Option<String>,
    },
    Failed {
        id: i64,
        message: String,
        /// Classified by the engine where the error was raised. The runner
        /// passes it through rather than deciding anything itself.
        retryable: bool,
    },
    Cancelled {
        id: i64,
    },
}

fn to_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// Starts one download and returns the control handle the manager keeps.
pub(crate) fn start(
    id: i64,
    spec: DownloadSpec,
    config: EngineConfig,
    reports: mpsc::Sender<Report>,
) -> Result<DownloadHandle, HostError> {
    let (handle, mut events) = spawn_download(spec, config).map_err(|error| HostError::Invalid {
        reason: error.to_string(),
    })?;

    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            match event {
                EngineEvent::Progress {
                    downloaded,
                    total,
                    speed_bps,
                    eta_secs,
                    ..
                } => {
                    let progress = Progress {
                        downloaded: to_i64(downloaded),
                        speed_bps: to_i64(speed_bps),
                        eta_seconds: eta_secs.map(to_i64),
                        total_bytes: total.map(to_i64),
                    };
                    let update = ProgressUpdate {
                        id,
                        downloaded: progress.downloaded,
                        speed_bps: progress.speed_bps,
                        eta_seconds: progress.eta_seconds,
                        total_bytes: progress.total_bytes,
                    };
                    let _ = reports.send(Report::Progress { id, progress, update }).await;
                }
                EngineEvent::Completed { sha256, .. } => {
                    let _ = reports.send(Report::Completed { id, sha256 }).await;
                    break;
                }
                EngineEvent::Failed {
                    message,
                    retryable,
                } => {
                    let _ = reports
                        .send(Report::Failed {
                            id,
                            message,
                            retryable,
                        })
                        .await;
                    break;
                }
                EngineEvent::Cancelled => {
                    let _ = reports.send(Report::Cancelled { id }).await;
                    break;
                }
                EngineEvent::StateChanged { .. } | EngineEvent::Retrying { .. } => {}
            }
        }
    });

    Ok(handle)
}
