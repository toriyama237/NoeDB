//! Local and distributed SQL engines.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use dashmap::DashMap;
use noedb_ast::Statement;
use noedb_planner::{
    apply_statement, execute_sql, explain_sql, ExecError, PlanError, Record, Value,
};
use noedb_raft::{Cluster, NodeId, RaftError, Role};
use noedb_storage::{LsmConfig, LsmTree};
use noedb_txn::TxnManager;
use parking_lot::{Mutex, RwLock};

use crate::audit::AuditLog;
use crate::command::Command;
use crate::error::EngineError;
use crate::machine::apply_command;
use crate::prepared::{bind_parameters, PrepareCache};
use crate::query_cache::QueryCache;
use crate::region::RegionId;
use crate::rls::{apply_rls, RlsCatalog};
use crate::schema::SchemaCatalog;
use crate::session::SessionContext;
use crate::shard::ShardRouter;
use crate::txn::{commit_to_storage, execute_select_in_txn, put_row_in_txn, txn_err};

/// Default session id for single-client CLI / tests.
pub const DEFAULT_SESSION: u64 = 1;

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

/// Single-node engine (no Raft) — concurrent via interior mutability.
pub struct LocalEngine {
    storage: Arc<RwLock<LsmTree>>,
    sessions: DashMap<u64, SessionContext>,
    prepare: Mutex<PrepareCache>,
    rls: Mutex<RlsCatalog>,
    audit: AuditLog,
    txn: Arc<TxnManager>,
    cache: Mutex<QueryCache>,
    schema: Mutex<SchemaCatalog>,
}

impl LocalEngine {
    /// Open or create storage at `path`.
    ///
    /// # Errors
    ///
    /// Storage initialization failures.
    pub fn open(path: impl AsRef<Path>) -> Result<Arc<Self>, EngineError> {
        Self::open_with_config(path, LsmConfig::default())
    }

    /// Open with throughput-oriented LSM (group commit, large memtable).
    ///
    /// # Errors
    ///
    /// Storage initialization failures.
    pub fn open_throughput(path: impl AsRef<Path>) -> Result<Arc<Self>, EngineError> {
        Self::open_with_config(path, LsmConfig::throughput())
    }

    fn open_with_config(
        path: impl AsRef<Path>,
        config: LsmConfig,
    ) -> Result<Arc<Self>, EngineError> {
        let data_dir = path.as_ref().to_path_buf();
        let tree = LsmTree::open(&data_dir, config)?;
        let audit = AuditLog::open(&data_dir)?;
        let sessions = DashMap::new();
        sessions.insert(DEFAULT_SESSION, SessionContext::dev());
        Ok(Arc::new(Self {
            storage: Arc::new(RwLock::new(tree)),
            sessions,
            prepare: Mutex::new(PrepareCache::default()),
            rls: Mutex::new(RlsCatalog::default()),
            audit,
            txn: Arc::new(TxnManager::new()),
            cache: Mutex::new(QueryCache::default()),
            schema: Mutex::new(SchemaCatalog::default()),
        }))
    }

    /// Schema catalog (DDL versions).
    #[must_use]
    pub fn schema(&self) -> &Mutex<SchemaCatalog> {
        &self.schema
    }

    /// Current session role (`SET ROLE`) for the default session.
    #[must_use]
    pub fn session_role(&self) -> String {
        self.session_role_for(DEFAULT_SESSION)
    }

    /// Role for a specific session.
    #[must_use]
    pub fn session_role_for(&self, session_id: u64) -> String {
        self.sessions
            .get(&session_id)
            .map(|s| s.role.clone())
            .unwrap_or_else(|| "anonymous".into())
    }

    /// Transaction manager (MVCC, lock-free TSO).
    #[must_use]
    pub fn txn(&self) -> &Arc<TxnManager> {
        &self.txn
    }

    /// Underlying LSM (shared, use read/write guards).
    #[must_use]
    pub fn storage(&self) -> &Arc<RwLock<LsmTree>> {
        &self.storage
    }

    /// Underlying LSM read guard (tests / benchmarks).
    pub fn store(&self) -> parking_lot::RwLockReadGuard<'_, LsmTree> {
        self.storage.read()
    }

    /// Insert a row cell (local only, not replicated).
    ///
    /// # Errors
    ///
    /// Storage write failure.
    pub fn put_row(
        &self,
        session_id: u64,
        table: &str,
        row: &str,
        column: &str,
        value: &[u8],
    ) -> Result<(), EngineError> {
        if self.txn.in_txn(session_id) {
            return put_row_in_txn(&self.txn, session_id, table, row, column, value);
        }
        let cmd = Command::Put {
            table: table.to_string(),
            row: row.to_string(),
            column: column.to_string(),
            value: value.to_vec(),
        };
        apply_command(&mut self.storage.write(), &cmd)?;
        self.cache.lock().invalidate_table(table);
        Ok(())
    }

    /// Convenience `put_row` on the default session.
    pub fn put_row_default(
        &self,
        table: &str,
        row: &str,
        column: &str,
        value: &[u8],
    ) -> Result<(), EngineError> {
        self.put_row(DEFAULT_SESSION, table, row, column, value)
    }

    /// Parse and run SQL on the default session.
    ///
    /// # Errors
    ///
    /// Validation, parse, or execution errors.
    pub fn execute(&self, sql: &str) -> Result<QueryResult, EngineError> {
        self.execute_session(DEFAULT_SESSION, sql)
    }

    /// Parse and run SQL for a concurrent session (`&self` — Rayon-safe).
    ///
    /// # Errors
    ///
    /// Validation, parse, or execution errors.
    pub fn execute_session(&self, session_id: u64, sql: &str) -> Result<QueryResult, EngineError> {
        validate_sql(sql)?;
        let stmt = noedb_parser::parse(sql)?;
        self.dispatch(session_id, stmt, sql)
    }

    fn dispatch(
        &self,
        session_id: u64,
        stmt: Statement,
        sql: &str,
    ) -> Result<QueryResult, EngineError> {
        match stmt {
            Statement::Prepare(p) => {
                if !matches!(*p.inner, Statement::Select(_)) {
                    return Err(EngineError::InvalidSql("PREPARE only supports SELECT"));
                }
                self.prepare.lock().insert(&p.name.value, *p.inner)?;
                self.audit_record(session_id, sql, 0)?;
                Ok(empty_ok())
            }
            Statement::Execute(e) => {
                let cache = self.prepare.lock();
                let prep = cache
                    .get(&e.name.value)
                    .ok_or(EngineError::InvalidSql("unknown prepared statement"))?;
                let bound = bind_parameters(&prep.stmt, &e.params)?;
                let role = self.session_role_for(session_id);
                let bound = apply_rls(bound, &self.rls.lock(), &role);
                self.run_data_statement(session_id, bound, sql)
            }
            Statement::SetRole(s) => {
                if let Some(mut ctx) = self.sessions.get_mut(&session_id) {
                    ctx.role = s.role;
                } else {
                    self.sessions.insert(
                        session_id,
                        SessionContext {
                            tenant: "default".into(),
                            role: s.role,
                        },
                    );
                }
                self.audit_record(session_id, sql, 0)?;
                Ok(empty_ok())
            }
            Statement::EnableRls(e) => {
                self.rls.lock().enable(&e.table.value);
                self.audit_record(session_id, sql, 0)?;
                Ok(empty_ok())
            }
            Statement::CreatePolicy(p) => {
                self.rls.lock().add_policy(&p);
                self.audit_record(session_id, sql, 0)?;
                Ok(empty_ok())
            }
            Statement::CreateTable(t) => {
                let ts = self.txn.oracle().next();
                self.schema.lock().create_table(&t.name.value, ts);
                self.cache.lock().invalidate_table("");
                self.audit_record(session_id, sql, 0)?;
                Ok(empty_ok())
            }
            Statement::BeginTxn(_) => {
                self.txn.begin(session_id).map_err(txn_err)?;
                self.audit_record(session_id, sql, 0)?;
                Ok(empty_ok())
            }
            Statement::CommitTxn(_) => {
                commit_to_storage(&self.txn, session_id, &mut self.storage.write())?;
                self.cache.lock().invalidate_table("");
                self.audit_record(session_id, sql, 0)?;
                Ok(empty_ok())
            }
            Statement::RollbackTxn(_) => {
                self.txn.rollback(session_id).map_err(txn_err)?;
                self.audit_record(session_id, sql, 0)?;
                Ok(empty_ok())
            }
            other => {
                let role = self.session_role_for(session_id);
                let other = apply_rls(other, &self.rls.lock(), &role);
                self.run_data_statement(session_id, other, sql)
            }
        }
    }

    fn run_data_statement(
        &self,
        session_id: u64,
        stmt: Statement,
        sql: &str,
    ) -> Result<QueryResult, EngineError> {
        let result = if self.txn.in_txn(session_id) && matches!(stmt, Statement::Select(_)) {
            execute_select_in_txn(self.txn.clone(), session_id, &stmt, &self.storage)?
        } else if matches!(stmt, Statement::Select(_)) {
            let role = self.session_role_for(session_id);
            let cached = self.cache.lock().get(&role, sql);
            if let Some(hit) = cached {
                hit
            } else {
                let tree = self.storage.read();
                let records = execute_sql(&stmt, &tree).map_err(EngineError::Exec)?;
                let qr = QueryResult::from_records(&records);
                self.cache.lock().put(&role, sql, qr.clone());
                qr
            }
        } else {
            match &stmt {
                Statement::Insert(i) => self.cache.lock().invalidate_table(&i.table.value),
                Statement::Update(u) => self.cache.lock().invalidate_table(&u.table.value),
                Statement::Delete(d) => self.cache.lock().invalidate_table(&d.table.value),
                _ => {}
            }
            let records = apply_statement(&stmt, &mut self.storage.write())?;
            QueryResult::from_records(&records)
        };
        self.audit_record(
            session_id,
            sql,
            u64::try_from(result.rows.len()).unwrap_or(0),
        )?;
        Ok(result)
    }

    fn audit_record(&self, session_id: u64, sql: &str, rows: u64) -> Result<(), EngineError> {
        let (tenant, role) = self
            .sessions
            .get(&session_id)
            .map(|s| (s.tenant.clone(), s.role.clone()))
            .unwrap_or_else(|| ("default".into(), "anonymous".into()));
        self.audit.record(&tenant, &role, sql, rows)
    }

    /// `EXPLAIN` for a `SELECT`.
    ///
    /// # Errors
    ///
    /// Validation, parse, or planner errors.
    pub fn explain(&self, sql: &str) -> Result<String, EngineError> {
        validate_sql(sql)?;
        let stmt = noedb_parser::parse(sql)?;
        let role = self.session_role_for(DEFAULT_SESSION);
        let stmt = apply_rls(stmt, &self.rls.lock(), &role);
        let tree = self.storage.read();
        explain_sql(&stmt, &tree).map_err(plan_err)
    }
}

fn empty_ok() -> QueryResult {
    QueryResult {
        columns: vec![],
        rows: vec![],
    }
}

/// Three-node in-process cluster: writes via Raft, concurrent reads on leader LSM.
pub struct DistributedEngine {
    cluster: Mutex<Cluster>,
    stores: HashMap<NodeId, Arc<RwLock<LsmTree>>>,
    voter_ids: Vec<NodeId>,
    applied_watermark: Mutex<HashMap<NodeId, usize>>,
    cached_leader: Mutex<Option<NodeId>>,
    region: RegionId,
    shards: ShardRouter,
}

impl DistributedEngine {
    /// Spin up voters `1..=n` with ephemeral on-disk stores.
    ///
    /// # Errors
    ///
    /// Raft or storage initialization failures.
    pub fn new_voters(n: u64) -> Result<Arc<Self>, EngineError> {
        let cluster = Cluster::new_voters(n)?;
        let voter_ids = cluster.voter_ids();
        let shard_n = voter_ids.len().max(1);
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
            stores.insert(*id, Arc::new(RwLock::new(tree)));
            applied_watermark.insert(*id, 0);
        }
        Ok(Arc::new(Self {
            cluster: Mutex::new(cluster),
            stores,
            voter_ids,
            applied_watermark: Mutex::new(applied_watermark),
            cached_leader: Mutex::new(None),
            region: RegionId::LOCAL,
            shards: ShardRouter::new(shard_n),
        }))
    }

    /// Deployment region id.
    #[must_use]
    pub const fn region(&self) -> RegionId {
        self.region
    }

    /// Shard router for horizontal partitioning.
    #[must_use]
    pub const fn shards(&self) -> &ShardRouter {
        &self.shards
    }

    /// Add a Raft voter at runtime (joint conf change).
    ///
    /// # Errors
    ///
    /// Raft or cluster errors.
    pub fn add_voter(&self, id: u64) -> Result<(), EngineError> {
        self.cluster
            .lock()
            .add_voter(NodeId(id))
            .map_err(EngineError::Raft)
    }

    /// Current voter count in the cluster.
    #[must_use]
    pub fn voter_count(&self) -> usize {
        self.cluster.lock().voter_count()
    }

    /// Run simulation rounds (election / heartbeat).
    ///
    /// # Errors
    ///
    /// Raft simulation errors.
    pub fn tick(&self, rounds: usize) -> Result<(), EngineError> {
        self.cluster.lock().run_rounds(rounds)?;
        self.sync_applied()?;
        Ok(())
    }

    /// Current leader, if unique.
    #[must_use]
    pub fn leader(&self) -> Option<NodeId> {
        self.cluster.lock().leader()
    }

    /// Replicate a row cell through Raft.
    ///
    /// # Errors
    ///
    /// No leader or replication failure.
    pub fn put_row(
        &self,
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
        &self,
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
        let mut store = self
            .stores
            .get(&leader)
            .ok_or_else(|| EngineError::Raft(RaftError::internal("leader store missing")))?
            .write();
        apply_command(&mut store, &cmd)?;
        Ok(())
    }

    /// Parse and execute SQL on the cluster (`&self` — parallel reads on leader).
    ///
    /// # Errors
    ///
    /// Validation, parse, Raft, or execution errors.
    pub fn execute(&self, sql: &str) -> Result<QueryResult, EngineError> {
        validate_sql(sql)?;
        let stmt = noedb_parser::parse(sql)?;
        match &stmt {
            Statement::Select(_) => self.execute_select_linearizable(&stmt),
            Statement::CreateIndex(idx) => {
                if idx.columns.len() != 1 {
                    return Err(EngineError::Exec(ExecError::UnsupportedExpr));
                }
                let cmd = Command::CreateIndex {
                    table: idx.table.value.clone(),
                    column: idx.columns[0].value.clone(),
                };
                self.replicate(&cmd)?;
                Ok(empty_ok())
            }
            _ => Err(EngineError::UnsupportedStatement),
        }
    }

    /// `EXPLAIN` on the leader store.
    ///
    /// # Errors
    ///
    /// Validation, parse, or planner errors.
    pub fn explain(&self, sql: &str) -> Result<String, EngineError> {
        validate_sql(sql)?;
        let stmt = noedb_parser::parse(sql)?;
        let leader = self.ensure_leader()?;
        let store = self
            .stores
            .get(&leader)
            .ok_or_else(|| EngineError::Raft(RaftError::internal("leader store missing")))?;
        let tree = store.read();
        explain_sql(&stmt, &tree).map_err(plan_err)
    }

    fn execute_select_linearizable(&self, stmt: &Statement) -> Result<QueryResult, EngineError> {
        let leader = self.ensure_leader()?;
        self.cluster
            .lock()
            .linearizable_barrier()
            .map_err(EngineError::Raft)?;
        let store = self
            .stores
            .get(&leader)
            .ok_or_else(|| EngineError::Raft(RaftError::internal("leader store missing")))?;
        let tree = store.read();
        let records = execute_sql(&stmt, &tree).map_err(EngineError::Exec)?;
        Ok(QueryResult::from_records(&records))
    }

    fn ensure_leader(&self) -> Result<NodeId, EngineError> {
        if let Some(id) = *self.cached_leader.lock() {
            if self.cluster.lock().raft_role(id) == Role::Leader {
                return Ok(id);
            }
        }
        let mut cluster = self.cluster.lock();
        if let Some(id) = cluster.leader() {
            *self.cached_leader.lock() = Some(id);
            return Ok(id);
        }
        cluster.run_rounds(8)?;
        let id = cluster.leader().ok_or(EngineError::NoLeader)?;
        *self.cached_leader.lock() = Some(id);
        Ok(id)
    }

    fn replicate(&self, cmd: &Command) -> Result<(), EngineError> {
        let bytes = cmd.encode()?;
        self.cluster.lock().propose_on_leader(bytes)?;
        self.sync_applied()?;
        Ok(())
    }

    fn sync_applied(&self) -> Result<(), EngineError> {
        let cluster = self.cluster.lock();
        let mut wm = self.applied_watermark.lock();
        for &id in &self.voter_ids {
            let applied = cluster.applied_at(id);
            let start = wm.get(&id).copied().unwrap_or(0);
            let Some(store) = self.stores.get(&id) else {
                continue;
            };
            let mut tree = store.write();
            for bytes in &applied[start..] {
                let cmd = Command::decode(bytes)?;
                apply_command(&mut tree, &cmd)?;
            }
            wm.insert(id, applied.len());
        }
        Ok(())
    }
}

/// Convenience: seed without going through SQL parser.
const fn plan_err(e: PlanError) -> EngineError {
    EngineError::Exec(ExecError::Plan(e))
}
