//! Public [`Lexer`] type — an iterator over SQL tokens.

use crate::cursor::Cursor;
use crate::error::LexError;
use crate::token::{SpannedToken, Token};

/// A pull-based lexer over a SQL source string.
///
/// Implements [`Iterator`] so callers can write `for tok in Lexer::new(sql)`
/// or collect with [`Lexer::tokenize`].
///
/// # Examples
///
/// ```
/// use noedb_lexer::{Lexer, Token, Keyword};
///
/// let mut lex = Lexer::new("SELECT name FROM users");
/// let t0 = lex.next().unwrap().unwrap();
/// assert_eq!(t0.kind, Token::Keyword(Keyword::Select));
/// ```
pub struct Lexer<'src> {
    cursor: Cursor<'src>,
}

impl<'src> Lexer<'src> {
    /// Create a lexer over `src`.
    #[inline]
    #[must_use]
    pub const fn new(src: &'src str) -> Self {
        Self {
            cursor: Cursor::new(src),
        }
    }

    /// Tokenize all of `src` into a vector ending with [`Token::Eof`].
    ///
    /// Convenience wrapper around [`Lexer::new`] + iteration.
    pub fn tokenize(src: &'src str) -> Result<Vec<SpannedToken>, LexError> {
        let mut lexer = Self::new(src);
        let mut out = Vec::new();

        for tok in &mut lexer {
            out.push(tok?);
        }

        out.push(SpannedToken {
            kind: Token::Eof,
            span: crate::span::Span::empty_at(src.len()),
        });

        Ok(out)
    }
}

impl Iterator for Lexer<'_> {
    type Item = Result<SpannedToken, LexError>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.cursor.next_token() {
            Ok(Some(tok)) => Some(Ok(tok)),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }
}
