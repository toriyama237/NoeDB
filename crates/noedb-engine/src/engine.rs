//! Local and distributed SQL engines.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use dashmap::DashMap;
use noedb_ast::Statement;
use noedb_metrics::Metrics;
use noedb_planner::{
    apply_statement, execute_sql, explain_sql, explain_sql_with_schema, ExecError, PlanError,
    Record, Value,
};
use noedb_raft::{Cluster, NodeId, RaftError, Role};
use noedb_storage::{LsmConfig, LsmTree, Version};
use noedb_txn::TxnManager;
use parking_lot::{Mutex, RwLock};
use tracing::info_span;

use crate::audit::AuditLog;
use crate::command::Command;
use crate::error::EngineError;
use crate::machine::{apply_command, row_key};
use crate::memory::MemoryBudget;
use crate::prepared::{bind_parameters, PrepareCache};
use crate::query_cache::QueryCache;
use crate::ratelimit::{Deadline, RateLimiter};
use crate::region::RegionId;
use crate::rls::{apply_rls, RlsCatalog};
use crate::schema::SchemaCatalog;
use crate::session::SessionContext;
use crate::shard::ShardRouter;
use crate::txn::{commit_to_storage, execute_select_in_txn, put_row_in_txn, txn_err};
use crate::vector_index::VectorIndexCatalog;

/// Default session id for single-client CLI / tests.
pub const DEFAULT_SESSION: u64 = 1;

/// Maximum SQL text accepted by the engine (DoS bound).
pub const MAX_SQL_BYTES: usize = 64 * 1024;

/// Tabular query result for clients and wire protocol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryResult {
    /// Column names in order.
    pub columns: Vec<String>,
    /// Rows as UTF-8 strings (`NULL` shown as `[NULL]`).
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
        Value::Null => "[NULL]".to_string(),
        Value::Integer(n) => n.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Bytes(b) | Value::Date(b) => {
            if b.len() == 1 && (b[0] == 0 || b[0] == 1) {
                return (b[0] != 0).to_string();
            }
            if let Some(v) = crate::dml::vector_from_bytes(b) {
                format!(
                    "[{}]",
                    v.iter()
                        .map(|f| f.to_string())
                        .collect::<Vec<_>>()
                        .join(",")
                )
            } else {
                String::from_utf8_lossy(b).into_owned()
            }
        }
        Value::Timestamp(ts) => ts.to_string(),
        Value::Vector(v) => format!(
            "[{}]",
            v.iter()
                .map(|f| f.to_string())
                .collect::<Vec<_>>()
                .join(",")
        ),
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
    crate::security::reject_multi_statement(sql)?;
    Ok(())
}

/// Single-node engine (no Raft) — concurrent via interior mutability.
pub struct LocalEngine {
    data_dir: PathBuf,
    storage: Arc<RwLock<LsmTree>>,
    sessions: DashMap<u64, SessionContext>,
    prepare: Mutex<PrepareCache>,
    rls: Mutex<RlsCatalog>,
    audit: AuditLog,
    txn: Arc<TxnManager>,
    cache: Mutex<QueryCache>,
    schema: Mutex<SchemaCatalog>,
    vector_indexes: Mutex<VectorIndexCatalog>,
    metrics: Arc<Metrics>,
    memory: Arc<MemoryBudget>,
    rate_limiter: RateLimiter,
    stmt_timeout_ms: u64,
    data_key: Option<crate::crypto::DataKey>,
    stmt_cache: Mutex<crate::stmt_cache::StatementCache>,
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
        let schema = SchemaCatalog::load(&data_dir).map_err(EngineError::from)?;
        let sessions = DashMap::new();
        sessions.insert(DEFAULT_SESSION, SessionContext::dev());
        Ok(Arc::new(Self {
            data_dir,
            storage: Arc::new(RwLock::new(tree)),
            sessions,
            prepare: Mutex::new(PrepareCache::default()),
            rls: Mutex::new(RlsCatalog::default()),
            audit,
            txn: Arc::new(TxnManager::new()),
            cache: Mutex::new(QueryCache::default()),
            schema: Mutex::new(schema),
            vector_indexes: Mutex::new(VectorIndexCatalog::default()),
            metrics: Metrics::new_shared(),
            memory: Arc::new(MemoryBudget::from_env()),
            rate_limiter: RateLimiter::from_env(),
            stmt_timeout_ms: std::env::var("NOEDB_STMT_TIMEOUT_MS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            data_key: crate::crypto::DataKey::from_env(),
            stmt_cache: Mutex::new(crate::stmt_cache::StatementCache::default()),
        }))
    }

    /// Memory budget guard (OOM prevention).
    #[must_use]
    pub fn memory_budget(&self) -> &Arc<MemoryBudget> {
        &self.memory
    }

    /// Prometheus-style metrics for this engine.
    #[must_use]
    pub fn metrics(&self) -> Arc<Metrics> {
        Arc::clone(&self.metrics)
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

    /// Search the in-memory HNSW index for one `VECTOR` column.
    #[must_use]
    pub fn vector_search(
        &self,
        table: &str,
        column: &str,
        query: &[f32],
        k: usize,
    ) -> Vec<(String, f32)> {
        self.vector_indexes.lock().search(table, column, query, k)
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
        let key = row_key(table, row, column);
        let ts = self.txn.oracle().next();
        self.storage
            .write()
            .put_version(&key, &Version::put(ts, value.to_vec()))
            .map_err(EngineError::Storage)?;
        self.cache.lock().invalidate_table(table);
        Ok(())
    }

    /// Whether at-rest encryption is configured (`NOEDB_DATA_KEY`).
    #[must_use]
    pub fn encryption_enabled(&self) -> bool {
        self.data_key.is_some()
    }

    /// Insert a cell whose value is sealed at rest (AEAD).
    ///
    /// The plaintext is encrypted with the engine data key and bound to the
    /// `table/row/column` coordinates, then stored. Requires `NOEDB_DATA_KEY`.
    ///
    /// # Errors
    ///
    /// [`EngineError::Crypto`] when no data key is configured, or storage errors.
    pub fn put_row_encrypted(
        &self,
        session_id: u64,
        table: &str,
        row: &str,
        column: &str,
        plaintext: &[u8],
    ) -> Result<(), EngineError> {
        let key = self
            .data_key
            .as_ref()
            .ok_or(EngineError::Crypto("no data key configured"))?;
        let aad = crate::crypto::cell_aad(table, row, column);
        let sealed = key.seal(&aad, plaintext)?;
        self.put_row(session_id, table, row, column, &sealed)
    }

    /// Read and decrypt a cell previously written with [`Self::put_row_encrypted`].
    ///
    /// # Errors
    ///
    /// [`EngineError::Crypto`] when no key is configured or decryption fails.
    pub fn get_row_decrypted(
        &self,
        table: &str,
        row: &str,
        column: &str,
    ) -> Result<Option<Vec<u8>>, EngineError> {
        let key = self
            .data_key
            .as_ref()
            .ok_or(EngineError::Crypto("no data key configured"))?;
        let storage_key = row_key(table, row, column);
        let sealed = {
            let tree = self.storage.read();
            tree.get(&storage_key).map_err(EngineError::Storage)?
        };
        match sealed {
            Some(bytes) => {
                let aad = crate::crypto::cell_aad(table, row, column);
                Ok(Some(key.open(&aad, &bytes)?))
            }
            None => Ok(None),
        }
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

    /// Insert a full row as one packed record (v2.3 layout).
    ///
    /// One storage key, one WAL append, and one MVCC version per row —
    /// instead of one per column. This is the fast path used by SQL
    /// `INSERT` and the recommended API for bulk ingest.
    ///
    /// # Errors
    ///
    /// Storage write failure.
    pub fn put_packed_row(
        &self,
        table: &str,
        row: &str,
        columns: &[(String, Vec<u8>)],
    ) -> Result<(), EngineError> {
        let key = noedb_storage::packed_row_key(table, row);
        let record = noedb_storage::encode_row(columns);
        let ts = self.txn.oracle().next();
        self.storage
            .write()
            .put_version(&key, &Version::put(ts, record))
            .map_err(EngineError::Storage)?;
        self.cache.lock().invalidate_table(table);
        Ok(())
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
        let _span = info_span!("noedb.sql.execute", session_id, len = sql.len()).entered();
        let start = Instant::now();
        let deadline = Deadline::new(self.stmt_timeout_ms);
        let result = (|| {
            validate_sql(sql)?;
            self.rate_limiter.acquire(session_id)?;
            deadline.check()?;
            let pressure = self.storage.read().memtable_pressure_bytes();
            self.memory
                .gate_write(pressure, sql.len().saturating_mul(4096))?;
            let _mem = self.memory.try_reserve(sql.len().max(4096))?;
            let stmt = self.parse_cached(sql)?;
            deadline.check()?;
            let out = self.dispatch(session_id, stmt, sql);
            deadline.check()?;
            out
        })();
        self.metrics.record_query(start.elapsed(), result.is_ok());
        result
    }

    fn persist_schema(&self) -> Result<(), EngineError> {
        self.schema
            .lock()
            .save(&self.data_dir)
            .map_err(EngineError::from)
    }

    /// Parse `sql`, reusing the AST cache for repeated statements.
    fn parse_cached(&self, sql: &str) -> Result<Statement, EngineError> {
        if let Some(stmt) = self.stmt_cache.lock().get(sql) {
            return Ok(stmt);
        }
        let stmt = noedb_parser::parse(sql)?;
        self.stmt_cache.lock().insert(sql, stmt.clone());
        Ok(stmt)
    }

    /// `(hits, misses)` of the parsed-statement cache.
    #[must_use]
    pub fn statement_cache_stats(&self) -> (u64, u64) {
        self.stmt_cache.lock().stats()
    }

    fn dispatch(
        &self,
        session_id: u64,
        stmt: Statement,
        sql: &str,
    ) -> Result<QueryResult, EngineError> {
        let role = self.session_role_for(session_id);
        crate::security::authorize_statement(&role, &stmt)?;
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
                crate::security::authorize_role_change(&role, &s.role)?;
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
            Statement::AlterTable(a) => {
                let noedb_ast::AlterTableAction::AddColumn(col) = &a.action;
                let ts = self.txn.oracle().next();
                let meta = crate::schema::ColumnMeta {
                    name: col.name.value.clone(),
                    data_type: col.data_type.clone(),
                    not_null: col.not_null || col.primary_key,
                    primary_key: col.primary_key,
                    unique: col.unique,
                };
                self.schema.lock().add_column(&a.table.value, meta, ts)?;
                self.cache.lock().invalidate_table(&a.table.value);
                self.persist_schema()?;
                self.audit_record(session_id, sql, 0)?;
                Ok(empty_ok())
            }
            Statement::CreatePolicy(p) => {
                self.rls.lock().add_policy(&p);
                self.audit_record(session_id, sql, 0)?;
                Ok(empty_ok())
            }
            Statement::CreateTable(t) => {
                if t.if_not_exists && self.schema.lock().contains(&t.name.value) {
                    self.audit_record(session_id, sql, 0)?;
                    return Ok(empty_ok());
                }
                let ts = self.txn.oracle().next();
                let pk_from_table: Vec<String> =
                    t.primary_key.iter().map(|c| c.value.clone()).collect();
                let cols: Vec<crate::schema::ColumnMeta> = t
                    .columns
                    .iter()
                    .map(|c| crate::schema::ColumnMeta {
                        name: c.name.value.clone(),
                        data_type: c.data_type.clone(),
                        not_null: c.not_null || c.primary_key,
                        primary_key: c.primary_key,
                        unique: c.unique,
                    })
                    .collect();
                let pk = if pk_from_table.is_empty() {
                    cols.iter()
                        .filter(|c| c.primary_key)
                        .map(|c| c.name.clone())
                        .collect()
                } else {
                    pk_from_table
                };
                self.schema
                    .lock()
                    .create_table(&t.name.value, ts, cols.clone(), pk)?;
                self.vector_indexes
                    .lock()
                    .register_table_schema(&t.name.value, &cols);
                self.cache.lock().invalidate_table("");
                self.persist_schema()?;
                self.audit_record(session_id, sql, 0)?;
                Ok(empty_ok())
            }
            Statement::DropTable(t) => {
                self.schema.lock().drop_table(&t.name.value)?;
                self.vector_indexes.lock().drop_table(&t.name.value);
                self.cache.lock().invalidate_table(&t.name.value);
                self.persist_schema()?;
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
            Statement::AnalyzeTable(a) => {
                let mut tree = self.storage.write();
                let table_stats = noedb_planner::analyze_table(&*tree, &a.table.value);
                noedb_planner::persist_table_stats(&mut tree, &a.table.value, &table_stats)?;
                self.cache.lock().invalidate_table(&a.table.value);
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
        let in_txn = self.txn.in_txn(session_id);
        let txn = in_txn.then_some((self.txn.as_ref(), session_id));
        let result = if in_txn && matches!(stmt, Statement::Select(_)) {
            let qschema = self.schema.lock().query_schema();
            execute_select_in_txn(self.txn.clone(), session_id, &stmt, &self.storage, &qschema)?
        } else if matches!(stmt, Statement::Select(_)) {
            if let Some(hit) = crate::knn::try_hnsw_select(self, &stmt)? {
                hit
            } else {
                let role = self.session_role_for(session_id);
                let cached = self.cache.lock().get(&role, sql);
                if let Some(hit) = cached {
                    self.metrics.record_cache(true);
                    hit
                } else {
                    self.metrics.record_cache(false);
                    let tree = self.storage.read();
                    let qschema = self.schema.lock().query_schema();
                    let records = noedb_planner::execute_sql_with_schema(
                        &stmt,
                        &*tree,
                        &tree,
                        Some(&qschema),
                    )
                    .map_err(EngineError::Exec)?;
                    let qr = QueryResult::from_records(&records);
                    self.cache.lock().put(&role, sql, qr.clone());
                    qr
                }
            }
        } else {
            match &stmt {
                Statement::Insert(i) => {
                    self.cache.lock().invalidate_table(&i.table.value);
                    let schema = self.schema.lock();
                    let mut tree = self.storage.write();
                    let mut vectors = self.vector_indexes.lock();
                    crate::dml::execute_insert(
                        i,
                        &schema,
                        &mut tree,
                        self.txn.oracle(),
                        Some(&mut vectors),
                        txn,
                    )?;
                    QueryResult::from_records(&[])
                }
                Statement::Update(u) => {
                    self.cache.lock().invalidate_table(&u.table.value);
                    let schema = self.schema.lock();
                    let mut tree = self.storage.write();
                    let mut vectors = self.vector_indexes.lock();
                    crate::dml::execute_update(
                        u,
                        &schema,
                        &mut tree,
                        self.txn.oracle(),
                        Some(&mut vectors),
                        txn,
                    )?;
                    QueryResult::from_records(&[])
                }
                Statement::Delete(d) => {
                    self.cache.lock().invalidate_table(&d.table.value);
                    let schema = self.schema.lock();
                    let mut tree = self.storage.write();
                    let mut vectors = self.vector_indexes.lock();
                    crate::dml::execute_delete(
                        d,
                        &schema,
                        &mut tree,
                        self.txn.oracle(),
                        Some(&mut vectors),
                        txn,
                    )?;
                    QueryResult::from_records(&[])
                }
                other => {
                    let records = apply_statement(other, &mut self.storage.write())?;
                    QueryResult::from_records(&records)
                }
            }
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
        let safe_sql = crate::redact::redact_sql(sql);
        self.audit.record(&tenant, &role, &safe_sql, rows)
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
        let qschema = self.schema.lock().query_schema();
        explain_sql_with_schema(&stmt, &tree, Some(&qschema)).map_err(plan_err)
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
    metrics: Arc<Metrics>,
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
        // Timestamps alone are not unique enough: two clusters created in
        // the same instant (parallel tests, coarse macOS clock) would share
        // node directories and corrupt each other's WAL. Disambiguate with
        // the process id and a process-wide counter.
        static CLUSTER_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let seq = CLUSTER_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        for id in &voter_ids {
            let dir = std::env::temp_dir().join(format!(
                "noedb-engine-{}-{}-{seq}-{}",
                std::process::id(),
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
            metrics: Metrics::new_shared(),
        }))
    }

    /// Prometheus-style metrics for this cluster.
    #[must_use]
    pub fn metrics(&self) -> Arc<Metrics> {
        Arc::clone(&self.metrics)
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
        let _span = info_span!("noedb.sql.execute", len = sql.len(), distributed = true).entered();
        let start = Instant::now();
        let result = (|| {
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
        })();
        self.metrics.record_query(start.elapsed(), result.is_ok());
        result
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
        let id = cluster.leader().ok_or_else(|| {
            self.metrics.record_no_leader();
            EngineError::NoLeader
        })?;
        *self.cached_leader.lock() = Some(id);
        self.metrics.set_raft_leader(id.0);
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
