//! Physical operators (Week 19–20).

use noedb_ast::{Expr, SelectItem};

/// Executable physical plan.
#[derive(Debug, Clone, PartialEq)]
pub enum PhysicalPlan {
    /// Sequential scan over a table prefix in storage.
    SeqScan {
        /// Table name.
        table: String,
    },
    /// Predicate filter.
    Filter {
        /// Child operator.
        input: Box<Self>,
        /// Boolean expression.
        predicate: Expr,
    },
    /// Column projection.
    Project {
        /// Child operator.
        input: Box<Self>,
        /// Projection items.
        items: Vec<SelectItem>,
    },
    /// Hash join (Week 21+).
    HashJoin {
        /// Left child.
        left: Box<Self>,
        /// Right child.
        right: Box<Self>,
        /// Join predicate.
        on: Expr,
    },
}
