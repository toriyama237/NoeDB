//! Joint-consensus membership changes (Phase 4 Week 27).

use serde::{Deserialize, Serialize};

use crate::log::ConfChange;
use crate::types::NodeId;

/// Magic prefix for conf-change log entries.
pub const CONF_MAGIC: &[u8] = b"NOEDB_CONF";

/// Active joint configuration (old + new voter sets).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JointConfig {
    /// Config before change.
    pub outgoing: Vec<NodeId>,
    /// Config after change.
    pub incoming: Vec<NodeId>,
}

impl JointConfig {
    /// Majority size for `outgoing`.
    #[must_use]
    pub fn outgoing_quorum(&self) -> usize {
        self.outgoing.len() / 2 + 1
    }

    /// Majority size for `incoming`.
    #[must_use]
    pub fn incoming_quorum(&self) -> usize {
        self.incoming.len() / 2 + 1
    }
}

/// Encode a membership change for the Raft log.
#[must_use]
pub fn encode_conf_change(cc: &ConfChange) -> Vec<u8> {
    let mut out = CONF_MAGIC.to_vec();
    out.extend(
        bincode::serialize(cc).unwrap_or_default(),
    );
    out
}

/// Decode membership change from log bytes.
#[must_use]
pub fn decode_conf_change(cmd: &[u8]) -> Option<ConfChange> {
    if !cmd.starts_with(CONF_MAGIC) {
        return None;
    }
    bincode::deserialize(&cmd[CONF_MAGIC.len()..]).ok()
}

/// Apply `AddVoter` during joint transition.
#[must_use]
pub fn joint_add_voter(
    joint: &mut Option<JointConfig>,
    voters: &mut Vec<NodeId>,
    id: NodeId,
) -> bool {
    if let Some(j) = joint {
        if !j.incoming.contains(&id) {
            j.incoming.push(id);
        }
        false
    } else {
        let outgoing = voters.clone();
        let mut incoming = voters.clone();
        if !incoming.contains(&id) {
            incoming.push(id);
        }
        *joint = Some(JointConfig { outgoing, incoming });
        false
    }
}

/// Finalize joint config into `voters` when safe.
pub fn joint_finalize(joint: &mut Option<JointConfig>, voters: &mut Vec<NodeId>) {
    if let Some(j) = joint.take() {
        *voters = j.incoming;
    }
}
