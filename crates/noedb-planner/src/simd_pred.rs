//! SIMD-style integer predicates (Phase 3 Week 17).
//!
//! Eight-wide unrolled comparisons; LLVM typically lowers these to AVX2 on x86_64.

const LANES: usize = 8;

/// Equality mask for `values == needle` (length preserved).
#[must_use]
pub fn filter_eq_i64(values: &[i64], needle: i64) -> Vec<bool> {
    let mut out = vec![false; values.len()];
    let mut i = 0;
    while i + LANES <= values.len() {
        let base = i;
        out[base] = values[base] == needle;
        out[base + 1] = values[base + 1] == needle;
        out[base + 2] = values[base + 2] == needle;
        out[base + 3] = values[base + 3] == needle;
        out[base + 4] = values[base + 4] == needle;
        out[base + 5] = values[base + 5] == needle;
        out[base + 6] = values[base + 6] == needle;
        out[base + 7] = values[base + 7] == needle;
        i += LANES;
    }
    for (slot, &v) in out[i..].iter_mut().zip(values[i..].iter()) {
        *slot = v == needle;
    }
    out
}

/// Range mask: `needle <= values < hi` (half-open).
#[must_use]
pub fn filter_range_i64(values: &[i64], lo: i64, hi: i64) -> Vec<bool> {
    let mut out = vec![false; values.len()];
    let mut i = 0;
    while i + LANES <= values.len() {
        let base = i;
        out[base] = (lo..hi).contains(&values[base]);
        out[base + 1] = (lo..hi).contains(&values[base + 1]);
        out[base + 2] = (lo..hi).contains(&values[base + 2]);
        out[base + 3] = (lo..hi).contains(&values[base + 3]);
        out[base + 4] = (lo..hi).contains(&values[base + 4]);
        out[base + 5] = (lo..hi).contains(&values[base + 5]);
        out[base + 6] = (lo..hi).contains(&values[base + 6]);
        out[base + 7] = (lo..hi).contains(&values[base + 7]);
        i += LANES;
    }
    for (slot, &v) in out[i..].iter_mut().zip(values[i..].iter()) {
        *slot = (lo..hi).contains(&v);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eq_mask_matches_scalar() {
        let vals: Vec<i64> = (0..32).collect();
        let mask = filter_eq_i64(&vals, 17);
        assert_eq!(mask.len(), vals.len());
        assert_eq!(mask.iter().filter(|b| **b).count(), 1);
        assert!(mask[17]);
    }

    #[test]
    fn range_mask_matches_scalar() {
        let vals: Vec<i64> = (0..20).collect();
        let mask = filter_range_i64(&vals, 5, 10);
        assert_eq!(mask.iter().filter(|b| **b).count(), 5);
    }
}
