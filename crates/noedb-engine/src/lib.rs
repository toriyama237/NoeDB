//! End-to-end SQL engine for NoeDB (Phase 5).
//!
//! Wires **parser → planner → Raft → LSM** with bounded inputs and
//! deterministic in-process clustering for integration tests.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
#![allow(
    clippy::cast_possible_truncation,
    clippy::needless_pass_by_value,
    clippy::missing_const_for_fn,
    clippy::map_unwrap_or,
    clippy::needless_borrow,
    clippy::significant_drop_tightening,
    clippy::significant_drop_in_scrutinee,
    clippy::type_complexity
)]

mod audit;
mod command;
mod crypto;
mod dml;
mod engine;
mod error;
mod knn;
mod machine;
mod memory;
mod prepared;
mod query_cache;
mod ratelimit;
mod redact;
mod region;
mod rls;
mod schema;
mod security;
mod session;
mod shard;
mod txn;
mod vector_index;

pub use audit::{AuditEntry, AuditLog};
pub use command::{Command, MAX_COMMAND_BYTES};
pub use crypto::{cell_aad, is_sealed, DataKey};
pub use engine::{
    validate_sql, DistributedEngine, LocalEngine, QueryResult, DEFAULT_SESSION, MAX_SQL_BYTES,
};
pub use error::EngineError;
pub use machine::{apply_command, row_key};
pub use memory::{MemoryBudget, MemoryGuard, DEFAULT_MEM_BUDGET_BYTES};
pub use noedb_metrics::{spawn_prometheus_listener, Metrics};
pub use noedb_txn::TxnManager;
pub use prepared::{bind_parameters, PrepareCache, PreparedStatement};
pub use query_cache::QueryCache;
pub use ratelimit::{Deadline, RateLimiter, DEFAULT_BURST, DEFAULT_QPS};
pub use redact::{is_sensitive, redact_sql};
pub use region::RegionId;
pub use rls::{apply_rls, materialize_session, Policy, RlsCatalog};
pub use schema::{SchemaCatalog, TableSchema};
pub use security::{
    authorize_role_change, authorize_statement, is_admin_role, reject_multi_statement,
};
pub use session::SessionContext;
pub use shard::ShardRouter;
pub use vector_index::VectorIndexCatalog;
