//! LZ4 SST blocks + XOR filter round-trip (Phase 3 Weeks 19–20).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use noedb_storage::{
    compress::{decompress_block, maybe_compress_block, BLOCK_LZ4},
    MemTable, SstReader, SstWriter, XorFilter,
};

#[test]
fn lz4_block_roundtrip() {
    let raw = b"hello world hello world hello world".repeat(20);
    let stored = maybe_compress_block(&raw);
    assert_eq!(stored[0], BLOCK_LZ4);
    assert!(stored.len() < raw.len());
    let back = decompress_block(&stored).unwrap();
    assert_eq!(back, raw);
}

#[test]
fn xor_filter_may_contain_small_set() {
    let keys: Vec<Vec<u8>> = (0..32).map(|i| format!("k{i}").into_bytes()).collect();
    let f = XorFilter::build(&keys, 32);
    for k in &keys {
        assert!(f.may_contain(k));
    }
}

#[test]
fn sst_v2_write_read() {
    let dir = std::env::temp_dir().join(format!(
        "noedb-sst-v2-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("t.sst");
    let mut mt = MemTable::new();
    mt.put(b"a", b"1").unwrap();
    mt.put(b"b", b"2").unwrap();
    SstWriter::write_from_memtable(&path, &mt).unwrap();
    let reader = SstReader::open(&path).unwrap();
    let mut scan = reader.scan().unwrap();
    let mut out = Vec::new();
    while let Some(Ok((k, v))) = scan.next() {
        out.push((k, v));
    }
    assert_eq!(out.len(), 2);
    let _ = std::fs::remove_dir_all(dir);
}
