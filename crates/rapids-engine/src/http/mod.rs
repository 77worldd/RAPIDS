pub(crate) mod fetch;
pub(crate) mod probe;

use std::time::Duration;

use reqwest::header::HeaderName;
use reqwest::Response;

use crate::config::EngineConfig;
use crate::error::EngineError;

/// Builds the shared HTTP client: rustls with the platform's trust store,
/// HTTP/1.1 connections only (segment ranges map 1:1 onto connections), no
/// content-encoding features (range bytes must arrive raw), and a connect
/// timeout. Per-request timeouts live at the call sites.
pub(crate) fn build_client(config: &EngineConfig) -> Result<reqwest::Client, EngineError> {
    reqwest::Client::builder()
        .user_agent(config.user_agent.clone())
        .connect_timeout(config.connect_timeout)
        .build()
        .map_err(|error| EngineError::network("build HTTP client", error.to_string(), 0))
}

pub(crate) fn header_str(response: &Response, name: &HeaderName) -> Option<String> {
    response
        .headers()
        .get(name)?
        .to_str()
        .ok()
        .map(|value| value.trim().to_string())
}

pub(crate) fn header_u64(response: &Response, name: &HeaderName) -> Option<u64> {
    header_str(response, name)?.parse::<u64>().ok()
}

pub(crate) fn header_accepts_bytes(response: &Response, name: &HeaderName) -> bool {
    matches!(
        header_str(response, name).as_deref(),
        Some(value) if value.eq_ignore_ascii_case("bytes")
    )
}

/// Exponential retry delay for `attempt` (1-based), capped by the config.
pub(crate) fn base_backoff(config: &EngineConfig, attempt: u32) -> Duration {
    let exponent = attempt.saturating_sub(1).min(16);
    // Milliseconds are already u128; scaling here keeps the multiplication
    // exact for any attempt count before the cap applies.
    let multiplier = 1u128 << exponent;
    let candidate = config.backoff_base.as_millis() * multiplier;
    let cap = config.backoff_cap.as_millis();
    Duration::from_millis(candidate.min(cap) as u64)
}

/// Splitmix64-style finalizer: turns sparse keys into well-spread values.
pub(crate) fn mix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

/// Fresh entropy for jitter, taken from the wall clock.
pub(crate) fn now_entropy() -> u64 {
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    mix64(elapsed.as_nanos() as u64)
}

/// Adds up to 25% of the base delay as deterministic-chaotic jitter so
/// parallel segments do not retry in lockstep.
pub(crate) fn jittered_backoff(base: Duration, entropy: u64) -> Duration {
    let base_ms = base.as_millis() as u64;
    if base_ms == 0 {
        return base;
    }
    let window = (base_ms / 4).max(1);
    let extra = mix64(entropy) % window;
    base + Duration::from_millis(extra)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> EngineConfig {
        EngineConfig::default()
    }

    #[test]
    fn backoff_doubles_and_caps() {
        let config = config();
        let schedule: [Duration; 8] = [
            base_backoff(&config, 1),
            base_backoff(&config, 2),
            base_backoff(&config, 3),
            base_backoff(&config, 4),
            base_backoff(&config, 5),
            base_backoff(&config, 6),
            base_backoff(&config, 7),
            base_backoff(&config, 8),
        ];
        assert_eq!(schedule[0], Duration::from_millis(500));
        assert_eq!(schedule[1], Duration::from_millis(1000));
        assert_eq!(schedule[2], Duration::from_millis(2000));
        assert_eq!(schedule[3], Duration::from_millis(4000));
        assert_eq!(schedule[4], Duration::from_millis(8000));
        assert_eq!(schedule[5], Duration::from_millis(16000));
        assert_eq!(schedule[6], Duration::from_millis(30000));
        assert_eq!(schedule[7], Duration::from_millis(30000));
        for window in 1..12 {
            assert!(base_backoff(&config, window) <= config.backoff_cap);
        }
    }

    #[test]
    fn jitter_stays_within_the_quarter_window() {
        let base = Duration::from_millis(800);
        let ceiling = base + Duration::from_millis(200);
        for entropy in 0..200u64 {
            let delay = jittered_backoff(base, mix64(entropy));
            assert!(delay >= base, "jitter must never shorten the delay");
            assert!(delay <= ceiling, "jitter must stay inside its window");
        }
    }

    #[test]
    fn mix_produces_distinct_streams() {
        let a = mix64(1);
        let b = mix64(2);
        assert_ne!(a, b);
        assert_ne!(mix64(u64::MAX), u64::MAX);
    }
}
