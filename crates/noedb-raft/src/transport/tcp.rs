//! Tokio TCP transport with length-delimited frames (Week 35).

use std::collections::HashMap;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::codec::{decode_message, encode_message};
use crate::error::RaftError;
use crate::rpc::RpcMessage;
use crate::security::ClusterAuth;
use crate::transport::Transport;
use crate::types::NodeId;

/// TCP transport (fresh connection per RPC — simple and robust for v1).
pub struct TcpTransport {
    auth: ClusterAuth,
    peers: HashMap<NodeId, SocketAddr>,
}

impl TcpTransport {
    /// Build transport from peer address table.
    #[must_use]
    pub const fn new(auth: ClusterAuth, peers: HashMap<NodeId, SocketAddr>) -> Self {
        Self { auth, peers }
    }
}

impl Transport for TcpTransport {
    fn call(
        &self,
        to: NodeId,
        msg: RpcMessage,
    ) -> Pin<Box<dyn Future<Output = Result<RpcMessage, RaftError>> + Send + '_>> {
        let auth = self.auth.clone();
        let peers = self.peers.clone();
        Box::pin(async move {
            let addr = peers
                .get(&to)
                .ok_or_else(|| RaftError::Transport("unknown peer".into()))?;
            let mut stream = TcpStream::connect(*addr)
                .await
                .map_err(|e| RaftError::Transport(e.to_string()))?;
            let frame = encode_message(&auth, &msg)?;
            write_frame(&mut stream, &frame).await?;
            let resp_frame = read_frame(&mut stream).await?;
            decode_message(&auth, &resp_frame)
        })
    }
}

async fn write_frame(stream: &mut TcpStream, data: &[u8]) -> Result<(), RaftError> {
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

async fn read_frame(stream: &mut TcpStream) -> Result<Vec<u8>, RaftError> {
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
