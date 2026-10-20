//! RAPIDS download engine.
//!
//! Segmented HTTP/HTTPS downloads with kill-safe resume (part files plus a
//! sidecar manifest), retry with exponential backoff, pause/resume/cancel
//! control, dynamic connection scaling, and optional SHA-256 verification.
//!
//! The crate is UI-agnostic: the desktop host and the command line driver
//! both consume it through [`spawn_download`]-style entry points and a
//! typed event stream.
