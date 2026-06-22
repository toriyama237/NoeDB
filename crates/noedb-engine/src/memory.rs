//! Process-wide memory budget — OOM prevention before the Linux killer strikes.

use std::sync::atomic::{AtomicUsize, Ordering};

use noedb_storage::StorageError;

use crate::error::EngineError;

/// Default soft cap (~70% of an 8 GiB LXC).
pub const DEFAULT_MEM_BUDGET_BYTES: usize = 6 * 1024 * 1024 * 1024;

/// Tracks estimated in-process memory pressure and rejects new work when over budget.
#[derive(Debug)]
pub struct MemoryBudget {
    limit: usize,
    reserved: AtomicUsize,
}

impl MemoryBudget {
    /// Build with an explicit byte limit.
    #[must_use]
    pub const fn new(limit: usize) -> Self {
        Self {
            limit,
            reserved: AtomicUsize::new(0),
        }
    }

    /// Limit from `NOEDB_MEM_BUDGET_BYTES` or [`DEFAULT_MEM_BUDGET_BYTES`].
    #[must_use]
    pub fn from_env() -> Self {
        let limit = std::env::var("NOEDB_MEM_BUDGET_BYTES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_MEM_BUDGET_BYTES);
        Self::new(limit)
    }

    /// Configured limit in bytes.
    #[must_use]
    pub const fn limit(&self) -> usize {
        self.limit
    }

    /// Currently reserved bytes (estimate).
    #[must_use]
    pub fn reserved(&self) -> usize {
        self.reserved.load(Ordering::Acquire)
    }

    /// Fail when LSM memtable pressure plus reservation would exceed the budget.
    pub fn gate_write(&self, memtable_bytes: usize, extra: usize) -> Result<(), EngineError> {
        let need = memtable_bytes.saturating_add(extra);
        if need > self.limit {
            return Err(EngineError::Storage(StorageError::resource_exhausted(
                "memory budget exceeded — flush or retry later",
            )));
        }
        Ok(())
    }

    /// Reserve `bytes` for the duration of a query (released when the returned
    /// [`MemoryGuard`] is dropped).
    pub fn try_reserve(&self, bytes: usize) -> Result<MemoryGuard<'_>, EngineError> {
        loop {
            let cur = self.reserved.load(Ordering::Acquire);
            if cur.saturating_add(bytes) > self.limit {
                return Err(EngineError::Storage(StorageError::resource_exhausted(
                    "memory reservation failed",
                )));
            }
            if self
                .reserved
                .compare_exchange(cur, cur + bytes, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return Ok(MemoryGuard {
                    budget: self,
                    bytes,
                });
            }
        }
    }

    pub(crate) fn release(&self, bytes: usize) {
        self.reserved.fetch_sub(bytes, Ordering::AcqRel);
    }
}

/// RAII release of a memory reservation.
pub struct MemoryGuard<'a> {
    budget: &'a MemoryBudget,
    bytes: usize,
}

impl Drop for MemoryGuard<'_> {
    fn drop(&mut self) {
        self.budget.release(self.bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reservation_fails_when_over_limit() {
        let b = MemoryBudget::new(100);
        let _g = b.try_reserve(80).unwrap();
        assert!(b.try_reserve(30).is_err());
    }
}
