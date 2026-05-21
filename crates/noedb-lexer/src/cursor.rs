//! The byte cursor that walks the SQL source.
//!
//! `Cursor` is a **peekable** scanner: callers can inspect the next byte
//! without consuming it, which keeps `next_token()` a straight switch on
//! the current character.

use crate::error::{LexError, LexErrorKind};
use crate::keywords::lookup;
use crate::span::Span;
use crate::token::{Operator, Punctuation, SpannedToken, Token};

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

    /// Produce the next token, if any.
    ///
    /// Returns `Ok(None)` at end of input. The caller appends [`Token::Eof`].
    #[inline]
    pub(crate) fn next_token(&mut self) -> Result<Option<SpannedToken>, LexError> {
        self.skip_trivia()?;

        if self.is_eof() {
            return Ok(None);
        }

        let Some(b) = self.peek_byte() else {
            return Ok(None);
        };

        if crate::ascii_lut::is_ident_start(b) {
            return Ok(Some(self.lex_identifier_or_keyword()));
        }

        if b.is_ascii_digit() {
            return self.lex_number_fast(b).map(Some);
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

        if Self::is_operator_or_punct_start(b) {
            return self.lex_operator_or_punct().map(Some);
        }

        let start = self.pos;
        self.bump();
        Err(LexError::new(
            LexErrorKind::UnexpectedChar(char::from(b)),
            Self::mk_span(start, self.pos),
        ))
    }

    #[inline]
    const fn is_operator_or_punct_start(b: u8) -> bool {
        matches!(
            b,
            b'(' | b')' | b',' | b';' | b'*' | b'.' | b'=' | b'!' | b'<' | b'>' | b'+' | b'-'
        )
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

    /// Skip whitespace and SQL comments (`--` line, `/* */` block).
    #[inline]
    fn skip_trivia(&mut self) -> Result<(), LexError> {
        loop {
            self.pos = crate::fast::skip_whitespace(self.bytes, self.pos);

            if self.peek_byte() == Some(b'-') && self.peek_byte_at(1) == Some(b'-') {
                self.pos += 2;
                while self.peek_byte().is_some_and(|b| b != b'\n') {
                    self.bump();
                }
                continue;
            }

            if self.peek_byte() == Some(b'/') && self.peek_byte_at(1) == Some(b'*') {
                self.skip_block_comment()?;
                continue;
            }

            break;
        }
        Ok(())
    }

    fn skip_block_comment(&mut self) -> Result<(), LexError> {
        let start = self.pos;
        self.pos += 2; // /*

        while !self.is_eof() {
            if self.peek_byte() == Some(b'*') && self.peek_byte_at(1) == Some(b'/') {
                self.pos += 2;
                return Ok(());
            }
            self.bump();
        }

        Err(LexError::new(
            LexErrorKind::UnterminatedBlockComment,
            Self::mk_span(start, self.pos),
        ))
    }

    #[inline]
    fn lex_identifier_or_keyword(&mut self) -> SpannedToken {
        let start = self.pos;
        self.bump();

        while self.pos < self.bytes.len()
            && crate::ascii_lut::is_ident_continue(self.bytes[self.pos])
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

    fn lex_operator_or_punct(&mut self) -> Result<SpannedToken, LexError> {
        let start = self.pos;
        let Some(b) = self.bump() else {
            debug_assert!(false, "lex_operator_or_punct called at EOF");
            return Err(LexError::new(
                LexErrorKind::UnexpectedChar('\0'),
                Self::mk_span(start, start),
            ));
        };

        let kind = match b {
            b'(' => Token::Punct(Punctuation::LParen),
            b')' => Token::Punct(Punctuation::RParen),
            b',' => Token::Punct(Punctuation::Comma),
            b';' => Token::Punct(Punctuation::Semicolon),
            b'*' => Token::Punct(Punctuation::Star),
            b'.' => Token::Punct(Punctuation::Dot),
            b'=' => Token::Op(Operator::Eq),
            b'+' => Token::Op(Operator::Plus),
            b'-' => Token::Op(Operator::Minus),
            b'!' if self.peek_byte() == Some(b'=') => {
                self.bump();
                Token::Op(Operator::Ne)
            }
            b'!' => {
                return Err(LexError::new(
                    LexErrorKind::UnexpectedChar('!'),
                    Self::mk_span(start, self.pos),
                ));
            }
            b'<' if self.peek_byte() == Some(b'=') => {
                self.bump();
                Token::Op(Operator::Le)
            }
            b'<' if self.peek_byte() == Some(b'>') => {
                self.bump();
                Token::Op(Operator::Ne)
            }
            b'<' => Token::Op(Operator::Lt),
            b'>' if self.peek_byte() == Some(b'=') => {
                self.bump();
                Token::Op(Operator::Ge)
            }
            b'>' => Token::Op(Operator::Gt),
            _ => {
                return Err(LexError::new(
                    LexErrorKind::UnexpectedChar(char::from(b)),
                    Self::mk_span(start, self.pos),
                ));
            }
        };

        Ok(SpannedToken::new(kind, Self::mk_span(start, self.pos)))
    }

    /// Fast path for integers (single-digit literals avoid `lex_number` setup).
    #[inline]
    fn lex_number_fast(&mut self, first: u8) -> Result<SpannedToken, LexError> {
        let start = self.pos;
        if !self
            .peek_byte_at(1)
            .is_some_and(|n| n.is_ascii_digit() || n == b'.' || matches!(n, b'e' | b'E'))
        {
            self.bump();
            let digit = i64::from(first - b'0');
            return Ok(SpannedToken::new(
                Token::Integer(digit),
                Self::mk_span(start, self.pos),
            ));
        }
        self.lex_number()
    }

    fn lex_number(&mut self) -> Result<SpannedToken, LexError> {
        let start = self.pos;
        let mut is_float = false;

        if self.peek_byte() == Some(b'.') {
            is_float = true;
            self.bump();
            if !self.peek_byte().is_some_and(|n| n.is_ascii_digit()) {
                return Err(LexError::new(
                    LexErrorKind::UnexpectedChar('.'),
                    Self::mk_span(start, self.pos),
                ));
            }
        }

        while self.peek_byte().is_some_and(|n| n.is_ascii_digit()) {
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
            if !self.peek_byte().is_some_and(|n| n.is_ascii_digit()) {
                let lit = &self.src[start..self.pos];
                return Err(LexError::new(
                    LexErrorKind::InvalidFloat(lit.to_owned()),
                    Self::mk_span(start, self.pos),
                ));
            }
            while self.peek_byte().is_some_and(|n| n.is_ascii_digit()) {
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
            parse_ascii_int(lit.as_bytes()).map_or_else(
                || {
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
        self.bump();

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
                Some(byte) => {
                    return Err(LexError::new(
                        LexErrorKind::UnexpectedChar(char::from(byte)),
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
        self.bump();

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
                Some(byte) => {
                    return Err(LexError::new(
                        LexErrorKind::UnexpectedChar(char::from(byte)),
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

/// Parse a non-empty ASCII integer literal without allocating.
#[inline]
fn parse_ascii_int(bytes: &[u8]) -> Option<i64> {
    let mut n: i64 = 0;
    for &b in bytes {
        if !b.is_ascii_digit() {
            return None;
        }
        n = n.checked_mul(10)?.checked_add(i64::from(b - b'0'))?;
    }
    Some(n)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::token::Keyword;

    fn kinds(src: &str) -> Result<Vec<Token>, LexError> {
        let mut c = Cursor::new(src);
        let mut out = Vec::new();
        while let Some(tok) = c.next_token()? {
            out.push(tok.kind);
        }
        Ok(out)
    }

    #[test]
    fn tokenizes_comparison_operators() {
        let toks = kinds("a = b != c <> d < e > f <= g >=").unwrap();
        assert_eq!(
            toks,
            vec![
                Token::Ident,
                Token::Op(Operator::Eq),
                Token::Ident,
                Token::Op(Operator::Ne),
                Token::Ident,
                Token::Op(Operator::Ne),
                Token::Ident,
                Token::Op(Operator::Lt),
                Token::Ident,
                Token::Op(Operator::Gt),
                Token::Ident,
                Token::Op(Operator::Le),
                Token::Ident,
                Token::Op(Operator::Ge),
            ]
        );
    }

    #[test]
    fn tokenizes_punctuation() {
        assert_eq!(
            kinds("SELECT a, b FROM t;").unwrap(),
            vec![
                Token::Keyword(Keyword::Select),
                Token::Ident,
                Token::Punct(Punctuation::Comma),
                Token::Ident,
                Token::Keyword(Keyword::From),
                Token::Ident,
                Token::Punct(Punctuation::Semicolon),
            ]
        );
    }

    #[test]
    fn tokenizes_star_and_dot() {
        assert_eq!(
            kinds("SELECT t.* FROM t.col").unwrap(),
            vec![
                Token::Keyword(Keyword::Select),
                Token::Ident,
                Token::Punct(Punctuation::Dot),
                Token::Punct(Punctuation::Star),
                Token::Keyword(Keyword::From),
                Token::Ident,
                Token::Punct(Punctuation::Dot),
                Token::Ident,
            ]
        );
    }

    #[test]
    fn skips_line_comments() {
        assert_eq!(
            kinds("-- leading comment\nSELECT 1").unwrap(),
            vec![Token::Keyword(Keyword::Select), Token::Integer(1)]
        );
        assert_eq!(
            kinds("SELECT 1 -- trailing").unwrap(),
            vec![Token::Keyword(Keyword::Select), Token::Integer(1)]
        );
    }

    #[test]
    fn skips_block_comments() {
        assert_eq!(
            kinds("SELECT /* block */ 1").unwrap(),
            vec![Token::Keyword(Keyword::Select), Token::Integer(1)]
        );
    }

    #[test]
    fn rejects_unterminated_block_comment() {
        let err = kinds("SELECT /* oops").unwrap_err();
        assert!(matches!(err.kind, LexErrorKind::UnterminatedBlockComment));
    }

    #[test]
    fn tokenizes_full_where_clause() {
        let src = "SELECT * FROM users WHERE id = 42 AND active IS NOT NULL";
        let toks = kinds(src).unwrap();
        assert_eq!(toks[0], Token::Keyword(Keyword::Select));
        assert_eq!(toks[1], Token::Punct(Punctuation::Star));
        assert_eq!(toks[2], Token::Keyword(Keyword::From));
        assert!(toks.contains(&Token::Op(Operator::Eq)));
        assert!(toks.contains(&Token::Keyword(Keyword::And)));
        assert!(toks.contains(&Token::Keyword(Keyword::Is)));
        assert!(toks.contains(&Token::Keyword(Keyword::Not)));
        assert!(toks.contains(&Token::Keyword(Keyword::Null)));
    }
}
