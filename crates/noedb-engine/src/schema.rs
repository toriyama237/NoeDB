//! Schema versioning for transactional DDL (Phase 2 Week 12).

use std::collections::BTreeMap;

use noedb_ast::SqlType;
use noedb_planner::QuerySchema;
use noedb_storage::CommitTs;

/// One column in the catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnMeta {
    /// Column name.
    pub name: String,
    /// Declared SQL type.
    pub data_type: SqlType,
    /// `NOT NULL` constraint.
    pub not_null: bool,
    /// Column-level `PRIMARY KEY`.
    pub primary_key: bool,
}

/// Catalog entry for a table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableSchema {
    /// Table name.
    pub name: String,
    /// Columns in DDL order.
    pub columns: Vec<ColumnMeta>,
    /// Primary key column names (table-level or per-column).
    pub primary_key: Vec<String>,
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
    pub fn create_table(
        &mut self,
        name: impl Into<String>,
        version_ts: CommitTs,
        columns: Vec<ColumnMeta>,
        primary_key: Vec<String>,
    ) {
        let name = name.into();
        self.epoch = self.epoch.max(version_ts);
        self.tables.insert(
            name.clone(),
            TableSchema {
                name,
                columns,
                primary_key,
                version_ts,
            },
        );
    }

    /// Column names for a registered table.
    #[must_use]
    pub fn column_names(&self, table: &str) -> Option<Vec<String>> {
        self.tables
            .get(table)
            .map(|t| t.columns.iter().map(|c| c.name.clone()).collect())
    }

    /// Column names slice helper (allocates).
    #[must_use]
    pub fn columns_for(&self, table: &str) -> Option<Vec<String>> {
        self.column_names(table)
    }

    /// Column metadata lookup.
    #[must_use]
    pub fn column(&self, table: &str, column: &str) -> Option<&ColumnMeta> {
        self.tables
            .get(table)
            .and_then(|t| t.columns.iter().find(|c| c.name == column))
    }

    /// Snapshot for the query planner (`SELECT *` expansion).
    #[must_use]
    pub fn query_schema(&self) -> QuerySchema {
        QuerySchema::from_tables(self.tables.iter().map(|(name, t)| {
            (
                name.clone(),
                t.columns.iter().map(|c| c.name.clone()).collect(),
            )
        }))
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
