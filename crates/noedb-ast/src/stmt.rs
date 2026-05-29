//! Statement AST nodes.

use noedb_lexer::Span;

use crate::expr::{Expr, Literal, SelectItem};
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

/// Body of a `WITH … AS (…)` common table expression.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum CteBody {
    /// Single `SELECT`.
    Select(SelectStmt),
    /// `anchor UNION [ALL] recursive` (Phase 5 Week 40).
    Union {
        /// Non-recursive anchor query.
        anchor: Box<SelectStmt>,
        /// Whether `ALL` was specified (`UNION ALL`).
        all: bool,
        /// Recursive member referencing the CTE name.
        recursive: Box<SelectStmt>,
    },
}

/// One named CTE in a `WITH` clause.
#[derive(Debug, Clone, PartialEq)]
pub struct CteDef {
    /// CTE name (`WITH name AS …`).
    pub name: Ident,
    /// Parenthesized query body.
    pub body: CteBody,
    /// Span covering `name AS (…)`.
    pub span: Span,
}

/// `WITH [RECURSIVE] …` clause on a `SELECT`.
#[derive(Debug, Clone, PartialEq)]
pub struct WithClause {
    /// Whether `RECURSIVE` was specified.
    pub recursive: bool,
    /// CTE definitions in source order.
    pub ctes: Vec<CteDef>,
    /// Span covering the whole clause.
    pub span: Span,
}

/// `SELECT` statement.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectStmt {
    /// Optional leading `WITH` clause.
    pub with_clause: Option<WithClause>,
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

/// `CREATE POLICY … ON … USING (…)`.
#[derive(Debug, Clone, PartialEq)]
pub struct CreatePolicyStmt {
    /// Policy name.
    pub name: Ident,
    /// Protected table.
    pub table: Ident,
    /// Row filter expression.
    pub using_expr: Expr,
    /// Span covering the statement.
    pub span: Span,
}

/// `ALTER TABLE … ENABLE ROW LEVEL SECURITY`.
#[derive(Debug, Clone, PartialEq)]
pub struct EnableRlsStmt {
    /// Table name.
    pub table: Ident,
    /// Span covering the statement.
    pub span: Span,
}

/// `PREPARE name AS …`.
#[derive(Debug, Clone, PartialEq)]
pub struct PrepareStmt {
    /// Statement name.
    pub name: Ident,
    /// Inner statement (typically `SELECT`).
    pub inner: Box<Statement>,
    /// Span covering the statement.
    pub span: Span,
}

/// `EXECUTE name ($1, …)` with bound literals only.
#[derive(Debug, Clone, PartialEq)]
pub struct ExecuteStmt {
    /// Prepared statement name.
    pub name: Ident,
    /// Bound parameter values (typed literals, never interpolated SQL).
    pub params: Vec<Literal>,
    /// Span covering the statement.
    pub span: Span,
}

/// `SET ROLE 'user'`.
#[derive(Debug, Clone, PartialEq)]
pub struct SetRoleStmt {
    /// Role / tenant user id.
    pub role: String,
    /// Span covering the statement.
    pub span: Span,
}

/// `BEGIN` / `START TRANSACTION`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeginTxnStmt {
    /// Span covering the statement.
    pub span: Span,
}

/// `COMMIT`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitTxnStmt {
    /// Span covering the statement.
    pub span: Span,
}

/// `ROLLBACK`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollbackTxnStmt {
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
    /// `PREPARE`.
    Prepare(PrepareStmt),
    /// `EXECUTE`.
    Execute(ExecuteStmt),
    /// `SET ROLE`.
    SetRole(SetRoleStmt),
    /// `ALTER TABLE … ENABLE ROW LEVEL SECURITY`.
    EnableRls(EnableRlsStmt),
    /// `CREATE POLICY`.
    CreatePolicy(CreatePolicyStmt),
    /// `BEGIN` transaction.
    BeginTxn(BeginTxnStmt),
    /// `COMMIT` transaction.
    CommitTxn(CommitTxnStmt),
    /// `ROLLBACK` transaction.
    RollbackTxn(RollbackTxnStmt),
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
            Self::Prepare(s) => s.span,
            Self::Execute(s) => s.span,
            Self::SetRole(s) => s.span,
            Self::EnableRls(s) => s.span,
            Self::CreatePolicy(s) => s.span,
            Self::BeginTxn(s) => s.span,
            Self::CommitTxn(s) => s.span,
            Self::RollbackTxn(s) => s.span,
        }
    }
}
