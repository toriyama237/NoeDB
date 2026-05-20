//! Abstract Syntax Tree types for NoeDB SQL statements.
//!
//! This crate owns the typed shape of every SQL statement NoeDB knows
//! how to parse. It is intentionally **dependency-light** (only
//! `noedb-lexer` for [`Span`]) so that consumers - parser, planner,
//! optimizer - can match on AST nodes without dragging in heavy
//! transitive deps.
//!
//! # Status
//!
//! **Day 2 / 260** — placeholder. The AST will be defined in Week 02 of
//! the sprint, when the parser starts emitting real nodes. The skeleton
//! below exists so that downstream crates can already depend on
//! `noedb-ast` and document the contract.
//!
//! [`Span`]: noedb_lexer::Span

#![forbid(unsafe_code)]

use noedb_lexer::Span;

/// A SQL statement.
///
/// This enum is `#[non_exhaustive]` so that new variants can be added
/// without it being a breaking change for downstream pattern matches.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Statement {
    /// Reserved for the first real variant, landing in Week 02.
    Placeholder {
        /// Source position of this node, propagated from the lexer.
        span: Span,
    },
}

impl Statement {
    /// Source span of the statement.
    ///
    /// Every AST node tracks the original source location so that the
    /// planner and the diagnostics layer can produce rustc-quality
    /// error messages.
    #[must_use]
    pub const fn span(&self) -> Span {
        match self {
            Self::Placeholder { span } => *span,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_returns_its_span() {
        let s = Statement::Placeholder {
            span: Span::new(0, 6),
        };
        assert_eq!(s.span(), Span::new(0, 6));
    }
}
