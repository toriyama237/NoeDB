//! Logical relational algebra (Week 17).

use noedb_ast::{Expr, SelectItem};

/// Hash aggregate function (Week 23).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AggFunc {
    /// `COUNT(*)`.
    CountStar,
    /// `COUNT(col)`.
    CountCol(String),
}

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
    /// Hash aggregate (`GROUP BY` / global agg).
    Aggregate {
        /// Child operator.
        input: Box<Self>,
        /// Grouping columns.
        group_by: Vec<String>,
        /// Aggregate functions.
        aggs: Vec<(String, AggFunc)>,
    },
    /// Sort (`ORDER BY`).
    Sort {
        /// Child operator.
        input: Box<Self>,
        /// `(column, ascending)`.
        keys: Vec<(String, bool)>,
    },
    /// Limit / offset.
    Limit {
        /// Child operator.
        input: Box<Self>,
        /// Max rows.
        limit: u64,
        /// Skip rows.
        offset: u64,
    },
}
