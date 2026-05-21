//! `SELECT` statement parsing.

use noedb_ast::{Join, JoinKind, SelectItem, SelectStmt, TableRef};
use noedb_lexer::{Keyword, Punctuation, Token};

use crate::error::ParseError;
use crate::expr::{parse_expr, parse_optional_alias};
use crate::parser::Parser;

pub(crate) fn parse_select(p: &mut Parser<'_>) -> Result<SelectStmt, ParseError> {
    let start = p.expect_keyword(Keyword::Select)?;
    let distinct = p.match_keyword(Keyword::Distinct);

    let mut items = Vec::new();
    loop {
        items.push(parse_select_item(p)?);
        if !matches!(p.peek_kind(), Token::Punct(Punctuation::Comma)) {
            break;
        }
        p.bump();
    }

    let from = if p.match_keyword(Keyword::From) {
        Some(parse_table_ref(p)?)
    } else {
        None
    };

    let mut joins = Vec::new();
    while matches!(
        p.peek_kind(),
        Token::Keyword(Keyword::Inner | Keyword::Left | Keyword::Join)
    ) {
        joins.push(parse_join(p)?);
    }

    let where_clause = if p.match_keyword(Keyword::Where) {
        Some(parse_expr(p)?)
    } else {
        None
    };

    p.expect_eof()?;

    let end = if p.pos > 0 {
        p.tokens[p.pos - 1].span
    } else {
        start
    };

    Ok(SelectStmt {
        distinct,
        items,
        from,
        joins,
        where_clause,
        span: Parser::merge_span(start, end),
    })
}

fn parse_select_item(p: &mut Parser<'_>) -> Result<SelectItem, ParseError> {
    let expr = parse_expr(p)?;
    let alias = parse_optional_alias(p)?;
    Ok(SelectItem { expr, alias })
}

fn parse_table_ref(p: &mut Parser<'_>) -> Result<TableRef, ParseError> {
    let start = p.peek().span;
    let name = p.parse_ident()?;
    let alias = parse_table_alias(p)?;
    let end = alias.as_ref().map_or(name.span, |alias| alias.span);
    Ok(TableRef {
        name,
        alias,
        span: Parser::merge_span(start, end),
    })
}

fn parse_table_alias(p: &mut Parser<'_>) -> Result<Option<noedb_ast::Ident>, ParseError> {
    if p.match_keyword(Keyword::As) {
        return Ok(Some(p.parse_ident()?));
    }
    if matches!(p.peek_kind(), Token::Ident | Token::QuotedIdent(_))
        && is_table_alias_boundary(p.peek_ahead(1))
    {
        return Ok(Some(p.parse_ident()?));
    }
    Ok(None)
}

#[allow(clippy::unnested_or_patterns)]
fn is_table_alias_boundary(next: Option<&Token>) -> bool {
    matches!(
        next,
        Some(Token::Keyword(
            Keyword::Inner | Keyword::Left | Keyword::Join
        )) | Some(Token::Keyword(Keyword::Where))
            | Some(Token::Keyword(Keyword::On))
            | Some(Token::Eof)
    )
}

fn parse_join(p: &mut Parser<'_>) -> Result<Join, ParseError> {
    let start = p.peek().span;
    let kind = if p.match_keyword(Keyword::Inner) {
        p.expect_keyword(Keyword::Join)?;
        JoinKind::Inner
    } else if p.match_keyword(Keyword::Left) {
        p.match_keyword(Keyword::Outer);
        p.expect_keyword(Keyword::Join)?;
        JoinKind::Left
    } else {
        p.expect_keyword(Keyword::Join)?;
        JoinKind::Inner
    };

    let table = parse_table_ref(p)?;
    p.expect_keyword(Keyword::On)?;
    let on = parse_expr(p)?;
    let end = on.span();

    Ok(Join {
        kind,
        table,
        on,
        span: Parser::merge_span(start, end),
    })
}
