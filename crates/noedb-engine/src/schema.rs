//! Schema versioning for transactional DDL (Phase 2 Week 12).

use std::collections::BTreeMap;

use noedb_storage::CommitTs;

/// Catalog entry for a table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableSchema {
    /// Table name.
    pub name: String,
    /// Schema version at creation / last DDL.
    pub version_ts: CommitTs,
}

/// In-memory schema catalog (persisted via engine on COMMIT in future).
#[derive(Debug, Default)]
pub struct SchemaCatalog {
    /// Monotonic schema epoch.
    pub epoch: CommitTs,
    /// Tables by name.
    pub tables: BTreeMap<String, TableSchema>,
}

impl SchemaCatalog {
    /// Register `CREATE TABLE` at `version_ts`.
    pub fn create_table(&mut self, name: impl Into<String>, version_ts: CommitTs) {
        let name = name.into();
        self.epoch = self.epoch.max(version_ts);
        self.tables.insert(
            name.clone(),
            TableSchema {
                name,
                version_ts,
            },
        );
    }

    /// Roll back a table created in the current txn (not yet committed to LSM).
    pub fn rollback_create(&mut self, name: &str) {
        self.tables.remove(name);
    }

    /// Current catalog version for query-cache invalidation.
    #[must_use]
    pub fn version(&self) -> CommitTs {
        self.epoch
    }
}
