use std::sync::Arc;
use std::time::Duration;

use crate::error::EngineError;
use crate::rate::RateLimiter;

/// Tuning knobs for the download engine.
///
/// Defaults follow the product specification: start 8 connections, scale
/// toward 32 while the measured throughput gain of an added connection is at
/// least 20%, and give every network operation a bounded, retried lifetime.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Connections opened when a segmented download starts.
    pub start_connections: u8,
    /// Hard ceiling for dynamic connection scaling.
    pub max_connections: u8,
    /// Smallest segment produced by the segmenter (bytes).
    pub min_segment_bytes: u64,
    /// Length of one throughput measurement window used for scaling.
    pub scale_window: Duration,
    /// Minimum relative throughput gain that justifies another connection.
    pub gain_threshold: f64,
    /// Timeout for establishing a connection.
    pub connect_timeout: Duration,
    /// Timeout for receiving response headers.
    pub header_timeout: Duration,
    /// Maximum quiet period between body chunks before a fetch is retried.
    pub idle_read_timeout: Duration,
    /// Attempts per network operation before the task fails.
    pub max_attempts: u32,
    /// First retry delay; subsequent delays double until `backoff_cap`.
    pub backoff_base: Duration,
    /// Ceiling for any retry delay.
    pub backoff_cap: Duration,
    /// Aggregate bytes between manifest checkpoints.
    pub checkpoint_bytes: u64,
    /// Maximum time between manifest checkpoints.
    pub checkpoint_interval: Duration,
    /// Interval between progress events.
    pub progress_interval: Duration,
    /// User-Agent header sent with every request.
    pub user_agent: String,
    /// Ceiling for this download alone, in bytes per second. `None` or `0`
    /// leaves it unlimited; a tighter global limiter can still apply.
    pub rate_limit_bps: Option<u64>,
    /// Ceiling shared by every running download, supplied by the host so a
    /// changed global limit reaches downloads that are already running.
    pub shared_limiter: Option<Arc<RateLimiter>>,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            start_connections: 8,
            max_connections: 32,
            min_segment_bytes: 1024 * 1024,
            scale_window: Duration::from_secs(5),
            gain_threshold: 0.20,
            connect_timeout: Duration::from_secs(10),
            header_timeout: Duration::from_secs(30),
            idle_read_timeout: Duration::from_secs(30),
            max_attempts: 5,
            backoff_base: Duration::from_millis(500),
            backoff_cap: Duration::from_secs(30),
            checkpoint_bytes: 4 * 1024 * 1024,
            checkpoint_interval: Duration::from_secs(2),
            progress_interval: Duration::from_millis(500),
            user_agent: format!("rapids/{}", env!("CARGO_PKG_VERSION")),
            rate_limit_bps: None,
            shared_limiter: None,
        }
    }
}

impl EngineConfig {
    /// Repairs out-of-range values instead of failing the task for a typo.
    pub fn validated(mut self) -> Result<Self, EngineError> {
        if self.start_connections == 0 {
            return Err(EngineError::invalid_spec("connections must be at least 1"));
        }
        if self.max_connections < self.start_connections {
            self.max_connections = self.start_connections;
        }
        if self.min_segment_bytes == 0 {
            self.min_segment_bytes = 1;
        }
        if self.max_attempts == 0 {
            self.max_attempts = 1;
        }
        if !self.gain_threshold.is_finite() {
            self.gain_threshold = 0.20;
        } else {
            self.gain_threshold = self.gain_threshold.clamp(0.0, 1.0);
        }
        // A zero byte-per-second ceiling is not a useful instruction; it
        // means the caller wanted no ceiling at all.
        if self.rate_limit_bps == Some(0) {
            self.rate_limit_bps = None;
        }
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_follow_the_spec() {
        let config = EngineConfig::default();
        assert_eq!(config.start_connections, 8);
        assert_eq!(config.max_connections, 32);
        assert_eq!(config.min_segment_bytes, 1024 * 1024);
        assert!((config.gain_threshold - 0.20).abs() < f64::EPSILON);
        assert_eq!(config.max_attempts, 5);
        assert_eq!(config.backoff_base, Duration::from_millis(500));
        assert_eq!(config.backoff_cap, Duration::from_secs(30));
        assert!(config.user_agent.starts_with("rapids/"));
        assert!(config.rate_limit_bps.is_none());
        assert!(config.shared_limiter.is_none());
    }

    #[test]
    fn validation_repairs_ranges() {
        let config = EngineConfig {
            start_connections: 0,
            ..EngineConfig::default()
        }
        .validated();
        assert!(config.is_err());

        let config = EngineConfig {
            start_connections: 16,
            max_connections: 4,
            min_segment_bytes: 0,
            max_attempts: 0,
            gain_threshold: f64::NAN,
            rate_limit_bps: Some(0),
            ..EngineConfig::default()
        }
        .validated()
        .expect("repairable values must validate");
        assert!(config.max_connections >= config.start_connections);
        assert!(config.min_segment_bytes >= 1);
        assert!(config.max_attempts >= 1);
        assert!((config.gain_threshold - 0.20).abs() < f64::EPSILON);
        assert!(
            config.rate_limit_bps.is_none(),
            "a zero ceiling means the caller wanted none"
        );
    }
}
