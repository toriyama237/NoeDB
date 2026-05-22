//! MVCC survives engine restart (LSM + WAL).

use noedb_engine::LocalEngine;

#[test]
fn committed_version_survives_reopen() {
    let dir = std::env::temp_dir().join(format!(
        "noedb-durable-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    {
        let eng = LocalEngine::open(&dir).unwrap();
        eng.execute("BEGIN").unwrap();
        eng.put_row_default("t", "1", "v", b"first").unwrap();
        eng.execute("COMMIT").unwrap();
        eng.execute("BEGIN").unwrap();
        eng.put_row_default("t", "1", "v", b"second").unwrap();
        eng.execute("COMMIT").unwrap();
    }
    let eng = LocalEngine::open(&dir).unwrap();
    let rows = eng.execute("SELECT v FROM t").unwrap().rows;
    assert_eq!(rows[0][0], "second");
    let _ = std::fs::remove_dir_all(dir);
}
