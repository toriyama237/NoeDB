//! Async networking (Weeks 35–36).

mod memory;
mod tcp;

pub use memory::MemoryTransport;
pub use tcp::TcpTransport;

use std::future::Future;
use std::pin::Pin;

use crate::error::RaftError;
use crate::rpc::RpcMessage;
use crate::types::NodeId;

/// Async RPC transport between peers.
pub trait Transport: Send + Sync {
    /// Send RPC and await response.
    fn call(
        &self,
        to: NodeId,
        msg: RpcMessage,
    ) -> Pin<Box<dyn Future<Output = Result<RpcMessage, RaftError>> + Send + '_>>;
}
