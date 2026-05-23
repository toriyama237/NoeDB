//! Expression evaluator for filters and projections (Week 19).

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
                let val = eval_expr(val_expr, row)?;
                if v.sql_eq(&val).unwrap_or(false) {
                    found = true;
                    break;
                }
            }
            Ok(Value::Bool(if *negated { !found } else { found }))
        }
        Expr::Between { .. }
        | Expr::InSubquery { .. }
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

fn eval_column(col: &ColumnRef, row: &[(String, Value)]) -> Result<Value, ExecError> {
    match col {
        ColumnRef::Named { column, .. } => row
            .iter()
            .find(|(n, _)| n == &column.value)
            .map(|(_, v)| v.clone())
            .ok_or_else(|| ExecError::UnknownColumn {
                name: column.value.clone(),
            }),
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
        _ => Err(ExecError::UnsupportedExpr),
    }
}

fn cmp_values(l: &Value, r: &Value, op: BinaryOp) -> Result<bool, ExecError> {
    if matches!(l, Value::Null) || matches!(r, Value::Null) {
        return Ok(false);
    }
    match (l, r) {
        (Value::Integer(a), Value::Integer(b)) => Ok(apply_cmp(*a, *b, op)),
        (Value::Float(a), Value::Float(b)) => Ok(apply_cmp_f(*a, *b, op)),
        (Value::Bytes(a), Value::Bytes(b)) => Ok(apply_cmp_bytes(a, b, op)),
        _ => Err(ExecError::TypeMismatch {
            message: "incompatible types in comparison".into(),
        }),
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
