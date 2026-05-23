//! Recursive-descent SQL parser for NoeDB.
//!
//! Consumes tokens produced by [`noedb-lexer`] and produces [`Statement`]s
//! defined by [`noedb-ast`]. Phase 1 covers `SELECT`, DML, and core DDL.
//!
//! [`noedb-lexer`]: noedb_lexer
//! [`noedb-ast`]: noedb_ast
//! [`Statement`]: noedb_ast::Statement

#![forbid(unsafe_code)]
#![allow(
    clippy::missing_const_for_fn,
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::match_same_arms
)]

mod ddl;
mod dml;
mod error;
mod expr;
mod parser;
mod prepare;
mod select;
mod session;
mod txn;

pub use crate::error::ParseError;
pub use crate::parser::Parser;

use noedb_ast::Statement;

/// Parse a single SQL statement from a source string.
///
/// On failure the parser attempts **error recovery** by skipping tokens
/// until the next `;` or end of input (Week 08 deliverable).
///
/// # Errors
///
/// Returns [`ParseError`] when the input is not valid SQL for the supported
/// subset.
pub fn parse(src: &str) -> Result<Statement, ParseError> {
    Parser::parse_statement(src)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use noedb_ast::{
        BinaryOp, ColumnRef, Expr, Ident, JoinKind, Literal, SelectItem, SqlType, Statement,
    };

    #[test]
    fn parse_select_one() {
        let stmt = parse("SELECT 1").unwrap();
        assert!(matches!(stmt, Statement::Select(_)));
    }

    #[test]
    fn parse_select_columns_from_table() {
        let stmt = parse("SELECT a, b FROM t").unwrap();
        let Statement::Select(s) = stmt else {
            panic!("expected select");
        };
        assert_eq!(s.items.len(), 2);
        assert!(s.from.is_some());
    }

    #[test]
    fn parse_select_with_where_and_precedence() {
        let stmt = parse("SELECT * FROM users WHERE id = 1 AND active IS NOT NULL").unwrap();
        let Statement::Select(s) = stmt else {
            panic!("expected select");
        };
        assert!(s.where_clause.is_some());
    }

    #[test]
    fn parse_inner_join() {
        let sql = "SELECT u.name FROM users u INNER JOIN orders o ON u.id = o.user_id";
        let stmt = parse(sql).unwrap();
        let Statement::Select(s) = stmt else {
            panic!("expected select");
        };
        assert_eq!(s.joins.len(), 1);
        assert_eq!(s.joins[0].kind, JoinKind::Inner);
    }

    #[test]
    fn parse_left_join() {
        let sql = "SELECT * FROM a LEFT JOIN b ON a.id = b.a_id";
        let stmt = parse(sql).unwrap();
        let Statement::Select(s) = stmt else {
            panic!("expected select");
        };
        assert_eq!(s.joins[0].kind, JoinKind::Left);
    }

    #[test]
    fn parse_insert_multiple_rows() {
        let sql = "INSERT INTO t (a, b) VALUES (1, 'x'), (2, 'y')";
        let stmt = parse(sql).unwrap();
        let Statement::Insert(i) = stmt else {
            panic!("expected insert");
        };
        assert_eq!(i.values.len(), 2);
    }

    #[test]
    fn parse_update_and_delete() {
        assert!(matches!(
            parse("UPDATE t SET x = 1 WHERE y = 2"),
            Ok(Statement::Update(_))
        ));
        assert!(matches!(
            parse("DELETE FROM t WHERE id = 3"),
            Ok(Statement::Delete(_))
        ));
    }

    #[test]
    fn parse_create_table_and_index() {
        let ddl = parse("CREATE TABLE users (id INT NOT NULL, name VARCHAR(64))").unwrap();
        let Statement::CreateTable(t) = ddl else {
            panic!("expected create table");
        };
        assert_eq!(t.columns.len(), 2);
        assert_eq!(t.columns[0].data_type, SqlType::Int);
        assert!(t.columns[0].not_null);

        let idx = parse("CREATE INDEX idx_users ON users (id)").unwrap();
        assert!(matches!(idx, Statement::CreateIndex(_)));
    }

    #[test]
    fn parse_drop_table() {
        assert!(matches!(
            parse("DROP TABLE legacy"),
            Ok(Statement::DropTable(_))
        ));
    }

    #[test]
    fn display_round_trip_select() {
        let sql = "SELECT a, b FROM t WHERE x = 1 AND y <> 0";
        let stmt = parse(sql).unwrap();
        let rendered = format!("{stmt}");
        let stmt2 = parse(&rendered).unwrap();
        assert_eq!(stmt, stmt2);
    }

    #[test]
    fn display_round_trip_dml() {
        let sql = "INSERT INTO logs VALUES (1, 'ok')";
        let stmt = parse(sql).unwrap();
        let rendered = format!("{stmt}");
        assert_eq!(parse(&rendered).unwrap(), stmt);
    }

    #[test]
    fn between_and_in_predicates() {
        let b = parse("SELECT 1 FROM t WHERE id BETWEEN 1 AND 10").unwrap();
        assert!(matches!(b, Statement::Select(_)));
        let i = parse("SELECT 1 FROM t WHERE id IN (1, 2, 3)").unwrap();
        assert!(matches!(i, Statement::Select(_)));
    }

    #[test]
    fn error_recovery_does_not_panic() {
        let err = parse("CREATE TABLE");
        assert!(err.is_err());
    }

    #[test]
    fn parse_window_frame_rows() {
        use noedb_ast::{FrameBound, FrameMode};
        let sql = "SELECT SUM(x) OVER (ORDER BY id ROWS BETWEEN 2 PRECEDING AND CURRENT ROW) FROM t";
        let stmt = parse(sql).unwrap();
        let Statement::Select(s) = stmt else {
            panic!();
        };
        let Expr::Function { over: Some(spec), .. } = &s.items[0].expr else {
            panic!();
        };
        let frame = spec.frame.as_ref().expect("frame");
        assert_eq!(frame.mode, FrameMode::Rows);
        assert_eq!(frame.start, FrameBound::Preceding(2));
        assert_eq!(frame.end, FrameBound::CurrentRow);
    }

    #[test]
    fn parse_window_order_desc() {
        let sql = "SELECT ROW_NUMBER() OVER (PARTITION BY grp ORDER BY id DESC) AS rn FROM t";
        let stmt = parse(sql).unwrap();
        let Statement::Select(s) = stmt else {
            panic!();
        };
        let Expr::Function { over: Some(spec), .. } = &s.items[0].expr else {
            panic!();
        };
        assert!(!spec.order_by[0].asc);
    }

    #[test]
    fn parse_row_number_over() {
        let stmt = parse("SELECT ROW_NUMBER() OVER (ORDER BY id) FROM t").unwrap();
        let Statement::Select(s) = stmt else {
            panic!();
        };
        assert!(matches!(
            &s.items[0].expr,
            Expr::Function {
                name,
                args,
                over: Some(spec),
                ..
            } if name.value == "ROW_NUMBER" && args.is_empty() && spec.order_by.len() == 1
        ));
    }

    fn ast_equality_on_parsed_select_item() {
        let stmt = parse("SELECT name AS n FROM users").unwrap();
        let Statement::Select(s) = stmt else {
            panic!();
        };
        assert!(matches!(
            &s.items[0],
            SelectItem {
                alias: Some(Ident { value, .. }),
                ..
            } if value == "n"
        ));
    }

    #[test]
    fn literal_integer_span_preserved() {
        let stmt = parse("SELECT 42").unwrap();
        let Statement::Select(s) = stmt else {
            panic!();
        };
        assert!(matches!(
            &s.items[0].expr,
            Expr::Literal(Literal::Integer(42, span)) if span.start < span.end
        ));
    }

    #[test]
    fn qualified_column_reference() {
        let stmt = parse("SELECT u.name FROM users u").unwrap();
        let Statement::Select(s) = stmt else {
            panic!();
        };
        assert!(matches!(
            &s.items[0].expr,
            Expr::Column(ColumnRef::Named { table: Some(_), .. })
        ));
    }

    #[test]
    fn binary_op_equality_in_where() {
        let stmt = parse("SELECT 1 FROM t WHERE a = b OR c = d").unwrap();
        let Statement::Select(s) = stmt else {
            panic!();
        };
        let Some(Expr::Binary {
            op: BinaryOp::Or, ..
        }) = &s.where_clause
        else {
            panic!("expected OR at top");
        };
        let _ = s.where_clause.as_ref().unwrap().span();
    }
}

