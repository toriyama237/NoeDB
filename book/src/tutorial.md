# Tutorial: banking app in 10 minutes

## 1. Start the server

```bash
mkdir -p /tmp/noedb-bank
cargo run -p noedb-cli -- --server --data-dir /tmp/noedb-bank
```

## 2. Create schema and data (REPL or gRPC)

```sql
CREATE TABLE accounts (id TEXT, owner TEXT, balance TEXT);
INSERT INTO accounts VALUES ('1', 'alice', '1000');
INSERT INTO accounts VALUES ('2', 'bob', '500');
```

## 3. Transfer with a transaction

```sql
BEGIN;
UPDATE accounts SET balance = '900' WHERE id = '1';
UPDATE accounts SET balance = '600' WHERE id = '2';
COMMIT;
```

## 4. Query with RLS (optional)

```sql
ALTER TABLE accounts ENABLE ROW LEVEL SECURITY;
CREATE POLICY own ON accounts USING (owner = CURRENT_USER);
SET ROLE 'alice';
SELECT id, balance FROM accounts;
```

## 5. Observe

```bash
curl -s http://127.0.0.1:9090/metrics   # if --metrics-listen set
```

Use a language client from `clients/python`, `clients/go`, or `clients/nodejs`
with dev PKI under `/tmp/noedb-bank/tls/` (created when the server starts).
