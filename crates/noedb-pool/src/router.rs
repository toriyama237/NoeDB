//! Read/write endpoint selection.

use std::sync::atomic::{AtomicUsize, Ordering};

/// Route hint for the load balancer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// Send to a read replica (round-robin).
    Read,
    /// Send to the leader / writer endpoint.
    Write,
}

/// Round-robin over read replicas; writes always hit the leader address.
#[derive(Debug)]
pub struct LoadBalancer {
    leader: String,
    replicas: Vec<String>,
    read_rr: AtomicUsize,
}

impl LoadBalancer {
    /// `leader` is the write target; `replicas` may be empty (reads use leader).
    #[must_use]
    pub fn new(leader: impl Into<String>, replicas: Vec<String>) -> Self {
        Self {
            leader: leader.into(),
            replicas,
            read_rr: AtomicUsize::new(0),
        }
    }

    /// Pick endpoint for `route`.
    #[must_use]
    pub fn pick(&self, route: Route) -> &str {
        match route {
            Route::Write => &self.leader,
            Route::Read => {
                if self.replicas.is_empty() {
                    &self.leader
                } else {
                    let i = self.read_rr.fetch_add(1, Ordering::Relaxed);
                    &self.replicas[i % self.replicas.len()]
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_round_robin() {
        let lb = LoadBalancer::new("leader:1", vec!["r1:1".into(), "r2:1".into()]);
        assert_eq!(lb.pick(Route::Write), "leader:1");
        assert_eq!(lb.pick(Route::Read), "r1:1");
        assert_eq!(lb.pick(Route::Read), "r2:1");
        assert_eq!(lb.pick(Route::Read), "r1:1");
    }
}
