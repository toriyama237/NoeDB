//! Monotonic timestamp oracle (Phase 2 Week 7).

use std::sync::atomic::{AtomicU64, Ordering};

use super::CommitTs;

/// Assigns strictly increasing commit timestamps (HLC-style local oracle).
#[derive(Debug, Default)]
pub struct TimestampOracle {
    last: AtomicU64,
}

impl TimestampOracle {
    /// Fresh oracle starting at 1.
    #[must_use]
    pub fn new() -> Self {
        Self {
            last: AtomicU64::new(0),
        }
    }

    /// Next commit timestamp via lock-free `fetch_add` (~1 ns).
    pub fn next(&self) -> CommitTs {
        self.last
            .fetch_add(1, Ordering::SeqCst)
            .saturating_add(1)
            .max(1)
    }

    /// Raw TSO bump (alias for benchmarks).
    pub fn fetch_add(&self, n: u64, order: Ordering) -> u64 {
        self.last.fetch_add(n, order)
    }

    /// Current high-water mark (may lag behind concurrent `next()`).
    #[must_use]
    pub fn now(&self) -> CommitTs {
        self.last.load(Ordering::SeqCst)
    }
}
