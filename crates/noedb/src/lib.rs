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
//! **Week 16 / Phase 2** — `LsmTree` with WAL segments, SSTables, Bloom filters,
//! and L0→L1 compaction. See `docs/sprint-plan.md`.
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
    pub use noedb_ast::{
        BinaryOp, ColumnDef, ColumnRef, CreateIndexStmt, CreateTableStmt, DeleteStmt, Expr, Ident,
        InsertStmt, Join, JoinKind, Literal, SelectItem, SelectStmt, SqlType, Statement, TableRef,
        UnaryOp, UpdateStmt,
    };
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
    pub use noedb_storage::{
        compact_level0_to_l1, replay_into_memtable, replay_wal_dir, BloomFilter, DurableStore,
        LogEntry, LsmConfig, LsmTree, MemTable, MemTableIter, MemTableRangeIter, OpType, SstReader,
        SstWriter, StorageEngine, StorageError, Wal, WalSegmentManager, DEFAULT_MAX_ENTRIES,
        DEFAULT_MAX_MEM_BYTES, L0_COMPACTION_TRIGGER, SST_MAGIC, SST_VERSION, WAL_MAGIC,
        WAL_VERSION,
    };
}

/// Raft consensus (re-exported from [`noedb_raft`]).
pub mod raft {
    pub use noedb_raft::StateMachine;
}
