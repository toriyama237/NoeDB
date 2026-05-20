//! Errors produced by the lexer.

use core::fmt;

use crate::span::Span;

/// An error produced by the lexer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    /// What went wrong.
    pub kind: LexErrorKind,
    /// Where it went wrong, in byte offsets.
    pub span: Span,
}

impl LexError {
    /// Construct a new `LexError`.
    #[inline]
    #[must_use]
    pub const fn new(kind: LexErrorKind, span: Span) -> Self {
        Self { kind, span }
    }
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at bytes {}..{}",
            self.kind, self.span.start, self.span.end
        )
    }
}

impl std::error::Error for LexError {}

/// The cause of a [`LexError`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LexErrorKind {
    /// An unsupported character was found.
    UnexpectedChar(char),
    /// A numeric literal that does not fit in an `i64`.
    IntegerOutOfRange(String),
    /// A floating-point literal that cannot be parsed.
    InvalidFloat(String),
    /// A single-quoted string without a closing `'`.
    UnterminatedString,
    /// A double-quoted identifier without a closing `"`.
    UnterminatedQuotedIdent,
}

impl fmt::Display for LexErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedChar(ch) => write!(f, "unexpected character {ch:?}"),
            Self::IntegerOutOfRange(lit) => {
                write!(f, "integer literal {lit:?} does not fit in i64")
            }
            Self::InvalidFloat(lit) => write!(f, "invalid floating-point literal {lit:?}"),
            Self::UnterminatedString => write!(f, "unterminated string literal"),
            Self::UnterminatedQuotedIdent => write!(f, "unterminated quoted identifier"),
        }
    }
}
