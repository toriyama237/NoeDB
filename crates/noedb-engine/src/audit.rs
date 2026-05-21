//! Append-only audit log (Phase 1 Week 6).

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::EngineError;

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
}

/// Append-only audit log under `data_dir/audit/`.
#[derive(Debug)]
pub struct AuditLog {
    path: PathBuf,
}

impl AuditLog {
    /// Open or create audit log at `data_dir/audit/audit.log`.
    pub fn open(data_dir: &Path) -> Result<Self, EngineError> {
        let dir = data_dir.join("audit");
        std::fs::create_dir_all(&dir).map_err(io_err)?;
        Ok(Self {
            path: dir.join("audit.log"),
        })
    }

    /// Append one entry (line-delimited JSON).
    pub fn append(&self, entry: &AuditEntry) -> Result<(), EngineError> {
        let line = serde_json::to_string(entry).map_err(|e| EngineError::Codec(e.to_string()))?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(io_err)?;
        writeln!(file, "{line}").map_err(io_err)?;
        file.sync_data().map_err(io_err)?;
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
        self.append(&AuditEntry {
            ts_unix_ms,
            tenant: tenant.to_string(),
            user: user.to_string(),
            query: query.to_string(),
            rows_affected,
        })
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

fn io_err(e: std::io::Error) -> EngineError {
    EngineError::Codec(format!("audit io: {e}"))
}
