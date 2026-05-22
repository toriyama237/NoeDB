//! LRU query result cache (Phase 3 Week 21).

use std::num::NonZeroUsize;

use lru::LruCache;
use parking_lot::Mutex;

use crate::engine::QueryResult;

/// Default cache capacity (entries).
const DEFAULT_CAPACITY: usize = 4096;

/// Key: schema epoch + session role + SQL text.
type CacheKey = (u64, String, String);

/// Thread-safe LRU cache for read-only query results.
#[derive(Debug)]
pub struct QueryCache {
    inner: Mutex<LruCache<CacheKey, QueryResult>>,
    /// Bumped on writes to invalidate logically related entries.
    epoch: Mutex<u64>,
}

impl Default for QueryCache {
    fn default() -> Self {
        Self::new(DEFAULT_CAPACITY)
    }
}

impl QueryCache {
    /// New LRU with `capacity` entries.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let cap = NonZeroUsize::new(capacity.max(1)).unwrap_or(NonZeroUsize::MIN);
        Self {
            inner: Mutex::new(LruCache::new(cap)),
            epoch: Mutex::new(0),
        }
    }

    /// Current schema epoch (invalidate by bumping).
    #[must_use]
    pub fn epoch(&self) -> u64 {
        *self.epoch.lock()
    }

    /// Invalidate all entries touching `table` (bump epoch).
    pub fn invalidate_table(&self, _table: &str) {
        *self.epoch.lock() += 1;
        self.inner.lock().clear();
    }

    /// Lookup cached result for `role` + `sql`.
    #[must_use]
    pub fn get(&self, role: &str, sql: &str) -> Option<QueryResult> {
        let key = (self.epoch(), role.to_string(), sql.to_string());
        self.inner.lock().get(&key).cloned()
    }

    /// Store result for `role` + `sql`.
    pub fn put(&self, role: &str, sql: &str, result: QueryResult) {
        let key = (self.epoch(), role.to_string(), sql.to_string());
        self.inner.lock().put(key, result);
    }
}
