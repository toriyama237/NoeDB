//! DML statement parsing: INSERT, UPDATE, DELETE.

use noedb_ast::{DeleteStmt, InsertStmt, UpdateStmt};
use noedb_lexer::{Keyword, Operator, Punctuation, Token};

use crate::error::ParseError;
use crate::expr::parse_expr;
use crate::parser::Parser;

pub(crate) fn parse_insert(p: &mut Parser<'_>) -> Result<InsertStmt, ParseError> {
    let start = p.expect_keyword(Keyword::Insert)?;
    p.expect_keyword(Keyword::Into)?;
    let table = p.parse_ident()?;

    let columns = if matches!(p.peek_kind(), Token::Punct(Punctuation::LParen)) {
        p.bump();
        let mut cols = Vec::new();
        loop {
            cols.push(p.parse_ident()?);
            if !matches!(p.peek_kind(), Token::Punct(Punctuation::Comma)) {
                break;
            }
            p.bump();
        }
        p.expect_punct(Punctuation::RParen)?;
        Some(cols)
    } else {
        None
    };

    p.expect_keyword(Keyword::Values)?;
    let mut values = Vec::new();
    loop {
        p.expect_punct(Punctuation::LParen)?;
        let mut row = Vec::new();
        loop {
            row.push(parse_expr(p)?);
            if !matches!(p.peek_kind(), Token::Punct(Punctuation::Comma)) {
                break;
            }
            p.bump();
        }
        p.expect_punct(Punctuation::RParen)?;
        values.push(row);
        if !matches!(p.peek_kind(), Token::Punct(Punctuation::Comma)) {
            break;
        }
        p.bump();
    }

    p.expect_eof()?;
    let end = p.tokens[p.pos.saturating_sub(1)].span;

    Ok(InsertStmt {
        table,
        columns,
        values,
        span: Parser::merge_span(start, end),
    })
}

pub(crate) fn parse_update(p: &mut Parser<'_>) -> Result<UpdateStmt, ParseError> {
    let start = p.expect_keyword(Keyword::Update)?;
    let table = p.parse_ident()?;
    p.expect_keyword(Keyword::Set)?;

    let mut assignments = Vec::new();
    loop {
        let col = p.parse_ident()?;
        if !matches!(p.peek_kind(), Token::Op(_)) {
            return Err(p.unexpected("= in SET clause"));
        }
        let op = p.bump();
        if !matches!(op.kind, Token::Op(Operator::Eq)) {
            return Err(ParseError::UnexpectedToken {
                span: op.span,
                context: "=",
            });
        }
        let val = parse_expr(p)?;
        assignments.push((col, val));
        if !matches!(p.peek_kind(), Token::Punct(Punctuation::Comma)) {
            break;
        }
        p.bump();
    }

    let where_clause = if p.match_keyword(Keyword::Where) {
        Some(parse_expr(p)?)
    } else {
        None
    };

    p.expect_eof()?;
    let end = p.tokens[p.pos.saturating_sub(1)].span;

    Ok(UpdateStmt {
        table,
        assignments,
        where_clause,
        span: Parser::merge_span(start, end),
    })
}

pub(crate) fn parse_delete(p: &mut Parser<'_>) -> Result<DeleteStmt, ParseError> {
    let start = p.expect_keyword(Keyword::Delete)?;
    p.expect_keyword(Keyword::From)?;
    let table = p.parse_ident()?;

    let where_clause = if p.match_keyword(Keyword::Where) {
        Some(parse_expr(p)?)
    } else {
        None
    };

    p.expect_eof()?;
    let end = p.tokens[p.pos.saturating_sub(1)].span;

    Ok(DeleteStmt {
        table,
        where_clause,
        span: Parser::merge_span(start, end),
    })
}
