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
            Ok(Value::Bool(if *negated { !(ge && le) } else { ge && le }))
        }
        Expr::InSubquery { .. }
        | Expr::Exists { .. }
        | Expr::Parameter { .. }
        | Expr::CurrentUser { .. } => Err(ExecError::UnsupportedExpr),
        Expr::Function {
            name, args, over, ..
        } => {
            if over.is_some() {
                return Err(ExecError::UnsupportedExpr);
            }
            eval_scalar_function(&name.value, args, row)
        }
        Expr::Paren(inner, _) => eval_expr(inner, row),
    }
}

/// Evaluate a predicate; `NULL` and `Unknown` are treated as false for `WHERE`.
pub fn eval_predicate(expr: &Expr, row: &[(String, Value)]) -> Result<bool, ExecError> {
    match eval_expr(expr, row)? {
        Value::Bool(true) => Ok(true),
        Value::Bool(false) | Value::Null => Ok(false),
        other => Err(ExecError::TypeMismatch {
            message: format!("expected boolean predicate, got {other:?}"),
        }),
    }
}

fn eval_scalar_function(
    name: &str,
    args: &[Expr],
    row: &[(String, Value)],
) -> Result<Value, ExecError> {
    match name.to_ascii_uppercase().as_str() {
        "UPPER" => {
            let v = eval_expr(&args[0], row)?;
            string_unary(v, |s| s.to_uppercase())
        }
        "LOWER" => {
            let v = eval_expr(&args[0], row)?;
            string_unary(v, |s| s.to_lowercase())
        }
        "LENGTH" => {
            let v = eval_expr(&args[0], row)?;
            if matches!(v, Value::Null) {
                return Ok(Value::Null);
            }
            let Value::Bytes(b) = v else {
                return Err(ExecError::TypeMismatch {
                    message: "LENGTH expects text".into(),
                });
            };
            Ok(Value::Integer(
                i64::try_from(String::from_utf8_lossy(&b).chars().count()).unwrap_or(i64::MAX),
            ))
        }
        "TRIM" => {
            let v = eval_expr(&args[0], row)?;
            string_unary(v, |s| s.trim().to_string())
        }
        "COALESCE" => {
            for arg in args {
                let v = eval_expr(arg, row)?;
                if !matches!(v, Value::Null) {
                    return Ok(v);
                }
            }
            Ok(Value::Null)
        }
        "ABS" => {
            let v = eval_expr(&args[0], row)?;
            match v {
                Value::Null => Ok(Value::Null),
                Value::Integer(n) => Ok(Value::Integer(n.abs())),
                Value::Float(f) => Ok(Value::Float(f.abs())),
                other => Err(ExecError::TypeMismatch {
                    message: format!("ABS expects numeric, got {other:?}"),
                }),
            }
        }
        "ROUND" => {
            let v = eval_expr(&args[0], row)?;
            match v {
                Value::Null => Ok(Value::Null),
                Value::Integer(n) => Ok(Value::Integer(n)),
                Value::Float(f) => Ok(Value::Float(f.round())),
                Value::Bytes(b) => {
                    let n: f64 = std::str::from_utf8(&b)
                        .ok()
                        .and_then(|s| s.trim().parse().ok())
                        .ok_or_else(|| ExecError::TypeMismatch {
                            message: "ROUND expects numeric".into(),
                        })?;
                    Ok(Value::Float(n.round()))
                }
                other => Err(ExecError::TypeMismatch {
                    message: format!("ROUND expects numeric, got {other:?}"),
                }),
            }
        }
        _ => Err(ExecError::UnsupportedExpr),
    }
}

fn string_unary(v: Value, f: impl FnOnce(String) -> String) -> Result<Value, ExecError> {
    if matches!(v, Value::Null) {
        return Ok(Value::Null);
    }
    let Value::Bytes(b) = v else {
        return Err(ExecError::TypeMismatch {
            message: "expected text argument".into(),
        });
    };
    Ok(Value::Bytes(
        f(String::from_utf8_lossy(&b).into_owned()).into_bytes(),
    ))
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
            let l = eval_expr(left, row)?;
            match l {
                Value::Bool(false) => Ok(Value::Bool(false)),
                Value::Null => {
                    let r = eval_expr(right, row)?;
                    if matches!(r, Value::Bool(false)) {
                        Ok(Value::Bool(false))
                    } else {
                        Ok(Value::Null)
                    }
                }
                Value::Bool(true) => eval_expr(right, row),
                other => Err(ExecError::TypeMismatch {
                    message: format!("AND applied to {other:?}"),
                }),
            }
        }
        BinaryOp::Or => {
            let l = eval_expr(left, row)?;
            match l {
                Value::Bool(true) => Ok(Value::Bool(true)),
                Value::Null => {
                    let r = eval_expr(right, row)?;
                    if matches!(r, Value::Bool(true)) {
                        Ok(Value::Bool(true))
                    } else {
                        Ok(Value::Null)
                    }
                }
                Value::Bool(false) => eval_expr(right, row),
                other => Err(ExecError::TypeMismatch {
                    message: format!("OR applied to {other:?}"),
                }),
            }
        }
        BinaryOp::Eq => {
            let l = eval_expr(left, row)?;
            let r = eval_expr(right, row)?;
            Ok(match l.sql_eq(&r) {
                Some(b) => Value::Bool(b),
                None => Value::Null,
            })
        }
        BinaryOp::Ne => {
            let l = eval_expr(left, row)?;
            let r = eval_expr(right, row)?;
            Ok(match l.sql_eq(&r) {
                Some(b) => Value::Bool(!b),
                None => Value::Null,
            })
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
        BinaryOp::Plus | BinaryOp::Minus | BinaryOp::Div => {
            let l = eval_expr(left, row)?;
            let r = eval_expr(right, row)?;
            eval_arithmetic(op, l, r)
        }
        BinaryOp::Distance => {
            let l = eval_expr(left, row)?;
            let r = eval_expr(right, row)?;
            Ok(Value::Float(vector_l2_distance(&l, &r)?))
        }
        // Defensive fallback for any BinaryOp not handled above.
        #[allow(unreachable_patterns)]
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

fn eval_arithmetic(op: BinaryOp, l: Value, r: Value) -> Result<Value, ExecError> {
    if matches!(l, Value::Null) || matches!(r, Value::Null) {
        return Ok(Value::Null);
    }
    let (a, b) = (
        coerce_numeric(&l).ok_or_else(|| ExecError::TypeMismatch {
            message: format!("arithmetic on {l:?}"),
        })?,
        coerce_numeric(&r).ok_or_else(|| ExecError::TypeMismatch {
            message: format!("arithmetic on {r:?}"),
        })?,
    );
    match op {
        BinaryOp::Plus => Ok(Value::Float(a + b)),
        BinaryOp::Minus => Ok(Value::Float(a - b)),
        BinaryOp::Div => {
            if b == 0.0 {
                return Err(ExecError::TypeMismatch {
                    message: "division by zero".into(),
                });
            }
            Ok(Value::Float(a / b))
        }
        _ => Err(ExecError::UnsupportedExpr),
    }
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

fn vector_l2_distance(left: &Value, right: &Value) -> Result<f64, ExecError> {
    let a = vector_as_f32s(left)?;
    let b = vector_as_f32s(right)?;
    if a.len() != b.len() {
        return Err(ExecError::TypeMismatch {
            message: format!("VECTOR dimension mismatch: {} vs {}", a.len(), b.len()),
        });
    }
    let sum: f64 = a
        .iter()
        .zip(b.iter())
        .map(|(x, y)| {
            let d = f64::from(*x) - f64::from(*y);
            d * d
        })
        .sum();
    Ok(sum.sqrt())
}

fn vector_as_f32s(value: &Value) -> Result<Vec<f32>, ExecError> {
    match value {
        Value::Vector(v) => Ok(v.clone()),
        Value::Bytes(b) if b.starts_with(b"NDV1") => decode_ndv1(b),
        Value::Bytes(b) => parse_vector_literal_flexible(b),
        Value::Null => Err(ExecError::TypeMismatch {
            message: "VECTOR distance with NULL".into(),
        }),
        other => Err(ExecError::TypeMismatch {
            message: format!("expected VECTOR value, got {other:?}"),
        }),
    }
}

fn decode_ndv1(bytes: &[u8]) -> Result<Vec<f32>, ExecError> {
    const MAGIC_LEN: usize = 4;
    if bytes.len() < MAGIC_LEN || &bytes[..MAGIC_LEN] != b"NDV1" {
        return Err(ExecError::TypeMismatch {
            message: "invalid VECTOR bytes".into(),
        });
    }
    let payload = &bytes[MAGIC_LEN..];
    if payload.len() % 4 != 0 {
        return Err(ExecError::TypeMismatch {
            message: "invalid VECTOR payload".into(),
        });
    }
    Ok(payload
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect())
}

fn parse_vector_literal_flexible(bytes: &[u8]) -> Result<Vec<f32>, ExecError> {
    let s = std::str::from_utf8(bytes).map_err(|_| ExecError::TypeMismatch {
        message: "invalid UTF-8 for VECTOR literal".into(),
    })?;
    let s = s.trim().trim_start_matches('[').trim_end_matches(']');
    if s.is_empty() {
        return Ok(Vec::new());
    }
    s.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| {
            part.parse::<f32>().map_err(|_| ExecError::TypeMismatch {
                message: format!("invalid float `{part}` in VECTOR literal"),
            })
        })
        .collect()
}
