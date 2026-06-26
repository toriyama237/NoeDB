//! Cost-based optimizer (Weeks 24–27).

use noedb_ast::{BinaryOp, ColumnRef, Expr, Literal};
use noedb_storage::LsmTree;

use crate::cost::{estimate, index_beats_seq_scan, PlanStats};
use crate::index::SecondaryIndex;
use crate::join::{extract_equi_join, orient_join_keys};
use crate::logical::AggFunc;
use crate::logical::LogicalPlan;
use crate::physical::PhysicalPlan;

/// Planning context (storage + stats).
pub struct PlanContext<'a> {
    /// LSM store (index catalog).
    pub store: &'a LsmTree,
    /// Cost statistics.
    pub stats: PlanStats,
}

impl<'a> PlanContext<'a> {
    /// Default stats (no catalog).
    #[must_use]
    pub fn new(store: &'a LsmTree) -> Self {
        Self::with_stats(store, PlanStats::default())
    }

    /// Planner context with explicit statistics.
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn with_stats(store: &'a LsmTree, stats: PlanStats) -> Self {
        Self { store, stats }
    }
}

/// Optimize logical plan → physical plan.
#[must_use]
pub fn optimize(logical: LogicalPlan, ctx: &PlanContext<'_>) -> PhysicalPlan {
    let logical = pushdown_predicates(logical);
    let physical = to_physical(logical, ctx);
    pushdown_columns(physical)
}

fn pushdown_predicates(plan: LogicalPlan) -> LogicalPlan {
    match plan {
        LogicalPlan::Filter { input, predicate } => match *input {
            // Only fuse the `WHERE` equi-predicate into an INNER join's `ON`;
            // doing so on a LEFT join would drop NULL-extended rows and change
            // its semantics.
            LogicalPlan::Join {
                left,
                right,
                on: _,
                left_outer,
            } if !left_outer && extract_equi_join(&predicate).is_some() => {
                pushdown_predicates(LogicalPlan::Join {
                    left: Box::new(pushdown_predicates(*left)),
                    right: Box::new(pushdown_predicates(*right)),
                    on: predicate,
                    left_outer,
                })
            }
            LogicalPlan::Join {
                left,
                right,
                on,
                left_outer,
            } => {
                let left = pushdown_predicates(*left);
                let right = pushdown_predicates(*right);
                let join = LogicalPlan::Join {
                    left: Box::new(left),
                    right: Box::new(right),
                    on,
                    left_outer,
                };
                LogicalPlan::Filter {
                    input: Box::new(join),
                    predicate,
                }
            }
            other => LogicalPlan::Filter {
                input: Box::new(pushdown_predicates(other)),
                predicate,
            },
        },
        LogicalPlan::Project { input, items } => LogicalPlan::Project {
            input: Box::new(pushdown_predicates(*input)),
            items,
        },
        LogicalPlan::Join {
            left,
            right,
            on,
            left_outer,
        } => LogicalPlan::Join {
            left: Box::new(pushdown_predicates(*left)),
            right: Box::new(pushdown_predicates(*right)),
            on,
            left_outer,
        },
        LogicalPlan::Aggregate {
            input,
            group_by,
            aggs,
        } => LogicalPlan::Aggregate {
            input: Box::new(pushdown_predicates(*input)),
            group_by,
            aggs,
        },
        LogicalPlan::Sort { input, keys } => LogicalPlan::Sort {
            input: Box::new(pushdown_predicates(*input)),
            keys,
        },
        LogicalPlan::Limit {
            input,
            limit,
            offset,
        } => LogicalPlan::Limit {
            input: Box::new(pushdown_predicates(*input)),
            limit,
            offset,
        },
        LogicalPlan::Window { input, windows } => LogicalPlan::Window {
            input: Box::new(pushdown_predicates(*input)),
            windows,
        },
        LogicalPlan::SemiJoin {
            left,
            right,
            left_key,
            right_key,
            corr_on,
            negated,
        } => LogicalPlan::SemiJoin {
            left: Box::new(pushdown_predicates(*left)),
            right: Box::new(pushdown_predicates(*right)),
            left_key,
            right_key,
            corr_on,
            negated,
        },
        LogicalPlan::SetOp {
            left,
            right,
            op,
            all,
        } => LogicalPlan::SetOp {
            left: Box::new(pushdown_predicates(*left)),
            right: Box::new(pushdown_predicates(*right)),
            op,
            all,
        },
        LogicalPlan::Distinct { input } => LogicalPlan::Distinct {
            input: Box::new(pushdown_predicates(*input)),
        },
        LogicalPlan::SubqueryScan { input, alias } => LogicalPlan::SubqueryScan {
            input: Box::new(pushdown_predicates(*input)),
            alias,
        },
        LogicalPlan::CteScan { .. } | LogicalPlan::Scan { .. } => plan,
    }
}

#[allow(clippy::too_many_lines)]
fn to_physical(plan: LogicalPlan, ctx: &PlanContext<'_>) -> PhysicalPlan {
    match plan {
        LogicalPlan::Scan { table, prefix } => PhysicalPlan::SeqScan {
            table,
            prefix,
            columns: None,
        },
        LogicalPlan::Filter { input, predicate } => {
            if let LogicalPlan::Scan { ref table, .. } = *input {
                if let Some((column, key)) = extract_equality_predicate(&predicate) {
                    if SecondaryIndex::exists(ctx.store, table, &column) {
                        return PhysicalPlan::IndexScan {
                            table: table.clone(),
                            column,
                            point_key: Some(key),
                            columns: None,
                        };
                    }
                }
            }
            PhysicalPlan::Filter {
                input: Box::new(to_physical(*input, ctx)),
                predicate,
            }
        }
        LogicalPlan::Project { input, items } => PhysicalPlan::Project {
            input: Box::new(to_physical(*input, ctx)),
            items,
        },
        LogicalPlan::Join {
            left,
            right,
            on,
            left_outer,
        } => {
            let keys = extract_equi_join(&on).map(|k| orient_join_keys(&left, &right, &k));
            let left_p = to_physical(*left, ctx);
            let right_p = to_physical(*right, ctx);
            let hash = PhysicalPlan::HashJoin {
                left: Box::new(left_p.clone()),
                right: Box::new(right_p.clone()),
                on: on.clone(),
                left_key: keys.as_ref().map_or_else(String::new, |k| k.left.clone()),
                right_key: keys.as_ref().map_or_else(String::new, |k| k.right.clone()),
                left_outer,
            };
            // LEFT OUTER semantics are only implemented in the hash-join
            // operator, so keep it rather than the cost-based alternatives.
            if left_outer {
                return hash;
            }
            let nested = PhysicalPlan::NestedLoopJoin {
                left: Box::new(left_p.clone()),
                right: Box::new(right_p.clone()),
                on: on.clone(),
            };
            let merge = PhysicalPlan::MergeJoin {
                left: Box::new(left_p),
                right: Box::new(right_p),
                on,
                left_key: keys.as_ref().map_or_else(String::new, |k| k.left.clone()),
                right_key: keys.as_ref().map_or_else(String::new, |k| k.right.clone()),
            };
            pick_join(hash, nested, merge, ctx)
        }
        LogicalPlan::Aggregate {
            input,
            group_by,
            aggs,
        } => PhysicalPlan::Aggregate {
            input: Box::new(to_physical(*input, ctx)),
            group_by,
            aggs,
        },
        LogicalPlan::Sort { input, keys } => PhysicalPlan::Sort {
            input: Box::new(to_physical(*input, ctx)),
            keys,
        },
        LogicalPlan::Limit {
            input,
            limit,
            offset,
        } => PhysicalPlan::Limit {
            input: Box::new(to_physical(*input, ctx)),
            limit,
            offset,
        },
        LogicalPlan::Window { input, windows } => PhysicalPlan::Window {
            input: Box::new(to_physical(*input, ctx)),
            windows,
        },
        LogicalPlan::SemiJoin {
            left,
            right,
            left_key,
            right_key,
            corr_on,
            negated,
        } => PhysicalPlan::SemiJoin {
            left: Box::new(to_physical(*left, ctx)),
            right: Box::new(to_physical(*right, ctx)),
            left_key,
            right_key,
            corr_on,
            negated,
        },
        LogicalPlan::CteScan { name, prefix } => PhysicalPlan::CteScan {
            name,
            prefix,
            columns: None,
        },
        LogicalPlan::SubqueryScan { input, alias } => PhysicalPlan::SubqueryScan {
            input: Box::new(to_physical(*input, ctx)),
            prefix: alias,
            columns: None,
        },
        LogicalPlan::SetOp {
            left,
            right,
            op,
            all,
        } => PhysicalPlan::SetOp {
            left: Box::new(to_physical(*left, ctx)),
            right: Box::new(to_physical(*right, ctx)),
            op,
            all,
        },
        LogicalPlan::Distinct { input } => PhysicalPlan::Dedup {
            input: Box::new(to_physical(*input, ctx)),
        },
    }
}

fn pick_join(
    hash: PhysicalPlan,
    nested: PhysicalPlan,
    merge: PhysicalPlan,
    ctx: &PlanContext<'_>,
) -> PhysicalPlan {
    let h = estimate(&hash, &ctx.stats);
    let n = estimate(&nested, &ctx.stats);
    let m = estimate(&merge, &ctx.stats);
    if h <= n && h <= m {
        hash
    } else if m <= n {
        merge
    } else {
        nested
    }
}

fn pushdown_columns(plan: PhysicalPlan) -> PhysicalPlan {
    let needed = collect_columns(&plan);
    apply_columns(plan, needed)
}

fn collect_columns(plan: &PhysicalPlan) -> Option<Vec<String>> {
    match plan {
        PhysicalPlan::SeqScan { .. }
        | PhysicalPlan::IndexScan { .. }
        | PhysicalPlan::CteScan { .. } => None,
        PhysicalPlan::Filter { input, predicate } => {
            let mut cols = collect_columns(input).unwrap_or_default();
            cols.extend(columns_in_expr(predicate));
            Some(dedup(cols))
        }
        PhysicalPlan::Project { input, items } => {
            let mut cols = collect_columns(input).unwrap_or_default();
            for item in items {
                cols.extend(columns_in_expr(&item.expr));
            }
            let cols = dedup(cols);
            if cols.is_empty() {
                None
            } else {
                Some(cols)
            }
        }
        PhysicalPlan::HashJoin {
            left,
            right,
            left_key,
            right_key,
            ..
        }
        | PhysicalPlan::MergeJoin {
            left,
            right,
            left_key,
            right_key,
            ..
        } => {
            let mut cols = collect_columns(left).unwrap_or_default();
            cols.extend(collect_columns(right).unwrap_or_default());
            cols.push(left_key.clone());
            cols.push(right_key.clone());
            Some(dedup(cols))
        }
        PhysicalPlan::NestedLoopJoin { left, right, on } => {
            let mut cols = collect_columns(left).unwrap_or_default();
            cols.extend(collect_columns(right).unwrap_or_default());
            cols.extend(columns_in_expr(on));
            Some(dedup(cols))
        }
        PhysicalPlan::Aggregate {
            input,
            group_by,
            aggs,
        } => {
            let mut cols = collect_columns(input).unwrap_or_default();
            cols.extend(group_by.clone());
            for (_, func) in aggs {
                match func {
                    AggFunc::CountCol(c)
                    | AggFunc::Sum(c)
                    | AggFunc::Avg(c)
                    | AggFunc::Min(c)
                    | AggFunc::Max(c) => {
                        cols.push(c.clone());
                    }
                    AggFunc::CountStar => {}
                }
            }
            let cols = dedup(cols);
            if cols.is_empty() {
                None
            } else {
                Some(cols)
            }
        }
        PhysicalPlan::Sort { input, keys } => {
            let mut cols = collect_columns(input).unwrap_or_default();
            cols.extend(keys.iter().map(|(k, _)| k.clone()));
            Some(dedup(cols))
        }
        PhysicalPlan::Limit { input, .. } => collect_columns(input),
        PhysicalPlan::Window { input, windows } => {
            let mut cols = collect_columns(input).unwrap_or_default();
            for win in windows {
                if let Some(arg) = &win.arg {
                    cols.extend(columns_in_expr(arg));
                }
                for expr in &win.spec.partition_by {
                    cols.extend(columns_in_expr(expr));
                }
                for key in &win.spec.order_by {
                    cols.extend(columns_in_expr(&key.expr));
                }
            }
            Some(dedup(cols))
        }
        PhysicalPlan::SemiJoin {
            left,
            right,
            left_key,
            right_key,
            corr_on,
            ..
        } => {
            let mut cols = collect_columns(left).unwrap_or_default();
            // The inner subquery still needs the columns referenced in its own
            // WHERE/projection (e.g. a `WHERE balance > …` filter); the shared
            // column set is pushed to every scan, so include them here.
            cols.extend(collect_columns(right).unwrap_or_default());
            cols.push(left_key.clone());
            if let Some(pred) = corr_on {
                cols.extend(columns_in_expr(pred));
            }
            cols.push(right_key.clone());
            Some(dedup(cols))
        }
        PhysicalPlan::SetOp { left, right, .. } => {
            let mut cols = collect_columns(left).unwrap_or_default();
            cols.extend(collect_columns(right).unwrap_or_default());
            Some(dedup(cols))
        }
        PhysicalPlan::Dedup { input } => collect_columns(input),
        PhysicalPlan::SubqueryScan { input, .. } => collect_columns(input),
    }
}

#[allow(clippy::too_many_lines)]
fn apply_columns(plan: PhysicalPlan, columns: Option<Vec<String>>) -> PhysicalPlan {
    match plan {
        PhysicalPlan::SeqScan { table, prefix, .. } => PhysicalPlan::SeqScan {
            table,
            prefix,
            columns,
        },
        PhysicalPlan::IndexScan {
            table,
            column,
            point_key,
            ..
        } => PhysicalPlan::IndexScan {
            table,
            column,
            point_key,
            columns,
        },
        PhysicalPlan::Filter { input, predicate } => PhysicalPlan::Filter {
            input: Box::new(apply_columns(*input, columns)),
            predicate,
        },
        PhysicalPlan::Project { input, items } => PhysicalPlan::Project {
            input: Box::new(apply_columns(*input, columns)),
            items,
        },
        PhysicalPlan::HashJoin {
            left,
            right,
            on,
            left_key,
            right_key,
            left_outer,
        } => PhysicalPlan::HashJoin {
            left: Box::new(apply_columns(*left, columns.clone())),
            right: Box::new(apply_columns(*right, columns)),
            on,
            left_key,
            right_key,
            left_outer,
        },
        PhysicalPlan::NestedLoopJoin { left, right, on } => PhysicalPlan::NestedLoopJoin {
            left: Box::new(apply_columns(*left, columns.clone())),
            right: Box::new(apply_columns(*right, columns)),
            on,
        },
        PhysicalPlan::MergeJoin {
            left,
            right,
            on,
            left_key,
            right_key,
        } => PhysicalPlan::MergeJoin {
            left: Box::new(apply_columns(*left, columns.clone())),
            right: Box::new(apply_columns(*right, columns)),
            on,
            left_key,
            right_key,
        },
        PhysicalPlan::Aggregate {
            input,
            group_by,
            aggs,
        } => PhysicalPlan::Aggregate {
            input: Box::new(apply_columns(*input, columns)),
            group_by,
            aggs,
        },
        PhysicalPlan::Sort { input, keys } => PhysicalPlan::Sort {
            input: Box::new(apply_columns(*input, columns)),
            keys,
        },
        PhysicalPlan::Limit {
            input,
            limit,
            offset,
        } => PhysicalPlan::Limit {
            input: Box::new(apply_columns(*input, columns)),
            limit,
            offset,
        },
        PhysicalPlan::Window { input, windows } => PhysicalPlan::Window {
            input: Box::new(apply_columns(*input, columns)),
            windows,
        },
        PhysicalPlan::SemiJoin {
            left,
            right,
            left_key,
            right_key,
            corr_on,
            negated,
        } => PhysicalPlan::SemiJoin {
            left: Box::new(apply_columns(*left, columns.clone())),
            right: Box::new(apply_columns(*right, columns)),
            left_key,
            right_key,
            corr_on,
            negated,
        },
        PhysicalPlan::CteScan { name, prefix, .. } => PhysicalPlan::CteScan {
            name,
            prefix,
            columns,
        },
        PhysicalPlan::SubqueryScan { input, prefix, .. } => PhysicalPlan::SubqueryScan {
            input: Box::new(apply_columns(*input, columns.clone())),
            prefix,
            columns,
        },
        PhysicalPlan::SetOp {
            left,
            right,
            op,
            all,
        } => PhysicalPlan::SetOp {
            left: Box::new(apply_columns(*left, columns.clone())),
            right: Box::new(apply_columns(*right, columns)),
            op,
            all,
        },
        PhysicalPlan::Dedup { input } => PhysicalPlan::Dedup {
            input: Box::new(apply_columns(*input, columns)),
        },
    }
}

fn extract_equality_predicate(expr: &Expr) -> Option<(String, Vec<u8>)> {
    match expr {
        Expr::Binary {
            op: BinaryOp::Eq,
            left,
            right,
            ..
        } => {
            if let (Some(col), Some(val)) = (column_name(left), literal_bytes(right)) {
                return Some((col, val));
            }
            if let (Some(col), Some(val)) = (column_name(right), literal_bytes(left)) {
                return Some((col, val));
            }
        }
        Expr::Paren(inner, _) => return extract_equality_predicate(inner),
        _ => {}
    }
    None
}

fn column_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Column(ColumnRef::Named { column, .. }) => Some(column.value.clone()),
        _ => None,
    }
}

fn literal_bytes(expr: &Expr) -> Option<Vec<u8>> {
    match expr {
        Expr::Literal(lit) => Some(match lit {
            Literal::Null { .. } => Vec::new(),
            Literal::Integer(n, _) => format!("{n}").into_bytes(),
            Literal::Float(f, _) => format!("{f}").into_bytes(),
            Literal::String(s, _) => s.as_bytes().to_vec(),
            Literal::Boolean(b, _) => vec![u8::from(*b)],
        }),
        _ => None,
    }
}

fn columns_in_expr(expr: &Expr) -> Vec<String> {
    match expr {
        Expr::Column(ColumnRef::Named {
            table: Some(t),
            column,
        }) => vec![format!("{}.{}", t.value, column.value)],
        Expr::Column(ColumnRef::Named { column, .. }) => vec![column.value.clone()],
        Expr::Unary { expr, .. } | Expr::IsNull { expr, .. } | Expr::Cast { expr, .. } => {
            columns_in_expr(expr)
        }
        Expr::Binary { left, right, .. } => {
            let mut v = columns_in_expr(left);
            v.extend(columns_in_expr(right));
            v
        }
        Expr::Paren(inner, _) => columns_in_expr(inner),
        Expr::Function { args, .. } => args.iter().flat_map(columns_in_expr).collect(),
        _ => Vec::new(),
    }
}

fn dedup(mut cols: Vec<String>) -> Vec<String> {
    cols.sort();
    cols.dedup();
    cols
}

/// Compare estimated cost of seq scan + filter vs index scan for a table/column.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn index_wins(table_rows: u64, column: &str) -> bool {
    use std::collections::HashMap;

    use crate::stats::{ColumnStats, TableStats};

    let stats = PlanStats {
        default_rows: table_rows,
        tables: HashMap::from([(
            "t".into(),
            TableStats {
                row_count: table_rows,
                columns: HashMap::from([(column.to_string(), ColumnStats { ndv: table_rows })]),
            },
        )]),
    };
    index_beats_seq_scan("t", column, &stats)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::index::SecondaryIndex;
    use noedb_ast::{BinaryOp, ColumnRef, Expr, Ident, Literal};
    use noedb_lexer::Span;
    use noedb_storage::{LsmConfig, LsmTree};

    #[test]
    fn index_wins_at_scale() {
        assert!(index_wins(10_000, "id"));
        assert!(!index_wins(2, "id"));
    }

    #[test]
    fn optimize_uses_index_when_present() {
        let dir = std::env::temp_dir().join(format!(
            "noedb-opt-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut tree = LsmTree::open(&dir, LsmConfig::default()).unwrap();
        let mut key = b"users".to_vec();
        key.push(0);
        key.extend_from_slice(b"1");
        key.push(0);
        key.extend_from_slice(b"id");
        tree.put(&key, b"42").unwrap();
        SecondaryIndex::build(&mut tree, "users", "id").unwrap();

        let pred = Expr::Binary {
            op: BinaryOp::Eq,
            left: Box::new(Expr::Column(ColumnRef::Named {
                table: None,
                column: Ident::new("id".into(), Span::new(0, 0)),
            })),
            right: Box::new(Expr::Literal(Literal::String("42".into(), Span::new(0, 0)))),
            span: Span::new(0, 0),
        };
        let logical = LogicalPlan::Filter {
            input: Box::new(LogicalPlan::Scan {
                table: "users".into(),
                prefix: "users".into(),
            }),
            predicate: pred,
        };
        let ctx = PlanContext::new(&tree);
        let physical = optimize(logical, &ctx);
        assert!(matches!(physical, PhysicalPlan::IndexScan { .. }));
        let _ = std::fs::remove_dir_all(dir);
    }
}
