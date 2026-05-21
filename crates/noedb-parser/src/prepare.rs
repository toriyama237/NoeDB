//! `PREPARE` / `EXECUTE` parsing (Phase 1 Week 4).

use noedb_ast::{ExecuteStmt, PrepareStmt, Statement};
use noedb_lexer::{Keyword, Punctuation, Token};

use crate::error::ParseError;
use crate::expr::parse_literal;
use crate::parser::Parser;

pub(crate) fn parse_prepare(p: &mut Parser<'_>) -> Result<Statement, ParseError> {
    let start = p.expect_keyword(Keyword::Prepare)?;
    let name = p.parse_ident()?;
    p.expect_keyword(Keyword::As)?;
    let inner = Box::new(p.dispatch_statement()?);
    p.expect_eof()?;
    let end = p.tokens[p.pos.saturating_sub(1)].span;
    Ok(Statement::Prepare(PrepareStmt {
        name,
        inner,
        span: Parser::merge_span(start, end),
    }))
}

pub(crate) fn parse_execute(p: &mut Parser<'_>) -> Result<Statement, ParseError> {
    let start = p.expect_keyword(Keyword::Execute)?;
    let name = p.parse_ident()?;
    let mut params = Vec::new();
    if matches!(p.peek_kind(), Token::Punct(Punctuation::LParen)) {
        p.bump();
        if !matches!(p.peek_kind(), Token::Punct(Punctuation::RParen)) {
            loop {
                params.push(parse_literal(p)?);
                if !matches!(p.peek_kind(), Token::Punct(Punctuation::Comma)) {
                    break;
                }
                p.bump();
            }
        }
        p.expect_punct(Punctuation::RParen)?;
    }
    p.expect_eof()?;
    let end = p.tokens[p.pos.saturating_sub(1)].span;
    Ok(Statement::Execute(ExecuteStmt {
        name,
        params,
        span: Parser::merge_span(start, end),
    }))
}
