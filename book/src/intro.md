# NoeDB

NoeDB is a distributed SQL engine written in Rust: lexer → parser → planner → Raft → LSM.

**v2.0.0** ships MVCC transactions, TLS/mTLS gRPC, window functions, CTEs, set operations,
column statistics, Prometheus metrics, and multi-language gRPC clients.

```bash
cargo run -p noedb-cli
cargo run -p noedb-cli -- --server --metrics-listen 127.0.0.1:9090
```
