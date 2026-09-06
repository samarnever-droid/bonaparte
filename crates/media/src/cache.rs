//! Two-tier LRU RAM (capped) + disk-backed frame store for playback & rendering.
//!
//! Architectural guarantees:
//! - Flat RAM consumption: RAM slot count is strictly bounded by max_ram_slots.
//! - No memory leaks across long playback loops or random timeline scrubs.
//! - Seamless fallback: frames not in Tier 1 RAM are loaded on-demand from Tier 2 disk.
//! - Implements onaparte_engine::reference::MediaFrames so the compositor can render
//!   footage layers without touching the filesystem directly.

use bonaparte_engine::reference::{FrameView, MediaFrames};
use bonaparte_model::{MediaId, Time};
use std::cell::UnsafeCell;
use std::collections::{HashMap, VecDeque};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

struct FrameSlot {
    key: Option<(MediaId, i64)>,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl FrameSlot {
    fn new() -> Self {
        Self {
            key: None,
            width: 0,
            height: 0,
            rgba: Vec::new(),
        }
    }
}

struct LruState {
    /// Map from (MediaId, tick) to slot index in slots
    map: HashMap<(MediaId, i64), usize>,
    /// Order of slot indices from most recently used (front) to least recently used (back)
    lru: VecDeque<usize>,
    next_unallocated: usize,
}

/// Two-tier playback cache with strictly capped RAM and disk spillover.
pub struct DiskPlaybackCache {
    cache_dir: PathBuf,
    capacity: usize,
    slots: Vec<UnsafeCell<FrameSlot>>,
    tracker: Mutex<LruState>,
}

unsafe impl Send for DiskPlaybackCache {}
unsafe impl Sync for DiskPlaybackCache {}

impl DiskPlaybackCache {
    /// Create a new cache with a specified directory and max RAM frame slots.
    pub fn new(cache_dir: impl AsRef<Path>, ram_capacity: usize) -> std::io::Result<Self> {
        let dir = cache_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&dir)?;

        let capacity = ram_capacity.max(1);
        let mut slots = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            slots.push(UnsafeCell::new(FrameSlot::new()));
        }

        Ok(Self {
            cache_dir: dir,
            capacity,
            slots,
            tracker: Mutex::new(LruState {
                map: HashMap::with_capacity(capacity),
                lru: VecDeque::with_capacity(capacity),
                next_unallocated: 0,
            }),
        })
    }

    /// Create a cache inside a freshly created temporary directory.
    pub fn with_temp_dir(ram_capacity: usize) -> std::io::Result<Self> {
        let temp_base = std::env::temp_dir();
        let unique_dir = temp_base.join(format!(
            "bonaparte_cache_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        Self::new(unique_dir, ram_capacity)
    }

    /// Root directory of the disk cache tier.
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// Maximum number of uncompressed frames kept in memory.
    pub fn ram_capacity(&self) -> usize {
        self.capacity
    }

    /// Number of frames currently held in RAM slots.
    pub fn ram_count(&self) -> usize {
        let tracker = self.tracker.lock().unwrap();
        tracker.map.len()
    }

    /// Check if a frame is currently cached in Tier 1 RAM.
    pub fn contains_ram(&self, media: MediaId, time: Time) -> bool {
        let tracker = self.tracker.lock().unwrap();
        tracker.map.contains_key(&(media, time.0))
    }

    /// Check if a frame is persisted to Tier 2 disk.
    pub fn contains_disk(&self, media: MediaId, time: Time) -> bool {
        self.frame_disk_path(media, time).is_file()
    }

    /// Total number of frame files persisted on disk in this cache.
    pub fn disk_count(&self) -> usize {
        if let Ok(entries) = std::fs::read_dir(&self.cache_dir) {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().is_some_and(|ext| ext == "raw"))
                .count()
        } else {
            0
        }
    }

    fn frame_disk_path(&self, media: MediaId, time: Time) -> PathBuf {
        self.cache_dir.join(format!("{}_{}.raw", media.0, time.0))
    }

    /// Insert a decoded frame into the two-tier cache (written to disk and loaded into RAM).
    pub fn insert_frame(
        &self,
        media: MediaId,
        time: Time,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> std::io::Result<()> {
        // 1. Write to disk store (Tier 2)
        let disk_path = self.frame_disk_path(media, time);
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&disk_path)?;

        file.write_all(&width.to_le_bytes())?;
        file.write_all(&height.to_le_bytes())?;
        file.write_all(rgba)?;
        file.flush()?;

        // 2. Put into RAM store (Tier 1)
        self.put_into_ram(media, time, width, height, rgba);

        Ok(())
    }

    fn put_into_ram(
        &self,
        media: MediaId,
        time: Time,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> usize {
        let mut tracker = self.tracker.lock().unwrap();
        let key = (media, time.0);

        let slot_idx = if let Some(&idx) = tracker.map.get(&key) {
            // Already in slot, update LRU position
            if let Some(pos) = tracker.lru.iter().position(|&x| x == idx) {
                tracker.lru.remove(pos);
            }
            tracker.lru.push_front(idx);
            idx
        } else if tracker.next_unallocated < self.capacity {
            let idx = tracker.next_unallocated;
            tracker.next_unallocated += 1;
            tracker.map.insert(key, idx);
            tracker.lru.push_front(idx);
            idx
        } else {
            // Evict least recently used slot from RAM
            let evict_idx = tracker.lru.pop_back().unwrap();
            let old_key = unsafe { (*self.slots[evict_idx].get()).key };
            if let Some(ok) = old_key {
                tracker.map.remove(&ok);
            }
            tracker.map.insert(key, evict_idx);
            tracker.lru.push_front(evict_idx);
            evict_idx
        };

        unsafe {
            let slot = &mut *self.slots[slot_idx].get();
            slot.key = Some(key);
            slot.width = width;
            slot.height = height;
            slot.rgba.clear();
            slot.rgba.extend_from_slice(rgba);
        }

        slot_idx
    }

    /// Retrieve a frame view from RAM or on-demand disk read.
    pub fn get_frame_view(&self, media: MediaId, time: Time) -> Option<FrameView<'_>> {
        let key = (media, time.0);

        // Fast path: already in RAM
        {
            let mut tracker = self.tracker.lock().unwrap();
            if let Some(&slot_idx) = tracker.map.get(&key) {
                if let Some(pos) = tracker.lru.iter().position(|&x| x == slot_idx) {
                    tracker.lru.remove(pos);
                }
                tracker.lru.push_front(slot_idx);

                let slot = unsafe { &*self.slots[slot_idx].get() };
                return Some(FrameView {
                    width: slot.width,
                    height: slot.height,
                    rgba: &slot.rgba,
                });
            }
        }

        // Slow path: check disk and load into a RAM slot
        let disk_path = self.frame_disk_path(media, time);
        if !disk_path.is_file() {
            return None;
        }

        let mut file = File::open(&disk_path).ok()?;
        let mut header = [0u8; 8];
        file.read_exact(&mut header).ok()?;

        let width = u32::from_le_bytes(header[0..4].try_into().unwrap());
        let height = u32::from_le_bytes(header[4..8].try_into().unwrap());
        let expected_bytes = (width * height * 4) as usize;

        let mut rgba = vec![0u8; expected_bytes];
        file.read_exact(&mut rgba).ok()?;

        let slot_idx = self.put_into_ram(media, time, width, height, &rgba);
        let slot = unsafe { &*self.slots[slot_idx].get() };

        Some(FrameView {
            width: slot.width,
            height: slot.height,
            rgba: &slot.rgba,
        })
    }

    /// Clear all RAM slots and disk cache files.
    pub fn clear(&self) -> std::io::Result<()> {
        let mut tracker = self.tracker.lock().unwrap();
        tracker.map.clear();
        tracker.lru.clear();
        tracker.next_unallocated = 0;

        for slot in &self.slots {
            unsafe {
                let s = &mut *slot.get();
                s.key = None;
                s.rgba.clear();
            }
        }

        if self.cache_dir.exists() {
            for entry in std::fs::read_dir(&self.cache_dir)? {
                let entry = entry?;
                if entry.path().extension().is_some_and(|ext| ext == "raw") {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }

        Ok(())
    }
}

impl Drop for DiskPlaybackCache {
    fn drop(&mut self) {
        // If temporary directory, attempt cleanup
        let dir_str = self.cache_dir.to_string_lossy();
        if dir_str.contains("bonaparte_cache_") {
            let _ = std::fs::remove_dir_all(&self.cache_dir);
        }
    }
}

impl MediaFrames for DiskPlaybackCache {
    fn frame_rgba(&self, media: MediaId, time: Time) -> Option<FrameView<'_>> {
        self.get_frame_view(media, time)
    }
}

impl MediaFrames for &DiskPlaybackCache {
    fn frame_rgba(&self, media: MediaId, time: Time) -> Option<FrameView<'_>> {
        (*self).get_frame_view(media, time)
    }
}
