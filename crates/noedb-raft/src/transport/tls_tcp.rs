//! mTLS TCP transport for Raft RPC (Phase 1, Week 2).

use noedb_tls::DevCertPem;
use std::collections::HashMap;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

use crate::codec::{decode_message, encode_message};
use crate::error::RaftError;
use crate::rpc::RpcMessage;
use crate::security::ClusterAuth;
use crate::transport::Transport;
use crate::types::NodeId;

/// mTLS transport with pinned server certificate per connection.
pub struct TlsTcpTransport {
    auth: ClusterAuth,
    peers: HashMap<NodeId, SocketAddr>,
    connector: TlsConnector,
}

impl TlsTcpTransport {
    /// Build transport using dev PKI (pin server leaf, present client cert).
    ///
    /// # Errors
    ///
    /// TLS configuration failures.
    pub fn new(
        auth: ClusterAuth,
        peers: HashMap<NodeId, SocketAddr>,
        certs: &DevCertPem,
    ) -> Result<Self, noedb_tls::TlsError> {
        let connector = noedb_tls::client_connector_mtls_pinned(certs)?;
        Ok(Self {
            auth,
            peers,
            connector,
        })
    }
}

impl Transport for TlsTcpTransport {
    fn call(
        &self,
        to: NodeId,
        msg: RpcMessage,
    ) -> Pin<Box<dyn Future<Output = Result<RpcMessage, RaftError>> + Send + '_>> {
        let auth = self.auth.clone();
        let peers = self.peers.clone();
        let connector = self.connector.clone();
        Box::pin(async move {
            let addr = peers
                .get(&to)
                .ok_or_else(|| RaftError::Transport("unknown peer".into()))?;
            let tcp = TcpStream::connect(*addr)
                .await
                .map_err(|e| RaftError::Transport(e.to_string()))?;
            let server_name =
                "localhost"
                    .try_into()
                    .map_err(|e: rustls::pki_types::InvalidDnsNameError| {
                        RaftError::Transport(e.to_string())
                    })?;
            let mut stream = connector
                .connect(server_name, tcp)
                .await
                .map_err(|e| RaftError::Transport(e.to_string()))?;
            let frame = encode_message(&auth, &msg)?;
            write_frame(&mut stream, &frame).await?;
            let resp_frame = read_frame(&mut stream).await?;
            decode_message(&auth, &resp_frame)
        })
    }
}

async fn write_frame<S>(stream: &mut S, data: &[u8]) -> Result<(), RaftError>
where
    S: AsyncWriteExt + Unpin,
{
    let len =
        u32::try_from(data.len()).map_err(|_| RaftError::Transport("frame too large".into()))?;
    stream
        .write_all(&len.to_le_bytes())
        .await
        .map_err(|e| RaftError::Transport(e.to_string()))?;
    stream
        .write_all(data)
        .await
        .map_err(|e| RaftError::Transport(e.to_string()))?;
    Ok(())
}

async fn read_frame<S>(stream: &mut S) -> Result<Vec<u8>, RaftError>
where
    S: AsyncReadExt + Unpin,
{
    let mut len_buf = [0u8; 4];
    stream
        .read_exact(&mut len_buf)
        .await
        .map_err(|e| RaftError::Transport(e.to_string()))?;
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > crate::security::MAX_FRAME_BYTES {
        return Err(RaftError::Security(
            crate::error::SecurityError::FrameTooLarge { size: len },
        ));
    }
    let mut buf = vec![0u8; len];
    stream
        .read_exact(&mut buf)
        .await
        .map_err(|e| RaftError::Transport(e.to_string()))?;
    Ok(buf)
}
