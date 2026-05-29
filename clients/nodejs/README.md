# @noe/noedb-client

Node.js gRPC client for [NoeDB](https://github.com/toriyama237/NoeDB) v2.

```bash
npm install
node --input-type=module -e "
import { NoeDbClient } from './src/index.js';
const c = NoeDbClient.fromDevCerts('127.0.0.1:5434', '/tmp/noedb-dev');
await c.ping();
console.log(await c.execute('SELECT 1'));
c.close();
"
```
