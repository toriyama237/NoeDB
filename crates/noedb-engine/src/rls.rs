//! Row Level Security policies (Phase 1 Week 5).

use noedb_ast::{BinaryOp, CreatePolicyStmt, Expr, FromItem, SelectStmt, Statement};

/// One RLS policy on a table.
#[derive(Debug, Clone)]
pub struct Policy {
    /// Policy name.
    pub name: String,
    /// `USING` predicate (may reference `CURRENT_USER`).
    pub using_expr: Expr,
}

/// RLS catalog per table.
#[derive(Debug, Default)]
pub struct RlsCatalog {
    enabled: std::collections::HashSet<String>,
    policies: std::collections::HashMap<String, Vec<Policy>>,
}

impl RlsCatalog {
    /// Enable RLS on a table.
    pub fn enable(&mut self, table: &str) {
        self.enabled.insert(table.to_ascii_lowercase());
    }

    /// Register a policy.
    pub fn add_policy(&mut self, stmt: &CreatePolicyStmt) {
        let table = stmt.table.value.to_ascii_lowercase();
        self.policies.entry(table).or_default().push(Policy {
            name: stmt.name.value.clone(),
            using_expr: stmt.using_expr.clone(),
        });
    }

    /// Whether RLS is enforced for reads on `table`.
    #[must_use]
    pub fn is_enabled(&self, table: &str) -> bool {
        self.enabled.contains(&table.to_ascii_lowercase())
    }

    /// Inject policy predicates into a `SELECT` (AND-combined with existing `WHERE`).
    pub fn apply_select(&self, mut select: SelectStmt, role: &str) -> SelectStmt {
        let Some(from) = select.from.as_ref() else {
            return select;
        };
        let table_name = match from {
            FromItem::Table(t) => &t.name.value,
            FromItem::Subquery { .. } => return select,
        };
        if !self.is_enabled(table_name) {
            return select;
        }
        let Some(policies) = self.policies.get(&table_name.to_ascii_lowercase()) else {
            return select;
        };
        for policy in policies {
            let pred = materialize_session(&policy.using_expr, role);
            select.where_clause = Some(match select.where_clause.take() {
                None => pred,
                Some(existing) => and_expr(existing, pred),
            });
        }
        select
    }
}

/// Replace `CURRENT_USER` with a string literal (constant per query).
#[must_use]
pub fn materialize_session(expr: &Expr, role: &str) -> Expr {
    match expr {
        Expr::CurrentUser { span } => {
            Expr::Literal(noedb_ast::Literal::String(role.to_string(), *span))
        }
        Expr::Binary {
            op,
            left,
            right,
            span,
        } => Expr::Binary {
            op: *op,
            left: Box::new(materialize_session(left, role)),
            right: Box::new(materialize_session(right, role)),
            span: *span,
        },
        Expr::Unary { op, expr, span } => Expr::Unary {
            op: *op,
            expr: Box::new(materialize_session(expr, role)),
            span: *span,
        },
        Expr::IsNull {
            expr,
            negated,
            span,
        } => Expr::IsNull {
            expr: Box::new(materialize_session(expr, role)),
            negated: *negated,
            span: *span,
        },
        Expr::In {
            expr,
            values,
            negated,
            span,
        } => Expr::In {
            expr: Box::new(materialize_session(expr, role)),
            values: values
                .iter()
                .map(|v| materialize_session(v, role))
                .collect(),
            negated: *negated,
            span: *span,
        },
        Expr::Between {
            expr,
            low,
            high,
            negated,
            span,
        } => Expr::Between {
            expr: Box::new(materialize_session(expr, role)),
            low: Box::new(materialize_session(low, role)),
            high: Box::new(materialize_session(high, role)),
            negated: *negated,
            span: *span,
        },
        Expr::Paren(inner, span) => Expr::Paren(Box::new(materialize_session(inner, role)), *span),
        other => other.clone(),
    }
}

fn and_expr(left: Expr, right: Expr) -> Expr {
    let span = left.span();
    Expr::Binary {
        op: BinaryOp::And,
        left: Box::new(left),
        right: Box::new(right),
        span,
    }
}

/// Apply RLS to a statement if it is a table scan `SELECT`.
#[must_use]
pub fn apply_rls(stmt: Statement, catalog: &RlsCatalog, role: &str) -> Statement {
    match stmt {
        Statement::Select(s) => Statement::Select(catalog.apply_select(s, role)),
        other => other,
    }
}
