//! Logical → physical lowering (Week 17).

use crate::logical::LogicalPlan;
use crate::physical::PhysicalPlan;

/// Naive lowering: one-to-one mapping (superseded by [`crate::optimize::optimize`]).
#[must_use]
pub fn lower(logical: LogicalPlan) -> PhysicalPlan {
    match logical {
        LogicalPlan::Scan { table } => PhysicalPlan::SeqScan {
            table,
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
        LogicalPlan::Join { left, right, on } => {
            use crate::join::extract_equi_join;
            let keys = extract_equi_join(&on);
            PhysicalPlan::HashJoin {
                left: Box::new(lower(*left)),
                right: Box::new(lower(*right)),
                on,
                left_key: keys.as_ref().map_or_else(String::new, |k| k.left.clone()),
                right_key: keys.as_ref().map_or_else(String::new, |k| k.right.clone()),
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
    }
}
