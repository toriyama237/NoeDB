//! Public API smoke test for the meta-crate `noedb`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

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
    let err = noedb::parser::parse("SELECT 1").unwrap_err();
    assert_eq!(err, noedb::parser::ParseError::NotYetImplemented);
}

#[test]
fn ast_re_export_is_reachable() {
    let s = noedb::ast::Statement::Placeholder {
        span: noedb::lexer::Span::new(0, 0),
    };
    assert_eq!(s.span(), noedb::lexer::Span::new(0, 0));
}
