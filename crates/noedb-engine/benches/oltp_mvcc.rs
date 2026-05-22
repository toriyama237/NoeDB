//! OLTP bank transfer benchmark with MVCC + Rayon (interior mutability).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::panic,
    clippy::items_after_statements,
    clippy::redundant_closure,
    clippy::unnecessary_operation,
    missing_docs
)]

use std::sync::Arc;
use std::time::Instant;

use noedb_engine::{EngineError, LocalEngine};
use rayon::prelude::*;

fn balance_key(account: &str) -> String {
    format!("accounts\0{account}\0balance")
}

fn read_balance(eng: &LocalEngine, session_id: u64, account: &str) -> i64 {
    let key = balance_key(account);
    let raw = if eng.txn().in_txn(session_id) {
        eng.txn()
            .get(session_id, key.as_bytes())
            .expect("txn get")
            .unwrap_or_else(|| b"0".to_vec())
    } else {
        eng.store()
            .get_latest(key.as_bytes())
            .expect("get")
            .unwrap_or_else(|| b"0".to_vec())
    };
    std::str::from_utf8(&raw)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

fn run_transfer(
    eng: &Arc<LocalEngine>,
    session_id: u64,
    from: &str,
    to: &str,
) -> Result<(), EngineError> {
    let bal_from = read_balance(eng, session_id, from);
    if bal_from < 100 {
        return Ok(());
    }
    let bal_to = read_balance(eng, session_id, to);

    eng.execute_session(session_id, "BEGIN")?;
    eng.put_row(
        session_id,
        "accounts",
        from,
        "balance",
        &(bal_from - 100).to_string().into_bytes(),
    )?;
    eng.put_row(
        session_id,
        "accounts",
        to,
        "balance",
        &(bal_to + 100).to_string().into_bytes(),
    )?;
    eng.execute_session(session_id, "COMMIT")?;
    Ok(())
}

fn main() {
    const ACCOUNTS: u32 = 100;
    const TXNS: u32 = 2_000;

    let threads = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map_or(4, std::num::NonZeroUsize::get)
        });

    rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build_global()
        .expect("rayon pool");

    let dir = std::env::temp_dir().join(format!(
        "noedb-oltp-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    let eng = LocalEngine::open_throughput(&dir).expect("open engine");

    for i in 0..ACCOUNTS {
        eng.put_row_default("accounts", &i.to_string(), "balance", b"1000")
            .expect("seed");
    }

    let eng = Arc::new(eng);

    let start = Instant::now();
    let ok: u32 = (0..TXNS)
        .into_par_iter()
        .map(|t| {
            let session_id = u64::from(t) + 1;
            let from = (t % ACCOUNTS).to_string();
            let to = ((t + 1) % ACCOUNTS).to_string();
            let mut retries = 0u32;
            loop {
                match run_transfer(&eng, session_id, &from, &to) {
                    Ok(()) => return 1,
                    Err(EngineError::SerializationFailure(_)) => {
                        retries += 1;
                        assert!(
                            retries <= 10,
                            "too many serialization conflicts for session {session_id}"
                        );
                        let _ = eng.execute_session(session_id, "ROLLBACK");
                        std::thread::yield_now();
                    }
                    Err(e) => panic!("fatal error session {session_id}: {e:?}"),
                }
            }
        })
        .sum();

    let elapsed = start.elapsed();
    let tps = f64::from(ok) / elapsed.as_secs_f64();

    println!("oltp_mvcc: {ok}/{TXNS} transfers in {elapsed:?} ({tps:.0} txn/s, threads={threads})");
    let _ = std::fs::remove_dir_all(dir);
}
