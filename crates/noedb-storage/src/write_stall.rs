//! LSM write-stall controller — RocksDB-style slowdown before hard stop.

use std::thread;
use std::time::Duration;

use crate::error::StorageError;

/// Thresholds for write throttling when flush/compaction falls behind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteStallConfig {
    /// Start adding latency when L0 file count reaches this fraction of stop trigger.
    pub l0_slowdown_trigger: usize,
    /// Reject or heavily delay writes when L0 reaches this count.
    pub l0_stop_trigger: usize,
    /// Delay per L0 file above slowdown trigger (milliseconds).
    pub slowdown_ms_per_file: u64,
    /// Max slowdown sleep per write (milliseconds).
    pub max_slowdown_ms: u64,
}

impl Default for WriteStallConfig {
    fn default() -> Self {
        Self {
            l0_slowdown_trigger: 3,
            l0_stop_trigger: 8,
            slowdown_ms_per_file: 8,
            max_slowdown_ms: 250,
        }
    }
}

impl WriteStallConfig {
    /// High-throughput profile: tolerate more L0 files before throttling.
    #[must_use]
    pub const fn relaxed() -> Self {
        Self {
            l0_slowdown_trigger: 12,
            l0_stop_trigger: 32,
            slowdown_ms_per_file: 4,
            max_slowdown_ms: 500,
        }
    }
}

/// Applies progressive write delays when the LSM tree is under pressure.
#[derive(Debug, Clone, Copy, Default)]
pub struct WriteStallController {
    config: WriteStallConfig,
}

impl WriteStallController {
    /// Build with explicit thresholds.
    #[must_use]
    pub const fn new(config: WriteStallConfig) -> Self {
        Self { config }
    }

    /// Sleep (slowdown) or return [`StorageError::ResourceExhausted`] (stop) before a write.
    pub fn gate_write(
        &self,
        l0_count: usize,
        mem_bytes: usize,
        max_mem_bytes: usize,
    ) -> Result<(), StorageError> {
        if l0_count >= self.config.l0_stop_trigger {
            return Err(StorageError::resource_exhausted(
                "L0 SST backlog at stop threshold — compaction required",
            ));
        }

        if l0_count >= self.config.l0_slowdown_trigger {
            let excess = l0_count - self.config.l0_slowdown_trigger + 1;
            let delay = (excess as u64)
                .saturating_mul(self.config.slowdown_ms_per_file)
                .min(self.config.max_slowdown_ms);
            thread::sleep(Duration::from_millis(delay));
        }

        if max_mem_bytes > 0 && mem_bytes >= max_mem_bytes * 9 / 10 {
            thread::sleep(Duration::from_millis(5));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_trigger_returns_resource_exhausted() {
        let ctrl = WriteStallController::new(WriteStallConfig {
            l0_slowdown_trigger: 2,
            l0_stop_trigger: 4,
            ..Default::default()
        });
        assert!(ctrl.gate_write(4, 0, 1024).is_err());
    }
}
