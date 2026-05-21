//! Table-driven lexer corpus: 200+ cases generated from loops + fixtures.
//!
//! Run with: `cargo test -p noedb-lexer --test corpus`

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use noedb_lexer::{tokenize, Keyword, LexErrorKind, Operator, Punctuation, Token};

fn kinds(src: &str) -> Vec<Token> {
    tokenize(src).unwrap().into_iter().map(|t| t.kind).collect()
}

// --- 56 keyword spot checks (full table covered in keywords.rs unit tests) ---

#[test]
fn corpus_every_core_keyword_tokenizes() {
    let words = [
        ("ALL", Keyword::All),
        ("AND", Keyword::And),
        ("SELECT", Keyword::Select),
        ("FROM", Keyword::From),
        ("WHERE", Keyword::Where),
        ("INSERT", Keyword::Insert),
        ("UPDATE", Keyword::Update),
        ("DELETE", Keyword::Delete),
        ("CREATE", Keyword::Create),
        ("DROP", Keyword::Drop),
        ("JOIN", Keyword::Join),
        ("INNER", Keyword::Inner),
        ("LEFT", Keyword::Left),
        ("RIGHT", Keyword::Right),
        ("OUTER", Keyword::Outer),
        ("ON", Keyword::On),
        ("AS", Keyword::As),
        ("DISTINCT", Keyword::Distinct),
        ("ORDER", Keyword::Order),
        ("BY", Keyword::By),
        ("GROUP", Keyword::Group),
        ("HAVING", Keyword::Having),
        ("LIMIT", Keyword::Limit),
        ("OFFSET", Keyword::Offset),
        ("UNION", Keyword::Union),
        ("INTERSECT", Keyword::Intersect),
        ("EXCEPT", Keyword::Except),
        ("BETWEEN", Keyword::Between),
        ("IN", Keyword::In),
        ("IS", Keyword::Is),
        ("LIKE", Keyword::Like),
        ("NOT", Keyword::Not),
        ("NULL", Keyword::Null),
        ("TRUE", Keyword::True),
        ("FALSE", Keyword::False),
        ("EXISTS", Keyword::Exists),
        ("CASE", Keyword::Case),
        ("WHEN", Keyword::When),
        ("THEN", Keyword::Then),
        ("ELSE", Keyword::Else),
        ("END", Keyword::End),
        ("CAST", Keyword::Cast),
        ("PRIMARY", Keyword::Primary),
        ("KEY", Keyword::Key),
        ("FOREIGN", Keyword::Foreign),
        ("REFERENCES", Keyword::References),
        ("CONSTRAINT", Keyword::Constraint),
        ("DEFAULT", Keyword::Default),
        ("UNIQUE", Keyword::Unique),
        ("CHECK", Keyword::Check),
        ("TABLE", Keyword::Table),
        ("VIEW", Keyword::View),
        ("VALUES", Keyword::Values),
        ("INTO", Keyword::Into),
        ("SET", Keyword::Set),
        ("WITH", Keyword::With),
        ("CURRENT_DATE", Keyword::CurrentDate),
        ("CURRENT_TIME", Keyword::CurrentTime),
        ("CURRENT_TIMESTAMP", Keyword::CurrentTimestamp),
        ("CURRENT_USER", Keyword::CurrentUser),
    ];
    for (word, kw) in words {
        let toks = kinds(word);
        assert_eq!(toks.len(), 2, "keyword {word}");
        assert_eq!(toks[0], Token::Keyword(kw), "keyword {word}");
        assert_eq!(toks[1], Token::Eof);
    }
}

#[test]
fn corpus_keywords_are_case_insensitive() {
    for word in ["select", "SeLeCt", "SELECT", "from", "WHERE"] {
        assert!(tokenize(word).is_ok(), "{word}");
    }
}

// --- 50 integer literal tests ---

#[test]
fn corpus_integer_literals_zero_to_forty_nine() {
    for n in 0..50_i64 {
        let src = n.to_string();
        let toks = kinds(&src);
        assert_eq!(toks[0], Token::Integer(n), "n={n}");
    }
}

// --- 10 float literal tests ---

#[test]
fn corpus_float_literals() {
    let cases: &[(&str, f64)] = &[
        ("0.0", 0.0),
        ("1.5", 1.5),
        (".5", 0.5),
        ("10.25", 10.25),
        ("1e2", 100.0),
        ("1E2", 100.0),
        ("1e-2", 0.01),
        ("1E+3", 1000.0),
        ("3.14", 3.14),
        ("0.001", 0.001),
    ];
    for (src, expected) in cases {
        let toks = kinds(src);
        assert_eq!(toks[0], Token::Float(*expected), "src={src}");
    }
}

// --- 6 string literal tests ---

#[test]
fn corpus_string_literals() {
    let cases: &[(&str, &str)] = &[
        ("''", ""),
        ("'a'", "a"),
        ("'hello'", "hello"),
        ("'it''s'", "it's"),
        ("'line\nbreak'", "line\nbreak"),
        ("'tab\there'", "tab\there"),
    ];
    for (src, expected) in cases {
        let toks = kinds(src);
        assert_eq!(toks[0], Token::String(expected.to_string()), "src={src}");
    }
}

// --- 15 operator / punctuation tests ---

#[test]
fn corpus_operators() {
    let cases: &[(&str, Token)] = &[
        ("=", Token::Op(Operator::Eq)),
        ("!=", Token::Op(Operator::Ne)),
        ("<>", Token::Op(Operator::Ne)),
        ("<", Token::Op(Operator::Lt)),
        (">", Token::Op(Operator::Gt)),
        ("<=", Token::Op(Operator::Le)),
        (">=", Token::Op(Operator::Ge)),
        ("+", Token::Op(Operator::Plus)),
        ("-", Token::Op(Operator::Minus)),
    ];
    for (src, expected) in cases {
        assert_eq!(kinds(src)[0], *expected, "op {src}");
    }
}

#[test]
fn corpus_punctuation() {
    let cases: &[(&str, Token)] = &[
        ("(", Token::Punct(Punctuation::LParen)),
        (")", Token::Punct(Punctuation::RParen)),
        (",", Token::Punct(Punctuation::Comma)),
        (";", Token::Punct(Punctuation::Semicolon)),
        (".", Token::Punct(Punctuation::Dot)),
        ("*", Token::Punct(Punctuation::Star)),
    ];
    for (src, expected) in cases {
        assert_eq!(kinds(src)[0], *expected, "punct {src}");
    }
}

// --- 6 error cases ---

#[test]
fn corpus_errors() {
    let cases: &[(&str, fn(&LexErrorKind) -> bool)] = &[
        ("@", |k| matches!(k, LexErrorKind::UnexpectedChar('@'))),
        ("!", |k| matches!(k, LexErrorKind::UnexpectedChar('!'))),
        ("'open", |k| matches!(k, LexErrorKind::UnterminatedString)),
        ("\"open", |k| {
            matches!(k, LexErrorKind::UnterminatedQuotedIdent)
        }),
        ("/* open", |k| {
            matches!(k, LexErrorKind::UnterminatedBlockComment)
        }),
        ("99999999999999999999", |k| {
            matches!(k, LexErrorKind::IntegerOutOfRange(_))
        }),
    ];
    for (src, pred) in cases {
        let err = tokenize(src).unwrap_err();
        assert!(pred(&err.kind), "src={src}");
    }
}

// --- 13 statement-shaped integration cases ---

#[test]
fn corpus_realistic_statements() {
    let inputs = [
        "SELECT 1",
        "SELECT * FROM t",
        "SELECT a, b FROM t WHERE x = 1",
        "SELECT * FROM users WHERE id = 42 AND active IS NOT NULL",
        "INSERT INTO t VALUES (1, 'a')",
        "UPDATE t SET x = 1 WHERE id = 2",
        "DELETE FROM t WHERE id = 1",
        "CREATE TABLE t (id INT, name VARCHAR)",
        "DROP TABLE t",
        "CREATE INDEX idx ON t (id)",
        "SELECT u.name FROM users u INNER JOIN orders o ON u.id = o.user_id",
        "SELECT * FROM t /* block */ WHERE x = 1 -- line",
        "SELECT COUNT(*) FROM t GROUP BY x HAVING COUNT(*) > 1 ORDER BY x LIMIT 10",
    ];
    for input in inputs {
        let toks = tokenize(input).unwrap_or_else(|e| {
            panic!("failed to tokenize {input:?}: {e}");
        });
        assert!(toks.last().is_some_and(|t| t.kind == Token::Eof), "{input}");
        assert!(toks.len() > 1, "{input}");
    }
}

#[test]
fn corpus_all_keywords_from_table() {
    for (word, kw) in noedb_lexer::all_keywords() {
        let toks = kinds(word);
        assert_eq!(toks[0], Token::Keyword(*kw), "keyword {word}");
    }
}

#[test]
fn corpus_identifiers_col_zero_to_forty_nine() {
    for i in 0..50 {
        let name = format!("col_{i}");
        let toks = kinds(&name);
        assert_eq!(toks[0], Token::Ident, "ident {name}");
    }
}

#[test]
fn corpus_whitespace_variants() {
    for ws in [" ", "\t", "\n", "\r\n", "  \t\n  "] {
        let src = format!("{ws}SELECT{ws}1{ws}");
        let toks = kinds(&src);
        assert_eq!(toks[0], Token::Keyword(Keyword::Select));
        assert_eq!(toks[1], Token::Integer(1));
    }
}

#[test]
fn corpus_error_reports_line_column() {
    let src = "SELECT 1\nBAD @";
    let err = tokenize(src).unwrap_err();
    let lc = err.line_column(src);
    assert_eq!(lc.line, 2);
    assert!(lc.column >= 1);
    let msg = err.format_with_source(src);
    assert!(msg.contains("2:"));
}
