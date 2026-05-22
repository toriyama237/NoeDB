//! In-memory transport for deterministic cluster tests.

#![allow(clippy::expect_used, clippy::significant_drop_tightening)]

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use crate::error::RaftError;
use crate::raft_core::Raft;
use crate::rpc::RpcMessage;
use crate::transport::Transport;
use crate::types::NodeId;

type Handler = Arc<Mutex<Raft>>;

/// Hub connecting multiple Raft nodes in-process.
#[derive(Clone, Default)]
pub struct MemoryTransport {
    routes: Arc<Mutex<HashMap<NodeId, Handler>>>,
}

impl MemoryTransport {
    /// Register a node handler.
    pub fn register(&self, id: NodeId, raft: Handler) {
        self.routes.lock().expect("lock").insert(id, raft);
    }
}

impl Transport for MemoryTransport {
    fn call(
        &self,
        to: NodeId,
        msg: RpcMessage,
    ) -> Pin<Box<dyn Future<Output = Result<RpcMessage, RaftError>> + Send + '_>> {
        let routes = Arc::clone(&self.routes);
        Box::pin(async move {
            let routes = routes
                .lock()
                .map_err(|e| RaftError::Transport(e.to_string()))?;
            let target = routes
                .get(&to)
                .ok_or_else(|| RaftError::Transport("unknown peer".into()))?;
            let mut raft = target
                .lock()
                .map_err(|e| RaftError::Transport(e.to_string()))?;
            let (resp, _actions) = raft.step(NodeId(0), msg);
            resp.ok_or_else(|| RaftError::Transport("no response".into()))
        })
    }
}
