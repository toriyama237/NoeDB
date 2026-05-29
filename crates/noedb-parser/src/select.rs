//! `SELECT` statement parsing.

use noedb_ast::{CteBody, CteDef, Join, JoinKind, SelectItem, SelectStmt, TableRef, WithClause};
use noedb_lexer::{Keyword, Punctuation, Token};

use crate::error::ParseError;
use crate::expr::{parse_expr, parse_optional_alias};
use crate::parser::Parser;

/// Parse `SELECT` for use inside `IN (SELECT …)` (no trailing EOF).
pub(crate) fn parse_select_subquery(p: &mut Parser<'_>) -> Result<SelectStmt, ParseError> {
    let with_clause = parse_optional_with(p)?;
    let mut stmt = parse_select_inner(p, false, false)?;
    stmt.with_clause = with_clause;
    Ok(stmt)
}

pub(crate) fn parse_select(p: &mut Parser<'_>) -> Result<SelectStmt, ParseError> {
    let with_clause = parse_optional_with(p)?;
    let mut stmt = parse_select_inner(p, true, false)?;
    stmt.with_clause = with_clause;
    Ok(stmt)
}

fn parse_optional_with(p: &mut Parser<'_>) -> Result<Option<WithClause>, ParseError> {
    if !p.match_keyword(Keyword::With) {
        return Ok(None);
    }
    let start = p.peek().span;
    let recursive = p.match_keyword(Keyword::Recursive);
    let mut ctes = Vec::new();
    loop {
        let name = p.parse_ident()?;
        p.expect_keyword(Keyword::As)?;
        p.expect_punct(Punctuation::LParen)?;
        let body = parse_cte_body(p)?;
        let end = p.peek().span;
        ctes.push(CteDef {
            name,
            body,
            span: Parser::merge_span(start, end),
        });
        if !matches!(p.peek_kind(), Token::Punct(Punctuation::Comma)) {
            break;
        }
        p.bump();
    }
    let end = if p.pos > 0 {
        p.tokens[p.pos - 1].span
    } else {
        start
    };
    Ok(Some(WithClause {
        recursive,
        ctes,
        span: Parser::merge_span(start, end),
    }))
}

fn parse_cte_body(p: &mut Parser<'_>) -> Result<CteBody, ParseError> {
    let anchor = parse_select_inner(p, false, true)?;
    if p.match_keyword(Keyword::Union) {
        let all = p.match_keyword(Keyword::All);
        let recursive = parse_select_inner(p, false, true)?;
        p.expect_punct(Punctuation::RParen)?;
        return Ok(CteBody::Union {
            anchor: Box::new(anchor),
            all,
            recursive: Box::new(recursive),
        });
    }
    p.expect_punct(Punctuation::RParen)?;
    Ok(CteBody::Select(anchor))
}

fn parse_select_inner(
    p: &mut Parser<'_>,
    expect_eof: bool,
    stop_at_rparen: bool,
) -> Result<SelectStmt, ParseError> {
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
        Some(parse_table_ref(p, stop_at_rparen)?)
    } else {
        None
    };

    let mut joins = Vec::new();
    while matches!(
        p.peek_kind(),
        Token::Keyword(Keyword::Inner | Keyword::Left | Keyword::Join)
    ) {
        joins.push(parse_join(p, stop_at_rparen)?);
    }

    let where_clause = if p.match_keyword(Keyword::Where) {
        Some(parse_expr(p)?)
    } else {
        None
    };

    if stop_at_rparen
        && matches!(
            p.peek_kind(),
            Token::Punct(Punctuation::RParen) | Token::Keyword(Keyword::Union)
        )
    {
        // CTE fragment ends before `UNION` or closing paren.
    } else if expect_eof {
        p.expect_eof()?;
    }

    let end = if p.pos > 0 {
        p.tokens[p.pos - 1].span
    } else {
        start
    };

    Ok(SelectStmt {
        with_clause: None,
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

fn parse_table_ref(p: &mut Parser<'_>, stop_at_rparen: bool) -> Result<TableRef, ParseError> {
    let start = p.peek().span;
    let name = p.parse_ident()?;
    let alias = parse_table_alias(p, stop_at_rparen)?;
    let end = alias.as_ref().map_or(name.span, |alias| alias.span);
    Ok(TableRef {
        name,
        alias,
        span: Parser::merge_span(start, end),
    })
}

fn parse_table_alias(
    p: &mut Parser<'_>,
    stop_at_rparen: bool,
) -> Result<Option<noedb_ast::Ident>, ParseError> {
    if p.match_keyword(Keyword::As) {
        return Ok(Some(p.parse_ident()?));
    }
    if matches!(p.peek_kind(), Token::Ident | Token::QuotedIdent(_))
        && is_table_alias_boundary(p.peek_ahead(1), stop_at_rparen)
    {
        return Ok(Some(p.parse_ident()?));
    }
    Ok(None)
}

#[allow(clippy::unnested_or_patterns)]
fn is_table_alias_boundary(next: Option<&Token>, stop_at_rparen: bool) -> bool {
    matches!(
        next,
        Some(Token::Keyword(
            Keyword::Inner | Keyword::Left | Keyword::Join
        )) | Some(Token::Keyword(Keyword::Where))
            | Some(Token::Keyword(Keyword::On))
            | Some(Token::Keyword(Keyword::Union))
            | Some(Token::Punct(Punctuation::RParen))
            | Some(Token::Punct(Punctuation::Comma))
            | Some(Token::Eof)
    ) || (stop_at_rparen && matches!(next, Some(Token::Punct(Punctuation::RParen))))
}

fn parse_join(p: &mut Parser<'_>, stop_at_rparen: bool) -> Result<Join, ParseError> {
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

    let table = parse_table_ref(p, stop_at_rparen)?;
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
