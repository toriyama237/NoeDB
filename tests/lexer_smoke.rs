//! Day-1 integration test.
//!
//! Verifies that the public surface of the crate is wired up correctly:
//! `use noedb::lexer::*` must work from outside the crate.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use noedb::lexer::{tokenize, Token};

#[test]
fn public_api_can_tokenize_select_one() {
    let toks = tokenize("SELECT 1").expect("tokenize should succeed on 'SELECT 1'");
    assert_eq!(toks, vec![Token::Select, Token::Number(1), Token::Eof]);
}
