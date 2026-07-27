//! Multi-version concurrency control (Phase 2, Weeks 7–14).
//!
//! - [`Version`] — `(commit_ts, value, deleted)` chain per user key
//! - [`TimestampOracle`] — monotonic, process-local commit timestamps
//! - [`MvccMemTable`] — in-memory multi-version store
//! - [`ReadView`] — snapshot visibility (Snapshot Isolation)

mod codec;
mod gc;
mod key;
mod memtable;
mod oracle;
mod read_view;
mod snapshot_store;
mod version;

pub use codec::{decode_or_legacy, decode_version, decode_version_ref, encode_version};
pub use gc::{gc_versions, GcStats};
pub use key::{decode_user_key, encode_internal_key, user_key_prefix_end};
pub use memtable::MvccMemTable;
pub use oracle::TimestampOracle;
pub use read_view::ReadView;
pub use snapshot_store::SnapshotStore;
pub use version::Version;

/// Logical transaction / commit timestamp (strictly increasing).
pub type CommitTs = u64;
/// Opaque transaction identifier.
pub type TxnId = u64;
