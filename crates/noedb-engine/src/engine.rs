//! Local and distributed SQL engines.

use std::collections::HashMap;
use std::path::Path;

use noedb_ast::Statement;
use noedb_planner::{
    apply_statement, execute_sql, explain_sql, ExecError, PlanError, Record, Value,
};
use noedb_raft::{Cluster, NodeId, RaftError, Role};
use noedb_storage::{LsmConfig, LsmTree};

use crate::command::Command;
use crate::error::EngineError;
use crate::machine::apply_command;

/// Maximum SQL text accepted by the engine (DoS bound).
pub const MAX_SQL_BYTES: usize = 64 * 1024;

/// Tabular query result for clients and wire protocol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryResult {
    /// Column names in order.
    pub columns: Vec<String>,
    /// Rows as UTF-8 strings (`NULL` shown as empty for now).
    pub rows: Vec<Vec<String>>,
}

impl QueryResult {
    /// Build from planner records (first row defines column order).
    #[must_use]
    pub fn from_records(records: &[Record]) -> Self {
        if records.is_empty() {
            return Self {
                columns: Vec::new(),
                rows: Vec::new(),
            };
        }
        let columns: Vec<String> = records[0].fields.iter().map(|(n, _)| n.clone()).collect();
        let rows = records
            .iter()
            .map(|r| {
                columns
                    .iter()
                    .map(|col| {
                        r.fields
                            .iter()
                            .find(|(n, _)| n == col)
                            .map(|(_, v)| value_to_string(v))
                            .unwrap_or_default()
                    })
                    .collect()
            })
            .collect();
        Self { columns, rows }
    }
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::Integer(n) => n.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
    }
}

/// Validate SQL input before parse (security / DoS).
///
/// # Errors
///
/// [`EngineError::InvalidSql`] when rejected.
pub fn validate_sql(sql: &str) -> Result<(), EngineError> {
    if sql.is_empty() {
        return Err(EngineError::InvalidSql("empty query"));
    }
    if sql.len() > MAX_SQL_BYTES {
        return Err(EngineError::InvalidSql("query too long"));
    }
    if sql.bytes().any(|b| b == 0) {
        return Err(EngineError::InvalidSql("NUL byte in query"));
    }
    Ok(())
}

/// Single-node engine (no Raft) — fast local development.
pub struct LocalEngine {
    tree: LsmTree,
}

impl LocalEngine {
    /// Open or create storage at `path`.
    ///
    /// # Errors
    ///
    /// Storage initialization failures.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, EngineError> {
        let tree = LsmTree::open(path, LsmConfig::default())?;
        Ok(Self { tree })
    }

    /// Underlying LSM (tests).
    #[must_use]
    pub const fn store(&self) -> &LsmTree {
        &self.tree
    }

    /// Mutable store (tests).
    pub const fn store_mut(&mut self) -> &mut LsmTree {
        &mut self.tree
    }

    /// Insert a row cell (local only, not replicated).
    ///
    /// # Errors
    ///
    /// Storage write failure.
    pub fn put_row(
        &mut self,
        table: &str,
        row: &str,
        column: &str,
        value: &[u8],
    ) -> Result<(), EngineError> {
        let cmd = Command::Put {
            table: table.to_string(),
            row: row.to_string(),
            column: column.to_string(),
            value: value.to_vec(),
        };
        apply_command(&mut self.tree, &cmd)?;
        Ok(())
    }

    /// Parse and run SQL.
    ///
    /// # Errors
    ///
    /// Validation, parse, or execution errors.
    pub fn execute(&mut self, sql: &str) -> Result<QueryResult, EngineError> {
        validate_sql(sql)?;
        let stmt = noedb_parser::parse(sql)?;
        let records = apply_statement(&stmt, &mut self.tree)?;
        Ok(QueryResult::from_records(&records))
    }

    /// `EXPLAIN` for a `SELECT`.
    ///
    /// # Errors
    ///
    /// Validation, parse, or planner errors.
    pub fn explain(&self, sql: &str) -> Result<String, EngineError> {
        validate_sql(sql)?;
        let stmt = noedb_parser::parse(sql)?;
        explain_sql(&stmt, &self.tree).map_err(plan_err)
    }
}

/// Three-node in-process cluster: writes via Raft, reads on leader LSM.
pub struct DistributedEngine {
    cluster: Cluster,
    stores: HashMap<NodeId, LsmTree>,
    voter_ids: Vec<NodeId>,
    applied_watermark: HashMap<NodeId, usize>,
    cached_leader: Option<NodeId>,
}

impl DistributedEngine {
    /// Spin up voters `1..=n` with ephemeral on-disk stores.
    ///
    /// # Errors
    ///
    /// Raft or storage initialization failures.
    pub fn new_voters(n: u64) -> Result<Self, EngineError> {
        let cluster = Cluster::new_voters(n)?;
        let voter_ids = cluster.voter_ids();
        let mut stores = HashMap::new();
        let mut applied_watermark = HashMap::new();
        for id in &voter_ids {
            let dir = std::env::temp_dir().join(format!(
                "noedb-engine-{}-{}",
                id.0,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_nanos())
            ));
            let tree = LsmTree::open(&dir, LsmConfig::default())?;
            stores.insert(*id, tree);
            applied_watermark.insert(*id, 0);
        }
        Ok(Self {
            cluster,
            stores,
            voter_ids,
            applied_watermark,
            cached_leader: None,
        })
    }

    /// Run simulation rounds (election / heartbeat).
    ///
    /// # Errors
    ///
    /// Raft simulation errors.
    pub fn tick(&mut self, rounds: usize) -> Result<(), EngineError> {
        self.cluster.run_rounds(rounds)?;
        self.sync_applied()?;
        Ok(())
    }

    /// Current leader, if unique.
    #[must_use]
    pub fn leader(&self) -> Option<NodeId> {
        self.cluster.leader()
    }

    /// Replicate a row cell through Raft.
    ///
    /// # Errors
    ///
    /// No leader or replication failure.
    pub fn put_row(
        &mut self,
        table: &str,
        row: &str,
        column: &str,
        value: &[u8],
    ) -> Result<(), EngineError> {
        let cmd = Command::Put {
            table: table.to_string(),
            row: row.to_string(),
            column: column.to_string(),
            value: value.to_vec(),
        };
        self.replicate(&cmd)
    }

    /// Seed the leader's LSM directly (tests/benches — skips Raft for bulk load).
    ///
    /// # Errors
    ///
    /// Storage or planner errors.
    pub fn seed_leader_row(
        &mut self,
        table: &str,
        row: &str,
        column: &str,
        value: &[u8],
    ) -> Result<(), EngineError> {
        let leader = self.ensure_leader()?;
        let cmd = Command::Put {
            table: table.to_string(),
            row: row.to_string(),
            column: column.to_string(),
            value: value.to_vec(),
        };
        let store = self.stores.get_mut(&leader).ok_or_else(|| {
            EngineError::Raft(RaftError::internal("leader store missing"))
        })?;
        apply_command(store, &cmd)?;
        Ok(())
    }

    /// Parse and execute SQL on the cluster.
    ///
    /// # Errors
    ///
    /// Validation, parse, Raft, or execution errors.
    pub fn execute(&mut self, sql: &str) -> Result<QueryResult, EngineError> {
        validate_sql(sql)?;
        let stmt = noedb_parser::parse(sql)?;
        match &stmt {
            Statement::Select(_) => {
                let leader = self.ensure_leader()?;
                let store = self.stores.get(&leader).ok_or_else(|| {
                    EngineError::Raft(RaftError::internal("leader store missing"))
                })?;
                let records = execute_sql(&stmt, store)?;
                Ok(QueryResult::from_records(&records))
            }
            Statement::CreateIndex(idx) => {
                if idx.columns.len() != 1 {
                    return Err(EngineError::Exec(ExecError::UnsupportedExpr));
                }
                let cmd = Command::CreateIndex {
                    table: idx.table.value.clone(),
                    column: idx.columns[0].value.clone(),
                };
                self.replicate(&cmd)?;
                Ok(QueryResult {
                    columns: vec![],
                    rows: vec![],
                })
            }
            _ => Err(EngineError::UnsupportedStatement),
        }
    }

    /// `EXPLAIN` on the leader store.
    ///
    /// # Errors
    ///
    /// Validation, parse, or planner errors.
    pub fn explain(&mut self, sql: &str) -> Result<String, EngineError> {
        validate_sql(sql)?;
        let stmt = noedb_parser::parse(sql)?;
        let leader = self.ensure_leader()?;
        let store = self
            .stores
            .get(&leader)
            .ok_or_else(|| EngineError::Raft(RaftError::internal("leader store missing")))?;
        explain_sql(&stmt, store).map_err(plan_err)
    }

    fn ensure_leader(&mut self) -> Result<NodeId, EngineError> {
        if let Some(id) = self.cached_leader {
            if self.cluster.raft_role(id) == Role::Leader {
                return Ok(id);
            }
        }
        if let Some(id) = self.cluster.leader() {
            self.cached_leader = Some(id);
            return Ok(id);
        }
        self.cluster.run_rounds(8)?;
        let id = self.cluster.leader().ok_or(EngineError::NoLeader)?;
        self.cached_leader = Some(id);
        Ok(id)
    }

    fn replicate(&mut self, cmd: &Command) -> Result<(), EngineError> {
        let bytes = cmd.encode()?;
        self.cluster.propose_on_leader(bytes)?;
        self.sync_applied()?;
        Ok(())
    }

    fn sync_applied(&mut self) -> Result<(), EngineError> {
        for &id in &self.voter_ids {
            let applied = self.cluster.applied_at(id);
            let wm = self.applied_watermark.get(&id).copied().unwrap_or(0);
            let Some(store) = self.stores.get_mut(&id) else {
                continue;
            };
            for bytes in &applied[wm..] {
                let cmd = Command::decode(bytes)?;
                apply_command(store, &cmd)?;
            }
            self.applied_watermark.insert(id, applied.len());
        }
        Ok(())
    }
}

/// Convenience: seed without going through SQL parser.
const fn plan_err(e: PlanError) -> EngineError {
    EngineError::Exec(ExecError::Plan(e))
}
