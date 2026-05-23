//! Logical relational algebra (Week 17).

use noedb_ast::{Expr, SelectItem, WindowFunc, WindowSpec};

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
    /// Window functions (`OVER` clause, Phase 5 Week 37).
    Window {
        /// Child operator (base rows).
        input: Box<Self>,
        /// Window computations to apply.
        windows: Vec<WindowCompute>,
    },
    /// Semi-join for decorrelated `IN (SELECT …)` (Phase 5 Week 39).
    SemiJoin {
        /// Outer (left) input.
        left: Box<Self>,
        /// Subquery (right) input.
        right: Box<Self>,
        /// Outer join key column name.
        left_key: String,
        /// Inner projected column name.
        right_key: String,
        /// Correlation equalities (`WHERE` from subquery), evaluated on merged rows.
        corr_on: Option<Expr>,
        /// `NOT IN` semantics.
        negated: bool,
    },
}

/// One analytic function in a `Window` operator.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowCompute {
    /// Function (`ROW_NUMBER`, `SUM`, …).
    pub func: WindowFunc,
    /// Argument for `SUM` / `AVG`; `None` for ranking functions.
    pub arg: Option<Expr>,
    /// `OVER (...)` specification.
    pub spec: WindowSpec,
    /// Output column name in the row map.
    pub output_name: String,
}
