//! Logical → physical lowering (Week 17).

use crate::logical::LogicalPlan;
use crate::physical::PhysicalPlan;

/// Naive lowering: one-to-one mapping (superseded by [`crate::optimize::optimize`]).
#[must_use]
pub fn lower(logical: LogicalPlan) -> PhysicalPlan {
    match logical {
        LogicalPlan::Scan { table, prefix } => PhysicalPlan::SeqScan {
            table,
            prefix,
            columns: None,
        },
        LogicalPlan::Filter { input, predicate } => PhysicalPlan::Filter {
            input: Box::new(lower(*input)),
            predicate,
        },
        LogicalPlan::Project { input, items } => PhysicalPlan::Project {
            input: Box::new(lower(*input)),
            items,
        },
        LogicalPlan::Join {
            left,
            right,
            on,
            left_outer,
        } => {
            use crate::join::{extract_equi_join, orient_join_keys};
            let keys = extract_equi_join(&on).map(|k| orient_join_keys(&left, &right, &k));
            PhysicalPlan::HashJoin {
                left: Box::new(lower(*left)),
                right: Box::new(lower(*right)),
                on,
                left_key: keys.as_ref().map_or_else(String::new, |k| k.left.clone()),
                right_key: keys.as_ref().map_or_else(String::new, |k| k.right.clone()),
                left_outer,
            }
        }
        LogicalPlan::Aggregate {
            input,
            group_by,
            aggs,
        } => PhysicalPlan::Aggregate {
            input: Box::new(lower(*input)),
            group_by,
            aggs,
        },
        LogicalPlan::Sort { input, keys } => PhysicalPlan::Sort {
            input: Box::new(lower(*input)),
            keys,
            top_k: None,
        },
        LogicalPlan::Limit {
            input,
            limit,
            offset,
        } => PhysicalPlan::Limit {
            input: Box::new(lower(*input)),
            limit,
            offset,
        },
        LogicalPlan::Window { input, windows } => PhysicalPlan::Window {
            input: Box::new(lower(*input)),
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
            left: Box::new(lower(*left)),
            right: Box::new(lower(*right)),
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
            input: Box::new(lower(*input)),
            prefix: alias,
            columns: None,
        },
        LogicalPlan::SetOp {
            left,
            right,
            op,
            all,
        } => PhysicalPlan::SetOp {
            left: Box::new(lower(*left)),
            right: Box::new(lower(*right)),
            op,
            all,
        },
        LogicalPlan::Distinct { input } => PhysicalPlan::Dedup {
            input: Box::new(lower(*input)),
        },
    }
}
