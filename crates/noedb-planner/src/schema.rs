//! Lightweight schema view for planning (`SELECT *` expansion).

use std::collections::HashMap;

/// Column names per base table (from the engine catalog).
#[derive(Debug, Clone, Default)]
pub struct QuerySchema {
    tables: HashMap<String, Vec<String>>,
}

impl QuerySchema {
    /// Empty catalog.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Build from `(table_name, column_names)` pairs.
    #[must_use]
    pub fn from_tables(tables: impl IntoIterator<Item = (String, Vec<String>)>) -> Self {
        Self {
            tables: tables.into_iter().collect(),
        }
    }

    /// Column names for a table, if registered.
    #[must_use]
    pub fn columns_for(&self, table: &str) -> Option<&[String]> {
        self.tables.get(table).map(Vec::as_slice)
    }
}
