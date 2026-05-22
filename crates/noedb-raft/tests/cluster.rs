//! Integration tests: election, replication (Weeks 37–38).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use noedb_raft::{Cluster, Role};

#[test]
fn three_node_elects_leader() {
    let mut c = Cluster::new_voters(3).unwrap();
    c.run_rounds(80).unwrap();
    assert!(c.leader().is_some());
}

#[test]
fn replicates_ten_commands() {
    let mut c = Cluster::new_voters(3).unwrap();
    c.run_rounds(80).unwrap();
    for i in 0..10u32 {
        c.propose_on_leader(format!("cmd-{i}").into_bytes())
            .unwrap();
    }
    assert!(c.applied_count() >= 10);
}

#[test]
fn all_voters_see_leader_role() {
    let mut c = Cluster::new_voters(3).unwrap();
    c.run_rounds(500).unwrap();
    let leader = c.leader().expect("leader");
    assert_eq!(c.raft_role(leader), Role::Leader);
}
