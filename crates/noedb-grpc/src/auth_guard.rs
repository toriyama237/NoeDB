//! Brute-force protection for cluster auth on gRPC and TCP.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

const DEFAULT_MAX_FAILURES: u32 = 8;
const DEFAULT_WINDOW: Duration = Duration::from_secs(60);
const DEFAULT_BAN: Duration = Duration::from_secs(300);

#[derive(Debug, Clone)]
struct Entry {
    failures: u32,
    first: Instant,
    banned_until: Option<Instant>,
}

/// Sliding-window auth failure tracker keyed by client IP.
#[derive(Debug)]
pub struct AuthGuard {
    max_failures: u32,
    window: Duration,
    ban: Duration,
    entries: Mutex<HashMap<String, Entry>>,
}

impl Default for AuthGuard {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_FAILURES, DEFAULT_WINDOW, DEFAULT_BAN)
    }
}

impl AuthGuard {
    /// Build with explicit thresholds.
    #[must_use]
    pub fn new(max_failures: u32, window: Duration, ban: Duration) -> Self {
        Self {
            max_failures,
            window,
            ban,
            entries: Mutex::new(HashMap::new()),
        }
    }

    fn key(addr: Option<SocketAddr>) -> String {
        addr.map(|a| a.ip().to_string())
            .unwrap_or_else(|| "unknown".into())
    }

    /// Returns true when the peer is temporarily banned.
    #[must_use]
    pub fn is_banned(&self, addr: Option<SocketAddr>) -> bool {
        let key = Self::key(addr);
        let mut map = self.entries.lock();
        let Some(entry) = map.get(&key) else {
            return false;
        };
        if let Some(until) = entry.banned_until {
            if Instant::now() < until {
                return true;
            }
            map.remove(&key);
        }
        false
    }

    /// Record a failed auth attempt; returns true if ban was triggered.
    pub fn record_failure(&self, addr: Option<SocketAddr>) -> bool {
        let key = Self::key(addr);
        let now = Instant::now();
        let mut map = self.entries.lock();
        let entry = map.entry(key).or_insert(Entry {
            failures: 0,
            first: now,
            banned_until: None,
        });
        if now.duration_since(entry.first) > self.window {
            *entry = Entry {
                failures: 0,
                first: now,
                banned_until: None,
            };
        }
        entry.failures += 1;
        if entry.failures >= self.max_failures {
            entry.banned_until = Some(now + self.ban);
            return true;
        }
        false
    }

    /// Clear failure state after successful authentication.
    pub fn record_success(&self, addr: Option<SocketAddr>) {
        self.entries.lock().remove(&Self::key(addr));
    }

    /// Reset all peers (unit tests — global guard state).
    pub fn clear(&self) {
        self.entries.lock().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn bans_after_repeated_failures() {
        let guard = AuthGuard::new(3, Duration::from_secs(60), Duration::from_secs(10));
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1234);
        assert!(!guard.record_failure(Some(addr)));
        assert!(!guard.record_failure(Some(addr)));
        assert!(guard.record_failure(Some(addr)));
        assert!(guard.is_banned(Some(addr)));
        guard.record_success(Some(addr));
        assert!(!guard.is_banned(Some(addr)));
    }
}
