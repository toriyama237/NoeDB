//! Append-only audit log with hash chain integrity (Phase 1 Week 6).

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::EngineError;

/// Genesis hash when the log is empty.
const GENESIS: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// One audited SQL execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuditEntry {
    /// Unix timestamp (milliseconds).
    pub ts_unix_ms: u64,
    /// Tenant id.
    pub tenant: String,
    /// Session role / user.
    pub user: String,
    /// SQL text (bounded by engine).
    pub query: String,
    /// Rows returned or affected.
    pub rows_affected: u64,
    /// SHA-256 chain link (hex).
    pub chain_hash: String,
}

/// Append-only audit log under `data_dir/audit/`.
#[derive(Debug)]
pub struct AuditLog {
    path: PathBuf,
    last_hash: Mutex<String>,
}

impl AuditLog {
    /// Open or create audit log at `data_dir/audit/audit.log`.
    pub fn open(data_dir: &Path) -> Result<Self, EngineError> {
        let dir = data_dir.join("audit");
        std::fs::create_dir_all(&dir).map_err(io_err)?;
        let path = dir.join("audit.log");
        let last_hash = if path.exists() {
            load_last_hash(&path)?
        } else {
            GENESIS.to_string()
        };
        Ok(Self {
            path,
            last_hash: Mutex::new(last_hash),
        })
    }

    /// Append one entry (line-delimited JSON + hash chain).
    pub fn append(&self, entry: &AuditEntry) -> Result<(), EngineError> {
        let line = serde_json::to_string(entry).map_err(|e| EngineError::Codec(e.to_string()))?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(io_err)?;
        writeln!(file, "{line}").map_err(io_err)?;
        file.sync_data().map_err(io_err)?;
        self.last_hash
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone_from(&entry.chain_hash);
        Ok(())
    }

    /// Record a query execution.
    pub fn record(
        &self,
        tenant: &str,
        user: &str,
        query: &str,
        rows_affected: u64,
    ) -> Result<(), EngineError> {
        let ts_unix_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);
        let prev = self
            .last_hash
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let chain_hash = chain_link(&prev, ts_unix_ms, tenant, user, query, rows_affected);
        self.append(&AuditEntry {
            ts_unix_ms,
            tenant: tenant.to_string(),
            user: user.to_string(),
            query: query.to_string(),
            rows_affected,
            chain_hash,
        })
    }

    /// Verify hash chain integrity over the whole log.
    pub fn verify_chain(&self) -> Result<(), EngineError> {
        let content = std::fs::read_to_string(&self.path).map_err(io_err)?;
        let mut prev = GENESIS.to_string();
        for line in content.lines().filter(|l| !l.is_empty()) {
            let entry: AuditEntry =
                serde_json::from_str(line).map_err(|e| EngineError::Codec(e.to_string()))?;
            let expected = chain_link(
                &prev,
                entry.ts_unix_ms,
                &entry.tenant,
                &entry.user,
                &entry.query,
                entry.rows_affected,
            );
            if entry.chain_hash != expected {
                return Err(EngineError::Codec("audit chain tampered".into()));
            }
            prev.clone_from(&entry.chain_hash);
        }
        Ok(())
    }

    /// Read all entries (tests).
    pub fn read_all(&self) -> Result<Vec<AuditEntry>, EngineError> {
        let content = std::fs::read_to_string(&self.path).map_err(io_err)?;
        content
            .lines()
            .filter(|l| !l.is_empty())
            .map(|line| serde_json::from_str(line).map_err(|e| EngineError::Codec(e.to_string())))
            .collect()
    }
}

fn chain_link(
    prev: &str,
    ts_unix_ms: u64,
    tenant: &str,
    user: &str,
    query: &str,
    rows_affected: u64,
) -> String {
    let mut h = Sha256::new();
    h.update(prev.as_bytes());
    h.update(ts_unix_ms.to_le_bytes());
    h.update(tenant.as_bytes());
    h.update(user.as_bytes());
    h.update(query.as_bytes());
    h.update(rows_affected.to_le_bytes());
    hex::encode(h.finalize())
}

fn load_last_hash(path: &Path) -> Result<String, EngineError> {
    let content = std::fs::read_to_string(path).map_err(io_err)?;
    let Some(last) = content.lines().rfind(|l| !l.is_empty()) else {
        return Ok(GENESIS.to_string());
    };
    let entry: AuditEntry =
        serde_json::from_str(last).map_err(|e| EngineError::Codec(e.to_string()))?;
    Ok(entry.chain_hash)
}

fn io_err(e: std::io::Error) -> EngineError {
    EngineError::Codec(format!("audit io: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_chain_survives_append() {
        let dir = std::env::temp_dir().join(format!(
            "noedb-audit-chain-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let log = AuditLog::open(&dir).unwrap();
        log.record("t", "alice", "SELECT 1", 1).unwrap();
        log.record("t", "alice", "SELECT 2", 1).unwrap();
        log.verify_chain().unwrap();
        let _ = std::fs::remove_dir_all(dir);
    }
}
