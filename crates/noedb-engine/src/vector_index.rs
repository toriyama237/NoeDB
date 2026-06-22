//! In-memory HNSW indexes keyed by `(table, column)`.

use std::collections::BTreeMap;

use noedb_ast::SqlType;
use noedb_planner::Value;
use noedb_storage::HnswIndex;

use crate::dml::vector_from_bytes;

/// Process-local vector indexes for `VECTOR(n)` columns.
#[derive(Debug, Default)]
pub struct VectorIndexCatalog {
    indexes: BTreeMap<(String, String), HnswIndex>,
}

impl VectorIndexCatalog {
    /// Register an empty index when a `VECTOR` column is created.
    pub fn register_column(&mut self, table: &str, column: &str, dim: u32) {
        self.indexes.insert(
            (table.to_string(), column.to_string()),
            HnswIndex::new(dim as usize),
        );
    }

    /// Drop all indexes for a table.
    pub fn drop_table(&mut self, table: &str) {
        self.indexes
            .retain(|(t, _), _| !t.eq_ignore_ascii_case(table));
    }

    /// Upsert one row vector after DML.
    pub fn upsert(&mut self, table: &str, column: &str, row_id: &str, value: &Value) {
        let key = (table.to_string(), column.to_string());
        let Some(index) = self.indexes.get_mut(&key) else {
            return;
        };
        let Some(vec) = value_as_f32s(value) else {
            return;
        };
        index.insert(row_id, &vec);
    }

    /// Remove one row from an index.
    pub fn remove(&mut self, table: &str, column: &str, row_id: &str) {
        if let Some(index) = self
            .indexes
            .get_mut(&(table.to_string(), column.to_string()))
        {
            index.remove(row_id);
        }
    }

    /// Top-k row ids by L2 distance (used by K-NN fast path / tests).
    #[must_use]
    pub fn search(&self, table: &str, column: &str, query: &[f32], k: usize) -> Vec<(String, f32)> {
        self.indexes
            .get(&(table.to_string(), column.to_string()))
            .map_or_else(Vec::new, |idx| idx.search(query, k))
    }

    /// Register indexes for all `VECTOR` columns in a table schema.
    pub fn register_table_schema(&mut self, table: &str, columns: &[crate::schema::ColumnMeta]) {
        for col in columns {
            if let SqlType::Vector { dim } = col.data_type {
                self.register_column(table, &col.name, dim);
            }
        }
    }
}

fn value_as_f32s(value: &Value) -> Option<Vec<f32>> {
    match value {
        Value::Vector(v) => Some(v.clone()),
        Value::Bytes(b) => vector_from_bytes(b).or_else(|| parse_vector_text_bytes(b)),
        _ => None,
    }
}

fn parse_vector_text_bytes(bytes: &[u8]) -> Option<Vec<f32>> {
    let s = std::str::from_utf8(bytes).ok()?;
    let s = s.trim().trim_start_matches('[').trim_end_matches(']');
    if s.is_empty() {
        return Some(Vec::new());
    }
    s.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| part.parse().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_search_matches_brute_force_order() {
        let mut cat = VectorIndexCatalog::default();
        cat.register_column("docs", "emb", 3);
        cat.upsert("docs", "emb", "1", &Value::Vector(vec![0.0, 0.0, 0.0]));
        cat.upsert("docs", "emb", "2", &Value::Vector(vec![1.0, 0.0, 0.0]));
        let top = cat.search("docs", "emb", &[0.0, 0.0, 0.0], 2);
        assert_eq!(top[0].0, "1");
        assert_eq!(top[1].0, "2");
    }
}
