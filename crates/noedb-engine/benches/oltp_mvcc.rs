//! OLTP-style bank transfer benchmark (Phase 2 Week 14).

use std::time::Instant;

use noedb_engine::LocalEngine;

fn main() {
    let dir = std::env::temp_dir().join(format!(
        "noedb-oltp-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut eng = LocalEngine::open(&dir).expect("open engine");

    const ACCOUNTS: u32 = 100;
    const TXNS: u32 = 1_000;

    for i in 0..ACCOUNTS {
        eng.put_row("accounts", &i.to_string(), "balance", b"1000")
            .expect("seed");
    }

    let start = Instant::now();
    for t in 0..TXNS {
        let from = (t % ACCOUNTS).to_string();
        let to = ((t + 1) % ACCOUNTS).to_string();
        eng.execute("BEGIN").expect("begin");
        eng.put_row("accounts", &from, "balance", b"900").expect("debit");
        eng.put_row("accounts", &to, "balance", b"1100").expect("credit");
        eng.execute("COMMIT").expect("commit");
    }
    let elapsed = start.elapsed();
    let tps = f64::from(TXNS) / elapsed.as_secs_f64();

    println!("oltp_mvcc: {TXNS} transfers in {elapsed:?} ({tps:.0} txn/s)");
    let _ = std::fs::remove_dir_all(dir);
}
