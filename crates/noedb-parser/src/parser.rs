//! Parser core: token stream navigation and shared helpers.

use noedb_ast::{Ident, Statement};
use noedb_lexer::{tokenize, Keyword, Operator, Punctuation, Span, SpannedToken, Token};

use crate::error::ParseError;

/// Recursive-descent SQL parser over a pre-tokenized stream.
pub struct Parser<'a> {
    /// Original source (for zero-copy identifier lexemes).
    pub(crate) src: &'a str,
    /// Token stream including trailing [`Token::Eof`].
    pub(crate) tokens: Vec<SpannedToken>,
    /// Current read cursor.
    pub(crate) pos: usize,
    /// Whether error recovery already ran for this statement.
    pub(crate) recovered: bool,
}

impl<'a> Parser<'a> {
    /// Parse one SQL statement from source text.
    pub fn parse_statement(src: &'a str) -> Result<Statement, ParseError> {
        let tokens = tokenize(src).map_err(ParseError::Lex)?;
        let mut p = Self {
            src,
            tokens,
            pos: 0,
            recovered: false,
        };
        p.parse_statement_inner()
    }

    fn parse_statement_inner(&mut self) -> Result<Statement, ParseError> {
        let result = self.dispatch_statement();
        if result.is_err() && !self.recovered {
            self.recover_to_semicolon();
        }
        result
    }

    pub(crate) fn dispatch_statement(&mut self) -> Result<Statement, ParseError> {
        match self.peek_kind() {
            Token::Keyword(Keyword::Select | Keyword::With) => {
                crate::select::parse_select(self).map(Statement::Select)
            }
            Token::Keyword(Keyword::Insert) => {
                crate::dml::parse_insert(self).map(Statement::Insert)
            }
            Token::Keyword(Keyword::Update) => {
                crate::dml::parse_update(self).map(Statement::Update)
            }
            Token::Keyword(Keyword::Delete) => {
                crate::dml::parse_delete(self).map(Statement::Delete)
            }
            Token::Keyword(Keyword::Create) => crate::ddl::parse_create(self),
            Token::Keyword(Keyword::Drop) => {
                crate::ddl::parse_drop_table(self).map(Statement::DropTable)
            }
            Token::Keyword(Keyword::Alter) => crate::ddl::parse_alter_table_rls(self),
            Token::Keyword(Keyword::Analyze) => crate::analyze::parse_analyze(self),
            Token::Keyword(Keyword::Prepare) => crate::prepare::parse_prepare(self),
            Token::Keyword(Keyword::Execute) => crate::prepare::parse_execute(self),
            Token::Keyword(Keyword::Set) => crate::session::parse_set(self),
            Token::Keyword(Keyword::Begin) => crate::txn::parse_begin(self),
            Token::Keyword(Keyword::Commit) => crate::txn::parse_commit(self),
            Token::Keyword(Keyword::Rollback) => crate::txn::parse_rollback(self),
            _ => Err(self.unexpected("SQL statement")),
        }
    }

    /// Skip tokens until the next `;` or EOF (error recovery).
    pub(crate) fn recover_to_semicolon(&mut self) {
        self.recovered = true;
        while !self.is_at_end() {
            if matches!(self.peek_kind(), Token::Punct(Punctuation::Semicolon)) {
                self.bump();
                break;
            }
            self.bump();
        }
    }

    #[inline]
    pub(crate) fn is_at_end(&self) -> bool {
        matches!(self.peek_kind(), Token::Eof)
    }

    pub(crate) fn peek(&self) -> &SpannedToken {
        &self.tokens[self.pos]
    }

    pub(crate) fn peek_kind(&self) -> &Token {
        &self.peek().kind
    }

    pub(crate) fn peek_ahead(&self, n: usize) -> Option<&Token> {
        self.tokens.get(self.pos + n).map(|t| &t.kind)
    }

    pub(crate) fn bump(&mut self) -> SpannedToken {
        let tok = self.tokens[self.pos].clone();
        if !matches!(tok.kind, Token::Eof) {
            self.pos += 1;
        }
        tok
    }

    pub(crate) fn expect_eof(&self) -> Result<(), ParseError> {
        if self.is_at_end() {
            Ok(())
        } else {
            Err(self.unexpected("end of statement"))
        }
    }

    pub(crate) fn expect_keyword(&mut self, kw: Keyword) -> Result<Span, ParseError> {
        let tok = self.bump();
        match tok.kind {
            Token::Keyword(k) if k == kw => Ok(tok.span),
            _ => Err(ParseError::ExpectedKeyword {
                expected: kw,
                span: tok.span,
            }),
        }
    }

    pub(crate) fn match_keyword(&mut self, kw: Keyword) -> bool {
        if matches!(self.peek_kind(), Token::Keyword(k) if *k == kw) {
            self.bump();
            true
        } else {
            false
        }
    }

    pub(crate) fn expect_punct(&mut self, p: Punctuation) -> Result<Span, ParseError> {
        let tok = self.bump();
        match tok.kind {
            Token::Punct(pp) if pp == p => Ok(tok.span),
            _ => Err(ParseError::ExpectedPunctuation {
                expected: p,
                span: tok.span,
            }),
        }
    }

    pub(crate) fn parse_ident(&mut self) -> Result<Ident, ParseError> {
        let tok = self.bump();
        match &tok.kind {
            Token::Ident => {
                let value = tok
                    .lexeme(self.src)
                    .ok_or(ParseError::UnexpectedToken {
                        span: tok.span,
                        context: "identifier",
                    })?
                    .to_string();
                Ok(Ident::new(value, tok.span))
            }
            Token::QuotedIdent(s) => Ok(Ident::new(s.clone(), tok.span)),
            Token::Keyword(kw) => {
                let value = tok
                    .lexeme(self.src)
                    .map_or_else(|| keyword_as_str(*kw).to_string(), str::to_string);
                Ok(Ident::new(value, tok.span))
            }
            _ => Err(ParseError::UnexpectedToken {
                span: tok.span,
                context: "identifier",
            }),
        }
    }

    pub(crate) fn unexpected(&self, context: &'static str) -> ParseError {
        ParseError::UnexpectedToken {
            span: self.peek().span,
            context,
        }
    }

    pub(crate) fn merge_span(start: Span, end: Span) -> Span {
        Span::new(start.start, end.end)
    }

    pub(crate) fn operator_to_binary(op: Operator) -> noedb_ast::BinaryOp {
        match op {
            Operator::Eq => noedb_ast::BinaryOp::Eq,
            Operator::Ne => noedb_ast::BinaryOp::Ne,
            Operator::Lt => noedb_ast::BinaryOp::Lt,
            Operator::Gt => noedb_ast::BinaryOp::Gt,
            Operator::Le => noedb_ast::BinaryOp::Le,
            Operator::Ge => noedb_ast::BinaryOp::Ge,
            Operator::Plus => noedb_ast::BinaryOp::Plus,
            Operator::Minus => noedb_ast::BinaryOp::Minus,
            Operator::Div => noedb_ast::BinaryOp::Div,
            Operator::Mod => noedb_ast::BinaryOp::Mod,
            Operator::Distance => noedb_ast::BinaryOp::Distance,
            _ => noedb_ast::BinaryOp::Eq,
        }
    }
}

fn keyword_as_str(kw: Keyword) -> &'static str {
    match kw {
        Keyword::Select => "SELECT",
        Keyword::From => "FROM",
        Keyword::Where => "WHERE",
        Keyword::Insert => "INSERT",
        Keyword::Into => "INTO",
        Keyword::Values => "VALUES",
        Keyword::Update => "UPDATE",
        Keyword::Set => "SET",
        Keyword::Delete => "DELETE",
        Keyword::Create => "CREATE",
        Keyword::Drop => "DROP",
        Keyword::Table => "TABLE",
        Keyword::Index => "INDEX",
        Keyword::On => "ON",
        Keyword::Join => "JOIN",
        Keyword::Inner => "INNER",
        Keyword::Left => "LEFT",
        Keyword::Outer => "OUTER",
        Keyword::Distinct => "DISTINCT",
        Keyword::As => "AS",
        Keyword::And => "AND",
        Keyword::Or => "OR",
        Keyword::Not => "NOT",
        Keyword::Null => "NULL",
        Keyword::Is => "IS",
        Keyword::In => "IN",
        Keyword::Between => "BETWEEN",
        Keyword::Like => "LIKE",
        Keyword::True => "TRUE",
        Keyword::False => "FALSE",
        _ => "KEYWORD",
    }
}
