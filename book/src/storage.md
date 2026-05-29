# Storage engine

## LSM layout

1. **MemTable** — in-memory sorted map, flushed when full.
2. **WAL** — append-only durability before memtable apply.
3. **SSTables** — immutable sorted files with bloom / XOR filters and optional LZ4 blocks.

## MVCC

Each key stores multiple `Version` values keyed by commit timestamp. Reads use a
`ReadView` snapshot; writes buffer in `TxnManager` until `COMMIT`.

Garbage collection (`gc_versions`, `gc_mvcc_active`) drops versions below the
oldest active snapshot.

## Compaction

L0 files merge into L1+ via k-way merge, preserving ordering and filters.
