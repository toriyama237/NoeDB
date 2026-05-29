"""Sync gRPC client for NoeDB SQL API v1."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Sequence

import grpc

from noedb.auth import cluster_auth_hex
from noedb.v1 import sql_pb2, sql_pb2_grpc

AUTH_METADATA = "x-noedb-auth"


@dataclass
class QueryResult:
    """Tabular SQL result."""

    columns: list[str]
    rows: list[list[str]]


class NoeDbClient:
    """TLS gRPC client with cluster auth metadata."""

    def __init__(
        self,
        target: str,
        *,
        passphrase: str = "noedb-dev",
        ca_pem: bytes | None = None,
        cert_pem: bytes | None = None,
        key_pem: bytes | None = None,
    ) -> None:
        host = target if "://" in target else f"https://{target}"
        creds = grpc.ssl_channel_credentials(
            root_certificates=ca_pem,
            private_key=key_pem,
            certificate_chain=cert_pem,
        )
        self._channel = grpc.secure_channel(host, creds)
        self._stub = sql_pb2_grpc.SqlStub(self._channel)
        self._auth_hex = cluster_auth_hex(passphrase)

    @classmethod
    def from_dev_certs(
        cls,
        target: str,
        data_dir: str | Path,
        *,
        passphrase: str = "noedb-dev",
        node_id: int = 1,
    ) -> NoeDbClient:
        """Load dev PKI from a `noedb-cli` data directory."""
        base = Path(data_dir)
        ca = (base / "ca.pem").read_bytes()
        cert = (base / f"node-{node_id}.pem").read_bytes()
        key = (base / f"node-{node_id}-key.pem").read_bytes()
        return cls(target, passphrase=passphrase, ca_pem=ca, cert_pem=cert, key_pem=key)

    def _metadata(self) -> Sequence[tuple[str, str]]:
        return ((AUTH_METADATA, self._auth_hex),)

    def execute(self, query: str) -> QueryResult:
        """Run SQL and return rows."""
        req = sql_pb2.SqlRequest(query=query)
        resp = self._stub.Execute(req, metadata=self._metadata())
        return _parse_result(resp)

    def explain(self, query: str) -> str:
        """Return `EXPLAIN` plan text."""
        req = sql_pb2.SqlRequest(query=query)
        resp = self._stub.Explain(req, metadata=self._metadata())
        if resp.WhichOneof("body") == "explain":
            return resp.explain
        if resp.WhichOneof("body") == "error":
            raise RuntimeError(resp.error.message)
        raise RuntimeError(f"unexpected response: {resp}")

    def ping(self) -> None:
        """Health check."""
        resp = self._stub.Ping(sql_pb2.PingRequest(), metadata=self._metadata())
        if resp.WhichOneof("body") != "pong":
            raise RuntimeError(f"ping failed: {resp}")

    def close(self) -> None:
        """Close the gRPC channel."""
        self._channel.close()


def _parse_result(resp: sql_pb2.SqlResponse) -> QueryResult:
    kind = resp.WhichOneof("body")
    if kind == "result":
        rs = resp.result
        return QueryResult(
            columns=list(rs.columns),
            rows=[list(r.cells) for r in rs.rows],
        )
    if kind == "error":
        raise RuntimeError(resp.error.message)
    raise RuntimeError(f"unexpected response: {resp}")
