//! Day 1 of the lexer.
//!
//! Goal of this file today: tokenize a *single* SQL statement - `SELECT 1` -
//! and nothing else. Everything below is intentionally minimal. The token enum
//! will grow to 40+ variants this week (see sprint plan, Week 01).

/// A SQL token.
///
/// This enum will grow significantly during Week 01. Today it carries the
/// strict minimum required to tokenize `SELECT 1`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    /// The `SELECT` keyword.
    Select,
    /// An integer literal, e.g. `1`, `42`.
    Number(i64),
    /// End of input.
    Eof,
}

/// Errors produced by the lexer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LexError {
    /// An unexpected character was found at the given byte offset.
    UnexpectedChar {
        /// The offending character.
        ch: char,
        /// Byte offset in the source where the character was found.
        offset: usize,
    },
}

impl core::fmt::Display for LexError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnexpectedChar { ch, offset } => {
                write!(f, "unexpected character {ch:?} at byte offset {offset}")
            }
        }
    }
}

impl std::error::Error for LexError {}

/// Tokenize a SQL source string.
///
/// This is a *day 1* implementation: it only knows the `SELECT` keyword
/// (case-insensitive) and base-10 integer literals, separated by ASCII
/// whitespace. Any other character is a hard error.
///
/// # Errors
///
/// Returns [`LexError::UnexpectedChar`] when an unsupported character is
/// encountered.
///
/// # Examples
///
/// ```
/// use noedb::lexer::{tokenize, Token};
///
/// # fn main() -> Result<(), noedb::lexer::LexError> {
/// let toks = tokenize("SELECT 1")?;
/// assert_eq!(toks, vec![Token::Select, Token::Number(1), Token::Eof]);
/// # Ok(())
/// # }
/// ```
pub fn tokenize(src: &str) -> Result<Vec<Token>, LexError> {
    let mut tokens = Vec::new();
    let bytes = src.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];

        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }

        if b.is_ascii_alphabetic() {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let word = &src[start..i];
            if word.eq_ignore_ascii_case("SELECT") {
                tokens.push(Token::Select);
            } else {
                return Err(LexError::UnexpectedChar {
                    ch: word.chars().next().unwrap_or('?'),
                    offset: start,
                });
            }
            continue;
        }

        if b.is_ascii_digit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            let n: i64 = src[start..i]
                .parse()
                .map_err(|_| LexError::UnexpectedChar {
                    ch: char::from(bytes[start]),
                    offset: start,
                })?;
            tokens.push(Token::Number(n));
            continue;
        }

        return Err(LexError::UnexpectedChar {
            ch: char::from(b),
            offset: i,
        });
    }

    tokens.push(Token::Eof);
    Ok(tokens)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_select_one() {
        let toks = tokenize("SELECT 1").unwrap();
        assert_eq!(toks, vec![Token::Select, Token::Number(1), Token::Eof]);
    }

    #[test]
    fn tokenize_is_case_insensitive_on_keywords() {
        let toks = tokenize("select 42").unwrap();
        assert_eq!(toks, vec![Token::Select, Token::Number(42), Token::Eof]);
    }

    #[test]
    fn tokenize_handles_extra_whitespace() {
        let toks = tokenize("   SELECT\t\n  7  ").unwrap();
        assert_eq!(toks, vec![Token::Select, Token::Number(7), Token::Eof]);
    }

    #[test]
    fn tokenize_empty_input_yields_only_eof() {
        let toks = tokenize("").unwrap();
        assert_eq!(toks, vec![Token::Eof]);
    }

    #[test]
    fn tokenize_rejects_unknown_punctuation() {
        let err = tokenize("SELECT @").unwrap_err();
        assert!(matches!(err, LexError::UnexpectedChar { ch: '@', .. }));
    }

    #[test]
    fn tokenize_rejects_unknown_keyword() {
        let err = tokenize("INSERT 1").unwrap_err();
        assert!(matches!(err, LexError::UnexpectedChar { .. }));
    }
}
