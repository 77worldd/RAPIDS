//! RAPIDS download engine.
//!
//! Segmented HTTP/HTTPS downloads with kill-safe resume (part files plus a
//! sidecar manifest), retry with exponential backoff, pause/resume/cancel
//! control, dynamic connection scaling, optional SHA-256 verification, and
//! HLS recordings (m3u8 playlists, AES-128 segments).
//!
//! The crate is UI-agnostic: the desktop host and the command line driver
//! both consume it through the `spawn_download` entry point and a typed
//! event stream.

mod coordinator;
mod hash;
mod http;

pub mod aes;
pub mod base64;
pub mod config;
pub mod connections;
pub mod error;
pub mod events;
pub mod headers;
pub mod m3u8;
pub mod manifest;
pub mod rate;
pub mod segmenter;
pub mod util;
pub mod workspace;

pub use config::EngineConfig;
pub use connections::ConnectionBudget;
pub use coordinator::{DownloadHandle, DownloadSpec, spawn_download};
pub use error::EngineError;
pub use events::{DownloadState, EngineEvent};
pub use hash::{sha256_file, sha256_hex};
pub use headers::{BasicAuth, HeaderProfile, RequestHeaders};
pub use http::probe::{filename_from_url, probe, probe_into};
pub use manifest::{Manifest, SegmentState};
pub use rate::{RateLimiter, RateLimits};
pub use segmenter::{ScaleDecision, Segment, scaling_decision, tile};
pub use workspace::Workspace;
