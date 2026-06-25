//! Schema versioning for transactional DDL (Phase 2 Week 12).

use std::collections::BTreeMap;
use std::path::Path;

use noedb_ast::SqlType;
use noedb_planner::QuerySchema;
use noedb_storage::CommitTs;
use serde::{Deserialize, Serialize};

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
    /// Column-level `UNIQUE` constraint.
    pub unique: bool,
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

/// On-disk catalog snapshot (`<data_dir>/schema.catalog.json`).
#[derive(Debug, Serialize, Deserialize)]
struct SchemaSnapshot {
    epoch: CommitTs,
    tables: Vec<PersistedTable>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedTable {
    name: String,
    columns: Vec<PersistedColumn>,
    primary_key: Vec<String>,
    version_ts: CommitTs,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedColumn {
    name: String,
    data_type: String,
    vector_dim: Option<u32>,
    not_null: bool,
    primary_key: bool,
    #[serde(default)]
    unique: bool,
}

const SCHEMA_FILE: &str = "schema.catalog.json";

/// In-memory schema catalog persisted under the engine data directory.
#[derive(Debug, Default)]
pub struct SchemaCatalog {
    /// Monotonic schema epoch.
    pub epoch: CommitTs,
    /// Tables by name.
    pub tables: BTreeMap<String, TableSchema>,
}

impl SchemaCatalog {
    /// Load catalog from `data_dir`, or return empty when no snapshot exists.
    ///
    /// # Errors
    ///
    /// Corrupt or unreadable snapshot files.
    pub fn load(data_dir: &Path) -> Result<Self, String> {
        let path = data_dir.join(SCHEMA_FILE);
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let snap: SchemaSnapshot = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
        Ok(snap.into())
    }

    /// Atomically persist catalog to `data_dir`.
    ///
    /// # Errors
    ///
    /// I/O or serialization failures.
    pub fn save(&self, data_dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
        let path = data_dir.join(SCHEMA_FILE);
        let snap: SchemaSnapshot = self.into();
        let json = serde_json::to_string_pretty(&snap).map_err(|e| e.to_string())?;
        noedb_storage::atomic_write(&path, json.as_bytes()).map_err(|e| e.to_string())
    }

    /// Whether `name` is already registered.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.tables.contains_key(name)
    }

    /// Register `CREATE TABLE` at `version_ts`.
    ///
    /// # Errors
    ///
    /// `SchemaError::TableExists` when the table name is already taken.
    pub fn create_table(
        &mut self,
        name: impl Into<String>,
        version_ts: CommitTs,
        columns: Vec<ColumnMeta>,
        primary_key: Vec<String>,
    ) -> Result<(), SchemaError> {
        let name = name.into();
        if self.tables.contains_key(&name) {
            return Err(SchemaError::TableExists(name));
        }
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
        Ok(())
    }

    /// Remove a table from the catalog.
    pub fn drop_table(&mut self, name: &str) -> Result<(), SchemaError> {
        self.tables
            .remove(name)
            .ok_or_else(|| SchemaError::UnknownTable(name.to_string()))?;
        Ok(())
    }

    /// Append a column to an existing table (`ALTER TABLE … ADD COLUMN`).
    ///
    /// # Errors
    ///
    /// `SchemaError::UnknownTable` when the table is missing, or
    /// `SchemaError::ColumnExists` when the column name is already present.
    pub fn add_column(
        &mut self,
        table: &str,
        column: ColumnMeta,
        version_ts: CommitTs,
    ) -> Result<(), SchemaError> {
        let entry = self
            .tables
            .get_mut(table)
            .ok_or_else(|| SchemaError::UnknownTable(table.to_string()))?;
        if entry.columns.iter().any(|c| c.name == column.name) {
            return Err(SchemaError::ColumnExists(column.name));
        }
        if column.primary_key && !entry.primary_key.contains(&column.name) {
            entry.primary_key.push(column.name.clone());
        }
        entry.columns.push(column);
        entry.version_ts = version_ts;
        self.epoch = self.epoch.max(version_ts);
        Ok(())
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

/// Schema catalog errors.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SchemaError {
    /// Table already exists.
    #[error("table already exists: {0}")]
    TableExists(String),
    /// Unknown table.
    #[error("unknown table: {0}")]
    UnknownTable(String),
    /// Column already exists on the table.
    #[error("column already exists: {0}")]
    ColumnExists(String),
}

impl From<&SchemaCatalog> for SchemaSnapshot {
    fn from(cat: &SchemaCatalog) -> Self {
        Self {
            epoch: cat.epoch,
            tables: cat
                .tables
                .values()
                .map(|t| PersistedTable {
                    name: t.name.clone(),
                    columns: t
                        .columns
                        .iter()
                        .map(|c| PersistedColumn {
                            name: c.name.clone(),
                            data_type: sql_type_tag(&c.data_type),
                            vector_dim: match c.data_type {
                                SqlType::Vector { dim } => Some(dim),
                                _ => None,
                            },
                            not_null: c.not_null,
                            primary_key: c.primary_key,
                            unique: c.unique,
                        })
                        .collect(),
                    primary_key: t.primary_key.clone(),
                    version_ts: t.version_ts,
                })
                .collect(),
        }
    }
}

impl From<SchemaSnapshot> for SchemaCatalog {
    fn from(snap: SchemaSnapshot) -> Self {
        let mut tables = BTreeMap::new();
        for t in snap.tables {
            let columns = t
                .columns
                .into_iter()
                .map(|c| ColumnMeta {
                    name: c.name,
                    data_type: decode_sql_type(&c.data_type, c.vector_dim),
                    not_null: c.not_null,
                    primary_key: c.primary_key,
                    unique: c.unique,
                })
                .collect();
            tables.insert(
                t.name.clone(),
                TableSchema {
                    name: t.name,
                    columns,
                    primary_key: t.primary_key,
                    version_ts: t.version_ts,
                },
            );
        }
        Self {
            epoch: snap.epoch,
            tables,
        }
    }
}

fn sql_type_tag(ty: &SqlType) -> String {
    match ty {
        SqlType::Int => "INT".into(),
        SqlType::Boolean => "BOOL".into(),
        SqlType::Float => "FLOAT".into(),
        SqlType::Varchar { .. } => "VARCHAR".into(),
        SqlType::Text => "TEXT".into(),
        SqlType::Date => "DATE".into(),
        SqlType::Timestamp => "TIMESTAMP".into(),
        SqlType::Vector { .. } => "VECTOR".into(),
        SqlType::Named(s) => s.clone(),
    }
}

fn decode_sql_type(tag: &str, vector_dim: Option<u32>) -> SqlType {
    match tag {
        "INT" | "INTEGER" => SqlType::Int,
        "BOOL" | "BOOLEAN" => SqlType::Boolean,
        "FLOAT" | "REAL" | "DOUBLE" => SqlType::Float,
        "TEXT" => SqlType::Text,
        "DATE" => SqlType::Date,
        "TIMESTAMP" => SqlType::Timestamp,
        "VECTOR" => SqlType::Vector {
            dim: vector_dim.unwrap_or(0),
        },
        "VARCHAR" => SqlType::Varchar { max_len: None },
        other => SqlType::Named(other.to_string()),
    }
}
