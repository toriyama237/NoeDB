//! Pool tuning knobs.

use std::time::Duration;

/// Connection pool limits and health-check interval.
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Target idle connections kept warm.
    pub min_idle: usize,
    /// Hard cap on concurrent open channels.
    pub max_open: usize,
    /// Re-`Ping` a connection idle longer than this.
    pub health_interval: Duration,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            min_idle: 2,
            max_open: 64,
            health_interval: Duration::from_secs(30),
        }
    }
}

impl PoolConfig {
    /// Throughput-oriented preset (Week 50 load test).
    #[must_use]
    pub fn high_concurrency() -> Self {
        Self {
            min_idle: 8,
            max_open: 1024,
            health_interval: Duration::from_secs(60),
        }
    }
}
