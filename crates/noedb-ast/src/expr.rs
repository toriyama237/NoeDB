//! Expression AST nodes.

use noedb_lexer::Span;

use crate::name::{ColumnRef, Ident};
use crate::stmt::{SelectStmt, SqlType};
use crate::window::WindowSpec;

/// A literal value.
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    /// SQL `NULL`.
    Null {
        /// Source span.
        span: Span,
    },
    /// Integer literal.
    Integer(i64, Span),
    /// Floating-point literal.
    Float(f64, Span),
    /// Single-quoted string literal.
    String(String, Span),
    /// `TRUE` / `FALSE`.
    Boolean(bool, Span),
}

/// Binary operators in expressions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinaryOp {
    /// `=`
    Eq,
    /// `!=` or `<>`
    Ne,
    /// `<`
    Lt,
    /// `>`
    Gt,
    /// `<=`
    Le,
    /// `>=`
    Ge,
    /// `AND`
    And,
    /// `OR`
    Or,
    /// `+`
    Plus,
    /// `-`
    Minus,
    /// `/`
    Div,
    /// `%`
    Mod,
    /// `*`
    Mul,
    /// `LIKE`
    Like,
    /// Vector L2 distance `<->` (pgvector-style).
    Distance,
}

/// Unary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    /// Unary `-`
    Minus,
    /// `NOT`
    Not,
}

/// A SQL expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// Literal constant.
    Literal(Literal),
    /// Column reference.
    Column(ColumnRef),
    /// Binary operation.
    Binary {
        /// Operator.
        op: BinaryOp,
        /// Left operand.
        left: Box<Expr>,
        /// Right operand.
        right: Box<Expr>,
        /// Span covering the whole expression.
        span: Span,
    },
    /// Unary operation.
    Unary {
        /// Operator.
        op: UnaryOp,
        /// Operand.
        expr: Box<Expr>,
        /// Span covering the whole expression.
        span: Span,
    },
    /// `expr IS [NOT] NULL`.
    IsNull {
        /// Inner expression.
        expr: Box<Expr>,
        /// Whether `NOT` was present.
        negated: bool,
        /// Span covering the whole predicate.
        span: Span,
    },
    /// `expr [NOT] IN (values…)`.
    In {
        /// Left-hand expression.
        expr: Box<Expr>,
        /// Parenthesized value list.
        values: Vec<Expr>,
        /// Whether `NOT IN` was used.
        negated: bool,
        /// Span covering the whole predicate.
        span: Span,
    },
    /// `expr [NOT] IN (SELECT …)` — decorrelated at plan time (Phase 5 Week 39).
    InSubquery {
        /// Left-hand expression (usually a column).
        expr: Box<Expr>,
        /// Subquery returning one column.
        query: Box<SelectStmt>,
        /// Whether `NOT IN` was used.
        negated: bool,
        /// Span covering the whole predicate.
        span: Span,
    },
    /// `[NOT] EXISTS (SELECT …)`.
    Exists {
        /// Subquery body.
        query: Box<SelectStmt>,
        /// `NOT EXISTS` when true.
        negated: bool,
        /// Span covering the whole predicate.
        span: Span,
    },
    /// `expr [NOT] BETWEEN low AND high`.
    Between {
        /// Subject expression.
        expr: Box<Expr>,
        /// Lower bound.
        low: Box<Expr>,
        /// Upper bound.
        high: Box<Expr>,
        /// Whether `NOT BETWEEN` was used.
        negated: bool,
        /// Span covering the whole predicate.
        span: Span,
    },
    /// Parenthesized sub-expression.
    Paren(Box<Expr>, Span),
    /// Prepared-statement placeholder (`$1`, …).
    Parameter {
        /// 1-based index.
        index: u16,
        /// Source span.
        span: Span,
    },
    /// `CURRENT_USER` session value (RLS policies).
    CurrentUser {
        /// Source span.
        span: Span,
    },
    /// `CAST(expr AS type)` (Phase 5 Week 42).
    Cast {
        /// Operand.
        expr: Box<Expr>,
        /// Target SQL type.
        data_type: SqlType,
        /// Span covering the whole cast.
        span: Span,
    },
    /// SQL function call (`name(args…) [OVER (…)]`).
    Function {
        /// Function name (`ROW_NUMBER`, `COUNT`, …).
        name: Ident,
        /// Call arguments (empty for `ROW_NUMBER()`).
        args: Vec<Expr>,
        /// Window specification when used as analytic function.
        over: Option<WindowSpec>,
        /// Span covering the whole call.
        span: Span,
    },
}

impl Expr {
    /// Source span of this expression node.
    #[must_use]
    pub const fn span(&self) -> Span {
        match self {
            Self::Literal(lit) => match lit {
                Literal::Null { span } => *span,
                Literal::Integer(_, s)
                | Literal::Float(_, s)
                | Literal::String(_, s)
                | Literal::Boolean(_, s) => *s,
            },
            Self::Column(col) => match col {
                ColumnRef::Star { span } | ColumnRef::QualifiedStar { span, .. } => *span,
                ColumnRef::Named { column, .. } => column.span,
            },
            Self::Binary { span, .. }
            | Self::Unary { span, .. }
            | Self::IsNull { span, .. }
            | Self::In { span, .. }
            | Self::InSubquery { span, .. }
            | Self::Exists { span, .. }
            | Self::Between { span, .. }
            | Self::Paren(_, span)
            | Self::Parameter { span, .. }
            | Self::CurrentUser { span }
            | Self::Cast { span, .. }
            | Self::Function { span, .. } => *span,
        }
    }
}

/// One item in a `SELECT` list.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectItem {
    /// Expression being projected.
    pub expr: Expr,
    /// Optional `AS alias`.
    pub alias: Option<Ident>,
}
