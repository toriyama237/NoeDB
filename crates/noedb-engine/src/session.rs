//! Session identity (role / tenant) for RLS and audit (Phase 1 Week 5–6).

/// Active SQL session context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionContext {
    /// Tenant id (multi-tenant isolation).
    pub tenant: String,
    /// Current role / user for RLS (`SET ROLE`).
    pub role: String,
}

impl SessionContext {
    /// Default dev session (local engine — trusted operator).
    #[must_use]
    pub fn dev() -> Self {
        Self {
            tenant: "default".into(),
            role: "noedb_admin".into(),
        }
    }
}
