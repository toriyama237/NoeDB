//! Statement AST nodes.

use noedb_lexer::Span;

use crate::expr::{Expr, SelectItem};
use crate::name::{Ident, TableRef};

/// Kind of SQL join.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JoinKind {
    /// `INNER JOIN`
    Inner,
    /// `LEFT [OUTER] JOIN`
    Left,
}

/// A `JOIN` clause.
#[derive(Debug, Clone, PartialEq)]
pub struct Join {
    /// Join kind.
    pub kind: JoinKind,
    /// Right-hand table.
    pub table: TableRef,
    /// `ON` predicate.
    pub on: Expr,
    /// Span covering the join clause.
    pub span: Span,
}

/// `SELECT` statement.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectStmt {
    /// Whether `DISTINCT` was specified.
    pub distinct: bool,
    /// Projection list.
    pub items: Vec<SelectItem>,
    /// Primary `FROM` table (optional for `SELECT 1`).
    pub from: Option<TableRef>,
    /// Additional `JOIN` clauses.
    pub joins: Vec<Join>,
    /// Optional `WHERE` predicate.
    pub where_clause: Option<Expr>,
    /// Span covering the whole statement.
    pub span: Span,
}

/// Column type in `CREATE TABLE`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SqlType {
    /// `INT` / `INTEGER`.
    Int,
    /// `BOOLEAN`.
    Boolean,
    /// `FLOAT` / `REAL` / `DOUBLE`.
    Float,
    /// `VARCHAR(n)` or bare `VARCHAR`.
    Varchar {
        /// Optional length constraint.
        max_len: Option<u32>,
    },
    /// Any other type name preserved as text (`TEXT`, `TIMESTAMP`, …).
    Named(String),
}

/// Column definition in `CREATE TABLE`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnDef {
    /// Column name.
    pub name: Ident,
    /// Declared SQL type.
    pub data_type: SqlType,
    /// Whether `NOT NULL` was specified.
    pub not_null: bool,
    /// Span of the column definition.
    pub span: Span,
}

/// `INSERT INTO … VALUES …`.
#[derive(Debug, Clone, PartialEq)]
pub struct InsertStmt {
    /// Target table.
    pub table: Ident,
    /// Optional explicit column list.
    pub columns: Option<Vec<Ident>>,
    /// One or more parenthesized value rows.
    pub values: Vec<Vec<Expr>>,
    /// Span covering the statement.
    pub span: Span,
}

/// `UPDATE … SET … [WHERE …]`.
#[derive(Debug, Clone, PartialEq)]
pub struct UpdateStmt {
    /// Target table.
    pub table: Ident,
    /// `(column, value)` assignments.
    pub assignments: Vec<(Ident, Expr)>,
    /// Optional filter.
    pub where_clause: Option<Expr>,
    /// Span covering the statement.
    pub span: Span,
}

/// `DELETE FROM … [WHERE …]`.
#[derive(Debug, Clone, PartialEq)]
pub struct DeleteStmt {
    /// Target table.
    pub table: Ident,
    /// Optional filter.
    pub where_clause: Option<Expr>,
    /// Span covering the statement.
    pub span: Span,
}

/// `CREATE TABLE …`.
#[derive(Debug, Clone, PartialEq)]
pub struct CreateTableStmt {
    /// New table name.
    pub name: Ident,
    /// Column definitions.
    pub columns: Vec<ColumnDef>,
    /// Span covering the statement.
    pub span: Span,
}

/// `DROP TABLE …`.
#[derive(Debug, Clone, PartialEq)]
pub struct DropTableStmt {
    /// Table to drop.
    pub name: Ident,
    /// Span covering the statement.
    pub span: Span,
}

/// `CREATE INDEX … ON … (cols…)`.
#[derive(Debug, Clone, PartialEq)]
pub struct CreateIndexStmt {
    /// Index name.
    pub name: Ident,
    /// Indexed table.
    pub table: Ident,
    /// Indexed columns.
    pub columns: Vec<Ident>,
    /// Span covering the statement.
    pub span: Span,
}

/// Top-level SQL statement.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Statement {
    /// `SELECT`.
    Select(SelectStmt),
    /// `INSERT`.
    Insert(InsertStmt),
    /// `UPDATE`.
    Update(UpdateStmt),
    /// `DELETE`.
    Delete(DeleteStmt),
    /// `CREATE TABLE`.
    CreateTable(CreateTableStmt),
    /// `DROP TABLE`.
    DropTable(DropTableStmt),
    /// `CREATE INDEX`.
    CreateIndex(CreateIndexStmt),
}

impl Statement {
    /// Source span of the statement.
    #[must_use]
    pub const fn span(&self) -> Span {
        match self {
            Self::Select(s) => s.span,
            Self::Insert(s) => s.span,
            Self::Update(s) => s.span,
            Self::Delete(s) => s.span,
            Self::CreateTable(s) => s.span,
            Self::DropTable(s) => s.span,
            Self::CreateIndex(s) => s.span,
        }
    }
}
