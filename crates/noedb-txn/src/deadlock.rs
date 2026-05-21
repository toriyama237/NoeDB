//! Wait-for graph deadlock detection + timeout (Week 11).

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use crate::{Transaction, TxnError, TxnId};

/// Deadlock resolution policy.
#[derive(Debug, Clone, Copy)]
pub struct DeadlockPolicy {
    /// Background cycle check interval.
    pub check_interval: Duration,
    /// Abort txn waiting longer than this.
    pub wait_timeout: Duration,
}

impl Default for DeadlockPolicy {
    fn default() -> Self {
        Self {
            check_interval: Duration::from_millis(100),
            wait_timeout: Duration::from_secs(5),
        }
    }
}

/// Tracks lock waits and detects cycles (DFS).
#[derive(Debug, Default)]
pub struct DeadlockGuard {
    /// `waiter -> holder` edges.
    waits: BTreeMap<TxnId, TxnId>,
    policy: DeadlockPolicy,
    last_check: Option<Instant>,
}

impl DeadlockGuard {
    /// Create with default 100ms / 5s policy.
    #[must_use]
    pub fn new() -> Self {
        Self {
            waits: BTreeMap::new(),
            policy: DeadlockPolicy::default(),
            last_check: None,
        }
    }

    /// Record `waiter` blocked on `holder`.
    pub fn record_wait(&mut self, waiter: TxnId, holder: TxnId) {
        self.waits.insert(waiter, holder);
    }

    /// Clear waits for `txn_id`.
    pub fn clear(&mut self, txn_id: TxnId) {
        self.waits.remove(&txn_id);
        self.waits.retain(|_, h| *h != txn_id);
    }

    /// Enforce wait timeout for `txn_id` and periodic global cycle detection.
    pub fn poll(
        &mut self,
        txns: &BTreeMap<TxnId, Transaction>,
        committing: TxnId,
    ) -> Result<(), TxnError> {
        let now = Instant::now();
        if let Some(txn) = txns.get(&committing) {
            if now.duration_since(txn.started_at) > self.policy.wait_timeout {
                return Err(TxnError::WaitTimeout);
            }
        }

        let should_check = self
            .last_check
            .is_none_or(|t| now.duration_since(t) >= self.policy.check_interval);
        if should_check {
            self.last_check = Some(now);
            if let Some(victim) = self.find_cycle_victim() {
                self.clear(victim);
                return Err(TxnError::DeadlockVictim);
            }
        }
        Ok(())
    }

    fn find_cycle_victim(&self) -> Option<TxnId> {
        for &start in self.waits.keys() {
            let mut stack = vec![start];
            let mut visited = BTreeSet::new();
            while let Some(cur) = stack.pop() {
                if !visited.insert(cur) {
                    continue;
                }
                if let Some(&next) = self.waits.get(&cur) {
                    if next == start {
                        return Some(start);
                    }
                    stack.push(next);
                }
            }
        }
        None
    }
}
