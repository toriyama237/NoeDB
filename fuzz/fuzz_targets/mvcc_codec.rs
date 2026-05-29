#![no_main]

use libfuzzer_sys::fuzz_target;
use noedb_storage::mvcc::{decode_or_legacy, decode_version, encode_version, Version};

fuzz_target!(|data: &[u8]| {
    let _ = decode_or_legacy(data);
    let _ = decode_version(data);
    if data.len() < 256 {
        let v = Version::put(1, data.to_vec());
        if let Ok(encoded) = encode_version(&v) {
            let _ = decode_version(&encoded);
            let _ = decode_or_legacy(&encoded);
        }
    }
});
