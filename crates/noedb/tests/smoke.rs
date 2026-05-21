//! Public API smoke test for the meta-crate `noedb`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use noedb::ast::Statement;
use noedb::lexer::{Keyword, Token};

#[test]
fn lexer_re_export_is_reachable() {
    let toks = noedb::lexer::tokenize("SELECT 1").expect("tokenize");
    assert_eq!(toks[0].kind, Token::Keyword(Keyword::Select));
    assert_eq!(toks[1].kind, Token::Integer(1));
    assert_eq!(toks[2].kind, Token::Eof);
}

#[test]
fn parser_re_export_is_reachable() {
    let stmt = noedb::parser::parse("SELECT 1").expect("parse");
    assert!(matches!(stmt, Statement::Select(_)));
}

#[test]
fn ast_re_export_is_reachable() {
    let stmt = noedb::parser::parse("SELECT 1").expect("parse");
    assert!(stmt.span().start < stmt.span().end || stmt.span().start == 0);
}
