//! # NoeDB
//!
//! A distributed embedded SQL query engine in Rust - think SQLite meets CockroachDB.
//!
//! This is the crate root. The codebase is organized into modules that mirror the
//! data flow of a SQL query:
//!
//! ```text
//!   SQL text
//!      |
//!      v
//!   +-------+    +--------+    +---------+    +------+    +---------+
//!   | lexer | -> | parser | -> | planner | -> | raft | -> | storage |
//!   +-------+    +--------+    +---------+    +------+    +---------+
//! ```
//!
//! ## Status
//!
//! Day 1 of a 52-week sprint. Only the lexer skeleton exists right now.
//! See `README.md` for the full roadmap.

pub mod lexer;
