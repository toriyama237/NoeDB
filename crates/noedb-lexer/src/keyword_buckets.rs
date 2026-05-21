//! Length-bucketed keyword tables for O(1) length dispatch.

use crate::token::Keyword;

static KW_LEN_02: &[(&str, Keyword)] = &[
    ("AS", Keyword::As),
    ("BY", Keyword::By),
    ("IN", Keyword::In),
    ("IS", Keyword::Is),
    ("OF", Keyword::Of),
    ("ON", Keyword::On),
    ("OR", Keyword::Or),
    ("TO", Keyword::To),
];

static KW_LEN_03: &[(&str, Keyword)] = &[
    ("ALL", Keyword::All),
    ("AND", Keyword::And),
    ("ANY", Keyword::Any),
    ("ASC", Keyword::Asc),
    ("END", Keyword::End),
    ("FOR", Keyword::For),
    ("KEY", Keyword::Key),
    ("NOT", Keyword::Not),
    ("SET", Keyword::Set),
];

static KW_LEN_04: &[(&str, Keyword)] = &[
    ("CASE", Keyword::Case),
    ("CAST", Keyword::Cast),
    ("DESC", Keyword::Desc),
    ("DROP", Keyword::Drop),
    ("ELSE", Keyword::Else),
    ("FROM", Keyword::From),
    ("FULL", Keyword::Full),
    ("INTO", Keyword::Into),
    ("JOIN", Keyword::Join),
    ("LEFT", Keyword::Left),
    ("LIKE", Keyword::Like),
    ("NULL", Keyword::Null),
    ("SOME", Keyword::Some),
    ("THEN", Keyword::Then),
    ("TRUE", Keyword::True),
    ("USER", Keyword::User),
    ("VIEW", Keyword::View),
    ("WHEN", Keyword::When),
    ("WITH", Keyword::With),
];

static KW_LEN_05: &[(&str, Keyword)] = &[
    ("CHECK", Keyword::Check),
    ("CROSS", Keyword::Cross),
    ("FALSE", Keyword::False),
    ("FETCH", Keyword::Fetch),
    ("GRANT", Keyword::Grant),
    ("GROUP", Keyword::Group),
    ("INDEX", Keyword::Index),
    ("INNER", Keyword::Inner),
    ("LIMIT", Keyword::Limit),
    ("ORDER", Keyword::Order),
    ("OUTER", Keyword::Outer),
    ("RIGHT", Keyword::Right),
    ("TABLE", Keyword::Table),
    ("UNION", Keyword::Union),
    ("USING", Keyword::Using),
    ("WHERE", Keyword::Where),
];

static KW_LEN_06: &[(&str, Keyword)] = &[
    ("COLUMN", Keyword::Column),
    ("CREATE", Keyword::Create),
    ("DELETE", Keyword::Delete),
    ("ESCAPE", Keyword::Escape),
    ("EXCEPT", Keyword::Except),
    ("EXISTS", Keyword::Exists),
    ("HAVING", Keyword::Having),
    ("INSERT", Keyword::Insert),
    ("NULLIF", Keyword::Nullif),
    ("OFFSET", Keyword::Offset),
    ("SELECT", Keyword::Select),
    ("UNIQUE", Keyword::Unique),
    ("UPDATE", Keyword::Update),
    ("VALUES", Keyword::Values),
];

static KW_LEN_07: &[(&str, Keyword)] = &[
    ("BETWEEN", Keyword::Between),
    ("COLLATE", Keyword::Collate),
    ("CURRENT", Keyword::Current),
    ("DEFAULT", Keyword::Default),
    ("FOREIGN", Keyword::Foreign),
    ("NATURAL", Keyword::Natural),
    ("PRIMARY", Keyword::Primary),
];

static KW_LEN_08: &[(&str, Keyword)] = &[
    ("DISTINCT", Keyword::Distinct),
    ("ROLLBACK", Keyword::Rollback),
];

static KW_LEN_09: &[(&str, Keyword)] = &[
    ("INTERSECT", Keyword::Intersect),
];

static KW_LEN_10: &[(&str, Keyword)] = &[
    ("CONSTRAINT", Keyword::Constraint),
    ("REFERENCES", Keyword::References),
];

static KW_LEN_12: &[(&str, Keyword)] = &[
    ("CURRENT_DATE", Keyword::CurrentDate),
    ("CURRENT_TIME", Keyword::CurrentTime),
    ("CURRENT_USER", Keyword::CurrentUser),
];

static KW_LEN_13: &[(&str, Keyword)] = &[
    ("AUTHORIZATION", Keyword::Authorization),
];

static KW_LEN_17: &[(&str, Keyword)] = &[
    ("CURRENT_TIMESTAMP", Keyword::CurrentTimestamp),
];


#[inline]
pub(crate) const fn table_for_len(len: usize) -> &'static [(&'static str, Keyword)] {
    match len {
        2 => KW_LEN_02,
        3 => KW_LEN_03,
        4 => KW_LEN_04,
        5 => KW_LEN_05,
        6 => KW_LEN_06,
        7 => KW_LEN_07,
        8 => KW_LEN_08,
        9 => KW_LEN_09,
        10 => KW_LEN_10,
        12 => KW_LEN_12,
        13 => KW_LEN_13,
        17 => KW_LEN_17,
        _ => &[],
    }
}
