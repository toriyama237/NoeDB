//! `CAST(expr AS type)` runtime coercion (Phase 5 Week 42).

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::match_same_arms,
    clippy::unnecessary_wraps,
    clippy::explicit_counter_loop
)]

use noedb_ast::SqlType;

use crate::value::Value;
use crate::ExecError;

/// Apply SQL `CAST` to a runtime value.
pub fn cast_value(val: &Value, ty: &SqlType) -> Result<Value, ExecError> {
    if matches!(val, Value::Null) {
        return Ok(Value::Null);
    }
    match ty {
        SqlType::Int => cast_to_int(val),
        SqlType::Float => cast_to_float(val),
        SqlType::Boolean => cast_to_bool(val),
        SqlType::Varchar { .. } | SqlType::Text => {
            Ok(Value::Bytes(value_to_string(val)?.into_bytes()))
        }
        SqlType::Date => cast_to_date(val),
        SqlType::Timestamp => cast_to_timestamp(val),
        SqlType::Vector { dim } => cast_to_vector(val, *dim),
        SqlType::Named(name) if name.eq_ignore_ascii_case("TEXT") => {
            Ok(Value::Bytes(value_to_string(val)?.into_bytes()))
        }
        SqlType::Named(name) => {
            let upper = name.to_ascii_uppercase();
            match upper.as_str() {
                "INT" | "INTEGER" | "BIGINT" => cast_to_int(val),
                "FLOAT" | "REAL" | "DOUBLE" => cast_to_float(val),
                "BOOLEAN" | "BOOL" => cast_to_bool(val),
                "TEXT" => Ok(Value::Bytes(value_to_string(val)?.into_bytes())),
                "DATE" => cast_to_date(val),
                "TIMESTAMP" | "TIMESTAMPTZ" => cast_to_timestamp(val),
                "VECTOR" => {
                    return Err(ExecError::TypeMismatch {
                        message: "VECTOR requires VECTOR(n) dimension".into(),
                    });
                }
                _ => Err(ExecError::TypeMismatch {
                    message: format!("unsupported CAST target type {name}"),
                }),
            }
        }
    }
}

fn cast_to_int(val: &Value) -> Result<Value, ExecError> {
    Ok(Value::Integer(match val {
        Value::Integer(n) => *n,
        Value::Float(f) => f.trunc() as i64,
        Value::Bool(b) => i64::from(*b),
        Value::Bytes(b) => parse_int_bytes(b)?,
        Value::Date(b) => parse_int_bytes(b)?,
        Value::Timestamp(ts) => *ts,
        Value::Null => unreachable!(),
        Value::Vector(_) => return Err(type_err("cannot CAST VECTOR to INT")),
    }))
}

fn cast_to_float(val: &Value) -> Result<Value, ExecError> {
    Ok(Value::Float(match val {
        Value::Integer(n) => *n as f64,
        Value::Float(f) => *f,
        Value::Bool(b) => f64::from(*b),
        Value::Bytes(b) => parse_float_bytes(b)?,
        Value::Date(b) => parse_float_bytes(b)?,
        Value::Timestamp(ts) => *ts as f64,
        Value::Null => unreachable!(),
        Value::Vector(_) => return Err(type_err("cannot CAST VECTOR to FLOAT")),
    }))
}

fn cast_to_bool(val: &Value) -> Result<Value, ExecError> {
    Ok(Value::Bool(match val {
        Value::Bool(b) => *b,
        Value::Integer(n) => *n != 0,
        Value::Float(f) => *f != 0.0,
        Value::Bytes(b) => parse_bool_bytes(b)?,
        Value::Date(b) => !b.is_empty(),
        Value::Timestamp(ts) => *ts != 0,
        Value::Null => unreachable!(),
        Value::Vector(_) => return Err(type_err("cannot CAST VECTOR to BOOLEAN")),
    }))
}

fn cast_to_date(val: &Value) -> Result<Value, ExecError> {
    match val {
        Value::Date(b) => Ok(Value::Date(normalize_date_bytes(b)?)),
        Value::Bytes(b) => Ok(Value::Date(normalize_date_bytes(b)?)),
        Value::Timestamp(ts) => Ok(Value::Date(timestamp_to_date_bytes(*ts)?)),
        Value::Integer(n) => Ok(Value::Date(timestamp_to_date_bytes(*n)?)),
        Value::Float(f) => Ok(Value::Date(timestamp_to_date_bytes(f.trunc() as i64)?)),
        Value::Bool(b) => Err(ExecError::TypeMismatch {
            message: format!("cannot CAST boolean {b} to DATE"),
        }),
        Value::Null => unreachable!(),
        Value::Vector(_) => Err(type_err("cannot CAST VECTOR to DATE")),
    }
}

fn cast_to_vector(val: &Value, dim: u32) -> Result<Value, ExecError> {
    let floats = match val {
        Value::Vector(v) => v.clone(),
        Value::Bytes(b) => parse_vector_literal(b, dim)?,
        other => {
            return Err(ExecError::TypeMismatch {
                message: format!("cannot CAST {other:?} to VECTOR({dim})"),
            });
        }
    };
    if floats.len() != dim as usize {
        return Err(ExecError::TypeMismatch {
            message: format!("VECTOR({dim}) has {} elements", floats.len()),
        });
    }
    Ok(Value::Vector(floats))
}

fn parse_vector_literal(bytes: &[u8], dim: u32) -> Result<Vec<f32>, ExecError> {
    let s = std::str::from_utf8(bytes).map_err(|_| type_err("invalid UTF-8 for VECTOR"))?;
    let s = s.trim().trim_start_matches('[').trim_end_matches(']');
    let floats: Result<Vec<f32>, _> = s
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| {
            p.parse::<f32>()
                .map_err(|_| type_err(&format!("invalid float `{p}` in VECTOR")))
        })
        .collect();
    let floats = floats?;
    if floats.len() != dim as usize {
        return Err(type_err(&format!("VECTOR({dim}) literal has {} elements", floats.len())));
    }
    Ok(floats)
}

fn cast_to_timestamp(val: &Value) -> Result<Value, ExecError> {
    Ok(Value::Timestamp(match val {
        Value::Timestamp(ts) => *ts,
        Value::Integer(n) => *n,
        Value::Float(f) => f.trunc() as i64,
        Value::Bytes(b) | Value::Date(b) => parse_timestamp_bytes(b)?,
        Value::Bool(b) => {
            return Err(ExecError::TypeMismatch {
                message: format!("cannot CAST boolean {b} to TIMESTAMP"),
            });
        }
        Value::Null => unreachable!(),
        Value::Vector(_) => {
            return Err(ExecError::TypeMismatch {
                message: "cannot CAST VECTOR to TIMESTAMP".into(),
            });
        }
    }))
}

fn value_to_string(val: &Value) -> Result<String, ExecError> {
    Ok(match val {
        Value::Null => "NULL".into(),
        Value::Integer(n) => n.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
        Value::Date(b) => String::from_utf8_lossy(b).into_owned(),
        Value::Timestamp(ts) => ts.to_string(),
        Value::Vector(v) => format!(
            "[{}]",
            v.iter()
                .map(|f| f.to_string())
                .collect::<Vec<_>>()
                .join(",")
        ),
    })
}

fn parse_int_bytes(b: &[u8]) -> Result<i64, ExecError> {
    let s = std::str::from_utf8(b).map_err(|_| type_err("invalid UTF-8 for integer CAST"))?;
    s.trim()
        .parse::<i64>()
        .map_err(|_| type_err("invalid integer literal for CAST"))
}

fn parse_float_bytes(b: &[u8]) -> Result<f64, ExecError> {
    let s = std::str::from_utf8(b).map_err(|_| type_err("invalid UTF-8 for float CAST"))?;
    s.trim()
        .parse::<f64>()
        .map_err(|_| type_err("invalid float literal for CAST"))
}

fn parse_bool_bytes(b: &[u8]) -> Result<bool, ExecError> {
    let s = std::str::from_utf8(b).map_err(|_| type_err("invalid UTF-8 for boolean CAST"))?;
    match s.trim().to_ascii_lowercase().as_str() {
        "true" | "t" | "1" | "yes" => Ok(true),
        "false" | "f" | "0" | "no" | "" => Ok(false),
        _ => Err(type_err("invalid boolean literal for CAST")),
    }
}

fn normalize_date_bytes(b: &[u8]) -> Result<Vec<u8>, ExecError> {
    let s = std::str::from_utf8(b).map_err(|_| type_err("invalid UTF-8 for DATE"))?;
    let s = s.trim();
    if s.len() == 10 && s.as_bytes()[4] == b'-' && s.as_bytes()[7] == b'-' {
        return Ok(s.as_bytes().to_vec());
    }
    Err(type_err("DATE must be YYYY-MM-DD"))
}

fn timestamp_to_date_bytes(ts: i64) -> Result<Vec<u8>, ExecError> {
    use std::time::{Duration, UNIX_EPOCH};
    let dt = UNIX_EPOCH
        .checked_add(Duration::from_secs(ts.max(0) as u64))
        .ok_or_else(|| type_err("timestamp out of range"))?;
    let secs = dt
        .duration_since(UNIX_EPOCH)
        .map_err(|_| type_err("timestamp before epoch"))?
        .as_secs();
    let days = secs / 86_400;
    // Simple epoch-day to Y-M-D (Gregorian, 1970-based walk).
    let (y, m, d) = epoch_day_to_ymd(days);
    Ok(format!("{y:04}-{m:02}-{d:02}").into_bytes())
}

fn epoch_day_to_ymd(mut days: u64) -> (i32, u32, u32) {
    let mut y = 1970i32;
    loop {
        let diy = if is_leap(y) { 366 } else { 365 };
        if days < diy {
            break;
        }
        days -= diy;
        y += 1;
    }
    let month_days = if is_leap(y) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    for (idx, &md) in month_days.iter().enumerate() {
        if days < md {
            return (y, idx as u32 + 1, days as u32 + 1);
        }
        days -= md;
    }
    (y, 12, 31)
}

const fn is_leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn parse_timestamp_bytes(b: &[u8]) -> Result<i64, ExecError> {
    let s = std::str::from_utf8(b).map_err(|_| type_err("invalid UTF-8 for TIMESTAMP"))?;
    let s = s.trim();
    if s.chars().all(|c| c.is_ascii_digit() || c == '-') {
        if s.len() == 10 {
            return Ok(0); // date-only → midnight UTC
        }
        return s
            .parse::<i64>()
            .map_err(|_| type_err("invalid TIMESTAMP literal"));
    }
    Err(type_err("unsupported TIMESTAMP format"))
}

fn type_err(message: &str) -> ExecError {
    ExecError::TypeMismatch {
        message: message.into(),
    }
}
