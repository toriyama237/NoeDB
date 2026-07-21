//! LRU cache of parsed statements keyed by SQL text (v2.2).
//!
//! Parsing is pure, so a repeated query (dashboards, prepared workloads,
//! OLTP hot paths) can reuse its AST instead of re-lexing and re-parsing.

use std::collections::HashMap;

use noedb_ast::Statement;

/// Default number of cached statements.
pub const DEFAULT_STMT_CACHE_CAPACITY: usize = 256;

/// LRU statement cache: SQL text → parsed AST.
#[derive(Debug)]
pub struct StatementCache {
    capacity: usize,
    clock: u64,
    entries: HashMap<String, (Statement, u64)>,
    hits: u64,
    misses: u64,
}

impl Default for StatementCache {
    fn default() -> Self {
        Self::with_capacity(DEFAULT_STMT_CACHE_CAPACITY)
    }
}

impl StatementCache {
    /// Cache holding at most `capacity` statements (`0` disables caching).
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity,
            clock: 0,
            entries: HashMap::new(),
            hits: 0,
            misses: 0,
        }
    }

    /// Cached AST for `sql`, refreshing its recency.
    pub fn get(&mut self, sql: &str) -> Option<Statement> {
        self.clock += 1;
        let clock = self.clock;
        match self.entries.get_mut(sql) {
            Some((stmt, used)) => {
                *used = clock;
                self.hits += 1;
                Some(stmt.clone())
            }
            None => {
                self.misses += 1;
                None
            }
        }
    }

    /// Insert a freshly parsed statement, evicting the least recently used
    /// entry when full.
    pub fn insert(&mut self, sql: &str, stmt: Statement) {
        if self.capacity == 0 {
            return;
        }
        self.clock += 1;
        if self.entries.len() >= self.capacity && !self.entries.contains_key(sql) {
            if let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, (_, used))| *used)
                .map(|(k, _)| k.clone())
            {
                self.entries.remove(&oldest);
            }
        }
        self.entries.insert(sql.to_string(), (stmt, self.clock));
    }

    /// `(hits, misses)` counters since startup.
    #[must_use]
    pub fn stats(&self) -> (u64, u64) {
        (self.hits, self.misses)
    }

    /// Number of cached statements.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn parse(sql: &str) -> Statement {
        noedb_parser::parse(sql).unwrap()
    }

    #[test]
    fn hit_returns_equal_ast() {
        let mut cache = StatementCache::default();
        let sql = "SELECT id FROM users WHERE id = 1";
        assert!(cache.get(sql).is_none());
        cache.insert(sql, parse(sql));
        let cached = cache.get(sql).unwrap();
        assert_eq!(format!("{cached:?}"), format!("{:?}", parse(sql)));
        assert_eq!(cache.stats(), (1, 1));
    }

    #[test]
    fn evicts_least_recently_used() {
        let mut cache = StatementCache::with_capacity(2);
        cache.insert("SELECT 1", parse("SELECT 1"));
        cache.insert("SELECT 2", parse("SELECT 2"));
        // Touch "SELECT 1" so "SELECT 2" becomes the LRU victim.
        let _ = cache.get("SELECT 1");
        cache.insert("SELECT 3", parse("SELECT 3"));
        assert_eq!(cache.len(), 2);
        assert!(cache.get("SELECT 1").is_some());
        assert!(cache.get("SELECT 2").is_none());
        assert!(cache.get("SELECT 3").is_some());
    }

    #[test]
    fn zero_capacity_disables_cache() {
        let mut cache = StatementCache::with_capacity(0);
        cache.insert("SELECT 1", parse("SELECT 1"));
        assert!(cache.is_empty());
        assert!(cache.get("SELECT 1").is_none());
    }
}
