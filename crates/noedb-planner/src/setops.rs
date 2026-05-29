//! Set operations (`UNION` / `INTERSECT` / `EXCEPT`, Phase 5 Week 41).

use std::collections::{HashMap, HashSet};

use noedb_ast::SetOpKind;

use crate::executor::RowMap;

/// Stable row fingerprint for deduplication in set ops.
#[must_use]
pub fn row_key(row: &RowMap) -> Vec<u8> {
    let mut cols: Vec<_> = row.iter().collect();
    cols.sort_by(|a, b| a.0.cmp(&b.0));
    let mut key = Vec::new();
    for (name, val) in cols {
        key.extend_from_slice(name.as_bytes());
        key.push(0);
        key.extend_from_slice(&val.as_bytes());
        key.push(0);
    }
    key
}

/// Combine two row bags per SQL set-op semantics.
#[must_use]
pub fn combine_set_op(
    left: Vec<RowMap>,
    right: &[RowMap],
    op: SetOpKind,
    all: bool,
) -> Vec<RowMap> {
    match op {
        SetOpKind::Union => union_rows(left, right, all),
        SetOpKind::Intersect => intersect_rows(left, right, all),
        SetOpKind::Except => except_rows(left, right, all),
    }
}

fn union_rows(mut left: Vec<RowMap>, right: &[RowMap], all: bool) -> Vec<RowMap> {
    if all {
        left.extend_from_slice(right);
        return left;
    }
    let mut seen: HashSet<Vec<u8>> = left.iter().map(row_key).collect();
    for row in right {
        if seen.insert(row_key(row)) {
            left.push(row.clone());
        }
    }
    left
}

fn intersect_rows(left: Vec<RowMap>, right: &[RowMap], all: bool) -> Vec<RowMap> {
    let right_keys: HashSet<Vec<u8>> = right.iter().map(row_key).collect();
    if all {
        let mut right_counts: HashMap<Vec<u8>, usize> = HashMap::new();
        for row in right {
            *right_counts.entry(row_key(row)).or_insert(0) += 1;
        }
        let mut out = Vec::new();
        for row in left {
            let key = row_key(&row);
            if let Some(count) = right_counts.get_mut(&key) {
                if *count > 0 {
                    *count -= 1;
                    out.push(row);
                }
            }
        }
        return out;
    }
    let mut seen = HashSet::new();
    left.into_iter()
        .filter(|row| {
            let key = row_key(row);
            right_keys.contains(&key) && seen.insert(key)
        })
        .collect()
}

fn except_rows(left: Vec<RowMap>, right: &[RowMap], all: bool) -> Vec<RowMap> {
    if all {
        let mut right_counts: HashMap<Vec<u8>, usize> = HashMap::new();
        for row in right {
            *right_counts.entry(row_key(row)).or_insert(0) += 1;
        }
        let mut out = Vec::new();
        for row in left {
            let key = row_key(&row);
            if let Some(count) = right_counts.get_mut(&key) {
                if *count > 0 {
                    *count -= 1;
                    continue;
                }
            }
            out.push(row);
        }
        return out;
    }
    let right_keys: HashSet<Vec<u8>> = right.iter().map(row_key).collect();
    let mut seen = HashSet::new();
    left.into_iter()
        .filter(|row| {
            let key = row_key(row);
            !right_keys.contains(&key) && seen.insert(key)
        })
        .collect()
}
