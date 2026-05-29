# Query engine

## Pipeline

1. Parse SQL → AST (`noedb-parser`).
2. Build logical plan (`LogicalPlan`: scan, filter, join, aggregate, window, CTE, set ops).
3. Optimize — predicate pushdown, index vs seq scan costing (`cost.rs`, `stats.rs`).
4. Lower to physical plan and execute (`executor.rs`, `parallel.rs`, `simd_pred.rs`).

## Phase 5 features

- Window functions (`OVER`, frames `ROWS`/`RANGE`).
- Subqueries → `SemiJoin` decorrelation.
- `WITH` / `WITH RECURSIVE` CTEs.
- `UNION` / `INTERSECT` / `EXCEPT`.
- `CAST`, extended types, `ANALYZE TABLE`.

Run `EXPLAIN SELECT …` to inspect plans and `rows≈N` estimates.
