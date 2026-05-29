//! Phase 7 cross-language cluster auth contract (Week 49).

#![allow(clippy::unwrap_used)]

use noedb_raft::ClusterAuth;

#[test]
fn cluster_auth_hex_derivation_is_stable() {
    let auth = ClusterAuth::from_passphrase("noedb-dev");
    let hex = hex::encode(auth.0);
    let mut state = [0u8; 32];
    for (i, b) in b"noedb-dev".iter().enumerate() {
        state[i % 32] ^= b.wrapping_mul((i as u8).wrapping_add(31));
        state[(i + 7) % 32] = state[(i + 7) % 32].wrapping_add(*b);
    }
    assert_eq!(hex, hex::encode(state));
}
