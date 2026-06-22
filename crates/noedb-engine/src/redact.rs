//! Secret redaction for audit logs and traces.
//!
//! The audit trail must be safe to ship to a SIEM, so credential-bearing SQL
//! (e.g. `IDENTIFIED BY '...'`, `PASSWORD '...'`) is scrubbed before it is
//! written. Redaction is intentionally conservative: it only masks the
//! literal that follows a known sensitive keyword.

/// Keywords whose following string/identifier literal is a secret.
const SENSITIVE_KEYWORDS: &[&str] = &[
    "password",
    "identified by",
    "secret",
    "token",
    "api_key",
    "apikey",
    "private_key",
];

/// Placeholder substituted for any redacted literal.
pub(crate) const MASK: &str = "'***'";

/// Return a copy of `sql` with secret literals masked.
///
/// Spans to mask are collected over the original (immutable) text in a single
/// scan, then applied left-to-right, so there is no re-scanning of already
/// masked output.
#[must_use]
pub fn redact_sql(sql: &str) -> String {
    let lower = sql.to_ascii_lowercase();
    let mut spans: Vec<(usize, usize)> = Vec::new();
    for kw in SENSITIVE_KEYWORDS {
        let mut search_from = 0;
        while let Some(rel) = lower[search_from..].find(kw) {
            let kw_end = search_from + rel + kw.len();
            if let Some((start, end)) = next_literal_span(sql, kw_end) {
                spans.push((start, end));
                search_from = end;
            } else {
                search_from = kw_end;
            }
        }
    }
    if spans.is_empty() {
        return sql.to_string();
    }
    spans.sort_unstable();
    spans.dedup();
    let mut out = String::with_capacity(sql.len());
    let mut cursor = 0;
    for (start, end) in spans {
        if start < cursor {
            continue; // overlapping span already covered
        }
        out.push_str(&sql[cursor..start]);
        out.push_str(MASK);
        cursor = end;
    }
    out.push_str(&sql[cursor..]);
    out
}

/// Find the byte span of the next quoted string literal after `from`.
fn next_literal_span(s: &str, from: usize) -> Option<(usize, usize)> {
    let bytes = s.as_bytes();
    let mut i = from;
    while i < bytes.len() && (bytes[i] as char).is_whitespace() {
        i += 1;
    }
    // Skip an optional `=` between keyword and value (e.g. password = '...').
    if i < bytes.len() && bytes[i] == b'=' {
        i += 1;
        while i < bytes.len() && (bytes[i] as char).is_whitespace() {
            i += 1;
        }
    }
    if i >= bytes.len() || bytes[i] != b'\'' {
        return None;
    }
    let start = i;
    i += 1;
    while i < bytes.len() {
        if bytes[i] == b'\'' {
            return Some((start, i + 1));
        }
        i += 1;
    }
    None
}

/// Whether `sql` mentions any sensitive keyword (cheap pre-check).
#[must_use]
pub fn is_sensitive(sql: &str) -> bool {
    let lower = sql.to_ascii_lowercase();
    SENSITIVE_KEYWORDS.iter().any(|kw| lower.contains(kw))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_password_literal() {
        let out = redact_sql("CREATE USER bob PASSWORD 'hunter2'");
        assert!(out.contains(MASK));
        assert!(!out.contains("hunter2"));
    }

    #[test]
    fn masks_identified_by() {
        let out = redact_sql("ALTER USER a IDENTIFIED BY 'topsecret'");
        assert!(!out.contains("topsecret"));
    }

    #[test]
    fn masks_with_equals_sign() {
        let out = redact_sql("SET api_key = 'abc123'");
        assert!(!out.contains("abc123"));
    }

    #[test]
    fn leaves_plain_select_untouched() {
        let sql = "SELECT id FROM users WHERE id = 1";
        assert_eq!(redact_sql(sql), sql);
    }

    #[test]
    fn masks_multiple_secrets() {
        let out = redact_sql("X token 'aaa' and secret 'bbb'");
        assert!(!out.contains("aaa"));
        assert!(!out.contains("bbb"));
    }
}
