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
//! **v1.0.0** — Full pipeline: SQL → planner → Raft → LSM, CLI + wire protocol.
//! Lexer: &lt;18 ms / 1M tokens.
//!
//! [README]: https://github.com/toriyama237/NoeDB

#![forbid(unsafe_code)]

/// Lexer (re-exported from [`noedb_lexer`]).
pub mod lexer {
    pub use noedb_lexer::{
        tokenize, tokenize_into, Keyword, LexError, LexErrorKind, Lexer, LineColumn, Operator, Punctuation,
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
    pub use noedb_planner::{
        apply_statement, build, create_index, estimate, execute, execute_sql, explain,
        explain_sql, index_wins, lower, optimize, plan, AggFunc, BTreeIndex, ExecutionContext,
        ExecError, Executor, LogicalPlan, PhysicalPlan, PlanContext, PlanError, PlanStats,
        Record, SecondaryIndex, Value, INDEX_LOOKUP_COST, SEQ_SCAN_ROW_COST,
    };
}

/// Storage engine (re-exported from [`noedb_storage`]).
pub mod storage {
    pub use noedb_storage::{
        compact_level0_to_l1, replay_into_memtable, replay_wal_dir, BloomFilter, DurableStore,
        LogEntry, LsmConfig, LsmTree, MemTable, MemTableIter, MemTableRangeIter, OpType, SstReader,
        SstWriter, StorageEngine, StorageError, Wal, WalSegmentManager, WalSyncMode,
        DEFAULT_MAX_ENTRIES, DEFAULT_MAX_MEM_BYTES, L0_COMPACTION_TRIGGER, SST_MAGIC, SST_VERSION, WAL_MAGIC,
        WAL_VERSION,
    };
}

/// Integrated SQL engine (re-exported from [`noedb_engine`]).
pub mod engine {
    pub use noedb_engine::{
        apply_command, validate_sql, Command, DistributedEngine, EngineError, LocalEngine,
        QueryResult, MAX_COMMAND_BYTES, MAX_SQL_BYTES,
    };
}

/// Client wire protocol (re-exported from [`noedb_protocol`]).
pub mod protocol {
    pub use noedb_protocol::{
        decode_request, decode_response, encode_request, encode_response, ProtocolError,
        Request, Response, PROTOCOL_VERSION,
    };
}

/// Raft consensus (re-exported from [`noedb_raft`]).
pub mod raft {
    pub use noedb_raft::{
        decode_message, encode_message, Action, AppendEntriesReq, AppendEntriesResp, Cluster,
        ClusterAuth, FileStorage, HardState, LogEntry, MemNode, MemStorage, Raft, RaftConfig,
        RaftError, RaftLog, RaftNode, RaftStorage, RequestVoteReq, RequestVoteResp, Role, RpcMessage,
        Snapshot, StateMachine, Term, LogIndex, NodeId, MAX_FRAME_BYTES,
    };
}
