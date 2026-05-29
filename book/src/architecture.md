# Architecture

```text
 SQL → noedb-lexer → noedb-parser → noedb-ast
                    → noedb-planner → noedb-engine
                    → noedb-raft (optional) → noedb-storage (LSM)
```

| Crate | Role |
|-------|------|
| `noedb-lexer` | Tokenization, spans |
| `noedb-parser` | AST construction |
| `noedb-planner` | Logical/physical plans, execution |
| `noedb-storage` | LSM, WAL, MVCC, SSTables |
| `noedb-raft` | Consensus, replication |
| `noedb-engine` | End-to-end SQL |
| `noedb-grpc` | gRPC SQL API |
| `noedb-pool` | Connection pool + read/write routing |
| `noedb-metrics` | Prometheus exposition |

Clients live under `clients/` (Python, Go, Node.js).
