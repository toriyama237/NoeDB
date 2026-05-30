//! Window function execution (Phase 5 Weeks 37–38).

#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]

use std::collections::HashMap;

use noedb_ast::{Expr, FrameBound, FrameMode, OrderKey, WindowFrame, WindowFunc, WindowSpec};

use crate::eval::eval_expr;
use crate::executor::{compare_rows, RowMap};
use crate::logical::WindowCompute;
use crate::value::Value;
use crate::ExecError;

/// Apply window functions to materialized rows.
pub fn apply_windows(
    rows: Vec<RowMap>,
    windows: &[WindowCompute],
) -> Result<Vec<RowMap>, ExecError> {
    if windows.is_empty() {
        return Ok(rows);
    }
    let mut out = rows;
    for win in windows {
        out = apply_one_window(out, win)?;
    }
    Ok(out)
}

fn apply_one_window(mut rows: Vec<RowMap>, win: &WindowCompute) -> Result<Vec<RowMap>, ExecError> {
    if win.func.is_ranking() && win.spec.order_by.is_empty() {
        return Err(ExecError::UnsupportedExpr);
    }
    let sort_keys = order_keys_to_sort(&win.spec.order_by);
    let partitions = partition_rows(&rows, &win.spec.partition_by)?;
    let mut result = Vec::with_capacity(rows.len());
    for mut part in partitions {
        if !sort_keys.is_empty() {
            part.sort_by(|a, b| compare_rows(a, b, &sort_keys));
        }
        if win.func.is_ranking() {
            assign_ranking_values(&mut part, win.func, &win.output_name, &sort_keys);
        } else {
            let arg = win.arg.as_ref().ok_or(ExecError::UnsupportedExpr)?;
            let frame = effective_frame(&win.spec, win.func);
            assign_aggregate_values(&mut part, win.func, arg, &win.output_name, &frame)?;
        }
        result.append(&mut part);
    }
    rows = result;
    Ok(rows)
}

fn effective_frame(spec: &WindowSpec, _func: WindowFunc) -> WindowFrame {
    if let Some(frame) = &spec.frame {
        return frame.clone();
    }
    let end = if spec.order_by.is_empty() {
        FrameBound::UnboundedFollowing
    } else {
        FrameBound::CurrentRow
    };
    WindowFrame {
        mode: FrameMode::Rows,
        start: FrameBound::UnboundedPreceding,
        end,
    }
}

fn frame_row_range(len: usize, row_idx: usize, frame: &WindowFrame) -> (usize, usize) {
    let _ = frame.mode; // RANGE uses row indices in v1
    let start = bound_to_index(frame.start, len, row_idx);
    let end = bound_to_index(frame.end, len, row_idx);
    (start, end.min(len.saturating_sub(1)))
}

fn bound_to_index(bound: FrameBound, len: usize, row_idx: usize) -> usize {
    let last = len.saturating_sub(1);
    match bound {
        FrameBound::UnboundedPreceding => 0,
        FrameBound::Preceding(n) => row_idx.saturating_sub(n as usize),
        FrameBound::CurrentRow => row_idx,
        FrameBound::Following(n) => (row_idx + n as usize).min(last),
        FrameBound::UnboundedFollowing => last,
    }
    .min(last)
}

fn assign_aggregate_values(
    part: &mut [RowMap],
    func: WindowFunc,
    arg: &Expr,
    output: &str,
    frame: &WindowFrame,
) -> Result<(), ExecError> {
    let n = part.len();
    for i in 0..n {
        let (start, end) = frame_row_range(n, i, frame);
        let mut sum = 0.0f64;
        let mut count = 0u64;
        for row in &part[start..=end] {
            let v = eval_expr(arg, row)?;
            if let Some(x) = value_as_f64(&v) {
                sum += x;
                count += 1;
            }
        }
        let value = match func {
            WindowFunc::Sum => {
                if count == 0 {
                    Value::Null
                } else if sum.fract() == 0.0 {
                    Value::Integer(sum as i64)
                } else {
                    Value::Float(sum)
                }
            }
            WindowFunc::Avg => {
                if count == 0 {
                    Value::Null
                } else {
                    #[allow(clippy::cast_precision_loss)]
                    let avg = sum / count as f64;
                    if avg.fract() == 0.0 {
                        Value::Integer(avg as i64)
                    } else {
                        Value::Float(avg)
                    }
                }
            }
            _ => return Err(ExecError::UnsupportedExpr),
        };
        part[i].push((output.to_string(), value));
    }
    Ok(())
}

fn value_as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Null => None,
        Value::Integer(n) => Some(*n as f64),
        Value::Float(f) => Some(*f),
        Value::Bool(b) => Some(f64::from(u8::from(*b))),
        Value::Bytes(b) | Value::Date(b) => std::str::from_utf8(b).ok()?.trim().parse().ok(),
        Value::Timestamp(ts) => Some(*ts as f64),
        Value::Vector(_) => None,
    }
}

fn order_keys_to_sort(order_by: &[OrderKey]) -> Vec<(String, bool)> {
    order_by
        .iter()
        .filter_map(|key| column_name_from_expr(&key.expr).map(|name| (name, key.asc)))
        .collect()
}

fn column_name_from_expr(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Column(noedb_ast::ColumnRef::Named { column, .. }) => Some(column.value.clone()),
        _ => None,
    }
}

fn partition_rows(rows: &[RowMap], partition_by: &[Expr]) -> Result<Vec<Vec<RowMap>>, ExecError> {
    if partition_by.is_empty() {
        return Ok(vec![rows.to_vec()]);
    }
    let mut groups: HashMap<Vec<u8>, Vec<RowMap>> = HashMap::new();
    for row in rows {
        let key = partition_key(row, partition_by)?;
        groups.entry(key).or_default().push(row.clone());
    }
    Ok(groups.into_values().collect())
}

fn partition_key(row: &RowMap, partition_by: &[Expr]) -> Result<Vec<u8>, ExecError> {
    let mut key = Vec::new();
    for expr in partition_by {
        let v = eval_expr(expr, row)?;
        key.extend_from_slice(&v.as_bytes());
        key.push(0);
    }
    Ok(key)
}

fn assign_ranking_values(
    part: &mut [RowMap],
    func: WindowFunc,
    output: &str,
    sort_keys: &[(String, bool)],
) {
    let mut dense = 1i64;
    let mut rank = 1i64;
    let n = part.len();
    for i in 0..n {
        let same_as_prev =
            i > 0 && compare_rows(&part[i - 1], &part[i], sort_keys) == std::cmp::Ordering::Equal;
        if i > 0 && !same_as_prev {
            rank = i64::try_from(i).unwrap_or(i64::MAX).saturating_add(1);
            dense += 1;
        }
        let row_num = i64::try_from(i).unwrap_or(i64::MAX).saturating_add(1);
        let value = match func {
            WindowFunc::RowNumber => row_num,
            WindowFunc::Rank => rank,
            WindowFunc::DenseRank => dense,
            _ => continue,
        };
        part[i].push((output.to_string(), Value::Integer(value)));
    }
}
