# NoeDB language clients

gRPC clients for [`proto/noedb/v1/sql.proto`](../../proto/noedb/v1/sql.proto).

| Directory | Runtime | Install |
|-----------|---------|---------|
| [python/](python/) | Python 3.10+ | `pip install -e python` |
| [go/](go/) | Go 1.22+ | `go get github.com/toriyama237/noedb-go` |
| [nodejs/](nodejs/) | Node 18+ | `npm install` in `nodejs/` |

Start a server with dev TLS, then run `scripts/cross_driver_contract.sh`.
