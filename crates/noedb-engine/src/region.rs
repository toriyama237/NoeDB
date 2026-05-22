//! Multi-region metadata (Phase 4 Weeks 33–36).

/// Deployment region for latency-aware routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RegionId(pub u32);

impl RegionId {
    /// Default single-region deployment.
    pub const LOCAL: Self = Self(0);
}
