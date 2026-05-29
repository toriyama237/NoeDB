//! DML execution (`INSERT`) for the local engine.

use noedb_ast::InsertStmt;
use noedb_planner::{eval_expr, ExecError, Value};
use noedb_storage::LsmTree;

use crate::error::EngineError;
use crate::machine::row_key;
use crate::schema::SchemaCatalog;

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
        schema
            .columns_for(table)
            .map(<[String]>::to_vec)
            .ok_or_else(|| EngineError::Exec(ExecError::UnknownColumn {
                name: format!("table `{table}` not in schema — CREATE TABLE first"),
            }))?
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
        for (col, val) in col_names.iter().zip(values) {
            let bytes = value_to_bytes(&val);
            tree.put(&row_key(table, &row_id, col), &bytes)
                .map_err(EngineError::Storage)?;
        }
    }
    Ok(())
}

fn value_to_row_id(v: &Value) -> String {
    match v {
        Value::Null => "null".to_string(),
        Value::Integer(n) => n.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Bytes(b) | Value::Date(b) => String::from_utf8_lossy(b).into_owned(),
        Value::Timestamp(ts) => ts.to_string(),
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use noedb_ast::{Ident, Literal, Span};
    use noedb_storage::LsmConfig;

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
        let mut schema = SchemaCatalog::default();
        schema.create_table("users", 1, vec!["id".into(), "name".into()]);
        let ins = InsertStmt {
            table: Ident {
                value: "users".into(),
                span: Span::new(0, 0),
            },
            columns: None,
            values: vec![vec![
                Expr::Literal(Literal::String("1".into(), Span::new(0, 0))),
                Expr::Literal(Literal::String("Rykiel".into(), Span::new(0, 0))),
            ]],
            span: Span::new(0, 0),
        };
        execute_insert(&ins, &schema, &mut tree).unwrap();
        let name = tree
            .get(&row_key("users", "1", "name"))
            .unwrap()
            .unwrap();
        assert_eq!(name, b"Rykiel");
        let _ = std::fs::remove_dir_all(dir);
    }
}
