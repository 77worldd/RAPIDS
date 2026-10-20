//! RAPIDS download engine.
//!
//! Segmented HTTP/HTTPS downloads with kill-safe resume (part files plus a
//! sidecar manifest), retry with exponential backoff, pause/resume/cancel
//! control, dynamic connection scaling, and optional SHA-256 verification.
//!
//! The crate is UI-agnostic: the desktop host and the command line driver
//! both consume it through the `spawn_download` entry point and a typed
//! event stream.

mod coordinator;
mod hash;
mod http;

pub mod config;
pub mod error;
pub mod events;
pub mod manifest;
pub mod rate;
pub mod segmenter;
pub mod util;
pub mod workspace;

pub use config::EngineConfig;
pub use coordinator::{spawn_download, DownloadHandle, DownloadSpec};
pub use error::EngineError;
pub use events::{DownloadState, EngineEvent};
pub use hash::{sha256_file, sha256_hex};
pub use manifest::{Manifest, SegmentState};
pub use rate::{RateLimiter, RateLimits};
pub use segmenter::{scaling_decision, tile, ScaleDecision, Segment};
pub use workspace::Workspace;
