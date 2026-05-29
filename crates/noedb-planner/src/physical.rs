//! Physical operators (Week 19–22).

use noedb_ast::{Expr, SelectItem, SetOpKind};

use crate::logical::{AggFunc, WindowCompute};

/// Executable physical plan.
#[derive(Debug, Clone, PartialEq)]
pub enum PhysicalPlan {
    /// Sequential scan over a table prefix in storage.
    SeqScan {
        /// Table name.
        table: String,
        /// Column prefix for qualified keys in merged joins.
        prefix: String,
        /// Column pruning (Week 26); `None` = all columns.
        columns: Option<Vec<String>>,
    },
    /// B-tree index point/range scan (Week 20).
    IndexScan {
        /// Table name.
        table: String,
        /// Indexed column.
        column: String,
        /// Equality lookup key (`WHERE col = ?`).
        point_key: Option<Vec<u8>>,
        /// Columns to load for matching rows.
        columns: Option<Vec<String>>,
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
    /// Hash join build/probe (Week 21).
    HashJoin {
        /// Left child.
        left: Box<Self>,
        /// Right child.
        right: Box<Self>,
        /// Join predicate.
        on: Expr,
        /// Left join key column.
        left_key: String,
        /// Right join key column.
        right_key: String,
    },
    /// Nested-loop join (Week 22).
    NestedLoopJoin {
        /// Left child.
        left: Box<Self>,
        /// Right child.
        right: Box<Self>,
        /// Join predicate.
        on: Expr,
    },
    /// Merge join on sorted inputs (Week 22).
    MergeJoin {
        /// Left child.
        left: Box<Self>,
        /// Right child.
        right: Box<Self>,
        /// Join predicate.
        on: Expr,
        /// Left join key column.
        left_key: String,
        /// Right join key column.
        right_key: String,
    },
    /// Hash-based aggregate (Week 23).
    Aggregate {
        /// Child operator.
        input: Box<Self>,
        /// Grouping columns.
        group_by: Vec<String>,
        /// `(output_name, function)`.
        aggs: Vec<(String, AggFunc)>,
    },
    /// In-memory sort.
    Sort {
        /// Child operator.
        input: Box<Self>,
        /// `(column, ascending)`.
        keys: Vec<(String, bool)>,
    },
    /// Row cap.
    Limit {
        /// Child operator.
        input: Box<Self>,
        /// Max rows.
        limit: u64,
        /// Skip rows.
        offset: u64,
    },
    /// Window / analytic functions.
    Window {
        /// Child operator.
        input: Box<Self>,
        /// Window computations.
        windows: Vec<WindowCompute>,
    },
    /// Semi-join (`IN (SELECT …)` decorrelation, Week 39).
    SemiJoin {
        /// Outer input.
        left: Box<Self>,
        /// Subquery input.
        right: Box<Self>,
        /// Outer key column.
        left_key: String,
        /// Inner key column.
        right_key: String,
        /// Optional correlation predicate on merged rows.
        corr_on: Option<Expr>,
        /// `NOT IN`.
        negated: bool,
    },
    /// Scan rows materialized for a `WITH` CTE (Week 40).
    CteScan {
        /// CTE name.
        name: String,
        /// Column prefix (`alias` or CTE name).
        prefix: String,
        /// Column pruning.
        columns: Option<Vec<String>>,
    },
    /// Compound set operation (Week 41).
    SetOp {
        /// Left input.
        left: Box<Self>,
        /// Right input.
        right: Box<Self>,
        /// Operator kind.
        op: SetOpKind,
        /// `ALL` semantics.
        all: bool,
    },
}
