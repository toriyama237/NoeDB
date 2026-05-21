//! Logical → physical lowering (Week 17).

use crate::logical::LogicalPlan;
use crate::physical::PhysicalPlan;

/// Naive lowering: one-to-one mapping until the cost-based optimizer lands (Week 24).
#[must_use]
pub fn lower(logical: LogicalPlan) -> PhysicalPlan {
    match logical {
        LogicalPlan::Scan { table } => PhysicalPlan::SeqScan { table },
        LogicalPlan::Filter { input, predicate } => PhysicalPlan::Filter {
            input: Box::new(lower(*input)),
            predicate,
        },
        LogicalPlan::Project { input, items } => PhysicalPlan::Project {
            input: Box::new(lower(*input)),
            items,
        },
        LogicalPlan::Join { left, right, on } => PhysicalPlan::HashJoin {
            left: Box::new(lower(*left)),
            right: Box::new(lower(*right)),
            on,
        },
    }
}
