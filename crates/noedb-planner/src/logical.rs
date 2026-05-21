//! Logical relational algebra (Week 17).

use noedb_ast::{Expr, SelectItem};

/// Logical plan tree before optimization / lowering.
#[derive(Debug, Clone, PartialEq)]
pub enum LogicalPlan {
    /// Full table scan.
    Scan {
        /// Table name from `FROM`.
        table: String,
    },
    /// Row filter (`WHERE`).
    Filter {
        /// Child operator.
        input: Box<Self>,
        /// Boolean expression.
        predicate: Expr,
    },
    /// Column projection (`SELECT` list).
    Project {
        /// Child operator.
        input: Box<Self>,
        /// Projection items.
        items: Vec<SelectItem>,
    },
    /// Inner join.
    Join {
        /// Left subtree.
        left: Box<Self>,
        /// Right subtree.
        right: Box<Self>,
        /// `ON` predicate.
        on: Expr,
    },
}
