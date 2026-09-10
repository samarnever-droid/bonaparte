//! File-backed footage: full-framerate frames decoded on demand.
//!
//! A [`LiveVideo`] is the runtime companion of `MediaAsset::footage`: the
//! document stores only a path and metadata, while frames stream in from
//! FFmpeg as the playhead asks for them. A byte-capped LRU keeps recently
//! shown frames hot (scrubbing costs at most one decode per new source
//! frame), and a single-slot background prefetch hides decode latency behind
//! playback for clips whose framerate matches the comp.
//!
//! Nothing here may panic at playback time: a failed or missing decode
//! surfaces as `None`, which the engine already renders as unavailable
//! media, and the media panel reports as an offline asset (relinkable).

use bonaparte_effects::CpuFrame;
use bonaparte_media::decode_frame;
use bonaparte_model::{FrameRate, Time, TICKS_PER_SEC};
use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// A generated low-resolution transcode standing in for the source during
/// interactive playback. Export always ignores it and decodes the original.
#[derive(Debug, Clone)]
pub struct ProxyClip {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
}

/// One live video asset and its frame cache.
#[derive(Debug)]
pub struct LiveVideo {
    pub source: PathBuf,
    pub proxy: Option<ProxyClip>,
    pub width: u32,
    pub height: u32,
    pub frame_rate: FrameRate,
    /// Total frames in the clip; sample positions wrap within it so footage
    /// loops exactly like embedded video tracks do.
    pub frame_count: i64,
    cache: Mutex<Lru>,
    prefetch_queued: AtomicBool,
}

impl LiveVideo {
    pub fn new(
        source: PathBuf,
        proxy: Option<ProxyClip>,
        width: u32,
        height: u32,
        frame_rate: FrameRate,
        duration: Time,
    ) -> Self {
        let rate = frame_rate.num.max(1) as f64 / frame_rate.den.max(1) as f64;
        let frame_count = ((duration.0.max(1) as f64 / TICKS_PER_SEC as f64) * rate)
            .round()
            .max(1.0) as i64;
        Self {
            source,
            proxy,
            width,
            height,
            frame_rate,
            frame_count,
            cache: Mutex::new(Lru::default()),
            prefetch_queued: AtomicBool::new(false),
        }
    }

    /// Which source frame displays at `time` — integer math on the clip's
    /// own grid, so a comp running faster than the footage shows the same
    /// frame twice, and slower comps step by whole frames only.
    pub fn index_at(&self, time: Time) -> i64 {
        let ticks_per_frame = (TICKS_PER_SEC as i128 * self.frame_rate.den.max(1) as i128
            / self.frame_rate.num.max(1) as i128)
            .max(1);
        ((time.0.max(0) as i128 / ticks_per_frame) as i64).rem_euclid(self.frame_count)
    }

    /// The frame at `index`, decoded on miss. `allow_proxy` lets interactive
    /// playback use the half-resolution transcode; export clears the flag.
    pub fn frame(self: &Arc<Self>, index: i64, allow_proxy: bool) -> Option<Arc<CpuFrame>> {
        let index = index.rem_euclid(self.frame_count);
        if let Ok(mut lru) = self.cache.lock() {
            if let Some(hit) = lru.get(index) {
                return Some(hit);
            }
        }
        let frame = self.decode(index, allow_proxy)?;
        if let Ok(mut lru) = self.cache.lock() {
            lru.insert(index, frame.clone(), CACHE_BUDGET_BYTES);
        }
        Some(frame)
    }

    /// Queue the next frame on a background thread while playback is still
    /// showing the current one. One flight at a time per asset; a late
    /// prefetch simply lands in the cache (or fails silently).
    pub fn warm(self: &Arc<Self>, next_index: i64, allow_proxy: bool) {
        if self
            .prefetch_queued
            .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
        {
            return;
        }
        let me = Arc::clone(self);
        std::thread::Builder::new()
            .name("footage-prefetch".into())
            .spawn(move || {
                let index = next_index.rem_euclid(me.frame_count);
                let hit = me
                    .cache
                    .lock()
                    .ok()
                    .is_some_and(|lru| lru.hot.contains_key(&index));
                if !hit {
                    if let Some(frame) = me.decode(index, allow_proxy) {
                        if let Ok(mut lru) = me.cache.lock() {
                            lru.insert(index, frame, CACHE_BUDGET_BYTES);
                        }
                    }
                }
                me.prefetch_queued.store(false, Ordering::Relaxed);
            })
            .ok();
    }

    /// True when the underlying file is present. Cheap `stat`, used by the
    /// media panel's offline badge and relink affordance.
    pub fn online(&self) -> bool {
        self.source.exists()
    }

    /// Whether two handles read the same clip with the same proxy — so a
    /// document refresh (every commit) keeps the warm frame cache instead of
    /// re-decoding. Any path, geometry, or rate change rebinds.
    pub fn same_binding(&self, other: &LiveVideo) -> bool {
        let proxy_same = match (&self.proxy, &other.proxy) {
            (None, None) => true,
            (Some(a), Some(b)) => a.path == b.path && a.width == b.width && a.height == b.height,
            _ => false,
        };
        self.source == other.source
            && proxy_same
            && self.width == other.width
            && self.height == other.height
            && self.frame_rate == other.frame_rate
            && self.frame_count == other.frame_count
    }

    fn decode(&self, index: i64, allow_proxy: bool) -> Option<Arc<CpuFrame>> {
        let (path, w, h) = match (allow_proxy, &self.proxy) {
            (true, Some(p)) if p.path.exists() => (&p.path, p.width, p.height),
            _ => (&self.source, self.width, self.height),
        };
        let secs =
            index as f64 * self.frame_rate.den.max(1) as f64 / self.frame_rate.num.max(1) as f64;
        let frame = decode_frame(path, Time::from_secs_f64(secs), w, h).ok()?;
        Some(Arc::new(CpuFrame {
            width: frame.width,
            height: frame.height,
            rgba: frame.rgba,
        }))
    }
}

/// Decode budget kept per asset. A 1080p frame is 8 MiB, so this holds about
/// a dozen frames — enough for smooth playback while scrubbing.
const CACHE_BUDGET_BYTES: usize = 96 * 1024 * 1024;

#[derive(Debug, Default)]
struct Lru {
    hot: BTreeMap<i64, Arc<CpuFrame>>,
    /// Oldest first; popped when the byte budget overflows.
    order: VecDeque<i64>,
    bytes: usize,
}

impl Lru {
    fn get(&mut self, index: i64) -> Option<Arc<CpuFrame>> {
        let frame = self.hot.get(&index).cloned()?;
        self.touch(index);
        Some(frame)
    }

    fn insert(&mut self, index: i64, frame: Arc<CpuFrame>, budget: usize) {
        let size = frame.rgba.len();
        if size > budget {
            // A single frame bigger than the whole budget is still worth
            // caching exactly one copy of (4K exports on tiny machines).
            self.hot.clear();
            self.order.clear();
            self.bytes = 0;
        }
        if self.hot.insert(index, frame).is_some() {
            self.touch(index);
            return;
        }
        self.bytes += size;
        self.order.push_back(index);
        while self.bytes > budget {
            match self.order.pop_front() {
                Some(old) => {
                    if let Some(f) = self.hot.remove(&old) {
                        self.bytes -= f.rgba.len();
                    }
                }
                None => break,
            }
        }
    }

    fn touch(&mut self, index: i64) {
        if let Some(pos) = self.order.iter().position(|i| *i == index) {
            self.order.remove(pos);
            self.order.push_back(index);
        } else {
            self.order.push_back(index);
        }
    }
}
