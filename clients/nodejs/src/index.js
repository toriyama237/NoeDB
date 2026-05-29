import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import grpc from "@grpc/grpc-js";
import protoLoader from "@grpc/proto-loader";
import { clusterAuthHex } from "./auth.js";

const AUTH_METADATA = "x-noedb-auth";
const __dirname = path.dirname(fileURLToPath(import.meta.url));
const PROTO = path.resolve(__dirname, "../../../proto/noedb/v1/sql.proto");

const packageDef = protoLoader.loadSync(PROTO, {
  keepCase: false,
  longs: String,
  enums: String,
  defaults: true,
  oneofs: true,
});
const proto = grpc.loadPackageDefinition(packageDef).noedb.v1;

/**
 * @typedef {{ columns: string[], rows: string[][] }} QueryResult
 */

export class NoeDbClient {
  /**
   * @param {string} target host:port
   * @param {grpc.ChannelCredentials} credentials
   * @param {string} [passphrase]
   */
  constructor(target, credentials, passphrase = "noedb-dev") {
    this._client = new proto.Sql(target, credentials);
    this._authHex = clusterAuthHex(passphrase);
  }

  /**
   * @param {string} target
   * @param {string} dataDir
   * @param {{ nodeId?: number, passphrase?: string }} [opts]
   */
  static fromDevCerts(target, dataDir, opts = {}) {
    const nodeId = opts.nodeId ?? 1;
    const ca = fs.readFileSync(path.join(dataDir, "ca.pem"));
    const cert = fs.readFileSync(path.join(dataDir, `node-${nodeId}.pem`));
    const key = fs.readFileSync(path.join(dataDir, `node-${nodeId}-key.pem`));
    const creds = grpc.credentials.createSsl(ca, key, cert);
    return new NoeDbClient(target, creds, opts.passphrase ?? "noedb-dev");
  }

  /** @returns {grpc.Metadata} */
  _meta() {
    const m = new grpc.Metadata();
    m.set(AUTH_METADATA, this._authHex);
    return m;
  }

  ping() {
    return new Promise((resolve, reject) => {
      this._client.Ping({}, this._meta(), (err, resp) => {
        if (err) return reject(err);
        if (!resp.pong) return reject(new Error(`unexpected ping: ${JSON.stringify(resp)}`));
        resolve();
      });
    });
  }

  /**
   * @param {string} query
   * @returns {Promise<QueryResult>}
   */
  execute(query) {
    return new Promise((resolve, reject) => {
      this._client.Execute({ query }, this._meta(), (err, resp) => {
        if (err) return reject(err);
        if (resp.error) return reject(new Error(resp.error.message));
        if (!resp.result) return reject(new Error(`unexpected: ${JSON.stringify(resp)}`));
        resolve({
          columns: resp.result.columns ?? [],
          rows: (resp.result.rows ?? []).map((r) => r.cells ?? []),
        });
      });
    });
  }

  close() {
    grpc.closeClient(this._client);
  }
}

export { clusterAuthHex };
