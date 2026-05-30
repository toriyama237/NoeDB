//! SQL type names (`CREATE TABLE`, `CAST … AS …`).

use noedb_ast::SqlType;
use noedb_lexer::{Punctuation, Token};

use crate::error::ParseError;
use crate::parser::Parser;

/// Parse a SQL type name (`INT`, `VARCHAR(32)`, `TIMESTAMP`, …).
pub(crate) fn parse_sql_type(p: &mut Parser<'_>) -> Result<SqlType, ParseError> {
    let name = p.parse_ident()?;
    let upper = name.value.to_ascii_uppercase();
    match upper.as_str() {
        "INT" | "INTEGER" | "BIGINT" => Ok(SqlType::Int),
        "BOOLEAN" | "BOOL" => Ok(SqlType::Boolean),
        "FLOAT" | "REAL" | "DOUBLE" => Ok(SqlType::Float),
        "TEXT" => Ok(SqlType::Text),
        "DATE" => Ok(SqlType::Date),
        "TIMESTAMP" | "TIMESTAMPTZ" => Ok(SqlType::Timestamp),
        "VARCHAR" | "CHAR" => {
            if matches!(p.peek_kind(), Token::Punct(Punctuation::LParen)) {
                p.bump();
                let len = parse_type_length(p)?;
                p.expect_punct(Punctuation::RParen)?;
                Ok(SqlType::Varchar { max_len: Some(len) })
            } else {
                Ok(SqlType::Varchar { max_len: None })
            }
        }
        "VECTOR" => {
            p.expect_punct(Punctuation::LParen)?;
            let dim = parse_type_length(p)?;
            p.expect_punct(Punctuation::RParen)?;
            Ok(SqlType::Vector { dim })
        }
        other => Ok(SqlType::Named(other.to_string())),
    }
}

fn parse_type_length(p: &mut Parser<'_>) -> Result<u32, ParseError> {
    let tok = p.bump();
    match tok.kind {
        Token::Integer(v) if v >= 0 => u32::try_from(v).map_err(|_| ParseError::UnexpectedToken {
            span: tok.span,
            context: "type length",
        }),
        _ => Err(ParseError::UnexpectedToken {
            span: tok.span,
            context: "type length",
        }),
    }
}
