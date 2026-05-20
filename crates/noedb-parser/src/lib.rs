//! Recursive-descent SQL parser for NoeDB.
//!
//! Consumes [`SpannedToken`]s produced by [`noedb-lexer`] and produces
//! [`Statement`]s defined by [`noedb-ast`]. The parser follows the
//! classic Pratt / recursive descent split that the sprint plan calls
//! for in Week 05 and 06.
//!
//! # Status
//!
//! **Day 2 / 260** — placeholder. The real parser starts in Week 02
//! with single-statement support (`SELECT <integer>`) and grows by SQL
//! grammar rule each week. See `docs/sprint-plan.md`.
//!
//! [`SpannedToken`]: noedb_lexer::SpannedToken
//! [`noedb-lexer`]: noedb_lexer
//! [`Statement`]: noedb_ast::Statement
//! [`noedb-ast`]: noedb_ast

#![forbid(unsafe_code)]

use core::fmt;

use noedb_ast::Statement;
use noedb_lexer::{LexError, Span};

/// Parse a single SQL statement from a source string.
///
/// This is intentionally unimplemented today; the function is exported
/// so that `noedb-planner`, `noedb` (the meta-crate) and downstream code
/// can already wire the call site.
///
/// # Errors
///
/// Will return a [`ParseError`] once Week 02 lands. For now it always
/// returns [`ParseError::NotYetImplemented`].
pub const fn parse(src: &str) -> Result<Statement, ParseError> {
    let _ = src;
    Err(ParseError::NotYetImplemented)
}

/// An error produced by the parser.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParseError {
    /// Reserved: surfaced when the parser has not yet learnt the rule
    /// being requested.
    NotYetImplemented,
    /// A token-level error bubbled up from the lexer.
    Lex(LexError),
    /// A grammar rule could not be matched.
    UnexpectedToken {
        /// Position of the offending token.
        span: Span,
    },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotYetImplemented => write!(f, "noedb-parser: rule not yet implemented"),
            Self::Lex(e) => write!(f, "{e}"),
            Self::UnexpectedToken { span } => {
                write!(f, "unexpected token at bytes {}..{}", span.start, span.end)
            }
        }
    }
}

impl std::error::Error for ParseError {}

impl From<LexError> for ParseError {
    fn from(e: LexError) -> Self {
        Self::Lex(e)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn parse_is_not_yet_implemented() {
        assert_eq!(parse("SELECT 1"), Err(ParseError::NotYetImplemented));
    }
}
