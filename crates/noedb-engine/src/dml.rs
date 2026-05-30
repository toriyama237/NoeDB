//! DML execution (`INSERT`, `UPDATE`, `DELETE`) for the local engine.

use std::collections::BTreeMap;

use noedb_ast::{DeleteStmt, InsertStmt, SqlType, UpdateStmt};
use noedb_planner::{eval_expr, eval_predicate, increment_row_count, ExecError, Value};
use noedb_storage::{LsmTree, StorageEngine};

use crate::error::EngineError;
use crate::machine::row_key;
use crate::schema::SchemaCatalog;

type RowMap = Vec<(String, Value)>;

/// Apply `INSERT INTO … VALUES …` to the LSM.
///
/// # Errors
///
/// Unknown table, column mismatch, or storage failures.
pub(crate) fn execute_insert(
    ins: &InsertStmt,
    schema: &SchemaCatalog,
    tree: &mut LsmTree,
) -> Result<(), EngineError> {
    let table = &ins.table.value;
    let col_names: Vec<String> = if let Some(cols) = &ins.columns {
        cols.iter().map(|c| c.value.clone()).collect()
    } else {
        schema.column_names(table).ok_or_else(|| {
            EngineError::Exec(ExecError::UnknownColumn {
                name: format!("table `{table}` not in schema — CREATE TABLE first"),
            })
        })?
    };

    for row_exprs in &ins.values {
        if row_exprs.len() != col_names.len() {
            return Err(EngineError::Exec(ExecError::TypeMismatch {
                message: format!(
                    "INSERT column count mismatch: expected {}, got {}",
                    col_names.len(),
                    row_exprs.len()
                ),
            }));
        }
        let mut values = Vec::with_capacity(col_names.len());
        for expr in row_exprs {
            values.push(eval_expr(expr, &[]).map_err(EngineError::Exec)?);
        }
        let row_id = value_to_row_id(&values[0]);
        enforce_primary_key(table, &col_names, &values, schema, tree)?;
        for (col, val) in col_names.iter().zip(values) {
            if let Some(meta) = schema.column(table, col) {
                if meta.not_null && matches!(val, Value::Null) {
                    return Err(EngineError::Exec(ExecError::TypeMismatch {
                        message: format!("NOT NULL constraint failed for `{table}.{col}`"),
                    }));
                }
                let bytes = encode_for_type(&val, &meta.data_type)?;
                tree.put(&row_key(table, &row_id, col), &bytes)
                    .map_err(EngineError::Storage)?;
            } else {
                let bytes = value_to_bytes(&val);
                tree.put(&row_key(table, &row_id, col), &bytes)
                    .map_err(EngineError::Storage)?;
            }
        }
        increment_row_count(tree, table, 1).map_err(EngineError::Storage)?;
    }
    Ok(())
}

/// Apply `UPDATE … SET … [WHERE …]` via append-only LSM writes.
///
/// # Errors
///
/// Unknown table, constraint violations, or storage failures.
pub(crate) fn execute_update(
    upd: &UpdateStmt,
    schema: &SchemaCatalog,
    tree: &mut LsmTree,
) -> Result<u64, EngineError> {
    let table = &upd.table.value;
    let mut updated = 0u64;
    for (row_id, row) in scan_table(tree, table)? {
        if let Some(pred) = &upd.where_clause {
            if !eval_predicate(pred, &row).map_err(EngineError::Exec)? {
                continue;
            }
        }
        for (col_ident, expr) in &upd.assignments {
            let col = &col_ident.value;
            let val = eval_expr(expr, &row).map_err(EngineError::Exec)?;
            if let Some(meta) = schema.column(table, col) {
                if meta.not_null && matches!(val, Value::Null) {
                    return Err(EngineError::Exec(ExecError::TypeMismatch {
                        message: format!("NOT NULL constraint failed for `{table}.{col}`"),
                    }));
                }
                let bytes = encode_for_type(&val, &meta.data_type)?;
                tree.put(&row_key(table, &row_id, col), &bytes)
                    .map_err(EngineError::Storage)?;
            } else {
                let bytes = value_to_bytes(&val);
                tree.put(&row_key(table, &row_id, col), &bytes)
                    .map_err(EngineError::Storage)?;
            }
        }
        updated += 1;
    }
    Ok(updated)
}

/// Apply `DELETE FROM … [WHERE …]` via LSM tombstone deletes.
///
/// # Errors
///
/// Storage failures.
pub(crate) fn execute_delete(
    del: &DeleteStmt,
    tree: &mut LsmTree,
) -> Result<u64, EngineError> {
    let table = &del.table.value;
    let mut deleted = 0u64;
    for (row_id, row) in scan_table(tree, table)? {
        if let Some(pred) = &del.where_clause {
            if !eval_predicate(pred, &row).map_err(EngineError::Exec)? {
                continue;
            }
        }
        delete_row(tree, table, &row_id)?;
        deleted += 1;
    }
    if deleted > 0 {
        increment_row_count(tree, table, -(i64::try_from(deleted).unwrap_or(i64::MAX)))
            .map_err(EngineError::Storage)?;
    }
    Ok(deleted)
}

fn enforce_primary_key(
    table: &str,
    col_names: &[String],
    values: &[Value],
    schema: &SchemaCatalog,
    tree: &LsmTree,
) -> Result<(), EngineError> {
    let Some(table_schema) = schema.tables.get(table) else {
        return Ok(());
    };
    for pk in &table_schema.primary_key {
        let idx = col_names.iter().position(|c| c == pk).ok_or_else(|| {
            EngineError::Exec(ExecError::TypeMismatch {
                message: format!("PRIMARY KEY column `{pk}` missing from INSERT"),
            })
        })?;
        if matches!(values[idx], Value::Null) {
            return Err(EngineError::Exec(ExecError::TypeMismatch {
                message: format!("NOT NULL constraint failed for PRIMARY KEY `{table}.{pk}`"),
            }));
        }
        let pk_row = value_to_row_id(&values[idx]);
        if tree
            .get(&row_key(table, &pk_row, pk))
            .map_err(EngineError::Storage)?
            .is_some()
        {
            return Err(EngineError::Exec(ExecError::TypeMismatch {
                message: format!("PRIMARY KEY constraint failed: duplicate `{pk}` = `{pk_row}`"),
            }));
        }
    }
    Ok(())
}

fn scan_table(tree: &LsmTree, table: &str) -> Result<Vec<(String, RowMap)>, EngineError> {
    let mut prefix = table.as_bytes().to_vec();
    prefix.push(0);
    let mut grouped: BTreeMap<Vec<u8>, RowMap> = BTreeMap::new();
    for (key, val) in StorageEngine::iter(tree) {
        if !key.starts_with(&prefix) {
            continue;
        }
        let rest = &key[prefix.len()..];
        let Some(pos) = rest.iter().position(|&b| b == 0) else {
            continue;
        };
        let row_id = rest[..pos].to_vec();
        let col = String::from_utf8_lossy(&rest[pos + 1..]).into_owned();
        grouped
            .entry(row_id)
            .or_default()
            .push((col, Value::Bytes(val)));
    }
    Ok(grouped
        .into_iter()
        .map(|(id, row)| (String::from_utf8_lossy(&id).into_owned(), row))
        .collect())
}

fn delete_row(tree: &mut LsmTree, table: &str, row_id: &str) -> Result<(), EngineError> {
    let prefix = row_prefix(table, row_id);
    let keys: Vec<Vec<u8>> = StorageEngine::iter(tree)
        .filter_map(|(key, _)| key.starts_with(&prefix).then_some(key))
        .collect();
    for key in keys {
        tree.delete(&key).map_err(EngineError::Storage)?;
    }
    Ok(())
}

fn row_prefix(table: &str, row_id: &str) -> Vec<u8> {
    let mut key = table.as_bytes().to_vec();
    key.push(0);
    key.extend_from_slice(row_id.as_bytes());
    key.push(0);
    key
}

fn encode_for_type(val: &Value, ty: &SqlType) -> Result<Vec<u8>, EngineError> {
    match ty {
        SqlType::Vector { dim } => encode_vector(val, *dim),
        _ => Ok(value_to_bytes(val)),
    }
}

fn encode_vector(val: &Value, dim: u32) -> Result<Vec<u8>, EngineError> {
    let floats = match val {
        Value::Vector(v) => v.clone(),
        Value::Bytes(b) => parse_vector_text(b, dim)?,
        other => {
            return Err(EngineError::Exec(ExecError::TypeMismatch {
                message: format!("expected VECTOR({dim}) value, got {other:?}"),
            }));
        }
    };
    if floats.len() != dim as usize {
        return Err(EngineError::Exec(ExecError::TypeMismatch {
            message: format!(
                "VECTOR({dim}) dimension mismatch: got {} floats",
                floats.len()
            ),
        }));
    }
    Ok(vector_to_bytes(&floats))
}

/// Magic prefix for VECTOR cells in the LSM (`NDV1` + little-endian `f32` payload).
const VECTOR_MAGIC: &[u8] = b"NDV1";

/// Little-endian `f32` payload for LSM storage.
#[must_use]
pub(crate) fn vector_to_bytes(values: &[f32]) -> Vec<u8> {
    let mut out = VECTOR_MAGIC.to_vec();
    for v in values {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// Decode LSM bytes into `f32` vector when the magic prefix is present.
#[must_use]
pub(crate) fn vector_from_bytes(bytes: &[u8]) -> Option<Vec<f32>> {
    if !bytes.starts_with(VECTOR_MAGIC) {
        return None;
    }
    let payload = &bytes[VECTOR_MAGIC.len()..];
    if !payload.len().is_multiple_of(4) {
        return None;
    }
    Some(
        payload
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect(),
    )
}

fn parse_vector_text(bytes: &[u8], dim: u32) -> Result<Vec<f32>, EngineError> {
    let s = std::str::from_utf8(bytes).map_err(|_| EngineError::Exec(ExecError::TypeMismatch {
        message: "invalid UTF-8 for VECTOR literal".into(),
    }))?;
    let s = s.trim().trim_start_matches('[').trim_end_matches(']');
    let floats: Result<Vec<f32>, _> = s
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| {
            p.parse::<f32>().map_err(|_| EngineError::Exec(ExecError::TypeMismatch {
                message: format!("invalid float `{p}` in VECTOR literal"),
            }))
        })
        .collect();
    let floats = floats?;
    if floats.len() != dim as usize {
        return Err(EngineError::Exec(ExecError::TypeMismatch {
            message: format!("VECTOR({dim}) literal has {} elements", floats.len()),
        }));
    }
    Ok(floats)
}

fn value_to_row_id(v: &Value) -> String {
    match v {
        Value::Null => "null".to_string(),
        Value::Integer(n) => n.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Bytes(b) | Value::Date(b) => String::from_utf8_lossy(b).into_owned(),
        Value::Timestamp(ts) => ts.to_string(),
        Value::Vector(v) => v
            .first()
            .map(|f| f.to_string())
            .unwrap_or_else(|| "0".into()),
    }
}

fn value_to_bytes(v: &Value) -> Vec<u8> {
    match v {
        Value::Null => Vec::new(),
        Value::Integer(n) => n.to_string().into_bytes(),
        Value::Float(f) => f.to_string().into_bytes(),
        Value::Bool(b) => vec![u8::from(*b)],
        Value::Bytes(b) | Value::Date(b) => b.clone(),
        Value::Timestamp(ts) => ts.to_string().into_bytes(),
        Value::Vector(v) => vector_to_bytes(v),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::ColumnMeta;
    use noedb_ast::Statement;
    use noedb_parser::parse;
    use noedb_storage::LsmConfig;

    fn users_schema() -> SchemaCatalog {
        let mut schema = SchemaCatalog::default();
        schema.create_table(
            "users",
            1,
            vec![
                ColumnMeta {
                    name: "id".into(),
                    data_type: SqlType::Text,
                    not_null: true,
                    primary_key: true,
                },
                ColumnMeta {
                    name: "name".into(),
                    data_type: SqlType::Text,
                    not_null: false,
                    primary_key: false,
                },
            ],
            vec!["id".into()],
        );
        schema
    }

    #[test]
    fn insert_values_then_read_cell() {
        let dir = std::env::temp_dir().join(format!(
            "noedb-dml-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut tree = LsmTree::open(&dir, LsmConfig::default()).unwrap();
        let schema = users_schema();
        let insert = parse("INSERT INTO users VALUES ('1', 'Rykiel')").unwrap();
        let Statement::Insert(ins) = insert else {
            panic!("expected INSERT");
        };
        execute_insert(&ins, &schema, &mut tree).unwrap();
        let name = tree
            .get(&row_key("users", "1", "name"))
            .unwrap()
            .unwrap();
        assert_eq!(name, b"Rykiel");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn primary_key_rejects_duplicate() {
        let dir = std::env::temp_dir().join(format!(
            "noedb-pk-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut tree = LsmTree::open(&dir, LsmConfig::default()).unwrap();
        let schema = users_schema();
        let ins = parse("INSERT INTO users VALUES ('1', 'Ada')").unwrap();
        let Statement::Insert(ins) = ins else {
            panic!("expected INSERT");
        };
        execute_insert(&ins, &schema, &mut tree).unwrap();
        let dup = parse("INSERT INTO users VALUES ('1', 'Bob')").unwrap();
        let Statement::Insert(dup) = dup else {
            panic!("expected INSERT");
        };
        assert!(execute_insert(&dup, &schema, &mut tree).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn update_and_delete_rows() {
        let dir = std::env::temp_dir().join(format!(
            "noedb-dml-upd-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut tree = LsmTree::open(&dir, LsmConfig::default()).unwrap();
        let schema = users_schema();
        let ins = parse("INSERT INTO users VALUES ('1', 'Ada')").unwrap();
        let Statement::Insert(ins) = ins else {
            panic!("expected INSERT");
        };
        execute_insert(&ins, &schema, &mut tree).unwrap();
        let upd = parse("UPDATE users SET name = 'Augusta' WHERE id = '1'").unwrap();
        let Statement::Update(upd) = upd else {
            panic!("expected UPDATE");
        };
        assert_eq!(execute_update(&upd, &schema, &mut tree).unwrap(), 1);
        let name = tree.get(&row_key("users", "1", "name")).unwrap().unwrap();
        assert_eq!(name, b"Augusta");
        let del = parse("DELETE FROM users WHERE id = '1'").unwrap();
        let Statement::Delete(del) = del else {
            panic!("expected DELETE");
        };
        assert_eq!(execute_delete(&del, &mut tree).unwrap(), 1);
        assert!(tree.get(&row_key("users", "1", "name")).unwrap().is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn vector_round_trip_bytes() {
        let v = vec![1.0_f32, 2.5, -3.25];
        let bytes = vector_to_bytes(&v);
        assert_eq!(vector_from_bytes(&bytes).unwrap(), v);
    }
}
