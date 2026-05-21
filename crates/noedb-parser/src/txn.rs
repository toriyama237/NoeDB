//! Transaction control statements (`BEGIN`, `COMMIT`, `ROLLBACK`).

use noedb_ast::{BeginTxnStmt, CommitTxnStmt, RollbackTxnStmt, Statement};
use noedb_lexer::Keyword;

use crate::error::ParseError;
use crate::parser::Parser;

pub(crate) fn parse_begin(p: &mut Parser<'_>) -> Result<Statement, ParseError> {
    let start = p.expect_keyword(Keyword::Begin)?;
    p.expect_eof()?;
    let end = p.tokens[p.pos.saturating_sub(1)].span;
    Ok(Statement::BeginTxn(BeginTxnStmt {
        span: Parser::merge_span(start, end),
    }))
}

pub(crate) fn parse_commit(p: &mut Parser<'_>) -> Result<Statement, ParseError> {
    let start = p.expect_keyword(Keyword::Commit)?;
    p.expect_eof()?;
    let end = p.tokens[p.pos.saturating_sub(1)].span;
    Ok(Statement::CommitTxn(CommitTxnStmt {
        span: Parser::merge_span(start, end),
    }))
}

pub(crate) fn parse_rollback(p: &mut Parser<'_>) -> Result<Statement, ParseError> {
    let start = p.expect_keyword(Keyword::Rollback)?;
    p.expect_eof()?;
    let end = p.tokens[p.pos.saturating_sub(1)].span;
    Ok(Statement::RollbackTxn(RollbackTxnStmt {
        span: Parser::merge_span(start, end),
    }))
}
