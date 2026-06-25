//! Pre-execution folding of non-correlated scalar subqueries.
//!
//! `WHERE age > (SELECT AVG(age) FROM users)` is evaluated by running the inner
//! query once against the same storage snapshot and substituting the resulting
//! scalar as a literal before planning. This keeps the Volcano planner free of
//! a dedicated scalar-subquery operator while supporting the common case.

use noedb_ast::{Expr, Literal, SelectStmt, Statement};
use noedb_storage::{LsmTree, StorageEngine, StorageError};

use crate::value::Value;
use crate::{ExecError, QuerySchema};

/// Replace every scalar subquery in `stmt` with its computed literal value.
///
/// Returns `Ok(Some(stmt))` when at least one substitution happened, `Ok(None)`
/// when the statement contained no scalar subqueries (the common fast path).
///
/// # Errors
///
/// Propagates executor errors from running an inner subquery.
pub fn fold_scalar_subqueries<S: StorageEngine<Error = StorageError>>(
    stmt: &Statement,
    exec_store: &S,
    index_store: &LsmTree,
    schema: Option<&QuerySchema>,
) -> Result<Option<Statement>, ExecError> {
    let mut ctx = FoldCtx {
        exec_store,
        index_store,
        schema,
        changed: false,
    };
    let mut out = stmt.clone();
    match &mut out {
        Statement::Select(s) => ctx.fold_select(s)?,
        Statement::Update(u) => {
            for (_, e) in &mut u.assignments {
                ctx.fold_expr(e)?;
            }
            if let Some(w) = &mut u.where_clause {
                ctx.fold_expr(w)?;
            }
        }
        Statement::Delete(d) => {
            if let Some(w) = &mut d.where_clause {
                ctx.fold_expr(w)?;
            }
        }
        _ => {}
    }
    Ok(ctx.changed.then_some(out))
}

struct FoldCtx<'a, S: StorageEngine<Error = StorageError>> {
    exec_store: &'a S,
    index_store: &'a LsmTree,
    schema: Option<&'a QuerySchema>,
    changed: bool,
}

impl<S: StorageEngine<Error = StorageError>> FoldCtx<'_, S> {
    fn fold_select(&mut self, s: &mut SelectStmt) -> Result<(), ExecError> {
        for item in &mut s.items {
            self.fold_expr(&mut item.expr)?;
        }
        if let Some(w) = &mut s.where_clause {
            self.fold_expr(w)?;
        }
        if let Some(h) = &mut s.having_clause {
            self.fold_expr(h)?;
        }
        for g in &mut s.group_by {
            self.fold_expr(g)?;
        }
        for k in &mut s.order_by {
            self.fold_expr(&mut k.expr)?;
        }
        for j in &mut s.joins {
            self.fold_expr(&mut j.on)?;
        }
        if let Some(c) = &mut s.compound {
            self.fold_select(&mut c.right)?;
        }
        Ok(())
    }

    fn fold_expr(&mut self, expr: &mut Expr) -> Result<(), ExecError> {
        match expr {
            Expr::ScalarSubquery { query, span } => {
                let value = self.eval_scalar(query)?;
                *expr = Expr::Literal(value_to_literal(&value, *span));
                self.changed = true;
            }
            Expr::Binary { left, right, .. } => {
                self.fold_expr(left)?;
                self.fold_expr(right)?;
            }
            Expr::Unary { expr, .. }
            | Expr::IsNull { expr, .. }
            | Expr::Cast { expr, .. }
            | Expr::Paren(expr, _) => self.fold_expr(expr)?,
            Expr::In { expr, values, .. } => {
                self.fold_expr(expr)?;
                for v in values {
                    self.fold_expr(v)?;
                }
            }
            Expr::Between {
                expr, low, high, ..
            } => {
                self.fold_expr(expr)?;
                self.fold_expr(low)?;
                self.fold_expr(high)?;
            }
            Expr::Function { args, .. } => {
                for a in args {
                    self.fold_expr(a)?;
                }
            }
            // Subquery predicates (IN/EXISTS) are decorrelated separately; leave
            // them intact. Leaf nodes need no recursion.
            Expr::InSubquery { .. }
            | Expr::Exists { .. }
            | Expr::Literal(_)
            | Expr::Column(_)
            | Expr::Parameter { .. }
            | Expr::CurrentUser { .. } => {}
        }
        Ok(())
    }

    fn eval_scalar(&self, query: &SelectStmt) -> Result<Value, ExecError> {
        let stmt = Statement::Select(query.clone());
        let rows =
            crate::execute_sql_on_with_schema(&stmt, self.exec_store, self.index_store, self.schema)?;
        Ok(rows
            .first()
            .and_then(|r| r.fields.first())
            .map(|(_, v)| v.clone())
            .unwrap_or(Value::Null))
    }
}

fn value_to_literal(value: &Value, span: noedb_lexer::Span) -> Literal {
    match value {
        Value::Null => Literal::Null { span },
        Value::Integer(n) => Literal::Integer(*n, span),
        Value::Float(f) => Literal::Float(*f, span),
        Value::Bool(b) => Literal::Boolean(*b, span),
        Value::Bytes(b) | Value::Date(b) => {
            if b.is_empty() {
                Literal::Null { span }
            } else {
                Literal::String(String::from_utf8_lossy(b).into_owned(), span)
            }
        }
        Value::Timestamp(ts) => Literal::Integer(*ts, span),
        Value::Vector(v) => Literal::String(
            format!(
                "[{}]",
                v.iter().map(ToString::to_string).collect::<Vec<_>>().join(",")
            ),
            span,
        ),
    }
}
