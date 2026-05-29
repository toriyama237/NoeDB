//! Phase 6 (Week 46): property tests on the in-process Raft simulator.

#![allow(clippy::unwrap_used)]

use noedb_raft::Cluster;

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(32))]
    #[test]
    fn random_proposals_commit_on_majority(ops in 1usize..12) {
        let mut c = Cluster::new_voters(3).unwrap();
        c.run_rounds(80).unwrap();
        let before = c.applied_count();
        for i in 0..ops {
            let payload = format!("op-{i}");
            c.propose_on_leader(payload.into_bytes())?;
        }
        let after = c.applied_count();
        proptest::prop_assert!(after >= before + ops);
        let leader = c.leader().ok_or_else(|| {
            proptest::test_runner::TestCaseError::fail("no leader after proposals")
        })?;
        let applied = c.applied_at(leader).len();
        for id in c.voter_ids() {
            let n = c.applied_at(id).len();
            proptest::prop_assert!(
                n == applied || n + 1 == applied,
                "applied log diverged: leader={applied} node={} n={n}",
                id.0
            );
        }
    }
}
