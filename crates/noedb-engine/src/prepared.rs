//! Prepared statement cache and parameter binding (Phase 1 Week 4).
//!
//! Parameters are bound as typed [`Literal`] values — never concatenated into SQL text.

use noedb_ast::{Expr, Literal, Statement};

use crate::error::EngineError;

/// Cached prepared statement.
#[derive(Debug, Clone)]
pub struct PreparedStatement {
    /// Parsed statement (may contain [`Expr::Parameter`] nodes).
    pub stmt: Statement,
    /// Highest `$n` index referenced.
    pub param_count: u16,
}

/// Statement cache keyed by prepared name.
#[derive(Debug, Default)]
pub struct PrepareCache {
    stmts: std::collections::HashMap<String, PreparedStatement>,
}

impl PrepareCache {
    /// Store a prepared statement.
    pub fn insert(&mut self, name: &str, stmt: Statement) -> Result<(), EngineError> {
        let param_count = max_param_index(&stmt);
        self.stmts.insert(
            name.to_string(),
            PreparedStatement { stmt, param_count },
        );
        Ok(())
    }

    /// Look up a prepared statement.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&PreparedStatement> {
        self.stmts.get(name)
    }
}

/// Bind execute parameters into a fresh statement tree (no string interpolation).
///
/// # Errors
///
/// Wrong parameter count or unknown prepared name handled by caller.
pub fn bind_parameters(stmt: &Statement, params: &[Literal]) -> Result<Statement, EngineError> {
    let need = max_param_index(stmt);
    if u16::try_from(params.len()).unwrap_or(u16::MAX) != need {
        return Err(EngineError::InvalidSql("parameter count mismatch"));
    }
    Ok(substitute_params(stmt, params))
}

fn max_param_index(stmt: &Statement) -> u16 {
    let mut max = 0u16;
    walk_stmt(stmt, &mut |expr| {
        if let Expr::Parameter { index, .. } = expr {
            max = max.max(*index);
        }
    });
    max
}

fn substitute_params(stmt: &Statement, params: &[Literal]) -> Statement {
    match stmt {
        Statement::Select(s) => Statement::Select(noedb_ast::SelectStmt {
            distinct: s.distinct,
            items: s.items.iter().map(|i| noedb_ast::SelectItem {
                expr: substitute_expr(&i.expr, params),
                alias: i.alias.clone(),
            }).collect(),
            from: s.from.clone(),
            joins: s.joins.clone(),
            where_clause: s.where_clause.as_ref().map(|e| substitute_expr(e, params)),
            span: s.span,
        }),
        other => other.clone(),
    }
}

fn substitute_expr(expr: &Expr, params: &[Literal]) -> Expr {
    match expr {
        Expr::Parameter { index, span } => {
            let lit = params
                .get(usize::from(index.saturating_sub(1)))
                .cloned()
                .unwrap_or(Literal::Null { span: *span });
            Expr::Literal(lit)
        }
        Expr::Binary { op, left, right, span } => Expr::Binary {
            op: *op,
            left: Box::new(substitute_expr(left, params)),
            right: Box::new(substitute_expr(right, params)),
            span: *span,
        },
        Expr::Unary { op, expr, span } => Expr::Unary {
            op: *op,
            expr: Box::new(substitute_expr(expr, params)),
            span: *span,
        },
        Expr::IsNull { expr, negated, span } => Expr::IsNull {
            expr: Box::new(substitute_expr(expr, params)),
            negated: *negated,
            span: *span,
        },
        Expr::In { expr, values, negated, span } => Expr::In {
            expr: Box::new(substitute_expr(expr, params)),
            values: values.iter().map(|v| substitute_expr(v, params)).collect(),
            negated: *negated,
            span: *span,
        },
        Expr::Between { expr, low, high, negated, span } => Expr::Between {
            expr: Box::new(substitute_expr(expr, params)),
            low: Box::new(substitute_expr(low, params)),
            high: Box::new(substitute_expr(high, params)),
            negated: *negated,
            span: *span,
        },
        Expr::Paren(inner, span) => Expr::Paren(Box::new(substitute_expr(inner, params)), *span),
        other => other.clone(),
    }
}

fn walk_stmt(stmt: &Statement, f: &mut dyn FnMut(&Expr)) {
    match stmt {
        Statement::Select(s) => {
            for item in &s.items {
                walk_expr(&item.expr, f);
            }
            if let Some(w) = &s.where_clause {
                walk_expr(w, f);
            }
        }
        _ => {}
    }
}

fn walk_expr(expr: &Expr, f: &mut dyn FnMut(&Expr)) {
    f(expr);
    match expr {
        Expr::Binary { left, right, .. } => {
            walk_expr(left, f);
            walk_expr(right, f);
        }
        Expr::Unary { expr, .. } => walk_expr(expr, f),
        Expr::IsNull { expr, .. } => walk_expr(expr, f),
        Expr::In { expr, values, .. } => {
            walk_expr(expr, f);
            for v in values {
                walk_expr(v, f);
            }
        }
        Expr::Between { expr, low, high, .. } => {
            walk_expr(expr, f);
            walk_expr(low, f);
            walk_expr(high, f);
        }
        Expr::Paren(inner, _) => walk_expr(inner, f),
        _ => {}
    }
}
