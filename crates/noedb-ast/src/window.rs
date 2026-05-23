//! Window function AST (Phase 5 Weeks 37–38).

use crate::expr::Expr;

/// Ranking / analytic window function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowFunc {
    /// `ROW_NUMBER()`.
    RowNumber,
    /// `RANK()`.
    Rank,
    /// `DENSE_RANK()`.
    DenseRank,
    /// `SUM(expr)`.
    Sum,
    /// `AVG(expr)`.
    Avg,
}

impl WindowFunc {
    /// Parse ranking function name (no arguments).
    #[must_use]
    pub fn parse_name(name: &str) -> Option<Self> {
        match name.to_ascii_uppercase().as_str() {
            "ROW_NUMBER" => Some(Self::RowNumber),
            "RANK" => Some(Self::Rank),
            "DENSE_RANK" => Some(Self::DenseRank),
            _ => None,
        }
    }

    /// Parse aggregate window function name (one argument).
    #[must_use]
    pub fn parse_agg_name(name: &str) -> Option<Self> {
        match name.to_ascii_uppercase().as_str() {
            "SUM" => Some(Self::Sum),
            "AVG" => Some(Self::Avg),
            _ => None,
        }
    }

    /// Ranking functions require `ORDER BY` in `OVER`.
    #[must_use]
    pub const fn is_ranking(self) -> bool {
        matches!(self, Self::RowNumber | Self::Rank | Self::DenseRank)
    }

    /// Aggregate window functions (`SUM` / `AVG`).
    #[must_use]
    pub const fn is_aggregate(self) -> bool {
        matches!(self, Self::Sum | Self::Avg)
    }

    /// SQL name for display.
    #[must_use]
    pub const fn sql_name(self) -> &'static str {
        match self {
            Self::RowNumber => "ROW_NUMBER",
            Self::Rank => "RANK",
            Self::DenseRank => "DENSE_RANK",
            Self::Sum => "SUM",
            Self::Avg => "AVG",
        }
    }
}

/// `ORDER BY` key inside `OVER (...)`.
#[derive(Debug, Clone, PartialEq)]
pub struct OrderKey {
    /// Sort expression (usually a column).
    pub expr: Expr,
    /// `true` = ASC, `false` = DESC.
    pub asc: bool,
}

/// `ROWS` or `RANGE` frame mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameMode {
    /// Row-index frame (`ROWS BETWEEN …`).
    Rows,
    /// Range frame (`RANGE BETWEEN …`); v1 uses row indices like `ROWS`.
    Range,
}

/// One bound in `ROWS|RANGE BETWEEN start AND end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameBound {
    /// `UNBOUNDED PRECEDING`.
    UnboundedPreceding,
    /// `n PRECEDING`.
    Preceding(u64),
    /// `CURRENT ROW`.
    CurrentRow,
    /// `n FOLLOWING`.
    Following(u64),
    /// `UNBOUNDED FOLLOWING`.
    UnboundedFollowing,
}

/// Window frame clause inside `OVER`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowFrame {
    /// `ROWS` or `RANGE`.
    pub mode: FrameMode,
    /// Start bound (inclusive).
    pub start: FrameBound,
    /// End bound (inclusive).
    pub end: FrameBound,
}

/// `OVER (PARTITION BY … ORDER BY … [frame])` specification.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WindowSpec {
    /// `PARTITION BY` expressions.
    pub partition_by: Vec<Expr>,
    /// `ORDER BY` keys (required for ranking functions).
    pub order_by: Vec<OrderKey>,
    /// Explicit `ROWS` / `RANGE` frame; `None` → planner/executor default.
    pub frame: Option<WindowFrame>,
}

impl WindowSpec {
    /// Empty spec.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            partition_by: Vec::new(),
            order_by: Vec::new(),
            frame: None,
        }
    }
}
