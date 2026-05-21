//! SQL-92 reserved keyword lookup.
//!
//! Keywords are stored in a **sorted** static table and resolved with a
//! binary search. Comparison is ASCII case-insensitive and allocates
//! nothing — the hot path is a few pointer chases, not a `HashMap`.

use core::cmp::Ordering;

use crate::token::Keyword;

/// Sorted `(UPPERCASE_NAME, Keyword)` pairs. Must stay lexicographically
/// sorted by name — see [`KEYWORD_TABLE_IS_SORTED`].
static KEYWORD_TABLE: &[(&str, Keyword)] = &[
    ("ALL", Keyword::All),
    ("AND", Keyword::And),
    ("ANY", Keyword::Any),
    ("AS", Keyword::As),
    ("ASC", Keyword::Asc),
    ("AUTHORIZATION", Keyword::Authorization),
    ("BETWEEN", Keyword::Between),
    ("BY", Keyword::By),
    ("CASE", Keyword::Case),
    ("CAST", Keyword::Cast),
    ("CHECK", Keyword::Check),
    ("COLLATE", Keyword::Collate),
    ("COLUMN", Keyword::Column),
    ("CONSTRAINT", Keyword::Constraint),
    ("CREATE", Keyword::Create),
    ("CROSS", Keyword::Cross),
    ("CURRENT", Keyword::Current),
    ("CURRENT_DATE", Keyword::CurrentDate),
    ("CURRENT_TIME", Keyword::CurrentTime),
    ("CURRENT_TIMESTAMP", Keyword::CurrentTimestamp),
    ("CURRENT_USER", Keyword::CurrentUser),
    ("DEFAULT", Keyword::Default),
    ("DELETE", Keyword::Delete),
    ("DESC", Keyword::Desc),
    ("DISTINCT", Keyword::Distinct),
    ("DROP", Keyword::Drop),
    ("ELSE", Keyword::Else),
    ("END", Keyword::End),
    ("ESCAPE", Keyword::Escape),
    ("EXCEPT", Keyword::Except),
    ("EXISTS", Keyword::Exists),
    ("FALSE", Keyword::False),
    ("FETCH", Keyword::Fetch),
    ("FOR", Keyword::For),
    ("FOREIGN", Keyword::Foreign),
    ("FROM", Keyword::From),
    ("FULL", Keyword::Full),
    ("GRANT", Keyword::Grant),
    ("GROUP", Keyword::Group),
    ("HAVING", Keyword::Having),
    ("IN", Keyword::In),
    ("INDEX", Keyword::Index),
    ("INNER", Keyword::Inner),
    ("INSERT", Keyword::Insert),
    ("INTERSECT", Keyword::Intersect),
    ("INTO", Keyword::Into),
    ("IS", Keyword::Is),
    ("JOIN", Keyword::Join),
    ("KEY", Keyword::Key),
    ("LEFT", Keyword::Left),
    ("LIKE", Keyword::Like),
    ("LIMIT", Keyword::Limit),
    ("NATURAL", Keyword::Natural),
    ("NOT", Keyword::Not),
    ("NULL", Keyword::Null),
    ("NULLIF", Keyword::Nullif),
    ("OF", Keyword::Of),
    ("OFFSET", Keyword::Offset),
    ("ON", Keyword::On),
    ("OR", Keyword::Or),
    ("ORDER", Keyword::Order),
    ("OUTER", Keyword::Outer),
    ("PRIMARY", Keyword::Primary),
    ("REFERENCES", Keyword::References),
    ("RIGHT", Keyword::Right),
    ("ROLLBACK", Keyword::Rollback),
    ("SELECT", Keyword::Select),
    ("SET", Keyword::Set),
    ("SOME", Keyword::Some),
    ("TABLE", Keyword::Table),
    ("THEN", Keyword::Then),
    ("TO", Keyword::To),
    ("TRUE", Keyword::True),
    ("UNION", Keyword::Union),
    ("UNIQUE", Keyword::Unique),
    ("UPDATE", Keyword::Update),
    ("USER", Keyword::User),
    ("USING", Keyword::Using),
    ("VALUES", Keyword::Values),
    ("VIEW", Keyword::View),
    ("WHEN", Keyword::When),
    ("WHERE", Keyword::Where),
    ("WITH", Keyword::With),
];

/// Number of reserved keywords compiled into the lexer today.
pub const KEYWORD_COUNT: usize = KEYWORD_TABLE.len();

/// All reserved keywords and their token variants (for tests and tooling).
#[must_use]
pub fn all_keywords() -> &'static [(&'static str, Keyword)] {
    KEYWORD_TABLE
}

/// Look up a word (already lexed as an identifier body) as a keyword.
///
/// Returns `None` for ordinary identifiers such as `users` or `id`.
#[inline]
pub(crate) fn lookup(word: &str) -> Option<Keyword> {
    let mut lo = 0usize;
    let mut hi = KEYWORD_TABLE.len();
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        match cmp_ignore_ascii_case(word, KEYWORD_TABLE[mid].0) {
            Ordering::Less => hi = mid,
            Ordering::Equal => return Some(KEYWORD_TABLE[mid].1),
            Ordering::Greater => lo = mid + 1,
        }
    }
    None
}

#[inline]
fn cmp_ignore_ascii_case(a: &str, b: &str) -> Ordering {
    let mut ai = a.bytes();
    let mut bi = b.bytes();
    loop {
        match (ai.next(), bi.next()) {
            (Some(x), Some(y)) => {
                let x = x.to_ascii_uppercase();
                let y = y.to_ascii_uppercase();
                match x.cmp(&y) {
                    Ordering::Equal => {}
                    other => return other,
                }
            }
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn keyword_table_is_sorted() {
        for w in KEYWORD_TABLE.windows(2) {
            assert!(
                w[0].0 < w[1].0,
                "KEYWORD_TABLE out of order: {:?} >= {:?}",
                w[0].0,
                w[1].0
            );
        }
    }

    #[test]
    fn lookup_finds_select_case_insensitive() {
        assert_eq!(lookup("SELECT"), Some(Keyword::Select));
        assert_eq!(lookup("select"), Some(Keyword::Select));
        assert_eq!(lookup("SeLeCt"), Some(Keyword::Select));
    }

    #[test]
    fn lookup_returns_none_for_identifiers() {
        assert_eq!(lookup("users"), None);
        assert_eq!(lookup("id"), None);
        assert_eq!(lookup("my_table"), None);
    }

    #[test]
    fn lookup_multi_word_keywords() {
        assert_eq!(lookup("CURRENT_DATE"), Some(Keyword::CurrentDate));
        assert_eq!(lookup("current_timestamp"), Some(Keyword::CurrentTimestamp));
    }

    #[test]
    fn keyword_count_matches_table() {
        const { assert!(KEYWORD_COUNT > 50) };
    }
}
