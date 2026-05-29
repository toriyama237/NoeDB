//! Join predicate helpers (Weeks 21–22).

use noedb_ast::{BinaryOp, ColumnRef, Expr};

/// Equi-join columns extracted from `left_col = right_col`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EquiJoinKeys {
    /// Left side column name.
    pub left: String,
    /// Right side column name.
    pub right: String,
}

/// Extract equi-join column names from an `ON` clause.
#[must_use]
pub fn extract_equi_join(expr: &Expr) -> Option<EquiJoinKeys> {
    match expr {
        Expr::Binary {
            op: BinaryOp::Eq,
            left,
            right,
            ..
        } => {
            let left_col = column_name(left)?;
            let right_col = column_name(right)?;
            Some(EquiJoinKeys {
                left: left_col,
                right: right_col,
            })
        }
        Expr::Paren(inner, _) => extract_equi_join(inner),
        _ => None,
    }
}

fn column_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Column(ColumnRef::Named {
            table: Some(t),
            column,
        }) => Some(format!("{}.{}", t.value, column.value)),
        Expr::Column(ColumnRef::Named { column, .. }) => Some(column.value.clone()),
        _ => None,
    }
}

/// Join key value from a row.
#[must_use]
pub fn join_key_value(row: &[(String, crate::value::Value)], column: &str) -> Option<Vec<u8>> {
    row.iter()
        .find(|(n, _)| n == column)
        .map(|(_, v)| v.as_bytes())
}
