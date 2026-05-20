//! The byte cursor that walks the SQL source.
//!
//! `Cursor` is a **peekable** scanner: callers can inspect the next byte
//! without consuming it, which keeps `next_token()` a straight switch on
//! the current character.

use crate::error::{LexError, LexErrorKind};
use crate::keywords::lookup;
use crate::span::Span;
use crate::token::{SpannedToken, Token};

/// Stateful byte cursor over a SQL source string.
pub(crate) struct Cursor<'src> {
    src: &'src str,
    bytes: &'src [u8],
    pos: usize,
}

impl<'src> Cursor<'src> {
    pub(crate) const fn new(src: &'src str) -> Self {
        Self {
            src,
            bytes: src.as_bytes(),
            pos: 0,
        }
    }

    /// Produce the next non-whitespace token, if any.
    ///
    /// Returns `Ok(None)` at end of input. The caller appends [`Token::Eof`].
    pub(crate) fn next_token(&mut self) -> Result<Option<SpannedToken>, LexError> {
        self.skip_whitespace();

        if self.is_eof() {
            return Ok(None);
        }

        let Some(b) = self.peek_byte() else {
            return Ok(None);
        };

        if b.is_ascii_alphabetic() || b == b'_' {
            return Ok(Some(self.lex_identifier_or_keyword()));
        }

        if b.is_ascii_digit() {
            return self.lex_number().map(Some);
        }

        if b == b'\'' {
            return self.lex_string().map(Some);
        }

        if b == b'"' {
            return self.lex_quoted_ident().map(Some);
        }

        if b == b'.' && self.peek_byte_at(1).is_some_and(|n| n.is_ascii_digit()) {
            return self.lex_number().map(Some);
        }

        let start = self.pos;
        self.bump();
        Err(LexError::new(
            LexErrorKind::UnexpectedChar(char::from(b)),
            Self::mk_span(start, self.pos),
        ))
    }

    /// Peek the byte at the current position without advancing.
    #[inline]
    fn peek_byte(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    /// Peek the byte `offset` positions ahead of the current position.
    #[inline]
    fn peek_byte_at(&self, offset: usize) -> Option<u8> {
        self.bytes.get(self.pos + offset).copied()
    }

    /// Consume and return the current byte, advancing by one.
    #[inline]
    fn bump(&mut self) -> Option<u8> {
        if self.pos >= self.bytes.len() {
            return None;
        }
        let b = self.bytes[self.pos];
        self.pos += 1;
        Some(b)
    }

    #[inline]
    const fn is_eof(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn lex_identifier_or_keyword(&mut self) -> SpannedToken {
        let start = self.pos;
        self.bump(); // first char already known alphabetic or _

        while self.pos < self.bytes.len()
            && (self.bytes[self.pos].is_ascii_alphanumeric() || self.bytes[self.pos] == b'_')
        {
            self.pos += 1;
        }

        let word = &self.src[start..self.pos];
        let span = Self::mk_span(start, self.pos);

        lookup(word).map_or_else(
            || SpannedToken::new(Token::Ident, span),
            |kw| SpannedToken::new(Token::Keyword(kw), span),
        )
    }

    fn lex_number(&mut self) -> Result<SpannedToken, LexError> {
        let start = self.pos;
        let mut is_float = false;

        if self.peek_byte() == Some(b'.') {
            is_float = true;
            self.bump();
            if !self.peek_byte().is_some_and(|b| b.is_ascii_digit()) {
                return Err(LexError::new(
                    LexErrorKind::UnexpectedChar('.'),
                    Self::mk_span(start, self.pos),
                ));
            }
        }

        while self.peek_byte().is_some_and(|b| b.is_ascii_digit()) {
            self.bump();
        }

        if self.peek_byte() == Some(b'.')
            && !is_float
            && self.peek_byte_at(1).is_some_and(|n| n.is_ascii_digit())
        {
            is_float = true;
            self.bump();
            while self.peek_byte().is_some_and(|n| n.is_ascii_digit()) {
                self.bump();
            }
        }

        if matches!(self.peek_byte(), Some(b'e' | b'E')) {
            is_float = true;
            self.bump();
            if matches!(self.peek_byte(), Some(b'+' | b'-')) {
                self.bump();
            }
            if !self.peek_byte().is_some_and(|b| b.is_ascii_digit()) {
                let lit = &self.src[start..self.pos];
                return Err(LexError::new(
                    LexErrorKind::InvalidFloat(lit.to_owned()),
                    Self::mk_span(start, self.pos),
                ));
            }
            while self.peek_byte().is_some_and(|b| b.is_ascii_digit()) {
                self.bump();
            }
        }

        let lit = &self.src[start..self.pos];
        let span = Self::mk_span(start, self.pos);

        if is_float {
            lit.parse::<f64>().map_or_else(
                |_| {
                    Err(LexError::new(
                        LexErrorKind::InvalidFloat(lit.to_owned()),
                        span,
                    ))
                },
                |f| Ok(SpannedToken::new(Token::Float(f), span)),
            )
        } else {
            lit.parse::<i64>().map_or_else(
                |_| {
                    Err(LexError::new(
                        LexErrorKind::IntegerOutOfRange(lit.to_owned()),
                        span,
                    ))
                },
                |n| Ok(SpannedToken::new(Token::Integer(n), span)),
            )
        }
    }

    fn lex_string(&mut self) -> Result<SpannedToken, LexError> {
        let start = self.pos;
        self.bump(); // opening '

        let mut value = String::new();
        while !self.is_eof() {
            match self.bump() {
                Some(b'\'') => {
                    if self.peek_byte() == Some(b'\'') {
                        self.bump();
                        value.push('\'');
                    } else {
                        let span = Self::mk_span(start, self.pos);
                        return Ok(SpannedToken::new(Token::String(value), span));
                    }
                }
                Some(ch) if ch.is_ascii() => value.push(char::from(ch)),
                Some(b) => {
                    return Err(LexError::new(
                        LexErrorKind::UnexpectedChar(char::from(b)),
                        Self::mk_span(self.pos - 1, self.pos),
                    ));
                }
                None => break,
            }
        }

        Err(LexError::new(
            LexErrorKind::UnterminatedString,
            Self::mk_span(start, self.pos),
        ))
    }

    fn lex_quoted_ident(&mut self) -> Result<SpannedToken, LexError> {
        let start = self.pos;
        self.bump(); // opening "

        let mut value = String::new();
        while !self.is_eof() {
            match self.bump() {
                Some(b'"') => {
                    if self.peek_byte() == Some(b'"') {
                        self.bump();
                        value.push('"');
                    } else {
                        let span = Self::mk_span(start, self.pos);
                        return Ok(SpannedToken::new(Token::QuotedIdent(value), span));
                    }
                }
                Some(ch) if ch.is_ascii() => value.push(char::from(ch)),
                Some(b) => {
                    return Err(LexError::new(
                        LexErrorKind::UnexpectedChar(char::from(b)),
                        Self::mk_span(self.pos - 1, self.pos),
                    ));
                }
                None => break,
            }
        }

        Err(LexError::new(
            LexErrorKind::UnterminatedQuotedIdent,
            Self::mk_span(start, self.pos),
        ))
    }

    #[inline]
    fn mk_span(start: usize, end: usize) -> Span {
        debug_assert!(u32::try_from(start).is_ok(), "start > u32::MAX");
        debug_assert!(u32::try_from(end).is_ok(), "end > u32::MAX");
        #[allow(clippy::cast_possible_truncation)]
        Span::new(start as u32, end as u32)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::token::Keyword;

    fn kinds(src: &str) -> Vec<Token> {
        let mut c = Cursor::new(src);
        let mut out = Vec::new();
        while let Some(tok) = c.next_token().unwrap() {
            out.push(tok.kind);
        }
        out
    }

    #[test]
    fn cursor_peek_does_not_advance() {
        let mut c = Cursor::new("SELECT");
        assert_eq!(c.peek_byte(), Some(b'S'));
        assert_eq!(c.peek_byte(), Some(b'S'));
        assert_eq!(
            c.next_token().unwrap().unwrap().kind,
            Token::Keyword(Keyword::Select)
        );
    }

    #[test]
    fn tokenizes_insert_as_keyword() {
        assert_eq!(
            kinds("INSERT INTO t"),
            vec![
                Token::Keyword(Keyword::Insert),
                Token::Keyword(Keyword::Into),
                Token::Ident,
            ]
        );
    }

    #[test]
    fn tokenizes_table_name_as_ident() {
        assert_eq!(kinds("users"), vec![Token::Ident]);
    }

    #[test]
    fn tokenizes_float_literals() {
        assert_eq!(kinds("1.5"), vec![Token::Float(1.5)]);
        assert_eq!(kinds(".5"), vec![Token::Float(0.5)]);
        assert_eq!(kinds("1e2"), vec![Token::Float(100.0)]);
        assert_eq!(kinds("1E-3"), vec![Token::Float(0.001)]);
    }

    #[test]
    fn tokenizes_string_with_doubled_quote() {
        let toks = kinds("'it''s'");
        assert_eq!(toks.len(), 1);
        assert_eq!(toks[0], Token::String("it's".into()));
    }

    #[test]
    fn tokenizes_quoted_identifier() {
        let toks = kinds(r#""weird""name""#);
        assert_eq!(toks.len(), 1);
        assert_eq!(toks[0], Token::QuotedIdent("weird\"name".into()));
    }

    #[test]
    fn rejects_unterminated_string() {
        let err = Cursor::new("'open").next_token().unwrap_err();
        assert!(matches!(err.kind, LexErrorKind::UnterminatedString));
    }
}
