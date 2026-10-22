//! Bandwidth limiting for downloads.
//!
//! Two ceilings can apply to one download at the same time: a global one
//! shared by every running download, and the download's own. Both are token
//! buckets, and a chunk is only written once every bucket that applies has
//! paid for it — so the tighter limit wins without either limit knowing
//! about the other.
//!
//! Waiting is done in short slices and the caller is asked between slices
//! whether to keep going. That is what keeps a pause or a cancel responsive
//! at a 1 KiB/s limit, where honouring a full chunk would otherwise take a
//! quarter of a minute.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use tokio::sync::Notify;
use tokio::time::{Instant, sleep};

/// Smallest burst a bucket may hold. The engine writes whole chunks, so a
/// bucket has to be able to pay for at least one of them.
const MIN_BURST_BYTES: u64 = 16 * 1024;

/// A bucket holds a quarter second of traffic. Short enough that a change
/// takes effect promptly, long enough that per-chunk accounting stays cheap.
const BURST_DIVISOR: u64 = 4;

/// Longest single wait inside an acquisition. This bounds how long a pause
/// or cancel takes to land while a download is being throttled.
const MAX_WAIT_SLICE: Duration = Duration::from_millis(250);

/// Tokens a bucket of `rate` bytes per second can hold.
fn burst_bytes(rate: u64) -> u64 {
    (rate / BURST_DIVISOR).max(MIN_BURST_BYTES)
}

/// Bytes earned over `nanos` at `rate`.
fn earned_tokens(nanos: u128, rate: u64) -> u64 {
    let earned = nanos.saturating_mul(u128::from(rate)) / 1_000_000_000;
    u64::try_from(earned).unwrap_or(u64::MAX)
}

/// Time needed to earn `bytes` at `rate`.
fn payment_time(bytes: u64, rate: u64) -> Duration {
    let rate = rate.max(1);
    let nanos = u128::from(bytes).saturating_mul(1_000_000_000) / u128::from(rate);
    Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX))
}

#[derive(Debug)]
struct Bucket {
    /// `None` means unlimited.
    rate_bps: Option<u64>,
    capacity: u64,
    tokens: u64,
    updated: Instant,
}

impl Bucket {
    fn new(rate_bps: Option<u64>) -> Self {
        let capacity = rate_bps.map(burst_bytes).unwrap_or(MIN_BURST_BYTES);
        Self {
            rate_bps,
            capacity,
            tokens: capacity,
            updated: Instant::now(),
        }
    }

    /// Adds what the elapsed time earned. When the bucket is full the clock
    /// is reset instead, so an idle download cannot bank credit and then
    /// burst past the limit, and the fractional remainder is preserved by
    /// advancing the clock only by the time the earned tokens paid for.
    fn refill(&mut self, now: Instant, rate: u64) {
        if self.tokens >= self.capacity {
            self.tokens = self.capacity;
            self.updated = now;
            return;
        }
        let elapsed = now.saturating_duration_since(self.updated).as_nanos();
        let earned = earned_tokens(elapsed, rate);
        if earned == 0 {
            return;
        }
        self.tokens = self.tokens.saturating_add(earned).min(self.capacity);
        self.updated += payment_time(earned, rate);
        if self.tokens >= self.capacity {
            self.tokens = self.capacity;
            self.updated = now;
        }
    }
}

/// One bandwidth ceiling. Clones are not made: a limiter is shared through
/// an `Arc`, so a rate change reaches every download using it.
#[derive(Debug)]
pub struct RateLimiter {
    bucket: Mutex<Bucket>,
    changed: Notify,
}

/// A chunk may claim available credit before the rest has accrued. Return
/// that credit if its wait is cancelled or the future is dropped.
struct PendingClaim<'a> {
    limiter: &'a RateLimiter,
    bytes: u64,
}

impl Drop for PendingClaim<'_> {
    fn drop(&mut self) {
        if self.bytes == 0 {
            return;
        }
        {
            let mut bucket = self.limiter.lock();
            bucket.tokens = bucket
                .tokens
                .saturating_add(self.bytes)
                .min(bucket.capacity);
        }
        self.limiter.changed.notify_waiters();
    }
}

impl RateLimiter {
    /// A limiter for `rate_bps` bytes per second; `None` or `0` is unlimited.
    pub fn new(rate_bps: Option<u64>) -> Self {
        Self {
            bucket: Mutex::new(Bucket::new(rate_bps.filter(|rate| *rate > 0))),
            changed: Notify::new(),
        }
    }

    /// The rate this limiter currently enforces.
    pub fn rate_bps(&self) -> Option<u64> {
        self.lock().rate_bps
    }

    /// Changes the rate of a live limiter. Downloads waiting for tokens wake
    /// up and recompute against the new value, so a rising limit applies at
    /// once rather than after the old wait finishes.
    pub fn set_rate(&self, rate_bps: Option<u64>) {
        let rate_bps = rate_bps.filter(|rate| *rate > 0);
        {
            let mut bucket = self.lock();
            if bucket.rate_bps == rate_bps {
                return;
            }
            bucket.rate_bps = rate_bps;
            bucket.capacity = rate_bps.map(burst_bytes).unwrap_or(MIN_BURST_BYTES);
            bucket.tokens = bucket.tokens.min(bucket.capacity);
            bucket.updated = Instant::now();
        }
        self.changed.notify_waiters();
    }

    fn lock(&self) -> MutexGuard<'_, Bucket> {
        self.bucket
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Pays for `bytes`, asking `keep_going` between wait slices. Returns
    /// false the moment the callback says stop, so the caller can end the
    /// attempt without waiting for permission it no longer wants.
    pub async fn acquire_while<F>(&self, bytes: u64, mut keep_going: F) -> bool
    where
        F: FnMut() -> bool,
    {
        let mut remaining = bytes;
        let mut claim = PendingClaim {
            limiter: self,
            bytes: 0,
        };
        while remaining > 0 {
            if !keep_going() {
                return false;
            }
            let wait = {
                let mut bucket = self.lock();
                let Some(rate) = bucket.rate_bps else {
                    claim.bytes = 0;
                    return true;
                };
                bucket.refill(Instant::now(), rate);
                // Reserve even a partial refill. Under contention, another
                // waiter cannot take those bytes and force this one to sleep
                // for a whole chunk again on a coarse timer.
                let paid = remaining.min(bucket.tokens);
                bucket.tokens -= paid;
                remaining -= paid;
                claim.bytes += paid;
                if remaining == 0 {
                    claim.bytes = 0;
                    return true;
                }
                payment_time(remaining.min(bucket.capacity), rate).min(MAX_WAIT_SLICE)
            };
            tokio::select! {
                _ = sleep(wait) => {}
                _ = self.changed.notified() => {}
            }
        }
        true
    }
}

/// Every ceiling that applies to one download, paid in order.
#[derive(Debug, Clone, Default)]
pub struct RateLimits {
    buckets: Vec<Arc<RateLimiter>>,
}

impl RateLimits {
    /// No ceiling at all: every acquisition returns without waiting.
    pub fn none() -> Self {
        Self {
            buckets: Vec::new(),
        }
    }

    /// The global ceiling (if the host supplied one) followed by the
    /// download's own (if it has one).
    pub fn compose(global: Option<Arc<RateLimiter>>, task_rate_bps: Option<u64>) -> Self {
        let mut buckets = Vec::with_capacity(2);
        if let Some(global) = global {
            buckets.push(global);
        }
        if let Some(rate) = task_rate_bps.filter(|rate| *rate > 0) {
            buckets.push(Arc::new(RateLimiter::new(Some(rate))));
        }
        Self { buckets }
    }

    /// Whether any ceiling applies.
    pub fn is_limited(&self) -> bool {
        !self.buckets.is_empty()
    }

    /// Pays every ceiling for `bytes`, stopping as soon as `keep_going`
    /// reports that the transfer is no longer wanted.
    pub async fn acquire_while<F>(&self, bytes: u64, mut keep_going: F) -> bool
    where
        F: FnMut() -> bool,
    {
        for bucket in &self.buckets {
            if !bucket.acquire_while(bytes, &mut keep_going).await {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::task::{Context, Waker};

    #[test]
    fn burst_is_a_quarter_second_with_a_floor() {
        assert_eq!(burst_bytes(4 * 1024 * 1024), 1024 * 1024);
        assert_eq!(burst_bytes(1024), MIN_BURST_BYTES);
        assert_eq!(burst_bytes(64 * 1024), MIN_BURST_BYTES);
    }

    #[test]
    fn earned_tokens_follow_the_rate() {
        assert_eq!(earned_tokens(1_000_000_000, 1024), 1024);
        assert_eq!(earned_tokens(500_000_000, 1024), 512);
        assert_eq!(earned_tokens(1, 1024), 0, "sub-byte time earns nothing");
    }

    #[test]
    fn payment_time_inverts_earning() {
        assert_eq!(payment_time(1024, 1024), Duration::from_secs(1));
        assert_eq!(payment_time(512, 1024), Duration::from_millis(500));
        assert!(payment_time(1, u64::MAX) < Duration::from_nanos(10));
    }

    #[test]
    fn zero_and_none_both_mean_unlimited() {
        assert!(
            RateLimiter::new(None)
                .bucket
                .lock()
                .unwrap()
                .rate_bps
                .is_none()
        );
        assert!(
            RateLimiter::new(Some(0))
                .bucket
                .lock()
                .unwrap()
                .rate_bps
                .is_none()
        );
        assert_eq!(
            RateLimiter::new(Some(2048)).bucket.lock().unwrap().rate_bps,
            Some(2048)
        );
    }

    #[tokio::test]
    async fn unlimited_limits_never_wait() {
        let limits = RateLimits::none();
        assert!(!limits.is_limited());
        let start = Instant::now();
        assert!(limits.acquire_while(8 * 1024 * 1024, || true).await);
        assert!(start.elapsed() < Duration::from_millis(50));
    }

    #[tokio::test]
    async fn an_unlimited_limiter_pays_immediately() {
        let limiter = RateLimiter::new(None);
        let start = Instant::now();
        assert!(limiter.acquire_while(64 * 1024 * 1024, || true).await);
        assert!(start.elapsed() < Duration::from_millis(50));
    }

    #[tokio::test]
    async fn composition_keeps_the_tighter_ceiling() {
        let limits = RateLimits::compose(Some(Arc::new(RateLimiter::new(Some(4096)))), Some(1024));
        assert!(limits.is_limited());
        assert_eq!(limits.buckets.len(), 2);
        assert_eq!(limits.buckets[0].rate_bps(), Some(4096));
        assert_eq!(limits.buckets[1].rate_bps(), Some(1024));
    }

    #[test]
    fn a_rate_change_reaches_a_live_limiter() {
        let limiter = RateLimiter::new(Some(1024));
        limiter.set_rate(Some(2048));
        assert_eq!(limiter.rate_bps(), Some(2048));
        limiter.set_rate(None);
        assert_eq!(limiter.rate_bps(), None);
    }

    #[tokio::test]
    async fn a_stop_request_ends_a_wait() {
        let limiter = RateLimiter::new(Some(1024));
        let asked = AtomicUsize::new(0);
        let finished = limiter
            .acquire_while(1024 * 1024, || asked.fetch_add(1, Ordering::Relaxed) < 2)
            .await;
        assert!(!finished, "the callback asked to stop");
        // fetch_add reports the previous count: the callback answers continue
        // twice, and the third call is the one that says stop.
        assert_eq!(asked.load(Ordering::Relaxed), 3);
        assert_eq!(limiter.lock().tokens, MIN_BURST_BYTES);
    }

    #[tokio::test(start_paused = true)]
    async fn a_dropped_partial_claim_refunds_its_credit() {
        let limiter = RateLimiter::new(Some(64 * 1024));
        assert!(limiter.acquire_while(12 * 1024, || true).await);
        assert_eq!(limiter.lock().tokens, 4 * 1024);

        let mut waiting = Box::pin(limiter.acquire_while(8 * 1024, || true));
        let mut context = Context::from_waker(Waker::noop());
        assert!(waiting.as_mut().poll(&mut context).is_pending());
        assert_eq!(limiter.lock().tokens, 0, "the waiter claimed 4 KiB");

        drop(waiting);
        assert_eq!(limiter.lock().tokens, 4 * 1024, "unused credit returned");
        assert!(limiter.acquire_while(4 * 1024, || true).await);
    }
}
