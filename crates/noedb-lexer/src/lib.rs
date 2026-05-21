//! The NoeDB lexer.
//!
//! Turns SQL source text into a stream of [`Token`]s, each tagged with a
//! byte-precise [`Span`]. The lexer is the front door of the engine: every
//! later phase — parser, planner, executor — sees the world through what
//! this crate produces.
//!
//! # Design
//!
//! - **Zero dependencies.** No `serde`, no `regex`, no `logos`. The lexer
//!   is the fastest, smallest, and most reviewable crate in the workspace.
//! - **Zero-copy identifiers.** [`Token::Ident`] carries no `String`; the
//!   lexeme is recovered via [`SpannedToken::lexeme`] + [`Span::slice`].
//!   This is the same trick used by production parsers (DataFusion is
//!   moving its tokenizer in this direction — see sqlparser-rs #2036).
//! - **Static keyword table.** 80+ SQL-92 reserved words resolved with a
//!   sorted table + binary search — no `HashMap`, no allocation on the hot
//!   path.
//! - **Spans first.** Line/column display is computed on demand by
//!   [`SourceMap`], keeping each token at 8 bytes of position data.
//! - **`forbid(unsafe_code)`.** No `unsafe` anywhere in this crate.
//!
//! # Quick start
//!
//! ```
//! use noedb_lexer::{tokenize, Token, Keyword};
//!
//! let toks = tokenize("SELECT name FROM users")?;
//! assert_eq!(toks[0].kind, Token::Keyword(Keyword::Select));
//! assert_eq!(toks[1].kind, Token::Ident);
//! assert_eq!(toks[2].kind, Token::Keyword(Keyword::From));
//! # Ok::<_, noedb_lexer::LexError>(())
//! ```
//!
//! # What works today
//!
//! **Day 3 / 260** — Week 03 deliverable. The lexer recognizes:
//!
//! - 80+ SQL-92 reserved keywords (case-insensitive).
//! - Ordinary and delimited identifiers (zero-copy).
//! - Integer and floating-point literals.
//! - Single-quoted string literals (`''` escape).
//! - Comparison operators: `=`, `!=`, `<>`, `<`, `>`, `<=`, `>=`.
//! - Punctuation: `(`, `)`, `,`, `;`, `.`, `*`.
//! - Comments: `--` line and `/* */` block.
//!
//! Compound predicates (`IS NULL`, `LIKE`, `IN`, `BETWEEN`) are separate
//! keyword tokens — the parser combines them in Week 05+.

#![forbid(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

mod cursor;
mod error;
mod keywords;
mod lexer;
mod span;
mod token;

pub use crate::error::{LexError, LexErrorKind};
pub use crate::keywords::KEYWORD_COUNT;
pub use crate::lexer::Lexer;
pub use crate::span::{LineColumn, SourceMap, Span};
pub use crate::token::{Keyword, Operator, Punctuation, SpannedToken, Token};

/// Tokenize a SQL source string.
///
/// Returns a vector of [`SpannedToken`]s ending with [`Token::Eof`].
/// Prefer [`Lexer`] when you want incremental tokenization.
///
/// # Errors
///
/// Returns a [`LexError`] with a byte-precise [`Span`] on failure.
pub fn tokenize(src: &str) -> Result<Vec<SpannedToken>, LexError> {
    Lexer::tokenize(src)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::{Operator, Punctuation};

    fn kinds(src: &str) -> Vec<Token> {
        tokenize(src).unwrap().into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn tokenize_select_one() {
        assert_eq!(
            kinds("SELECT 1"),
            vec![
                Token::Keyword(Keyword::Select),
                Token::Integer(1),
                Token::Eof
            ]
        );
    }

    #[test]
    fn tokenize_full_select_statement() {
        let toks = tokenize("SELECT name FROM users WHERE id").unwrap();
        assert_eq!(toks[0].kind, Token::Keyword(Keyword::Select));
        assert_eq!(toks[1].kind, Token::Ident);
        assert_eq!(toks[2].kind, Token::Keyword(Keyword::From));
        assert_eq!(toks[3].kind, Token::Ident);
        assert_eq!(toks[4].kind, Token::Keyword(Keyword::Where));
        assert_eq!(toks[5].kind, Token::Ident);
        assert_eq!(toks.last().unwrap().kind, Token::Eof);
    }

    #[test]
    fn tokenize_is_case_insensitive_on_keywords() {
        assert_eq!(
            kinds("select 42"),
            vec![
                Token::Keyword(Keyword::Select),
                Token::Integer(42),
                Token::Eof
            ]
        );
    }

    #[test]
    fn tokenize_handles_extra_whitespace() {
        assert_eq!(
            kinds("   SELECT\t\n  7  "),
            vec![
                Token::Keyword(Keyword::Select),
                Token::Integer(7),
                Token::Eof
            ]
        );
    }

    #[test]
    fn tokenize_empty_input_yields_only_eof() {
        assert_eq!(kinds(""), vec![Token::Eof]);
    }

    #[test]
    fn tokenize_rejects_unknown_punctuation() {
        let err = tokenize("SELECT @").unwrap_err();
        assert!(matches!(err.kind, LexErrorKind::UnexpectedChar('@')));
        assert_eq!(err.span.start, 7);
    }

    #[test]
    fn tokenize_table_name_is_ident_not_keyword() {
        assert_eq!(
            kinds("FROM users"),
            vec![Token::Keyword(Keyword::From), Token::Ident, Token::Eof]
        );
    }

    #[test]
    fn tokenize_string_literal() {
        assert_eq!(
            kinds("SELECT 'hello'"),
            vec![
                Token::Keyword(Keyword::Select),
                Token::String("hello".into()),
                Token::Eof
            ]
        );
    }

    #[test]
    fn lexer_iterator_matches_tokenize() {
        let src = "INSERT INTO t VALUES 1";
        let from_fn: Vec<_> = tokenize(src).unwrap();
        let from_iter: Vec<_> = Lexer::new(src)
            .map(Result::unwrap)
            .chain(std::iter::once(SpannedToken {
                kind: Token::Eof,
                span: Span::empty_at(src.len()),
            }))
            .collect();
        assert_eq!(from_fn, from_iter);
    }

    #[test]
    fn tokenize_where_with_operators() {
        let toks = tokenize("SELECT * FROM t WHERE id = 42 AND x <> 0").unwrap();
        assert_eq!(toks[1].kind, Token::Punct(Punctuation::Star));
        assert_eq!(toks[6].kind, Token::Op(Operator::Eq));
        assert_eq!(toks[8].kind, Token::Keyword(Keyword::And));
        assert_eq!(toks[10].kind, Token::Op(Operator::Ne));
    }

    #[test]
    fn tokenize_skips_comments() {
        assert_eq!(
            kinds("/* init */ SELECT 1 -- done"),
            vec![
                Token::Keyword(Keyword::Select),
                Token::Integer(1),
                Token::Eof
            ]
        );
    }

    #[test]
    fn spans_point_to_exact_bytes() {
        let src = "  SELECT 99";
        let toks = tokenize(src).unwrap();
        assert_eq!(toks[0].span, Span::new(2, 8));
        assert_eq!(toks[0].lexeme(src), Some("SELECT"));
        assert_eq!(toks[1].span, Span::new(9, 11));
        assert_eq!(toks[1].lexeme(src), Some("99"));
    }
}
