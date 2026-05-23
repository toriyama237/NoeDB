# Semaine 9 — MVCC snapshot isolation ✅

**Phase 2 v2 · Livrable clôturé**

| Composant | Crate | Fichier clé |
|-----------|-------|-------------|
| `ReadView` | `noedb-storage` | `src/mvcc/read_view.rs` |
| `SnapshotStore` | `noedb-storage` | `src/mvcc/snapshot_store.rs` |
| Intégration txn | `noedb-txn` / `noedb-engine` | `TxnManager::read_view`, `execute_select_in_txn` |

## Critères d'acceptation

- [x] Isolation snapshot : lectures ne voient pas les écritures non commitées des autres txns
- [x] Read-your-writes dans une session `BEGIN` … `COMMIT`
- [x] 100 transactions concurrentes (`phase2_concurrent.rs`)

Documentation publique : dépôt **noedb-docs** → chapitres [ReadView](https://github.com/toriyama237/noedb-docs) et blog *Semaine 9*.
