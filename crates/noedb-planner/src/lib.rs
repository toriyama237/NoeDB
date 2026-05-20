//! Query planner for NoeDB.
//!
//! Turns a [`Statement`] (from `noedb-ast`) into a logical plan, then a
//! physical plan that the executor walks. The planner is Volcano-style
//! (Graefe, 1994) with a cost model that the sprint plan exposes to the
//! reader piece by piece in Phase 3.
//!
//! # Status
//!
//! **Day 2 / 260** — placeholder. Real work starts in Week 17.
//!
//! [`Statement`]: noedb_ast::Statement

#![forbid(unsafe_code)]

use noedb_ast::Statement;

/// A logical plan node.
///
/// Will mirror the relational algebra (Scan / Filter / Project / Join /
/// Aggregate / Sort / Limit) as soon as the AST stops being a
/// placeholder.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LogicalPlan {
    /// Reserved for the first real variant.
    Placeholder,
}

/// Turn a [`Statement`] into a [`LogicalPlan`].
///
/// # Errors
///
/// Reserved for the first planner pass; today this returns
/// [`PlanError::NotYetImplemented`] for every input.
pub const fn plan(stmt: &Statement) -> Result<LogicalPlan, PlanError> {
    let _ = stmt;
    Err(PlanError::NotYetImplemented)
}

/// An error produced by the planner.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PlanError {
    /// Reserved: surfaced when the planner has not yet learnt the rule
    /// being requested.
    NotYetImplemented,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use noedb_lexer::Span;

    #[test]
    fn plan_is_not_yet_implemented() {
        let stmt = Statement::Placeholder {
            span: Span::new(0, 0),
        };
        assert_eq!(plan(&stmt), Err(PlanError::NotYetImplemented));
    }
}
