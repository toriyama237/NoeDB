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

    /// Next commit timestamp (never repeats, never decreases).
    pub fn next(&self) -> CommitTs {
        loop {
            let cur = self.last.load(Ordering::Acquire);
            let next = cur.saturating_add(1).max(1);
            if self
                .last
                .compare_exchange(cur, next, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return next;
            }
        }
    }

    /// Current high-water mark (may lag behind concurrent `next()`).
    #[must_use]
    pub fn now(&self) -> CommitTs {
        self.last.load(Ordering::Acquire)
    }
}
