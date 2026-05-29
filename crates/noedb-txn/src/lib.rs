//! Transaction manager for NoeDB (Phase 2, Weeks 8–14).
//!
//! - [`Transaction`] — `txn_id`, `start_ts`, read/write sets
//! - [`TxnManager`] — `BEGIN` / `COMMIT` / `ROLLBACK`
//! - [`ReadView`] — snapshot isolation visibility
//! - [`SsiChecker`] — serializable snapshot isolation (Cahill 2008 subset)
//! - [`DeadlockGuard`] — wait-for graph + 5s timeout fallback

#![forbid(unsafe_code)]
#![allow(
    clippy::significant_drop_tightening,
    clippy::significant_drop_in_scrutinee
)]

mod deadlock;
mod error;
mod manager;
mod ssi;
mod transaction;

pub use deadlock::{DeadlockGuard, DeadlockPolicy};
pub use error::TxnError;
pub use manager::{CommitResult, TxnManager};
pub use noedb_storage::mvcc::{CommitTs, ReadView, TxnId};
pub use ssi::{SsiChecker, SsiDecision};
pub use transaction::{Transaction, TxnState, WriteOp};
