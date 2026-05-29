# Raft & distribution

NoeDB implements Raft (Ongaro & Ousterhout) in `noedb-raft`:

- **Election** — `RequestVote` with randomized timeouts.
- **Replication** — batched `AppendEntries`, pipelining, group commit.
- **Linearizable reads** — `ReadIndex` barrier on the leader.
- **Membership** — joint-configuration changes (`add_voter`).

`DistributedEngine` runs an in-process 3-node cluster for tests; production
paths use TLS-framed RPC (`TlsTcpTransport`) and gRPC (`noedb-grpc`).

## Sharding

`ShardRouter` hashes row keys to shards; each shard maps to a Raft group (roadmap:
multi-region deployment).
