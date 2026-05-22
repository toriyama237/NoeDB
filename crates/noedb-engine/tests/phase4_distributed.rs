//! Phase 4 — distributed engine integration.

#![allow(clippy::unwrap_used)]

use noedb_engine::DistributedEngine;

#[test]
fn linearizable_select_after_replicate() {
    let eng = DistributedEngine::new_voters(3).unwrap();
    eng.tick(80).unwrap();
    eng.put_row("users", "1", "name", b"ada").unwrap();
    let out = eng.execute("SELECT name FROM users").unwrap();
    assert_eq!(out.rows[0][0], "ada");
}

#[test]
fn shard_router_routes_rows() {
    let eng = DistributedEngine::new_voters(3).unwrap();
    assert_eq!(eng.shards().shard_count(), 3);
    let a = eng.shards().route("t", "1");
    let b = eng.shards().route("t", "2");
    assert!(a < 3);
    assert!(b < 3);
}

#[test]
fn add_voter_scales_cluster() {
    let eng = DistributedEngine::new_voters(3).unwrap();
    eng.tick(80).unwrap();
    assert_eq!(eng.voter_count(), 3);
    eng.add_voter(4).unwrap();
    assert_eq!(eng.voter_count(), 4);
}
