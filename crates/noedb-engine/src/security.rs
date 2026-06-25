//! SQL authorization — role gates for DDL and privilege escalation.

use noedb_ast::Statement;

use crate::error::EngineError;

/// Roles allowed to run DDL and security policy changes.
const ADMIN_ROLES: &[&str] = &["admin", "root", "noedb_admin"];

/// Whether `role` has administrative privileges.
#[must_use]
pub fn is_admin_role(role: &str) -> bool {
    ADMIN_ROLES.iter().any(|r| role.eq_ignore_ascii_case(r))
}

/// Reject multi-statement batches (classic SQL injection carrier).
pub fn reject_multi_statement(sql: &str) -> Result<(), EngineError> {
    let body = sql.trim().trim_end_matches(';').trim();
    if body.contains(';') {
        return Err(EngineError::InvalidSql(
            "multiple statements per request are forbidden",
        ));
    }
    Ok(())
}

/// Authorize `stmt` for `role` before execution.
pub fn authorize_statement(role: &str, stmt: &Statement) -> Result<(), EngineError> {
    if is_admin_role(role) {
        return Ok(());
    }
    if requires_admin(stmt) {
        return Err(EngineError::AccessDenied(
            "DDL and security statements require admin role",
        ));
    }
    Ok(())
}

/// Block non-admin sessions from assuming an admin role.
pub fn authorize_role_change(current: &str, new_role: &str) -> Result<(), EngineError> {
    if is_admin_role(new_role) && !is_admin_role(current) {
        return Err(EngineError::AccessDenied(
            "SET ROLE to admin requires admin privileges",
        ));
    }
    Ok(())
}

fn requires_admin(stmt: &Statement) -> bool {
    matches!(
        stmt,
        Statement::CreateTable(_)
            | Statement::DropTable(_)
            | Statement::CreateIndex(_)
            | Statement::EnableRls(_)
            | Statement::CreatePolicy(_)
            | Statement::AnalyzeTable(_)
            | Statement::AlterTable(_)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use noedb_parser::parse;

    #[test]
    fn rejects_multi_statement_injection() {
        assert!(reject_multi_statement("SELECT 1; DROP TABLE t").is_err());
    }

    #[test]
    fn ddl_blocked_for_anonymous() {
        let stmt = parse("CREATE TABLE t (id INT PRIMARY KEY)").unwrap();
        assert!(authorize_statement("anonymous", &stmt).is_err());
    }

    #[test]
    fn admin_role_escalation_blocked() {
        assert!(authorize_role_change("alice", "admin").is_err());
    }
}
