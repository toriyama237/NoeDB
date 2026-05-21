//! DDL statement parsing: CREATE TABLE/INDEX, DROP TABLE.

use noedb_ast::{ColumnDef, CreateIndexStmt, CreateTableStmt, DropTableStmt, SqlType, Statement};
use noedb_lexer::{Keyword, Punctuation, Token};

use crate::error::ParseError;
use crate::parser::Parser;

pub(crate) fn parse_create(p: &mut Parser<'_>) -> Result<Statement, ParseError> {
    let start = p.expect_keyword(Keyword::Create)?;
    if p.match_keyword(Keyword::Table) {
        return parse_create_table(p, start).map(Statement::CreateTable);
    }
    if p.match_keyword(Keyword::Index) {
        return parse_create_index(p, start).map(Statement::CreateIndex);
    }
    if p.match_keyword(Keyword::Policy) {
        return parse_create_policy(p, start).map(Statement::CreatePolicy);
    }
    Err(p.unexpected("TABLE, INDEX, or POLICY after CREATE"))
}

fn parse_create_policy(
    p: &mut Parser<'_>,
    start: noedb_lexer::Span,
) -> Result<noedb_ast::CreatePolicyStmt, ParseError> {
    let name = p.parse_ident()?;
    p.expect_keyword(Keyword::On)?;
    let table = p.parse_ident()?;
    p.expect_keyword(Keyword::Using)?;
    p.expect_punct(Punctuation::LParen)?;
    let using_expr = crate::expr::parse_expr(p)?;
    p.expect_punct(Punctuation::RParen)?;
    p.expect_eof()?;
    let end = p.tokens[p.pos.saturating_sub(1)].span;
    Ok(noedb_ast::CreatePolicyStmt {
        name,
        table,
        using_expr,
        span: Parser::merge_span(start, end),
    })
}

pub(crate) fn parse_alter_table_rls(p: &mut Parser<'_>) -> Result<Statement, ParseError> {
    let start = p.expect_keyword(Keyword::Alter)?;
    p.expect_keyword(Keyword::Table)?;
    let table = p.parse_ident()?;
    p.expect_keyword(Keyword::Enable)?;
    p.expect_keyword(Keyword::Row)?;
    p.expect_keyword(Keyword::Level)?;
    p.expect_keyword(Keyword::Security)?;
    p.expect_eof()?;
    Ok(Statement::EnableRls(noedb_ast::EnableRlsStmt {
        table,
        span: Parser::merge_span(start, p.tokens[p.pos.saturating_sub(1)].span),
    }))
}

fn parse_create_table(
    p: &mut Parser<'_>,
    start: noedb_lexer::Span,
) -> Result<CreateTableStmt, ParseError> {
    let name = p.parse_ident()?;
    p.expect_punct(Punctuation::LParen)?;

    let mut columns = Vec::new();
    loop {
        columns.push(parse_column_def(p)?);
        if !matches!(p.peek_kind(), Token::Punct(Punctuation::Comma)) {
            break;
        }
        p.bump();
    }
    p.expect_punct(Punctuation::RParen)?;
    p.expect_eof()?;
    let end = p.tokens[p.pos.saturating_sub(1)].span;

    Ok(CreateTableStmt {
        name,
        columns,
        span: Parser::merge_span(start, end),
    })
}

fn parse_column_def(p: &mut Parser<'_>) -> Result<ColumnDef, ParseError> {
    let start = p.peek().span;
    let name = p.parse_ident()?;
    let data_type = parse_sql_type(p)?;
    let not_null = p.match_keyword(Keyword::Not) && p.match_keyword(Keyword::Null);
    let end = p.tokens[p.pos.saturating_sub(1)].span;
    Ok(ColumnDef {
        name,
        data_type,
        not_null,
        span: Parser::merge_span(start, end),
    })
}

fn parse_sql_type(p: &mut Parser<'_>) -> Result<SqlType, ParseError> {
    let name = p.parse_ident()?;
    let upper = name.value.to_ascii_uppercase();
    match upper.as_str() {
        "INT" | "INTEGER" => Ok(SqlType::Int),
        "BOOLEAN" | "BOOL" => Ok(SqlType::Boolean),
        "FLOAT" | "REAL" | "DOUBLE" => Ok(SqlType::Float),
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

fn parse_create_index(
    p: &mut Parser<'_>,
    start: noedb_lexer::Span,
) -> Result<CreateIndexStmt, ParseError> {
    let name = p.parse_ident()?;
    p.expect_keyword(Keyword::On)?;
    let table = p.parse_ident()?;
    p.expect_punct(Punctuation::LParen)?;

    let mut columns = Vec::new();
    loop {
        columns.push(p.parse_ident()?);
        if !matches!(p.peek_kind(), Token::Punct(Punctuation::Comma)) {
            break;
        }
        p.bump();
    }
    p.expect_punct(Punctuation::RParen)?;
    p.expect_eof()?;
    let end = p.tokens[p.pos.saturating_sub(1)].span;

    Ok(CreateIndexStmt {
        name,
        table,
        columns,
        span: Parser::merge_span(start, end),
    })
}

pub(crate) fn parse_drop_table(p: &mut Parser<'_>) -> Result<DropTableStmt, ParseError> {
    let start = p.expect_keyword(Keyword::Drop)?;
    p.expect_keyword(Keyword::Table)?;
    let name = p.parse_ident()?;
    p.expect_eof()?;
    let end = name.span;
    Ok(DropTableStmt {
        name,
        span: Parser::merge_span(start, end),
    })
}
