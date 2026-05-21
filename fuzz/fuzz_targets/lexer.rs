//! Fuzz target: `cargo fuzz run lexer` (requires `cargo install cargo-fuzz`).
//!
//! The fuzzer feeds arbitrary bytes into the lexer. Invalid UTF-8 is rejected
//! at the API boundary — same as any real caller passing `&str`.

#![no_main]

use libfuzzer_sys::fuzz_target;
use noedb_lexer::tokenize;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = core::str::from_utf8(data) {
        let _ = tokenize(s);
    }
});
