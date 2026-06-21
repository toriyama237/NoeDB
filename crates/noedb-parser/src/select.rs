//! `SELECT` statement parsing.

use noedb_ast::{
    CompoundSelect, CteBody, CteDef, Expr, FromItem, Join, JoinKind, OrderKey, SelectItem,
    SelectStmt, SetOpKind, TableRef, WithClause,
};
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
    let mut stmt = parse_select_query(p)?;
    stmt.with_clause = with_clause;
    stmt.compound = parse_compound_chain(p)?;
    p.expect_eof()?;
    Ok(stmt)
}

fn parse_select_query(p: &mut Parser<'_>) -> Result<SelectStmt, ParseError> {
    parse_select_inner(p, false, false)
}

fn parse_compound_chain(p: &mut Parser<'_>) -> Result<Option<CompoundSelect>, ParseError> {
    let op = match p.peek_kind() {
        Token::Keyword(Keyword::Union) => SetOpKind::Union,
        Token::Keyword(Keyword::Intersect) => SetOpKind::Intersect,
        Token::Keyword(Keyword::Except) => SetOpKind::Except,
        _ => return Ok(None),
    };
    p.bump();
    let all = p.match_keyword(Keyword::All);
    let mut right = parse_select_query(p)?;
    right.compound = parse_compound_chain(p)?;
    Ok(Some(CompoundSelect {
        op,
        all,
        right: Box::new(right),
    }))
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
        Some(parse_from_item(p, stop_at_rparen)?)
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

    let group_by = parse_group_by(p)?;
    let having_clause = parse_having(p)?;
    let order_by = parse_order_by(p)?;
    let (limit, offset) = parse_limit_offset(p)?;

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
        group_by,
        having_clause,
        order_by,
        limit,
        offset,
        compound: None,
        span: Parser::merge_span(start, end),
    })
}

fn parse_select_item(p: &mut Parser<'_>) -> Result<SelectItem, ParseError> {
    let expr = parse_expr(p)?;
    let alias = parse_optional_alias(p)?;
    Ok(SelectItem { expr, alias })
}

fn parse_from_item(p: &mut Parser<'_>, stop_at_rparen: bool) -> Result<FromItem, ParseError> {
    if matches!(p.peek_kind(), Token::Punct(Punctuation::LParen)) {
        let start = p.bump().span;
        let query = parse_select_inner(p, false, true)?;
        p.expect_punct(Punctuation::RParen)?;
        p.match_keyword(Keyword::As);
        let alias = p.parse_ident()?;
        let end = alias.span;
        return Ok(FromItem::Subquery {
            query: Box::new(query),
            alias,
            span: Parser::merge_span(start, end),
        });
    }
    Ok(FromItem::Table(parse_table_ref(p, stop_at_rparen)?))
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
            Keyword::Inner
                | Keyword::Left
                | Keyword::Join
                | Keyword::Order
                | Keyword::Limit
                | Keyword::Group
        )) | Some(Token::Keyword(Keyword::Where))
            | Some(Token::Keyword(Keyword::On))
            | Some(Token::Keyword(Keyword::Union))
            | Some(Token::Keyword(Keyword::Intersect))
            | Some(Token::Keyword(Keyword::Except))
            | Some(Token::Keyword(Keyword::Offset))
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

fn parse_group_by(p: &mut Parser<'_>) -> Result<Vec<Expr>, ParseError> {
    if !p.match_keyword(Keyword::Group) {
        return Ok(Vec::new());
    }
    p.expect_keyword(Keyword::By)?;
    let mut cols = Vec::new();
    loop {
        cols.push(parse_expr(p)?);
        if !matches!(p.peek_kind(), Token::Punct(Punctuation::Comma)) {
            break;
        }
        p.bump();
    }
    Ok(cols)
}

fn parse_having(p: &mut Parser<'_>) -> Result<Option<Expr>, ParseError> {
    if !p.match_keyword(Keyword::Having) {
        return Ok(None);
    }
    Ok(Some(parse_expr(p)?))
}

fn parse_order_by(p: &mut Parser<'_>) -> Result<Vec<OrderKey>, ParseError> {
    if !p.match_keyword(Keyword::Order) {
        return Ok(Vec::new());
    }
    p.expect_keyword(Keyword::By)?;
    let mut keys = Vec::new();
    loop {
        let expr = parse_expr(p)?;
        let asc = if p.match_keyword(Keyword::Desc) {
            false
        } else {
            let _ = p.match_keyword(Keyword::Asc);
            true
        };
        keys.push(OrderKey { expr, asc });
        if !matches!(p.peek_kind(), Token::Punct(Punctuation::Comma)) {
            break;
        }
        p.bump();
    }
    Ok(keys)
}

fn parse_limit_offset(p: &mut Parser<'_>) -> Result<(Option<u64>, Option<u64>), ParseError> {
    let limit = if p.match_keyword(Keyword::Limit) {
        Some(parse_unsigned(p)?)
    } else {
        None
    };
    let offset = if p.match_keyword(Keyword::Offset) {
        Some(parse_unsigned(p)?)
    } else {
        None
    };
    Ok((limit, offset))
}

fn parse_unsigned(p: &mut Parser<'_>) -> Result<u64, ParseError> {
    let tok = p.bump();
    match tok.kind {
        Token::Integer(v) if v >= 0 => u64::try_from(v).map_err(|_| ParseError::UnexpectedToken {
            span: tok.span,
            context: "LIMIT/OFFSET",
        }),
        _ => Err(ParseError::UnexpectedToken {
            span: tok.span,
            context: "LIMIT/OFFSET integer",
        }),
    }
}
