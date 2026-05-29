//! `ANALYZE TABLE` parsing (Phase 5 Week 43).

use noedb_ast::{AnalyzeTableStmt, Statement};
use noedb_lexer::Keyword;

use crate::error::ParseError;
use crate::parser::Parser;

pub(crate) fn parse_analyze(p: &mut Parser<'_>) -> Result<Statement, ParseError> {
    let start = p.expect_keyword(Keyword::Analyze)?;
    p.expect_keyword(Keyword::Table)?;
    let table = p.parse_ident()?;
    let span = Parser::merge_span(start, table.span);
    p.expect_eof()?;
    Ok(Statement::AnalyzeTable(AnalyzeTableStmt { table, span }))
}
