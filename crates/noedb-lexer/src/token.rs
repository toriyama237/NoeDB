//! Token definitions.
//!
//! [`Token`] stays small on the stack: keywords are a dense `enum`,
//! identifiers are zero-copy (recover the lexeme via [`Span::slice`]),
//! and only string / quoted-identifier literals allocate.

use crate::span::Span;

/// A SQL-92 reserved keyword.
///
/// Grows through Week 04 as the lexer learns more of the grammar.
/// Non-reserved words (table names, column names) become [`Token::Ident`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
#[allow(missing_docs)] // per-variant docs land in Week 04 with the full token set
pub enum Keyword {
    All,
    Alter,
    Analyze,
    And,
    Any,
    As,
    Asc,
    Authorization,
    Between,
    Begin,
    By,
    Case,
    Cast,
    Check,
    Collate,
    Column,
    Commit,
    Constraint,
    Create,
    Cross,
    Current,
    CurrentDate,
    CurrentTime,
    CurrentTimestamp,
    CurrentUser,
    Default,
    Delete,
    Desc,
    Distinct,
    Drop,
    Else,
    Enable,
    End,
    Escape,
    Except,
    Execute,
    Exists,
    False,
    Fetch,
    Following,
    For,
    Foreign,
    From,
    Full,
    Grant,
    Group,
    Having,
    If,
    In,
    Index,
    Inner,
    Insert,
    Intersect,
    Into,
    Is,
    Join,
    Key,
    Left,
    Level,
    Like,
    Limit,
    Natural,
    Not,
    Null,
    Nullif,
    Of,
    Offset,
    On,
    Or,
    Order,
    Over,
    Outer,
    Partition,
    Policy,
    Preceding,
    Prepare,
    Primary,
    Range,
    Recursive,
    References,
    Right,
    Role,
    Rows,
    Rollback,
    Row,
    Select,
    Security,
    Set,
    Some,
    Table,
    Then,
    To,
    True,
    Union,
    Unbounded,
    Unique,
    Update,
    User,
    Using,
    Values,
    View,
    When,
    Where,
    With,
}

/// A SQL comparison or arithmetic operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Operator {
    /// `=`
    Eq,
    /// `!=` or `<>` (both spellings produce the same token).
    Ne,
    /// `<`
    Lt,
    /// `>`
    Gt,
    /// `<=`
    Le,
    /// `>=`
    Ge,
    /// `-` (unary or binary — disambiguation is the parser's job).
    Minus,
    /// `+`
    Plus,
    /// `/`
    Div,
    /// `%`
    Mod,
    /// `<->` pgvector-style distance
    Distance,
}

/// SQL punctuation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Punctuation {
    /// `(`
    LParen,
    /// `)`
    RParen,
    /// `,`
    Comma,
    /// `;`
    Semicolon,
    /// `.` (table.column — not part of a numeric literal).
    Dot,
    /// `*`
    Star,
}

/// A SQL token.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Token {
    /// A reserved keyword.
    Keyword(Keyword),
    /// An ordinary identifier (`users`, `id`, …).
    ///
    /// The lexeme is **not** stored here — use [`Span::slice`] on the
    /// paired [`SpannedToken`] to recover `&str` from the source without
    /// allocation.
    Ident,
    /// A delimited identifier (`"weird name"`, SQL-92 double quotes).
    QuotedIdent(String),
    /// A base-10 integer literal fitting in `i64`.
    Integer(i64),
    /// An approximate numeric literal (`1.5`, `.5`, `1e3`).
    Float(f64),
    /// A single-quoted character string literal (SQL escape: `''` → `'`).
    String(String),
    /// A comparison or arithmetic operator.
    Op(Operator),
    /// Parentheses, commas, semicolons, etc.
    Punct(Punctuation),
    /// End of input. Always the last token in the stream.
    Eof,
    /// Prepared-statement placeholder (`$1`, `$2`, …).
    Parameter(u16),
}

/// A [`Token`] paired with its position in the original source.
#[derive(Debug, Clone, PartialEq)]
pub struct SpannedToken {
    /// The token itself.
    pub kind: Token,
    /// Where the token was found in the source, in byte offsets.
    pub span: Span,
}

impl SpannedToken {
    /// Construct a new `SpannedToken`.
    #[inline]
    #[must_use]
    pub const fn new(kind: Token, span: Span) -> Self {
        Self { kind, span }
    }

    /// Recover the source text for this token, if `src` is the original input.
    #[inline]
    #[must_use]
    pub fn lexeme<'a>(&self, src: &'a str) -> Option<&'a str> {
        self.span.slice(src)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn spanned_token_is_equality_comparable() {
        let a = SpannedToken::new(Token::Keyword(Keyword::Select), Span::new(0, 6));
        let b = SpannedToken::new(Token::Keyword(Keyword::Select), Span::new(0, 6));
        let c = SpannedToken::new(Token::Integer(1), Span::new(7, 8));
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
