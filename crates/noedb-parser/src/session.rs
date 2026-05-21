//! Session statements (`SET ROLE`, …).

use noedb_ast::{SetRoleStmt, Statement};
use noedb_lexer::{Keyword, Token};

use crate::error::ParseError;
use crate::parser::Parser;

pub(crate) fn parse_set(p: &mut Parser<'_>) -> Result<Statement, ParseError> {
    let start = p.expect_keyword(Keyword::Set)?;
    p.expect_keyword(Keyword::Role)?;
    let role = parse_role_literal(p)?;
    p.expect_eof()?;
    let end = p.tokens[p.pos.saturating_sub(1)].span;
    Ok(Statement::SetRole(SetRoleStmt {
        role,
        span: Parser::merge_span(start, end),
    }))
}

fn parse_role_literal(p: &mut Parser<'_>) -> Result<String, ParseError> {
    match p.peek_kind() {
        Token::String(_) => {
            let tok = p.bump();
            match tok.kind {
                Token::String(s) => Ok(s),
                _ => unreachable!(),
            }
        }
        Token::Ident | Token::QuotedIdent(_) => Ok(p.parse_ident()?.value),
        _ => Err(ParseError::UnexpectedToken {
            span: p.peek().span,
            context: "role name",
        }),
    }
}
