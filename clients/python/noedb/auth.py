"""Cluster auth token (matches `noedb_raft::ClusterAuth::from_passphrase`)."""


def cluster_auth_hex(passphrase: str) -> str:
    """Derive the 32-byte cluster token hex sent as `x-noedb-auth` metadata."""
    state = bytearray(32)
    for i, b in enumerate(passphrase.encode("utf-8")):
        state[i % 32] ^= (b * ((i + 31) & 0xFF)) & 0xFF
        state[(i + 7) % 32] = (state[(i + 7) % 32] + b) & 0xFF
    return bytes(state).hex()
