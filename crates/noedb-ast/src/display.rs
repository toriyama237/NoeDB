//! `Display` implementations for SQL round-trip.

use core::fmt;

use noedb_lexer::{Keyword, Operator};

use crate::expr::{BinaryOp, Expr, Literal, SelectItem, UnaryOp};
use crate::name::{ColumnRef, Ident, TableRef};
use crate::stmt::{
    ColumnDef, CreateIndexStmt, CreateTableStmt, DeleteStmt, DropTableStmt, InsertStmt, Join,
    JoinKind, SelectStmt, SqlType, Statement, UpdateStmt, WithClause,
};
use crate::window::{FrameBound, FrameMode, WindowFrame, WindowSpec};

fn write_ident(f: &mut fmt::Formatter<'_>, ident: &Ident) -> fmt::Result {
    if ident.value.chars().all(is_simple_ident) {
        write!(f, "{}", ident.value)
    } else {
        write!(f, "\"{}\"", ident.value.replace('"', "\"\""))
    }
}

fn is_simple_ident(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn write_keyword(f: &mut fmt::Formatter<'_>, kw: Keyword) -> fmt::Result {
    write!(f, "{}", keyword_display(kw))
}

fn keyword_display(kw: Keyword) -> &'static str {
    match kw {
        Keyword::Select => "SELECT",
        Keyword::From => "FROM",
        Keyword::Where => "WHERE",
        Keyword::Insert => "INSERT",
        Keyword::Into => "INTO",
        Keyword::Values => "VALUES",
        Keyword::Update => "UPDATE",
        Keyword::Set => "SET",
        Keyword::Delete => "DELETE",
        Keyword::Create => "CREATE",
        Keyword::Drop => "DROP",
        Keyword::Table => "TABLE",
        Keyword::Index => "INDEX",
        Keyword::On => "ON",
        Keyword::Join => "JOIN",
        Keyword::Inner => "INNER",
        Keyword::Left => "LEFT",
        Keyword::Outer => "OUTER",
        Keyword::Distinct => "DISTINCT",
        Keyword::As => "AS",
        Keyword::And => "AND",
        Keyword::Or => "OR",
        Keyword::Not => "NOT",
        Keyword::Null => "NULL",
        Keyword::Is => "IS",
        Keyword::In => "IN",
        Keyword::Between => "BETWEEN",
        Keyword::Like => "LIKE",
        Keyword::True => "TRUE",
        Keyword::False => "FALSE",
        Keyword::Alter => "ALTER",
        Keyword::Enable => "ENABLE",
        Keyword::Execute => "EXECUTE",
        Keyword::Prepare => "PREPARE",
        Keyword::Policy => "POLICY",
        Keyword::Row => "ROW",
        Keyword::Level => "LEVEL",
        Keyword::Security => "SECURITY",
        Keyword::Role => "ROLE",
        Keyword::Using => "USING",
        Keyword::CurrentUser => "CURRENT_USER",
        Keyword::Over => "OVER",
        Keyword::Partition => "PARTITION",
        Keyword::Range => "RANGE",
        Keyword::Recursive => "RECURSIVE",
        Keyword::Rows => "ROWS",
        Keyword::Unbounded => "UNBOUNDED",
        Keyword::Preceding => "PRECEDING",
        Keyword::Following => "FOLLOWING",
        _ => "KEYWORD",
    }
}

impl fmt::Display for Ident {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_ident(f, self)
    }
}

impl fmt::Display for Literal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Null { .. } => write_keyword(f, Keyword::Null),
            Self::Integer(v, _) => write!(f, "{v}"),
            Self::Float(v, _) => write!(f, "{v}"),
            Self::String(s, _) => {
                write!(f, "'{}'", s.replace('\'', "''"))
            }
            Self::Boolean(true, _) => write_keyword(f, Keyword::True),
            Self::Boolean(false, _) => write_keyword(f, Keyword::False),
        }
    }
}

impl fmt::Display for ColumnRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Star { .. } => write!(f, "*"),
            Self::QualifiedStar { table, .. } => {
                write_ident(f, table)?;
                write!(f, ".*")
            }
            Self::Named {
                table: None,
                column,
            } => write_ident(f, column),
            Self::Named {
                table: Some(t),
                column,
            } => {
                write_ident(f, t)?;
                write!(f, ".")?;
                write_ident(f, column)
            }
        }
    }
}

impl fmt::Display for Expr {
    #[allow(clippy::too_many_lines)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Literal(l) => write!(f, "{l}"),
            Self::Column(c) => write!(f, "{c}"),
            Self::Binary {
                op, left, right, ..
            } => {
                write!(f, "{left}")?;
                write!(f, " {}", binary_op(*op))?;
                write!(f, " {right}")
            }
            Self::Unary { op, expr, .. } => match op {
                UnaryOp::Not => {
                    write_keyword(f, Keyword::Not)?;
                    write!(f, " {expr}")
                }
                UnaryOp::Minus => write!(f, "-{expr}"),
            },
            Self::IsNull { expr, negated, .. } => {
                write!(f, "{expr}")?;
                write!(f, " ")?;
                write_keyword(f, Keyword::Is)?;
                if *negated {
                    write!(f, " ")?;
                    write_keyword(f, Keyword::Not)?;
                }
                write!(f, " ")?;
                write_keyword(f, Keyword::Null)
            }
            Self::In {
                expr,
                values,
                negated,
                ..
            } => {
                write!(f, "{expr}")?;
                if *negated {
                    write!(f, " ")?;
                    write_keyword(f, Keyword::Not)?;
                }
                write!(f, " ")?;
                write_keyword(f, Keyword::In)?;
                write!(f, " (")?;
                for (i, v) in values.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{v}")?;
                }
                write!(f, ")")
            }
            Self::InSubquery {
                expr,
                query,
                negated,
                ..
            } => {
                write!(f, "{expr}")?;
                if *negated {
                    write!(f, " ")?;
                    write_keyword(f, Keyword::Not)?;
                }
                write!(f, " ")?;
                write_keyword(f, Keyword::In)?;
                write!(f, " (")?;
                write!(f, "{query}")?;
                write!(f, ")")
            }
            Self::Between {
                expr,
                low,
                high,
                negated,
                ..
            } => {
                write!(f, "{expr}")?;
                if *negated {
                    write!(f, " ")?;
                    write_keyword(f, Keyword::Not)?;
                }
                write!(f, " ")?;
                write_keyword(f, Keyword::Between)?;
                write!(f, " {low} ")?;
                write_keyword(f, Keyword::And)?;
                write!(f, " {high}")
            }
            Self::Paren(inner, _) => write!(f, "({inner})"),
            Self::Parameter { index, .. } => write!(f, "${index}"),
            Self::CurrentUser { .. } => write_keyword(f, Keyword::CurrentUser),
            Self::Function {
                name, args, over, ..
            } => {
                write_ident(f, name)?;
                write!(f, "(")?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{arg}")?;
                }
                write!(f, ")")?;
                if let Some(spec) = over {
                    write!(f, " {spec}")?;
                }
                Ok(())
            }
        }
    }
}

impl fmt::Display for WindowSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_keyword(f, Keyword::Over)?;
        write!(f, " (")?;
        if !self.partition_by.is_empty() {
            write_keyword(f, Keyword::Partition)?;
            write_keyword(f, Keyword::By)?;
            write!(f, " ")?;
            for (i, e) in self.partition_by.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{e}")?;
            }
        }
        if !self.order_by.is_empty() {
            if !self.partition_by.is_empty() {
                write!(f, " ")?;
            }
            write_keyword(f, Keyword::Order)?;
            write_keyword(f, Keyword::By)?;
            for (i, key) in self.order_by.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                } else {
                    write!(f, " ")?;
                }
                write!(f, "{}", key.expr)?;
                write!(f, " ")?;
                if key.asc {
                    write_keyword(f, Keyword::Asc)?;
                } else {
                    write_keyword(f, Keyword::Desc)?;
                }
            }
        }
        if let Some(frame) = &self.frame {
            write!(f, " ")?;
            write_frame(f, frame)?;
        }
        write!(f, ")")
    }
}

fn write_frame(f: &mut fmt::Formatter<'_>, frame: &WindowFrame) -> fmt::Result {
    match frame.mode {
        FrameMode::Rows => write_keyword(f, Keyword::Rows)?,
        FrameMode::Range => write_keyword(f, Keyword::Range)?,
    }
    write_keyword(f, Keyword::Between)?;
    write!(f, " ")?;
    write_frame_bound(f, frame.start)?;
    write_keyword(f, Keyword::And)?;
    write!(f, " ")?;
    write_frame_bound(f, frame.end)
}

fn write_frame_bound(f: &mut fmt::Formatter<'_>, bound: FrameBound) -> fmt::Result {
    match bound {
        FrameBound::UnboundedPreceding => {
            write_keyword(f, Keyword::Unbounded)?;
            write!(f, " ")?;
            write_keyword(f, Keyword::Preceding)
        }
        FrameBound::Preceding(n) => {
            write!(f, "{n} ")?;
            write_keyword(f, Keyword::Preceding)
        }
        FrameBound::CurrentRow => {
            write_keyword(f, Keyword::Current)?;
            write!(f, " ")?;
            write_keyword(f, Keyword::Row)
        }
        FrameBound::Following(n) => {
            write!(f, "{n} ")?;
            write_keyword(f, Keyword::Following)
        }
        FrameBound::UnboundedFollowing => {
            write_keyword(f, Keyword::Unbounded)?;
            write!(f, " ")?;
            write_keyword(f, Keyword::Following)
        }
    }
}

fn binary_op(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Eq => "=",
        BinaryOp::Ne => "<>",
        BinaryOp::Lt => "<",
        BinaryOp::Gt => ">",
        BinaryOp::Le => "<=",
        BinaryOp::Ge => ">=",
        BinaryOp::And => "AND",
        BinaryOp::Or => "OR",
        BinaryOp::Plus => "+",
        BinaryOp::Minus => "-",
        BinaryOp::Like => "LIKE",
    }
}

impl fmt::Display for SelectItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.expr)?;
        if let Some(alias) = &self.alias {
            write!(f, " ")?;
            write_keyword(f, Keyword::As)?;
            write!(f, " {alias}")?;
        }
        Ok(())
    }
}

impl fmt::Display for TableRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_ident(f, &self.name)?;
        if let Some(alias) = &self.alias {
            write!(f, " ")?;
            write_keyword(f, Keyword::As)?;
            write!(f, " {alias}")?;
        }
        Ok(())
    }
}

impl fmt::Display for JoinKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Inner => {
                write_keyword(f, Keyword::Inner)?;
                write!(f, " ")?;
                write_keyword(f, Keyword::Join)
            }
            Self::Left => {
                write_keyword(f, Keyword::Left)?;
                write!(f, " ")?;
                write_keyword(f, Keyword::Join)
            }
        }
    }
}

impl fmt::Display for Join {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.kind, self.table)?;
        write!(f, " ")?;
        write_keyword(f, Keyword::On)?;
        write!(f, " {}", self.on)
    }
}

impl fmt::Display for SelectStmt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(with) = &self.with_clause {
            write!(f, "{with} ")?;
        }
        write_keyword(f, Keyword::Select)?;
        if self.distinct {
            write!(f, " ")?;
            write_keyword(f, Keyword::Distinct)?;
        }
        write!(f, " ")?;
        for (i, item) in self.items.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{item}")?;
        }
        if let Some(from) = &self.from {
            write!(f, " ")?;
            write_keyword(f, Keyword::From)?;
            write!(f, " {from}")?;
        }
        for join in &self.joins {
            write!(f, " {join}")?;
        }
        if let Some(w) = &self.where_clause {
            write!(f, " ")?;
            write_keyword(f, Keyword::Where)?;
            write!(f, " {w}")?;
        }
        Ok(())
    }
}

impl fmt::Display for WithClause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_keyword(f, Keyword::With)?;
        if self.recursive {
            write!(f, " ")?;
            write_keyword(f, Keyword::Recursive)?;
        }
        for (i, cte) in self.ctes.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            } else {
                write!(f, " ")?;
            }
            write_ident(f, &cte.name)?;
            write!(f, " ")?;
            write_keyword(f, Keyword::As)?;
            write!(f, " (")?;
            write!(f, "{}", cte.body)?;
            write!(f, ")")?;
        }
        Ok(())
    }
}

impl fmt::Display for crate::stmt::CteBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use crate::stmt::CteBody;
        match self {
            CteBody::Select(s) => write!(f, "{s}"),
            CteBody::Union {
                anchor,
                all,
                recursive,
            } => {
                write!(f, "{anchor}")?;
                write!(f, " ")?;
                write_keyword(f, Keyword::Union)?;
                if *all {
                    write!(f, " ")?;
                    write_keyword(f, Keyword::All)?;
                }
                write!(f, " {recursive}")
            }
        }
    }
}

impl fmt::Display for SqlType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int => write!(f, "INT"),
            Self::Boolean => write!(f, "BOOLEAN"),
            Self::Float => write!(f, "FLOAT"),
            Self::Varchar { max_len: None } => write!(f, "VARCHAR"),
            Self::Varchar { max_len: Some(n) } => write!(f, "VARCHAR({n})"),
            Self::Named(s) => write!(f, "{s}"),
        }
    }
}

impl fmt::Display for ColumnDef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_ident(f, &self.name)?;
        write!(f, " {}", self.data_type)?;
        if self.not_null {
            write!(f, " ")?;
            write_keyword(f, Keyword::Not)?;
            write!(f, " ")?;
            write_keyword(f, Keyword::Null)?;
        }
        Ok(())
    }
}

impl fmt::Display for InsertStmt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_keyword(f, Keyword::Insert)?;
        write!(f, " ")?;
        write_keyword(f, Keyword::Into)?;
        write!(f, " {}", self.table)?;
        if let Some(cols) = &self.columns {
            write!(f, " (")?;
            for (i, c) in cols.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{c}")?;
            }
            write!(f, ")")?;
        }
        write!(f, " ")?;
        write_keyword(f, Keyword::Values)?;
        for (i, row) in self.values.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, " (")?;
            for (j, v) in row.iter().enumerate() {
                if j > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{v}")?;
            }
            write!(f, ")")?;
        }
        Ok(())
    }
}

impl fmt::Display for UpdateStmt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_keyword(f, Keyword::Update)?;
        write!(f, " {}", self.table)?;
        write!(f, " ")?;
        write_keyword(f, Keyword::Set)?;
        for (i, (col, val)) in self.assignments.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, " {col} = {val}")?;
        }
        if let Some(w) = &self.where_clause {
            write!(f, " ")?;
            write_keyword(f, Keyword::Where)?;
            write!(f, " {w}")?;
        }
        Ok(())
    }
}

impl fmt::Display for DeleteStmt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_keyword(f, Keyword::Delete)?;
        write!(f, " ")?;
        write_keyword(f, Keyword::From)?;
        write!(f, " {}", self.table)?;
        if let Some(w) = &self.where_clause {
            write!(f, " ")?;
            write_keyword(f, Keyword::Where)?;
            write!(f, " {w}")?;
        }
        Ok(())
    }
}

impl fmt::Display for CreateTableStmt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_keyword(f, Keyword::Create)?;
        write!(f, " ")?;
        write_keyword(f, Keyword::Table)?;
        write!(f, " {} (", self.name)?;
        for (i, col) in self.columns.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{col}")?;
        }
        write!(f, ")")
    }
}

impl fmt::Display for DropTableStmt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_keyword(f, Keyword::Drop)?;
        write!(f, " ")?;
        write_keyword(f, Keyword::Table)?;
        write!(f, " {}", self.name)
    }
}

impl fmt::Display for CreateIndexStmt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_keyword(f, Keyword::Create)?;
        write!(f, " ")?;
        write_keyword(f, Keyword::Index)?;
        write!(f, " {} ", self.name)?;
        write_keyword(f, Keyword::On)?;
        write!(f, " {} (", self.table)?;
        for (i, c) in self.columns.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{c}")?;
        }
        write!(f, ")")
    }
}

impl fmt::Display for Statement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Select(s) => write!(f, "{s}"),
            Self::Insert(s) => write!(f, "{s}"),
            Self::Update(s) => write!(f, "{s}"),
            Self::Delete(s) => write!(f, "{s}"),
            Self::CreateTable(s) => write!(f, "{s}"),
            Self::DropTable(s) => write!(f, "{s}"),
            Self::CreateIndex(s) => write!(f, "{s}"),
            Self::Prepare(s) => {
                write_keyword(f, Keyword::Prepare)?;
                write!(f, " {} ", s.name)?;
                write_keyword(f, Keyword::As)?;
                write!(f, " {}", s.inner)
            }
            Self::Execute(s) => {
                write_keyword(f, Keyword::Execute)?;
                write!(f, " {}", s.name)?;
                if !s.params.is_empty() {
                    write!(f, " (")?;
                    for (i, p) in s.params.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{p}")?;
                    }
                    write!(f, ")")?;
                }
                Ok(())
            }
            Self::SetRole(s) => {
                write_keyword(f, Keyword::Set)?;
                write!(f, " ")?;
                write_keyword(f, Keyword::Role)?;
                write!(f, " '{}'", s.role.replace('\'', "''"))
            }
            Self::EnableRls(s) => {
                write_keyword(f, Keyword::Alter)?;
                write!(f, " ")?;
                write_keyword(f, Keyword::Table)?;
                write!(f, " {} ", s.table)?;
                write_keyword(f, Keyword::Enable)?;
                write!(f, " ")?;
                write_keyword(f, Keyword::Row)?;
                write!(f, " ")?;
                write_keyword(f, Keyword::Level)?;
                write!(f, " ")?;
                write_keyword(f, Keyword::Security)
            }
            Self::CreatePolicy(s) => {
                write_keyword(f, Keyword::Create)?;
                write!(f, " ")?;
                write_keyword(f, Keyword::Policy)?;
                write!(f, " {} ", s.name)?;
                write_keyword(f, Keyword::On)?;
                write!(f, " {} ", s.table)?;
                write_keyword(f, Keyword::Using)?;
                write!(f, " ({})", s.using_expr)
            }
            Self::BeginTxn(_) => write_keyword(f, Keyword::Begin),
            Self::CommitTxn(_) => write_keyword(f, Keyword::Commit),
            Self::RollbackTxn(_) => write_keyword(f, Keyword::Rollback),
        }
    }
}

#[allow(dead_code)]
fn operator_display(op: Operator) -> &'static str {
    match op {
        Operator::Eq => "=",
        Operator::Ne => "<>",
        Operator::Lt => "<",
        Operator::Gt => ">",
        Operator::Le => "<=",
        Operator::Ge => ">=",
        Operator::Plus => "+",
        Operator::Minus => "-",
        _ => "?",
    }
}
