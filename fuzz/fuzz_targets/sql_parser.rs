#![no_main]

use libfuzzer_sys::fuzz_target;
use noedb_parser::parse;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = core::str::from_utf8(data) {
        let _ = parse(s);
    }
});
