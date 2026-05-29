//! Phase 6 reliability (Week 48): leader failure recovery (RTO smoke test).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::{Duration, Instant};

use noedb_raft::Cluster;

#[test]
fn raft_leader_crash_reelects_quickly() {
    let mut cluster = Cluster::new_voters(3).unwrap();
    cluster.run_rounds(80).unwrap();
    let leader = cluster.leader().expect("initial leader");
    cluster.propose_on_leader(b"seed".to_vec()).unwrap();

    let start = Instant::now();
    cluster.remove_node(leader);
    let mut elected = None;
    for _ in 0..64 {
        cluster.tick_all(25).unwrap();
        cluster.deliver_all().unwrap();
        if let Some(id) = cluster.leader() {
            elected = Some(id);
            break;
        }
    }
    let elapsed = start.elapsed();
    assert!(elected.is_some(), "cluster should elect a new leader");
    assert!(
        elapsed < Duration::from_secs(2),
        "RTO smoke: re-election took {:?}",
        elapsed
    );
    cluster.propose_on_leader(b"after-failover".to_vec()).unwrap();
}
