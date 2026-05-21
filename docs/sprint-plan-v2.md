# NoeDB v2.0 — Sprint plan (52 semaines)

> **Début :** 2026-06-01 · **Fin cible :** 2027-05-29 · **Base :** v1.0.1

PDF / DOCX détaillé : [`NoeDB_v2_Sprint_Plan.docx`](NoeDB_v2_Sprint_Plan.docx)

## Phase 1 — Security & Protocol (S1–S6) 🟡

| Semaine | Statut | Livrable |
|---------|--------|----------|
| 01 | 🟡 en cours | TLS 1.3 actif (`noedb-tls`, serveur `--server` TLS par défaut) |
| 02 | ⏳ | mTLS cluster Raft |
| 03 | ⏳ | gRPC (tonic) remplace TCP custom |
| 04 | ⏳ | Prepared statements |
| 05 | ⏳ | Row Level Security |
| 06 | ⏳ | Audit log, fuzz, SECURITY.md |

### Semaine 1 — fait / en cours

- Crate **`noedb-tls`** : certs dev `rcgen`, `rustls` 1.3, tests handshake
- **`noedb-cli --server`** : TLS par défaut (`--no-tls` pour debug)
- **`noedb-cli --ping`** : client TLS + requête `Ping`

## Phases suivantes

| Phase | Semaines | Thème |
|-------|----------|-------|
| 2 | 7–14 | MVCC & Transactions |
| 3 | 15–24 | Performance extrême |
| 4 | 25–36 | Distributed elite |
| 5 | 37–44 | Query engine v2 |
| 6 | 45–48 | Observabilité & fiabilité |
| 7 | 49–52 | Ecosystem & **v2.0.0** |
