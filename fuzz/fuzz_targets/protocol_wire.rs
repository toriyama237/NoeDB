#![no_main]

use libfuzzer_sys::fuzz_target;
use noedb_protocol::{decode_request, decode_response, encode_request, encode_response, Request, Response};
use noedb_raft::ClusterAuth;

fuzz_target!(|data: &[u8]| {
    let auth = ClusterAuth::from_passphrase("fuzz");
    let _ = decode_request(&auth, data);
    let _ = decode_response(&auth, data);
    if let Ok(frame) = encode_request(&auth, &Request::Ping) {
        let _ = decode_request(&auth, &frame);
    }
    if let Ok(frame) = encode_response(&auth, &Response::Pong) {
        let _ = decode_response(&auth, &frame);
    }
    if data.len() < 4096 {
        let q = String::from_utf8_lossy(data).into_owned();
        let _ = encode_request(
            &auth,
            &Request::Sql {
                query: q.clone(),
            },
        );
        let _ = encode_response(
            &auth,
            &Response::Error {
                code: 1,
                message: q,
            },
        );
    }
});
