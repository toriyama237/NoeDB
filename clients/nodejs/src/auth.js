/**
 * Cluster auth hex (matches noedb_raft::ClusterAuth::from_passphrase).
 * @param {string} passphrase
 * @returns {string}
 */
export function clusterAuthHex(passphrase) {
  const state = new Uint8Array(32);
  const bytes = Buffer.from(passphrase, "utf8");
  for (let i = 0; i < bytes.length; i++) {
    const b = bytes[i];
    state[i % 32] ^= (b * ((i + 31) & 0xff)) & 0xff;
    state[(i + 7) % 32] = (state[(i + 7) % 32] + b) & 0xff;
  }
  return Buffer.from(state).toString("hex");
}
