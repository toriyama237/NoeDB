//! Identifiers and name references.

use noedb_lexer::Span;

/// A SQL identifier (ordinary or delimited).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Ident {
    /// Canonical spelling as it appeared in source (case preserved).
    pub value: String,
    /// Source span of the identifier token.
    pub span: Span,
}

impl Ident {
    /// Construct a new identifier node.
    #[must_use]
    pub const fn new(value: String, span: Span) -> Self {
        Self { value, span }
    }
}

/// A column reference: bare name, qualified, or `*`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ColumnRef {
    /// `column` or `table.column`.
    Named {
        /// Optional table qualifier.
        table: Option<Ident>,
        /// Column name.
        column: Ident,
    },
    /// `*` in the select list.
    Star {
        /// Source span of `*`.
        span: Span,
    },
    /// `table.*`.
    QualifiedStar {
        /// Table alias or name.
        table: Ident,
        /// Span covering `table.*`.
        span: Span,
    },
}

/// A table reference with an optional alias.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TableRef {
    /// Base table name.
    pub name: Ident,
    /// Optional `AS alias` or bare alias.
    pub alias: Option<Ident>,
    /// Span covering the whole `FROM` item.
    pub span: Span,
}
