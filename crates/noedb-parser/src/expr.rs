//! Expression parsing with Pratt precedence.

use noedb_ast::{BinaryOp, ColumnRef, Expr, Ident, Literal, UnaryOp};
use noedb_lexer::{Keyword, Operator, Punctuation, Token};

use crate::error::ParseError;
use crate::parser::Parser;

#[allow(dead_code)]
#[derive(Clone, Copy)]
enum Prec {
    Or = 1,
    And = 2,
    Compare = 3,
    Add = 4,
    Primary = 5,
}

pub(crate) fn parse_expr(p: &mut Parser<'_>) -> Result<Expr, ParseError> {
    parse_expr_prec(p, Prec::Or)
}

fn parse_expr_prec(p: &mut Parser<'_>, min: Prec) -> Result<Expr, ParseError> {
    let mut left = parse_prefix(p)?;

    loop {
        // Postfix / infix keywords with higher binding than OR/AND
        if p.match_keyword(Keyword::Is) {
            let start = left.span();
            let negated = p.match_keyword(Keyword::Not);
            p.expect_keyword(Keyword::Null)?;
            let end = p.peek().span;
            left = Expr::IsNull {
                expr: Box::new(left),
                negated,
                span: Parser::merge_span(start, end),
            };
            continue;
        }

        if matches!(p.peek_kind(), Token::Keyword(Keyword::Not))
            && matches!(p.peek_ahead(1), Some(Token::Keyword(Keyword::In)))
        {
            let start = left.span();
            p.bump(); // NOT
            p.expect_keyword(Keyword::In)?;
            let values = parse_in_list(p)?;
            let end = p.peek().span;
            left = Expr::In {
                expr: Box::new(left),
                values,
                negated: true,
                span: Parser::merge_span(start, end),
            };
            continue;
        }

        if p.match_keyword(Keyword::In) {
            let start = left.span();
            let values = parse_in_list(p)?;
            let end = p.peek().span;
            left = Expr::In {
                expr: Box::new(left),
                values,
                negated: false,
                span: Parser::merge_span(start, end),
            };
            continue;
        }

        if p.match_keyword(Keyword::Not) && p.match_keyword(Keyword::Between) {
            let start = left.span();
            let low = parse_expr_prec(p, Prec::Compare)?;
            p.expect_keyword(Keyword::And)?;
            let high = parse_expr_prec(p, Prec::Compare)?;
            let end = p.peek().span;
            left = Expr::Between {
                expr: Box::new(left),
                low: Box::new(low),
                high: Box::new(high),
                negated: true,
                span: Parser::merge_span(start, end),
            };
            continue;
        }

        if p.match_keyword(Keyword::Between) {
            let start = left.span();
            let low = parse_expr_prec(p, Prec::Compare)?;
            p.expect_keyword(Keyword::And)?;
            let high = parse_expr_prec(p, Prec::Compare)?;
            let end = p.peek().span;
            left = Expr::Between {
                expr: Box::new(left),
                low: Box::new(low),
                high: Box::new(high),
                negated: false,
                span: Parser::merge_span(start, end),
            };
            continue;
        }

        if p.match_keyword(Keyword::Like) {
            let start = left.span();
            let right = parse_expr_prec(p, Prec::Compare)?;
            let end = right.span();
            left = Expr::Binary {
                op: BinaryOp::Like,
                left: Box::new(left),
                right: Box::new(right),
                span: Parser::merge_span(start, end),
            };
            continue;
        }

        // Binary operators by precedence
        if (min as u8) <= Prec::Or as u8 && p.match_keyword(Keyword::Or) {
            let left_span = left.span();
            let right = parse_expr_prec(p, Prec::And)?;
            let end = right.span();
            left = Expr::Binary {
                op: BinaryOp::Or,
                left: Box::new(left),
                right: Box::new(right),
                span: Parser::merge_span(left_span, end),
            };
            continue;
        }

        if (min as u8) <= Prec::And as u8 && p.match_keyword(Keyword::And) {
            let left_span = left.span();
            let right = parse_expr_prec(p, Prec::Compare)?;
            let end = right.span();
            left = Expr::Binary {
                op: BinaryOp::And,
                left: Box::new(left),
                right: Box::new(right),
                span: Parser::merge_span(left_span, end),
            };
            continue;
        }

        if (min as u8) <= Prec::Compare as u8 {
            if let Token::Op(op) = p.peek_kind().clone() {
                match op {
                    Operator::Eq
                    | Operator::Ne
                    | Operator::Lt
                    | Operator::Gt
                    | Operator::Le
                    | Operator::Ge => {
                        p.bump();
                        let bin = Parser::operator_to_binary(op);
                        let left_span = left.span();
                        let right = parse_expr_prec(p, Prec::Compare)?;
                        let end = right.span();
                        left = Expr::Binary {
                            op: bin,
                            left: Box::new(left),
                            right: Box::new(right),
                            span: Parser::merge_span(left_span, end),
                        };
                        continue;
                    }
                    _ => {}
                }
            }
        }

        if (min as u8) <= Prec::Add as u8 {
            if let Token::Op(op @ (Operator::Plus | Operator::Minus)) = p.peek_kind().clone() {
                p.bump();
                let bin = Parser::operator_to_binary(op);
                let left_span = left.span();
                let right = parse_expr_prec(p, Prec::Add)?;
                let end = right.span();
                left = Expr::Binary {
                    op: bin,
                    left: Box::new(left),
                    right: Box::new(right),
                    span: Parser::merge_span(left_span, end),
                };
                continue;
            }
        }

        break;
    }

    Ok(left)
}

fn parse_prefix(p: &mut Parser<'_>) -> Result<Expr, ParseError> {
    if p.match_keyword(Keyword::Not) {
        let start = p.peek().span;
        let inner = parse_expr_prec(p, Prec::And)?;
        let end = inner.span();
        return Ok(Expr::Unary {
            op: UnaryOp::Not,
            expr: Box::new(inner),
            span: Parser::merge_span(start, end),
        });
    }

    if matches!(p.peek_kind(), Token::Op(Operator::Minus)) {
        let start = p.bump().span;
        let inner = parse_expr_prec(p, Prec::Add)?;
        let end = inner.span();
        return Ok(Expr::Unary {
            op: UnaryOp::Minus,
            expr: Box::new(inner),
            span: Parser::merge_span(start, end),
        });
    }

    if matches!(p.peek_kind(), Token::Punct(Punctuation::LParen)) {
        let start = p.bump().span;
        let inner = parse_expr(p)?;
        let end = p.expect_punct(Punctuation::RParen)?;
        return Ok(Expr::Paren(Box::new(inner), Parser::merge_span(start, end)));
    }

    if p.match_keyword(Keyword::CurrentUser) {
        let span = p.tokens[p.pos.saturating_sub(1)].span;
        return Ok(Expr::CurrentUser { span });
    }

    if let Token::Parameter(index) = p.peek_kind().clone() {
        let span = p.bump().span;
        return Ok(Expr::Parameter { index, span });
    }

    parse_atom(p)
}

pub(crate) fn parse_literal(p: &mut Parser<'_>) -> Result<Literal, ParseError> {
    let tok = p.bump();
    match tok.kind {
        Token::Integer(v) => Ok(Literal::Integer(v, tok.span)),
        Token::Float(v) => Ok(Literal::Float(v, tok.span)),
        Token::String(s) => Ok(Literal::String(s, tok.span)),
        Token::Keyword(Keyword::Null) => Ok(Literal::Null { span: tok.span }),
        Token::Keyword(Keyword::True) => Ok(Literal::Boolean(true, tok.span)),
        Token::Keyword(Keyword::False) => Ok(Literal::Boolean(false, tok.span)),
        _ => Err(ParseError::UnexpectedToken {
            span: tok.span,
            context: "literal",
        }),
    }
}

fn parse_atom(p: &mut Parser<'_>) -> Result<Expr, ParseError> {
    let tok = p.bump();
    match tok.kind {
        Token::Integer(v) => Ok(Expr::Literal(Literal::Integer(v, tok.span))),
        Token::Float(v) => Ok(Expr::Literal(Literal::Float(v, tok.span))),
        Token::String(s) => Ok(Expr::Literal(Literal::String(s, tok.span))),
        Token::Keyword(Keyword::Null) => Ok(Expr::Literal(Literal::Null { span: tok.span })),
        Token::Keyword(Keyword::True) => Ok(Expr::Literal(Literal::Boolean(true, tok.span))),
        Token::Keyword(Keyword::False) => Ok(Expr::Literal(Literal::Boolean(false, tok.span))),
        Token::Ident | Token::QuotedIdent(_) | Token::Keyword(_) => {
            p.pos -= 1;
            parse_column_ref(p)
        }
        Token::Punct(Punctuation::Star) => Ok(Expr::Column(ColumnRef::Star { span: tok.span })),
        _ => Err(ParseError::UnexpectedToken {
            span: tok.span,
            context: "expression",
        }),
    }
}

fn parse_column_ref(p: &mut Parser<'_>) -> Result<Expr, ParseError> {
    let first = p.parse_ident()?;
    if matches!(p.peek_kind(), Token::Punct(Punctuation::Dot)) {
        p.bump();
        if matches!(p.peek_kind(), Token::Punct(Punctuation::Star)) {
            let star = p.bump();
            let span = Parser::merge_span(first.span, star.span);
            return Ok(Expr::Column(ColumnRef::QualifiedStar {
                table: first,
                span,
            }));
        }
        let column = p.parse_ident()?;
        return Ok(Expr::Column(ColumnRef::Named {
            table: Some(first),
            column,
        }));
    }
    Ok(Expr::Column(ColumnRef::Named {
        table: None,
        column: first,
    }))
}

fn parse_in_list(p: &mut Parser<'_>) -> Result<Vec<Expr>, ParseError> {
    p.expect_punct(Punctuation::LParen)?;
    let mut values = Vec::new();
    if !matches!(p.peek_kind(), Token::Punct(Punctuation::RParen)) {
        loop {
            values.push(parse_expr(p)?);
            if !matches!(p.peek_kind(), Token::Punct(Punctuation::Comma)) {
                break;
            }
            p.bump();
        }
    }
    p.expect_punct(Punctuation::RParen)?;
    Ok(values)
}

pub(crate) fn parse_optional_alias(p: &mut Parser<'_>) -> Result<Option<Ident>, ParseError> {
    if p.match_keyword(Keyword::As) {
        return Ok(Some(p.parse_ident()?));
    }
    if matches!(p.peek_kind(), Token::Ident | Token::QuotedIdent(_)) {
        // Bare alias: `SELECT 1 x`
        if is_expr_list_boundary(p.peek_ahead(1)) {
            return Ok(Some(p.parse_ident()?));
        }
    }
    Ok(None)
}

#[allow(clippy::unnested_or_patterns)]
fn is_expr_list_boundary(next: Option<&Token>) -> bool {
    matches!(
        next,
        Some(Token::Punct(Punctuation::Comma))
            | Some(Token::Keyword(Keyword::From))
            | Some(Token::Keyword(Keyword::Where))
            | Some(Token::Eof)
    )
}
