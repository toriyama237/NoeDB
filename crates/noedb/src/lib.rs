//! # NoeDB
//!
//! A distributed embedded SQL query engine in Rust - think SQLite meets
//! CockroachDB.
//!
//! This is the **meta-crate**: it re-exports the user-facing pieces of
//! the workspace so that downstream consumers can write
//!
//! ```toml
//! [dependencies]
//! noedb = "0"
//! ```
//!
//! and reach the lexer, parser, AST, planner, storage and raft modules
//! through a single dependency. Each sub-crate is also independently
//! publishable for users who only need one layer.
//!
//! # Architecture
//!
//! ```text
//!   SQL text
//!      |
//!      v
//!   +---------+    +---------+    +-------+    +---------+    +------+    +---------+
//!   |  lexer  | -> | parser  | -> |  ast  | -> | planner | -> | raft | -> | storage |
//!   +---------+    +---------+    +-------+    +---------+    +------+    +---------+
//! ```
//!
//! # Quick start
//!
//! ```
//! use noedb::lexer::{tokenize, Keyword, Token};
//!
//! let toks = tokenize("SELECT 1")?;
//! assert_eq!(toks[0].kind, Token::Keyword(Keyword::Select));
//! assert_eq!(toks[1].kind, Token::Integer(1));
//! # Ok::<_, noedb::lexer::LexError>(())
//! ```
//!
//! # Status
//!
//! **Day 3 / 260** — Week 03 of Phase 1 (Lexer & Parser). See the full
//! roadmap in the project [README] and in `docs/sprint-plan.md`.
//!
//! [README]: https://github.com/toriyama237/NoeDB

#![forbid(unsafe_code)]

/// Lexer (re-exported from [`noedb_lexer`]).
pub mod lexer {
    pub use noedb_lexer::{
        tokenize, Keyword, LexError, LexErrorKind, Lexer, LineColumn, Operator, Punctuation,
        SourceMap, Span, SpannedToken, Token, KEYWORD_COUNT,
    };
}

/// Abstract syntax tree (re-exported from [`noedb_ast`]).
pub mod ast {
    pub use noedb_ast::Statement;
}

/// Parser (re-exported from [`noedb_parser`]).
pub mod parser {
    pub use noedb_parser::{parse, ParseError};
}

/// Query planner (re-exported from [`noedb_planner`]).
pub mod planner {
    pub use noedb_planner::{plan, LogicalPlan, PlanError};
}

/// Storage engine (re-exported from [`noedb_storage`]).
pub mod storage {
    pub use noedb_storage::StorageEngine;
}

/// Raft consensus (re-exported from [`noedb_raft`]).
pub mod raft {
    pub use noedb_raft::StateMachine;
}
