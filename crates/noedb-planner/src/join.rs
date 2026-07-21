//! Join predicate helpers (Weeks 21–22).

use std::collections::HashSet;

use noedb_ast::{BinaryOp, ColumnRef, Expr};

use crate::logical::LogicalPlan;

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

/// Map ON-clause columns to the join input that owns each side.
#[must_use]
pub fn orient_join_keys(
    left: &LogicalPlan,
    right: &LogicalPlan,
    keys: &EquiJoinKeys,
) -> EquiJoinKeys {
    let left_aliases: HashSet<_> = table_aliases(left).into_iter().collect();
    let right_aliases: HashSet<_> = table_aliases(right).into_iter().collect();

    let a_on_left = column_table_alias(&keys.left).is_some_and(|a| left_aliases.contains(a));
    let b_on_left = column_table_alias(&keys.right).is_some_and(|a| left_aliases.contains(a));
    let a_on_right = column_table_alias(&keys.left).is_some_and(|a| right_aliases.contains(a));
    let b_on_right = column_table_alias(&keys.right).is_some_and(|a| right_aliases.contains(a));

    if a_on_left && b_on_right {
        keys.clone()
    } else if b_on_left && a_on_right {
        EquiJoinKeys {
            left: keys.right.clone(),
            right: keys.left.clone(),
        }
    } else {
        keys.clone()
    }
}

fn table_aliases(plan: &LogicalPlan) -> Vec<String> {
    match plan {
        LogicalPlan::Scan { prefix, .. } | LogicalPlan::CteScan { prefix, .. } => {
            if prefix.is_empty() {
                vec![]
            } else {
                vec![prefix.clone()]
            }
        }
        LogicalPlan::SubqueryScan { alias, input, .. } => {
            let mut aliases = table_aliases(input);
            aliases.push(alias.clone());
            aliases
        }
        LogicalPlan::Join { left, right, .. } | LogicalPlan::SetOp { left, right, .. } => {
            let mut aliases = table_aliases(left);
            aliases.extend(table_aliases(right));
            aliases
        }
        LogicalPlan::Filter { input, .. }
        | LogicalPlan::Project { input, .. }
        | LogicalPlan::Aggregate { input, .. }
        | LogicalPlan::Sort { input, .. }
        | LogicalPlan::Limit { input, .. }
        | LogicalPlan::Window { input, .. }
        | LogicalPlan::Distinct { input, .. } => table_aliases(input),
        LogicalPlan::SemiJoin { left, .. } => table_aliases(left),
    }
}

fn column_table_alias(col: &str) -> Option<&str> {
    col.rsplit_once('.').map(|(table, _)| table)
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

/// Match a stored row field name against a referenced column (exact, bare, or qualified).
#[must_use]
pub fn column_name_matches(stored: &str, wanted: &str) -> bool {
    if stored == wanted {
        return true;
    }
    if stored
        .rsplit_once('.')
        .is_some_and(|(_, bare)| bare == wanted)
    {
        return true;
    }
    if let Some((_, bare)) = wanted.rsplit_once('.') {
        if stored == bare {
            return true;
        }
        if stored.rsplit_once('.').is_some_and(|(_, sb)| sb == bare) {
            return true;
        }
    }
    false
}

/// Join key value from a row.
#[must_use]
pub fn join_key_value(row: &[(String, crate::value::Value)], column: &str) -> Option<Vec<u8>> {
    row.iter()
        .find(|(n, _)| column_name_matches(n, column))
        .map(|(_, v)| v.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logical::LogicalPlan;

    #[test]
    fn orient_swaps_when_on_clause_reversed() {
        let left = LogicalPlan::Scan {
            table: "agencies".into(),
            prefix: "ag".into(),
        };
        let right = LogicalPlan::Scan {
            table: "employees".into(),
            prefix: "e".into(),
        };
        let keys = EquiJoinKeys {
            left: "e.agency_id".into(),
            right: "ag.id".into(),
        };
        let oriented = orient_join_keys(&left, &right, &keys);
        assert_eq!(oriented.left, "ag.id");
        assert_eq!(oriented.right, "e.agency_id");
    }

    #[test]
    fn orient_keeps_natural_order() {
        let left = LogicalPlan::Scan {
            table: "users".into(),
            prefix: "u".into(),
        };
        let right = LogicalPlan::Scan {
            table: "orders".into(),
            prefix: "o".into(),
        };
        let keys = EquiJoinKeys {
            left: "u.id".into(),
            right: "o.user_id".into(),
        };
        let oriented = orient_join_keys(&left, &right, &keys);
        assert_eq!(oriented.left, "u.id");
        assert_eq!(oriented.right, "o.user_id");
    }

    #[test]
    fn join_key_value_matches_qualified_column() {
        use crate::value::Value;
        let row = vec![("e.id".into(), Value::Bytes(b"1".to_vec()))];
        assert_eq!(join_key_value(&row, "id"), Some(b"1".to_vec()));
    }
}
