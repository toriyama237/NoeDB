//! OLTP bank transfer benchmark with MVCC (Phase 2/3).

use std::time::Instant;

use noedb_engine::LocalEngine;

fn balance_key(account: &str) -> String {
    format!("accounts\0{account}\0balance")
}

fn read_balance(eng: &LocalEngine, account: &str) -> i64 {
    let key = balance_key(account);
    let raw = eng
        .store()
        .get_latest(key.as_bytes())
        .expect("get")
        .unwrap_or_else(|| b"0".to_vec());
    std::str::from_utf8(&raw)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

fn main() {
    let threads = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);

    let dir = std::env::temp_dir().join(format!(
        "noedb-oltp-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    let mut eng = LocalEngine::open_throughput(&dir).expect("open engine");

    const ACCOUNTS: u32 = 100;
    const TXNS: u32 = 2_000;

    for i in 0..ACCOUNTS {
        eng.put_row("accounts", &i.to_string(), "balance", b"1000")
            .expect("seed");
    }

    let start = Instant::now();
    let mut ok = 0u32;
    for t in 0..TXNS {
        let from = (t % ACCOUNTS).to_string();
        let to = ((t + 1) % ACCOUNTS).to_string();

        let bal_from = read_balance(&eng, &from);
        if bal_from < 100 {
            continue;
        }
        let bal_to = read_balance(&eng, &to);

        eng.execute("BEGIN").expect("begin");
        eng.put_row("accounts", &from, "balance", &(bal_from - 100).to_string().into_bytes())
            .expect("debit");
        eng.put_row(
            "accounts",
            &to,
            "balance",
            &(bal_to + 100).to_string().into_bytes(),
        )
        .expect("credit");
        eng.execute("COMMIT").expect("commit");
        ok += 1;
    }
    let elapsed = start.elapsed();
    let tps = f64::from(ok) / elapsed.as_secs_f64();

    println!(
        "oltp_mvcc: {ok}/{TXNS} transfers in {elapsed:?} ({tps:.0} txn/s, threads={threads})"
    );
    let _ = std::fs::remove_dir_all(dir);
}
