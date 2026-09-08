//! The content-addressed chunk store.
//!
//! - `put` splits a byte stream into fixed-size chunks, writes each once
//!   (dedup by digest), and returns the list of chunk hashes plus the
//!   aggregate stats.
//! - `read` reassembles (or reads a single chunk) through the Atra hot
//!   cache: first read mmaps from disk, repeated reads are probe hits.
//! - `gc(keep)` deletes every chunk not named in `keep` — the caller
//!   derives `keep` from the live project's media roots.
//!
//! Everything is crash-safe by construction: chunks are immutable and
//! written to temp names before an atomic rename, so a half-written
//! chunk can never be observed under its final hash.

use atra::Engine;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Chunk size: small enough that one chunk is cheap to cache and mmap,
/// large enough that per-chunk overhead is noise (4 MiB).
pub const CHUNK_SIZE: usize = 4 * 1024 * 1024;
/// Bumped when the on-disk layout ever changes incompatibly.
pub const STORE_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AstraStats {
    pub chunks: u64,
    pub bytes: u64,
    pub writes: u64,
    pub dedup_hits: u64,
    pub bytes_deduped: u64,
    pub cache_items: u64,
    pub cache_hit_ratio: f64,
    pub reclaimed_chunks: u64,
    pub reclaimed_bytes: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PutReport {
    /// Chunk hashes in order — the caller stores these as the object's extent list.
    pub chunks: Vec<String>,
    /// Total logical bytes written through (pre-dedup).
    pub logical_bytes: u64,
    /// Bytes that were already present and skipped.
    pub deduped_bytes: u64,
}

pub struct Store {
    /// Store root (kept for diagnostics and future pack migration).
    pub root: PathBuf,
    chunks_dir: PathBuf,
    hot: Engine,
    writes: AtomicU64,
    dedup_hits: AtomicU64,
    deduped_bytes: AtomicU64,
    reclaimed_chunks: AtomicU64,
    reclaimed_bytes: AtomicU64,
}

impl Store {
    /// Open (creating if needed) a store rooted at `dir`.
    pub fn open(dir: impl AsRef<Path>) -> std::io::Result<Self> {
        let root = dir.as_ref().to_path_buf();
        let chunks_dir = root.join("chunks");
        fs::create_dir_all(&chunks_dir)?;
        Ok(Self {
            root,
            hot: Engine::new(atra::EngineOptions {
                shard_hint: None,
                total_entries: 4096,
                cores: None,
                // 64 MiB hot budget: enough recent chunks that decoding a
                // long file twice is mostly cache probes, not disk.
                memory_bytes: 64 * 1024 * 1024,
                min_buckets: 1024,
            }),
            chunks_dir,
            writes: AtomicU64::new(0),
            dedup_hits: AtomicU64::new(0),
            deduped_bytes: AtomicU64::new(0),
            reclaimed_chunks: AtomicU64::new(0),
            reclaimed_bytes: AtomicU64::new(0),
        })
    }

    fn chunk_path(&self, hash: &str) -> PathBuf {
        let short = &hash[..2.min(hash.len())];
        self.chunks_dir.join(short).join(hash)
    }

    /// Write one chunk if absent. Returns true when it was new.
    fn put_chunk(&self, bytes: &[u8]) -> std::io::Result<String> {
        let hash = digest_hex(bytes);
        let path = self.chunk_path(&hash);
        if path.is_file() {
            self.dedup_hits.fetch_add(1, Ordering::Relaxed);
            self.deduped_bytes
                .fetch_add(bytes.len() as u64, Ordering::Relaxed);
            return Ok(hash);
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        // Atomic publish: write to a temp name, then rename into place.
        let tmp = path.with_extension(format!("tmp{}", std::process::id()));
        {
            let mut file = File::create(&tmp)?;
            file.write_all(bytes)?;
            file.sync_all()?;
        }
        fs::rename(&tmp, &path)?;
        self.writes.fetch_add(1, Ordering::Relaxed);
        Ok(hash)
    }

    /// Stream `reader` into the store, chunk by chunk. The reader is read
    /// in CHUNK_SIZE steps, so imports never buffer the whole file.
    pub fn put_stream(&self, mut reader: impl Read) -> std::io::Result<PutReport> {
        let mut chunks = Vec::new();
        let mut logical = 0u64;
        let mut buf = vec![0u8; CHUNK_SIZE];
        loop {
            let mut filled = 0;
            while filled < buf.len() {
                match reader.read(&mut buf[filled..])? {
                    0 => break,
                    n => filled += n,
                }
            }
            if filled == 0 {
                break;
            }
            logical += filled as u64;
            chunks.push(self.put_chunk(&buf[..filled])?);
        }
        self.hot.sweep();
        Ok(PutReport {
            chunks,
            logical_bytes: logical,
            deduped_bytes: self.deduped_bytes.load(Ordering::Relaxed),
        })
    }

    /// Convenience for in-memory bytes.
    pub fn put_bytes(&self, bytes: &[u8]) -> std::io::Result<PutReport> {
        self.put_stream(bytes)
    }

    /// Read a single chunk through the hot cache. `None` when absent.
    pub fn read_chunk(&self, hash: &str) -> Option<Vec<u8>> {
        if hash.len() != 64 {
            return None;
        }
        let key = hash.as_bytes();
        if let Some(cached) = self.hot.get(key) {
            return Some(cached);
        }
        let bytes = fs::read(self.chunk_path(hash)).ok()?;
        self.hot.set(key, &bytes);
        self.hot.sweep();
        Some(bytes)
    }

    /// Total logical size of a chunk extent list.
    pub fn extent_len(&self, chunks: &[String]) -> u64 {
        chunks
            .iter()
            .filter_map(|h| fs::metadata(self.chunk_path(h)).ok())
            .map(|m| m.len())
            .sum()
    }

    /// Whether every chunk of the extent is present and intact.
    pub fn verify(&self, chunks: &[String]) -> bool {
        chunks.iter().all(|h| {
            self.read_chunk(h)
                .is_some_and(|bytes| digest_hex(&bytes) == *h)
        })
    }

    /// Delete every chunk not named in `keep`. Returns reclaimed stats.
    pub fn gc(&self, keep: &[String]) -> AstraStats {
        let keep_set: std::collections::HashSet<&str> = keep.iter().map(|s| s.as_str()).collect();
        if let Ok(entries) = fs::read_dir(&self.chunks_dir) {
            for shard in entries.filter_map(|e| e.ok()) {
                let shard_path = shard.path();
                if !shard_path.is_dir() {
                    continue;
                }
                if let Ok(files) = fs::read_dir(&shard_path) {
                    for file in files.filter_map(|e| e.ok()) {
                        let path = file.path();
                        let name = path.file_name().and_then(|n| n.to_str());
                        let Some(name) = name else {
                            continue;
                        };
                        if name.ends_with(".tmp") || keep_set.contains(&name) {
                            continue;
                        }
                        let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                        if fs::remove_file(&path).is_ok() {
                            self.reclaimed_chunks.fetch_add(1, Ordering::Relaxed);
                            self.reclaimed_bytes.fetch_add(size, Ordering::Relaxed);
                        }
                    }
                }
            }
        }
        self.hot.flush();
        self.stats()
    }

    /// Aggregate observability.
    pub fn stats(&self) -> AstraStats {
        let mut chunks = 0u64;
        let mut bytes = 0u64;
        if let Ok(shards) = fs::read_dir(&self.chunks_dir) {
            for shard in shards.filter_map(|e| e.ok()) {
                if let Ok(files) = fs::read_dir(shard.path()) {
                    for file in files.filter_map(|e| e.ok()) {
                        let meta = match file.metadata() {
                            Ok(m) if m.is_file() => m,
                            _ => continue,
                        };
                        if file
                            .path()
                            .extension()
                            .is_some_and(|e| e.to_string_lossy().starts_with("tmp"))
                        {
                            continue;
                        }
                        chunks += 1;
                        bytes += meta.len();
                    }
                }
            }
        }
        let hot_stats = self.hot.stats();
        AstraStats {
            chunks,
            bytes,
            writes: self.writes.load(Ordering::Relaxed),
            dedup_hits: self.dedup_hits.load(Ordering::Relaxed),
            bytes_deduped: self.deduped_bytes.load(Ordering::Relaxed),
            cache_items: hot_stats.items,
            cache_hit_ratio: hot_stats.hit_ratio,
            reclaimed_chunks: self.reclaimed_chunks.load(Ordering::Relaxed),
            reclaimed_bytes: self.reclaimed_bytes.load(Ordering::Relaxed),
        }
    }

    /// Drop the hot cache contents (nothing on disk changes).
    pub fn flush_cache(&self) {
        self.hot.flush();
    }
}

static GLOBAL: std::sync::OnceLock<Store> = std::sync::OnceLock::new();

impl Store {
    /// The process-global store: pid-scoped temp dir, so it lives and dies
    /// with the session. Every import/decode path shares it.
    pub fn global() -> &'static Store {
        GLOBAL.get_or_init(|| {
            let dir = std::env::temp_dir().join(format!("bonaparte-astra-{}", std::process::id()));
            Store::open(dir).expect("Astra global store must open")
        })
    }
}

pub fn digest_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let out = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for b in out {
        hex.push_str(&format!("{b:02x}"));
    }
    hex
}
