// Package noedb provides a gRPC client for NoeDB SQL API v1.
package noedb

import "encoding/hex"

// ClusterAuthHex derives the 32-byte cluster token (hex) for x-noedb-auth metadata.
func ClusterAuthHex(passphrase string) string {
	var state [32]byte
	for i, b := range []byte(passphrase) {
		state[i%32] ^= b * byte(i+31)
		state[(i+7)%32] += b
	}
	return hex.EncodeToString(state[:])
}
