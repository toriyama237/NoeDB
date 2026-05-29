# noedb (Python)

gRPC client for [NoeDB](https://github.com/toriyama237/NoeDB) v2.

## Quickstart

```bash
# Terminal 1 — server with dev TLS
cargo run -p noedb-cli -- --server --data-dir /tmp/noedb-dev

# Terminal 2
pip install -e .
python -c "
from noedb import NoeDbClient
c = NoeDbClient.from_dev_certs('127.0.0.1:5434', '/tmp/noedb-dev')
c.ping()
print(c.execute('SELECT 1').rows)
c.close()
"
```

Auth uses passphrase `noedb-dev` by default (`x-noedb-auth` metadata).
