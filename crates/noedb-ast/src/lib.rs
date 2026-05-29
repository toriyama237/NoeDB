//! Abstract Syntax Tree types for NoeDB SQL statements.
//!
//! This crate owns the typed shape of every SQL statement NoeDB knows
//! how to parse. It is intentionally **dependency-light** (only
//! `noedb-lexer` for [`Span`]) so that consumers — parser, planner,
//! optimizer — can match on AST nodes without dragging in heavy
//! transitive deps.
//!
//! # Status
//!
//! **Week 08 / Phase 1** — `SELECT`, DML, and DDL AST nodes with
//! [`Display`](core::fmt::Display) round-trip support. See `docs/sprint-plan.md`.
//!
//! [`Span`]: noedb_lexer::Span

#![forbid(unsafe_code)]
#![allow(
    clippy::derive_partial_eq_without_eq,
    clippy::missing_const_for_fn,
    clippy::use_self
)]

mod display;
mod expr;
mod name;
mod stmt;
mod window;

pub use crate::expr::{BinaryOp, Expr, Literal, SelectItem, UnaryOp};
pub use crate::name::{ColumnRef, Ident, TableRef};
pub use crate::stmt::{
    BeginTxnStmt, ColumnDef, CommitTxnStmt, CreateIndexStmt, CreatePolicyStmt, CreateTableStmt,
    CteBody, CteDef, DeleteStmt, DropTableStmt, EnableRlsStmt, ExecuteStmt, InsertStmt, Join,
    JoinKind, PrepareStmt, RollbackTxnStmt, SelectStmt, SetRoleStmt, SqlType, Statement,
    UpdateStmt, WithClause,
};
pub use crate::window::{FrameBound, FrameMode, OrderKey, WindowFrame, WindowFunc, WindowSpec};

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use noedb_lexer::Span;

    #[test]
    fn select_stmt_span_round_trips() {
        let stmt = Statement::Select(SelectStmt {
            with_clause: None,
            distinct: false,
            items: vec![SelectItem {
                expr: Expr::Column(ColumnRef::Named {
                    table: None,
                    column: Ident::new("a".into(), Span::new(7, 8)),
                }),
                alias: None,
            }],
            from: Some(TableRef {
                name: Ident::new("t".into(), Span::new(14, 15)),
                alias: None,
                span: Span::new(14, 15),
            }),
            joins: vec![],
            where_clause: None,
            span: Span::new(0, 15),
        });
        assert_eq!(stmt.span(), Span::new(0, 15));
        assert!(format!("{stmt}").contains("SELECT"));
    }
}
