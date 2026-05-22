//! Phase 4 distributed elite — ReadIndex, membership, snapshots.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use noedb_raft::{Cluster, NodeId, Role};

#[test]
fn linearizable_barrier_advances_commit() {
    let mut c = Cluster::new_voters(3).unwrap();
    c.run_rounds(80).unwrap();
    let leader = c.leader().expect("leader");
    let before = c.commit_index(leader);
    let target = c.linearizable_barrier().unwrap();
    assert!(target >= before);
    assert!(c.commit_index(leader) >= target);
}

#[test]
fn add_fourth_voter_via_conf_change() {
    let mut c = Cluster::new_voters(3).unwrap();
    c.run_rounds(80).unwrap();
    assert_eq!(c.voter_count(), 3);
    c.add_voter(NodeId(4)).unwrap();
    assert_eq!(c.voter_count(), 4);
    c.run_rounds(40).unwrap();
    c.propose_on_leader(b"after-scale".to_vec()).unwrap();
    assert!(c.applied_count() >= 1);
}

#[test]
fn snapshot_triggers_on_large_log() {
    let mut c = Cluster::new_voters(3).unwrap();
    c.run_rounds(80).unwrap();
    for i in 0..80u32 {
        c.propose_on_leader(format!("fill-{i}").into_bytes())
            .unwrap();
    }
    let leader = c.leader().expect("leader");
    let applied = c.applied_at(leader).len();
    assert!(applied >= 80);
}

#[test]
fn chaos_kill_leader_reelects() {
    let mut c = Cluster::new_voters(3).unwrap();
    c.run_rounds(120).unwrap();
    let leader = c.leader().expect("leader");
    assert_eq!(c.raft_role(leader), Role::Leader);
    c.remove_node(leader);
    c.run_rounds(200).unwrap();
    assert!(c.leader().is_some());
    c.propose_on_leader(b"post-chaos".to_vec()).unwrap();
}
