//! Public API smoke test.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use noedb_lexer::{tokenize, Keyword, Lexer, LineColumn, SourceMap, Span, Token};

#[test]
fn public_api_can_tokenize_select_one() {
    let toks = tokenize("SELECT 1").expect("tokenize should succeed on 'SELECT 1'");
    let kinds: Vec<_> = toks.iter().map(|t| &t.kind).collect();
    assert_eq!(
        kinds,
        vec![
            &Token::Keyword(Keyword::Select),
            &Token::Integer(1),
            &Token::Eof
        ]
    );
}

#[test]
fn spans_round_trip_through_the_source() {
    let src = "  SELECT 42";
    let toks = tokenize(src).expect("tokenize");
    assert_eq!(toks[0].span, Span::new(2, 8));
    assert_eq!(toks[0].span.slice(src), Some("SELECT"));
    assert_eq!(toks[1].span, Span::new(9, 11));
    assert_eq!(toks[1].span.slice(src), Some("42"));
}

#[test]
fn ident_lexeme_is_zero_copy() {
    let src = "SELECT name FROM users";
    let toks = tokenize(src).expect("tokenize");
    assert_eq!(toks[1].kind, Token::Ident);
    assert_eq!(toks[1].lexeme(src), Some("name"));
    assert_eq!(toks[3].lexeme(src), Some("users"));
}

#[test]
fn source_map_resolves_token_positions() {
    let src = "SELECT 1\nSELECT 2";
    let toks = tokenize(src).expect("tokenize");
    let map = SourceMap::new(src);

    let second_select = toks
        .iter()
        .filter(|t| matches!(t.kind, Token::Keyword(Keyword::Select)))
        .nth(1)
        .expect("second SELECT");

    assert_eq!(
        map.line_column(second_select.span.start),
        LineColumn { line: 2, column: 1 }
    );
}

#[test]
fn lexer_iterator_api() {
    let src = "SELECT 1";
    let count = Lexer::new(src).count();
    assert_eq!(count, 2);
}

#[test]
fn distance_operator_lex() {
    use noedb_lexer::{Operator, Token};
    let tokens = tokenize("emb <-> \"[0,0,0]\"").unwrap();
    assert!(tokens
        .iter()
        .any(|t| matches!(t.kind, Token::Op(Operator::Distance))));
}
