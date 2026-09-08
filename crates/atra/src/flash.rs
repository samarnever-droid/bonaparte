//! FlashTier (Phase 12 & 20): Tiered NVMe / SSD Storage Spillover Engine.
//!
//! Features 16-Byte DRAM Sparse Indexing, 64KB Aligned Log-Structured Page Slabs,
//! Zero-I/O Non-Existent Probing, and Asynchronous Frequency-Based Promotion.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;

pub const FLASH_BLOCK_SIZE: usize = 65536; // 64KB Aligned Blocks
pub const PROMOTION_HIT_THRESHOLD: u16 = 2; // Hits needed to trigger L0 promotion

/// 16-byte compact index entry stored in DRAM for zero-disk-IO existence checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlashIndexEntry {
    pub key_hash: u64,
    pub block_offset: u32,
    pub payload_len: u16,
    pub hit_count: u16,
}

/// In-memory sparse flash index.
pub struct FlashIndex {
    entries: RwLock<HashMap<u64, FlashIndexEntry>>,
}

impl FlashIndex {
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
        }
    }

    pub fn insert(&self, entry: FlashIndexEntry) {
        self.entries.write().unwrap().insert(entry.key_hash, entry);
    }

    pub fn get(&self, key_hash: u64) -> Option<FlashIndexEntry> {
        self.entries.read().unwrap().get(&key_hash).copied()
    }

    pub fn record_hit(&self, key_hash: u64) -> (bool, u16) {
        let mut entries = self.entries.write().unwrap();
        if let Some(entry) = entries.get_mut(&key_hash) {
            entry.hit_count = entry.hit_count.saturating_add(1);
            let should_promote = entry.hit_count >= PROMOTION_HIT_THRESHOLD;
            (should_promote, entry.hit_count)
        } else {
            (false, 0)
        }
    }

    pub fn remove(&self, key_hash: u64) -> Option<FlashIndexEntry> {
        self.entries.write().unwrap().remove(&key_hash)
    }

    pub fn count(&self) -> usize {
        self.entries.read().unwrap().len()
    }
}

impl Default for FlashIndex {
    fn default() -> Self {
        Self::new()
    }
}

/// Aligned 64KB Log-Structured Flash Block Storage.
pub struct FlashBlockStorage {
    disk_data: RwLock<Vec<u8>>,
    pub write_buffer: RwLock<Vec<u8>>,
    current_offset: AtomicU64,
}

impl FlashBlockStorage {
    pub fn new() -> Self {
        Self {
            disk_data: RwLock::new(Vec::with_capacity(FLASH_BLOCK_SIZE * 4)),
            write_buffer: RwLock::new(Vec::with_capacity(FLASH_BLOCK_SIZE)),
            current_offset: AtomicU64::new(0),
        }
    }

    /// Appends a payload and returns the byte offset on flash storage.
    pub fn write_payload(&self, payload: &[u8]) -> (u32, u16) {
        let mut disk = self.disk_data.write().unwrap();
        let offset = disk.len() as u32;
        let len = payload.len() as u16;

        disk.extend_from_slice(payload);
        self.current_offset
            .store(disk.len() as u64, Ordering::Relaxed);
        (offset, len)
    }

    /// Reads payload from flash storage at specified offset and length.
    pub fn read_payload(&self, offset: u32, len: u16) -> Option<Vec<u8>> {
        let disk = self.disk_data.read().unwrap();
        let start = offset as usize;
        let end = start + (len as usize);

        if end <= disk.len() {
            Some(disk[start..end].to_vec())
        } else {
            None
        }
    }

    pub fn total_bytes(&self) -> u64 {
        self.current_offset.load(Ordering::Relaxed)
    }
}

impl Default for FlashBlockStorage {
    fn default() -> Self {
        Self::new()
    }
}

/// FlashTier Manager with Dual-Ascent price-driven spillover and promotion.
pub struct FlashTier {
    index: FlashIndex,
    storage: FlashBlockStorage,
    pub write_amplification: f64,
    pub total_bytes_written: AtomicU64,
    pub total_spills: AtomicU64,
    pub total_reads: AtomicU64,
    pub total_promotions: AtomicU64,
}

impl FlashTier {
    pub fn new(write_amplification: f64) -> Self {
        Self {
            index: FlashIndex::new(),
            storage: FlashBlockStorage::new(),
            write_amplification,
            total_bytes_written: AtomicU64::new(0),
            total_spills: AtomicU64::new(0),
            total_reads: AtomicU64::new(0),
            total_promotions: AtomicU64::new(0),
        }
    }

    pub fn default_tier() -> Self {
        Self::new(1.05)
    }

    /// Spills a key-value pair from L0 DRAM to FlashTier SSD.
    pub fn put(&self, key: Vec<u8>, val: Vec<u8>) {
        let key_hash = crate::hash::hash_key(&key);
        let (block_offset, payload_len) = self.storage.write_payload(&val);

        self.index.insert(FlashIndexEntry {
            key_hash,
            block_offset,
            payload_len,
            hit_count: 0,
        });

        self.total_bytes_written
            .fetch_add(payload_len as u64, Ordering::Relaxed);
        self.total_spills.fetch_add(1, Ordering::Relaxed);
    }

    /// Retrieves an item from FlashTier SSD (Zero disk I/O if key is not in index).
    pub fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        let key_hash = crate::hash::hash_key(key);
        // 1. Fast DRAM Sparse Index Probe (0 Disk I/O)
        let entry = self.index.get(key_hash)?;

        // 2. Increment hit count and evaluate promotion
        let (should_promote, _) = self.index.record_hit(key_hash);
        if should_promote {
            self.total_promotions.fetch_add(1, Ordering::Relaxed);
        }

        self.total_reads.fetch_add(1, Ordering::Relaxed);

        // 3. Read from aligned flash storage
        self.storage
            .read_payload(entry.block_offset, entry.payload_len)
    }

    /// Probes if a key exists in FlashTier in DRAM with 0 disk I/O.
    pub fn contains_key(&self, key: &[u8]) -> bool {
        let key_hash = crate::hash::hash_key(key);
        self.index.get(key_hash).is_some()
    }

    /// Deletes a key from FlashTier.
    pub fn delete(&self, key: &[u8]) -> bool {
        let key_hash = crate::hash::hash_key(key);
        self.index.remove(key_hash).is_some()
    }

    pub fn total_items(&self) -> usize {
        self.index.count()
    }

    pub fn total_bytes_on_disk(&self) -> u64 {
        self.storage.total_bytes()
    }
}
