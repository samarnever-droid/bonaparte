//! Astra engine: chunking, dedup, integrity, GC, hot-cache behaviour.

use astra::{Store, CHUNK_SIZE};

fn temp_store(tag: &str) -> Store {
    let dir = std::env::temp_dir().join(format!(
        "astra-test-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    Store::open(dir).unwrap()
}

#[test]
fn roundtrip_and_integrity() {
    let store = temp_store("roundtrip");
    let report = store.put_bytes(b"hello astra").unwrap();
    assert_eq!(report.chunks.len(), 1);
    assert_eq!(report.logical_bytes, 11);
    assert!(store.verify(&report.chunks));
    assert_eq!(store.read_chunk(&report.chunks[0]).unwrap(), b"hello astra");
    // A hash that was never written must never verify.
    let other = format!("{:064}", "0");
    assert!(!store.verify(&[other]));
}

#[test]
fn big_streams_chunk_and_read_back() {
    let store = temp_store("big");
    let blob: Vec<u8> = (0..CHUNK_SIZE * 2 + 1234)
        .map(|i| (i % 251) as u8)
        .collect();
    let report = store.put_bytes(&blob).unwrap();
    assert_eq!(report.chunks.len(), 3);
    assert_eq!(report.logical_bytes, blob.len() as u64);
    assert!(store.verify(&report.chunks));
    assert_eq!(store.extent_len(&report.chunks), blob.len() as u64);
    let mut reassembled = Vec::new();
    for chunk in &report.chunks {
        reassembled.extend_from_slice(&store.read_chunk(chunk).unwrap());
    }
    assert_eq!(reassembled, blob);
}

#[test]
fn identical_content_dedups_to_one_chunk() {
    let store = temp_store("dedup");
    let blob = vec![42u8; CHUNK_SIZE];
    let first = store.put_bytes(&blob).unwrap();
    let second = store.put_bytes(&blob).unwrap();
    assert_eq!(first.chunks, second.chunks);
    let stats = store.stats();
    assert!(stats.dedup_hits >= 1);
    assert!(stats.bytes_deduped >= CHUNK_SIZE as u64);
    assert_eq!(stats.chunks, 1);
}

#[test]
fn gc_reclaims_unreferenced_chunks() {
    let store = temp_store("gc");
    let keep = store.put_bytes(b"keep me").unwrap();
    let drop_me = store.put_bytes(b"drop me").unwrap();
    assert!(store.read_chunk(&drop_me.chunks[0]).is_some());
    store.gc(&keep.chunks);
    assert!(store.read_chunk(&keep.chunks[0]).is_some());
    assert!(store.read_chunk(&drop_me.chunks[0]).is_none());
    let stats = store.stats();
    assert!(stats.reclaimed_chunks >= 1);
    assert!(stats.reclaimed_bytes >= 7);
}

#[test]
fn repeat_reads_hit_the_hot_cache() {
    let store = temp_store("hot");
    let report = store.put_bytes(b"cache me please").unwrap();
    let hash = report.chunks[0].clone();
    store.read_chunk(&hash).unwrap(); // warm the Atra tier
    for _ in 0..5 {
        store.read_chunk(&hash).unwrap();
    }
    let stats = store.stats();
    assert!(stats.cache_hit_ratio > 0.5, "re-reads must be cache hits");
}
