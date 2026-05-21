//! Parser error types.

use core::fmt;

use noedb_lexer::{Keyword, LexError, Punctuation, Span};

/// An error produced by the parser.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParseError {
    /// A token-level error bubbled up from the lexer.
    Lex(LexError),
    /// A token was found where something else was expected.
    UnexpectedToken {
        /// Offending token span.
        span: Span,
        /// Short description of what was expected.
        context: &'static str,
    },
    /// A specific keyword was expected.
    ExpectedKeyword {
        /// Expected keyword.
        expected: Keyword,
        /// Actual token span.
        span: Span,
    },
    /// A specific punctuation token was expected.
    ExpectedPunctuation {
        /// Expected punctuation.
        expected: Punctuation,
        /// Actual token span.
        span: Span,
    },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lex(e) => write!(f, "{e}"),
            Self::UnexpectedToken { span, context } => {
                write!(
                    f,
                    "unexpected token at bytes {}..{} (expected {context})",
                    span.start, span.end
                )
            }
            Self::ExpectedKeyword { span, .. } => {
                write!(f, "expected keyword at bytes {}..{}", span.start, span.end)
            }
            Self::ExpectedPunctuation { span, .. } => {
                write!(
                    f,
                    "expected punctuation at bytes {}..{}",
                    span.start, span.end
                )
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
