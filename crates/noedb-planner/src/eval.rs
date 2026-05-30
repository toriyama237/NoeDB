//! Expression evaluator for filters and projections (Week 19).

#![allow(clippy::cast_precision_loss)]

use noedb_ast::{BinaryOp, ColumnRef, Expr, Literal, UnaryOp};

use crate::value::Value;
use crate::ExecError;

/// Evaluate `expr` in the context of one storage row (`columns` map).
pub fn eval_expr(expr: &Expr, row: &[(String, Value)]) -> Result<Value, ExecError> {
    match expr {
        Expr::Literal(lit) => Ok(literal_to_value(lit)),
        Expr::Column(col) => eval_column(col, row),
        Expr::Unary { op, expr, .. } => eval_unary(*op, expr, row),
        Expr::Binary {
            op, left, right, ..
        } => eval_binary(*op, left, right, row),
        Expr::IsNull { expr, negated, .. } => {
            let v = eval_expr(expr, row)?;
            let is_null = matches!(v, Value::Null);
            Ok(Value::Bool(if *negated { !is_null } else { is_null }))
        }
        Expr::In {
            expr,
            values,
            negated,
            ..
        } => {
            let v = eval_expr(expr, row)?;
            let mut found = false;
            for val_expr in values {
                let val = eval_in_rhs(val_expr, row)?;
                if v.sql_eq(&val).unwrap_or(false) {
                    found = true;
                    break;
                }
            }
            Ok(Value::Bool(if *negated { !found } else { found }))
        }
        Expr::Cast {
            expr, data_type, ..
        } => {
            let v = eval_expr(expr, row)?;
            crate::cast::cast_value(&v, data_type)
        }
        Expr::Between {
            expr,
            low,
            high,
            negated,
            ..
        } => {
            let v = eval_expr(expr, row)?;
            let lo = eval_expr(low, row)?;
            let hi = eval_expr(high, row)?;
            if matches!(v, Value::Null) || matches!(lo, Value::Null) || matches!(hi, Value::Null) {
                return Ok(Value::Bool(false));
            }
            let ge = cmp_values(&v, &lo, BinaryOp::Ge)?;
            let le = cmp_values(&v, &hi, BinaryOp::Le)?;
            Ok(Value::Bool(if *negated {
                !(ge && le)
            } else {
                ge && le
            }))
        }
        Expr::InSubquery { .. }
        | Expr::Parameter { .. }
        | Expr::CurrentUser { .. }
        | Expr::Function { .. } => Err(ExecError::UnsupportedExpr),
        Expr::Paren(inner, _) => eval_expr(inner, row),
    }
}

/// Evaluate a predicate; `NULL` is treated as false for `WHERE`.
pub fn eval_predicate(expr: &Expr, row: &[(String, Value)]) -> Result<bool, ExecError> {
    match eval_expr(expr, row)? {
        Value::Bool(b) => Ok(b),
        Value::Null => Ok(false),
        other => Err(ExecError::TypeMismatch {
            message: format!("expected boolean predicate, got {other:?}"),
        }),
    }
}

fn eval_in_rhs(expr: &Expr, row: &[(String, Value)]) -> Result<Value, ExecError> {
    match expr {
        Expr::Literal(_) | Expr::Parameter { .. } => eval_expr(expr, &[]),
        _ => eval_expr(expr, row),
    }
}

fn eval_column(col: &ColumnRef, row: &[(String, Value)]) -> Result<Value, ExecError> {
    match col {
        ColumnRef::Named {
            table: Some(t),
            column,
        } => {
            let qual = format!("{}.{}", t.value, column.value);
            if let Some((_, v)) = row.iter().find(|(n, _)| n == &qual) {
                return Ok(v.clone());
            }
            row.iter()
                .find(|(n, _)| n == &column.value)
                .map(|(_, v)| v.clone())
                .ok_or(ExecError::UnknownColumn { name: qual })
        }
        ColumnRef::Named { column, .. } => {
            if let Some((_, v)) = row.iter().find(|(n, _)| n == &column.value) {
                return Ok(v.clone());
            }
            let mut matches = row.iter().filter(|(n, _)| {
                n.rsplit_once('.')
                    .is_some_and(|(_, bare)| bare == column.value)
            });
            let Some((_, v)) = matches.next() else {
                return Err(ExecError::UnknownColumn {
                    name: column.value.clone(),
                });
            };
            if matches.next().is_some() {
                return Err(ExecError::UnknownColumn {
                    name: column.value.clone(),
                });
            }
            Ok(v.clone())
        }
        ColumnRef::Star { .. } | ColumnRef::QualifiedStar { .. } => Err(ExecError::UnsupportedExpr),
    }
}

fn eval_unary(op: UnaryOp, expr: &Expr, row: &[(String, Value)]) -> Result<Value, ExecError> {
    let v = eval_expr(expr, row)?;
    match op {
        UnaryOp::Not => match v {
            Value::Bool(b) => Ok(Value::Bool(!b)),
            Value::Null => Ok(Value::Null),
            other => Err(ExecError::TypeMismatch {
                message: format!("NOT applied to {other:?}"),
            }),
        },
        UnaryOp::Minus => match v {
            Value::Integer(n) => Ok(Value::Integer(-n)),
            Value::Float(f) => Ok(Value::Float(-f)),
            Value::Null => Ok(Value::Null),
            other => Err(ExecError::TypeMismatch {
                message: format!("unary minus on {other:?}"),
            }),
        },
    }
}

fn eval_binary(
    op: BinaryOp,
    left: &Expr,
    right: &Expr,
    row: &[(String, Value)],
) -> Result<Value, ExecError> {
    match op {
        BinaryOp::And => {
            if !eval_predicate(left, row)? {
                return Ok(Value::Bool(false));
            }
            Ok(Value::Bool(eval_predicate(right, row)?))
        }
        BinaryOp::Or => {
            if eval_predicate(left, row)? {
                return Ok(Value::Bool(true));
            }
            Ok(Value::Bool(eval_predicate(right, row)?))
        }
        BinaryOp::Eq => {
            let l = eval_expr(left, row)?;
            let r = eval_expr(right, row)?;
            Ok(Value::Bool(l.sql_eq(&r).unwrap_or(false)))
        }
        BinaryOp::Ne => {
            let l = eval_expr(left, row)?;
            let r = eval_expr(right, row)?;
            Ok(Value::Bool(!l.sql_eq(&r).unwrap_or(false)))
        }
        BinaryOp::Lt | BinaryOp::Gt | BinaryOp::Le | BinaryOp::Ge => {
            let l = eval_expr(left, row)?;
            let r = eval_expr(right, row)?;
            cmp_values(&l, &r, op).map(Value::Bool)
        }
        BinaryOp::Like => {
            let l = eval_expr(left, row)?;
            let r = eval_expr(right, row)?;
            Ok(Value::Bool(like_match(&l, &r)))
        }
        _ => Err(ExecError::UnsupportedExpr),
    }
}

fn like_match(left: &Value, pattern: &Value) -> bool {
    if matches!(left, Value::Null) || matches!(pattern, Value::Null) {
        return false;
    }
    let Value::Bytes(text) = left else {
        return false;
    };
    let Value::Bytes(pat) = pattern else {
        return false;
    };
    let text = String::from_utf8_lossy(text);
    let pat = String::from_utf8_lossy(pat);
    sql_like(text.as_ref(), pat.as_ref())
}

/// SQL `LIKE` with `%` (any sequence) and `_` (single char).
fn sql_like(text: &str, pattern: &str) -> bool {
    let t: Vec<char> = text.chars().collect();
    let p: Vec<char> = pattern.chars().collect();
    let m = t.len() + 1;
    let n = p.len() + 1;
    let mut dp = vec![vec![false; n]; m];
    dp[0][0] = true;
    for j in 1..p.len() + 1 {
        if p[j - 1] == '%' {
            dp[0][j] = dp[0][j - 1];
        }
    }
    for i in 1..t.len() + 1 {
        for j in 1..p.len() + 1 {
            if p[j - 1] == '%' {
                dp[i][j] = dp[i][j - 1] || dp[i - 1][j];
            } else if p[j - 1] == '_' || t[i - 1] == p[j - 1] {
                dp[i][j] = dp[i - 1][j - 1];
            }
        }
    }
    dp[t.len()][p.len()]
}

fn cmp_values(l: &Value, r: &Value, op: BinaryOp) -> Result<bool, ExecError> {
    if matches!(l, Value::Null) || matches!(r, Value::Null) {
        return Ok(false);
    }
    if let (Some(a), Some(b)) = (coerce_numeric(l), coerce_numeric(r)) {
        return Ok(apply_cmp_f(a, b, op));
    }
    match (l, r) {
        (Value::Bytes(a), Value::Bytes(b)) | (Value::Date(a), Value::Date(b)) => {
            Ok(apply_cmp_bytes(a, b, op))
        }
        (Value::Timestamp(a), Value::Timestamp(b)) => Ok(apply_cmp(*a, *b, op)),
        _ => Err(ExecError::TypeMismatch {
            message: "incompatible types in comparison".into(),
        }),
    }
}

fn coerce_numeric(v: &Value) -> Option<f64> {
    match v {
        Value::Integer(n) => Some(*n as f64),
        Value::Float(f) => Some(*f),
        Value::Bool(b) => Some(f64::from(*b)),
        Value::Bytes(b) => std::str::from_utf8(b).ok()?.trim().parse().ok(),
        Value::Timestamp(ts) => Some(*ts as f64),
        _ => None,
    }
}

const fn apply_cmp(a: i64, b: i64, op: BinaryOp) -> bool {
    match op {
        BinaryOp::Lt => a < b,
        BinaryOp::Gt => a > b,
        BinaryOp::Le => a <= b,
        BinaryOp::Ge => a >= b,
        _ => false,
    }
}

fn apply_cmp_f(a: f64, b: f64, op: BinaryOp) -> bool {
    match op {
        BinaryOp::Lt => a < b,
        BinaryOp::Gt => a > b,
        BinaryOp::Le => a <= b,
        BinaryOp::Ge => a >= b,
        _ => false,
    }
}

fn apply_cmp_bytes(a: &[u8], b: &[u8], op: BinaryOp) -> bool {
    match op {
        BinaryOp::Lt => a < b,
        BinaryOp::Gt => a > b,
        BinaryOp::Le => a <= b,
        BinaryOp::Ge => a >= b,
        _ => false,
    }
}

fn literal_to_value(lit: &Literal) -> Value {
    match lit {
        Literal::Null { .. } => Value::Null,
        Literal::Integer(n, _) => Value::Integer(*n),
        Literal::Float(f, _) => Value::Float(*f),
        Literal::String(s, _) => Value::Bytes(s.as_bytes().to_vec()),
        Literal::Boolean(b, _) => Value::Bool(*b),
    }
}
