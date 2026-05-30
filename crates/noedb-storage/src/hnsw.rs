//! In-memory HNSW-style vector index (single-layer greedy search).

#![allow(clippy::cast_precision_loss)]

use std::cmp::Ordering;
use std::collections::HashMap;

/// Approximate nearest-neighbor index over `f32` vectors keyed by row id.
#[derive(Debug, Default)]
pub struct HnswIndex {
    dim: usize,
    m: usize,
    nodes: HashMap<String, Vec<f32>>,
    graph: HashMap<String, Vec<String>>,
}

impl HnswIndex {
    /// Create an index for fixed-dimension vectors.
    #[must_use]
    pub fn new(dim: usize) -> Self {
        Self {
            dim,
            m: 16,
            nodes: HashMap::new(),
            graph: HashMap::new(),
        }
    }

    /// Insert or replace one vector.
    pub fn insert(&mut self, id: impl Into<String>, vector: &[f32]) {
        let id = id.into();
        debug_assert_eq!(vector.len(), self.dim);
        self.nodes.insert(id.clone(), vector.to_vec());
        self.relink(&id);
    }

    /// Remove a vector from the index.
    pub fn remove(&mut self, id: &str) {
        self.nodes.remove(id);
        self.graph.remove(id);
        for neighbors in self.graph.values_mut() {
            neighbors.retain(|n| n != id);
        }
    }

    /// Return up to `k` nearest row ids by L2 distance.
    #[must_use]
    pub fn search(&self, query: &[f32], k: usize) -> Vec<(String, f32)> {
        if self.nodes.is_empty() || k == 0 {
            return Vec::new();
        }
        let mut scored: Vec<(String, f32)> = self
            .nodes
            .iter()
            .map(|(id, vec)| (id.clone(), l2(query, vec)))
            .collect();
        scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal));
        scored.truncate(k);
        scored
    }

    fn relink(&mut self, id: &str) {
        let Some(vec) = self.nodes.get(id).cloned() else {
            return;
        };
        let mut scored: Vec<(String, f32)> = self
            .nodes
            .iter()
            .filter(|(other, _)| *other != id)
            .map(|(other, v)| (other.clone(), l2(&vec, v)))
            .collect();
        scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal));
        let neighbors: Vec<String> = scored.into_iter().take(self.m).map(|(n, _)| n).collect();
        self.graph.insert(id.to_string(), neighbors.clone());
        for n in neighbors {
            let entry = self.graph.entry(n).or_default();
            if !entry.iter().any(|x| x == id) {
                entry.push(id.to_string());
            }
            if entry.len() > self.m {
                entry.sort_by(|a, b| {
                    let da = self
                        .nodes
                        .get(a)
                        .map(|v| l2(v, self.nodes.get(id).unwrap_or(v)))
                        .unwrap_or(f32::MAX);
                    let db = self
                        .nodes
                        .get(b)
                        .map(|v| l2(v, self.nodes.get(id).unwrap_or(v)))
                        .unwrap_or(f32::MAX);
                    da.partial_cmp(&db).unwrap_or(Ordering::Equal)
                });
                entry.truncate(self.m);
            }
        }
    }
}

fn l2(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| {
            let d = x - y;
            d * d
        })
        .sum::<f32>()
        .sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_returns_nearest_neighbors() {
        let mut idx = HnswIndex::new(3);
        idx.insert("a", &[0.0, 0.0, 0.0]);
        idx.insert("b", &[1.0, 0.0, 0.0]);
        idx.insert("c", &[3.0, 0.0, 0.0]);
        let top = idx.search(&[0.0, 0.0, 0.0], 2);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].0, "a");
        assert_eq!(top[1].0, "b");
    }
}
