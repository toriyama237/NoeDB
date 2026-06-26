//! Per-session token-bucket rate limiting and statement deadlines.
//!
//! Two independent DoS guards:
//! - [`RateLimiter`] caps the sustained statement rate per session with a
//!   short burst allowance (classic token bucket).
//! - [`Deadline`] bounds wall-clock execution time so a single pathological
//!   query cannot pin a worker forever.

use std::time::{Duration, Instant};

use dashmap::DashMap;

use crate::error::EngineError;

/// Default sustained statements/second per session.
pub const DEFAULT_QPS: f64 = 5_000.0;
/// Default burst bucket capacity (statements).
pub const DEFAULT_BURST: f64 = 10_000.0;

#[derive(Debug)]
struct Bucket {
    tokens: f64,
    last: Instant,
}

/// Token-bucket rate limiter keyed by session id.
#[derive(Debug)]
pub struct RateLimiter {
    qps: f64,
    burst: f64,
    buckets: DashMap<u64, Bucket>,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new(DEFAULT_QPS, DEFAULT_BURST)
    }
}

impl RateLimiter {
    /// Build with explicit sustained rate and burst capacity.
    #[must_use]
    pub fn new(qps: f64, burst: f64) -> Self {
        Self {
            qps: qps.max(1.0),
            burst: burst.max(1.0),
            buckets: DashMap::new(),
        }
    }

    /// Build from `NOEDB_QPS` / `NOEDB_BURST` env (falls back to defaults).
    #[must_use]
    pub fn from_env() -> Self {
        let qps = std::env::var("NOEDB_QPS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_QPS);
        let burst = std::env::var("NOEDB_BURST")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_BURST);
        Self::new(qps, burst)
    }

    /// Try to consume one token for `session_id`.
    ///
    /// # Errors
    ///
    /// [`EngineError::RateLimited`] when the bucket is empty.
    pub fn acquire(&self, session_id: u64) -> Result<(), EngineError> {
        let now = Instant::now();
        let mut bucket = self.buckets.entry(session_id).or_insert(Bucket {
            tokens: self.burst,
            last: now,
        });
        let elapsed = now.duration_since(bucket.last).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * self.qps).min(self.burst);
        bucket.last = now;
        if bucket.tokens < 1.0 {
            return Err(EngineError::RateLimited("session statement rate exceeded"));
        }
        bucket.tokens -= 1.0;
        Ok(())
    }

    /// Drop tracking state for a closed session.
    pub fn forget(&self, session_id: u64) {
        self.buckets.remove(&session_id);
    }
}

/// Wall-clock execution deadline for a single statement.
#[derive(Debug, Clone, Copy)]
pub struct Deadline {
    start: Instant,
    limit: Duration,
}

impl Deadline {
    /// Start a deadline that fires after `limit_ms` milliseconds.
    /// A `limit_ms` of `0` disables the deadline.
    #[must_use]
    pub fn new(limit_ms: u64) -> Self {
        Self {
            start: Instant::now(),
            limit: Duration::from_millis(limit_ms),
        }
    }

    /// Deadline from `NOEDB_STMT_TIMEOUT_MS` (0 = disabled).
    #[must_use]
    pub fn from_env() -> Self {
        let ms = std::env::var("NOEDB_STMT_TIMEOUT_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        Self::new(ms)
    }

    /// Whether the deadline is active (non-zero limit).
    #[must_use]
    pub fn is_active(&self) -> bool {
        !self.limit.is_zero()
    }

    /// Return an error if the deadline has elapsed.
    ///
    /// # Errors
    ///
    /// [`EngineError::Timeout`] once `limit` has passed.
    pub fn check(&self) -> Result<(), EngineError> {
        if self.is_active() && self.start.elapsed() >= self.limit {
            return Err(EngineError::Timeout(self.limit.as_millis() as u64));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limiter_blocks_after_burst_drained() {
        let rl = RateLimiter::new(1.0, 3.0);
        assert!(rl.acquire(7).is_ok());
        assert!(rl.acquire(7).is_ok());
        assert!(rl.acquire(7).is_ok());
        assert!(rl.acquire(7).is_err());
    }

    #[test]
    fn limiter_refills_over_time() {
        let rl = RateLimiter::new(1000.0, 1.0);
        assert!(rl.acquire(1).is_ok());
        assert!(rl.acquire(1).is_err());
        std::thread::sleep(Duration::from_millis(15));
        assert!(rl.acquire(1).is_ok());
    }

    #[test]
    fn sessions_are_isolated() {
        let rl = RateLimiter::new(1.0, 1.0);
        assert!(rl.acquire(1).is_ok());
        assert!(rl.acquire(2).is_ok());
        assert!(rl.acquire(1).is_err());
    }

    #[test]
    fn deadline_disabled_never_fires() {
        let d = Deadline::new(0);
        assert!(!d.is_active());
        assert!(d.check().is_ok());
    }

    #[test]
    fn deadline_fires_after_limit() {
        let d = Deadline::new(1);
        std::thread::sleep(Duration::from_millis(5));
        assert!(d.check().is_err());
    }
}
