//! The sharded engine: seqlock bucket table, TTL, eviction, sweeping.
//!
//! Read path (§3): hash → shard → bounded probe window of buckets →
//! per-bucket seqlock validation → byte-exact key compare in the cell.
//! No locks, no RMW on the read path; a reader that loses the seqlock race
//! 64 times in a row falls back to a locked lookup so progress is guaranteed.
//!
//! Write path: one mutex per shard serializes writers; every bucket mutation
//! is wrapped in odd/even version bumps so concurrent readers detect it.
//!
//! Reclamation: epoch-based (Phase 3). Unlinked cells are retired with the
//! current epoch tag and freed by the sweeper once the collector barrier
//! proves no pinned reader can still touch them — see `epoch.rs`. Zero-copy
//! `ValueRef` handles hold a pin, so their cells cannot be reclaimed while
//! the handle lives.

use std::collections::{HashMap, VecDeque};
use std::ptr::NonNull;
use std::sync::atomic::{compiler_fence, AtomicU64, Ordering};
use std::sync::{Mutex, RwLock};
use std::time::{Duration, Instant};

use crate::hash::hash_key;
use crate::shard_count::{derived_shard_count, MIN_SHARD_BYTES};
use crate::types::{match_bytes, CellData, Entry, ProbeLine, WAYS};

/// A lookup inspects at most this many buckets (§2.5/§3.3 probe bound).
pub const PROBE_LIMIT: usize = 4;
const SEQLOCK_RETRIES: usize = 64;

/// Timing wheel: 4096 slots × 32 ms ≈ 131 s horizon. Deadlines beyond the
/// horizon wrap and are reinserted on inspection (approximate wheel, v0).
const WHEEL_SLOTS: usize = 4096;
const WHEEL_TICK_MS: u64 = 32;
/// Work bound per sweep() call: slots of wheel advance, so maintenance
/// stays off the p99 tail.
const WHEEL_SLOTS_PER_CALL: u64 = 64;
/// Work-credit bound: max expirations unlinked per shard per sweep call.
/// Overflow is reinserted one slot ahead — a burst spreads across calls
/// instead of monopolizing the writer lock.
const WHEEL_BURST_CAP: usize = 1024;

pub struct EngineOptions {
    pub shard_hint: Option<usize>,
    pub total_entries: usize,
    pub cores: Option<usize>,
    pub memory_bytes: usize,
    pub min_buckets: usize,
}

impl Default for EngineOptions {
    fn default() -> Self {
        EngineOptions {
            shard_hint: None,
            total_entries: 1 << 20,
            cores: None,
            memory_bytes: 8 << 30,
            min_buckets: 64,
        }
    }
}

/// SLO declaration (§13.10 controller surface; stored, not yet enforced).
#[derive(Clone, Debug)]
pub struct Slo {
    pub class: String,
    pub freshness_p99_ms: u64,
    pub origin_qps_max: u64,
    pub latency_p99_us: u64,
    pub priority: u8,
}

#[derive(Default, Clone)]
pub struct SetOpts {
    pub ttl: Option<Duration>,
    pub nx: bool,
    pub xx: bool,
    pub keepttl: bool,
    pub get_old: bool,
}

#[derive(Debug)]
pub enum SetOutcome {
    /// Previous value when `get_old` was requested and one existed.
    Stored(Option<Vec<u8>>),
    NotStored,
}

#[derive(Debug)]
pub enum TtlStatus {
    Missing,
    Persistent,
    /// milliseconds remaining
    Expires(u64),
}

/// Zero-copy read handle. It holds an epoch pin for its lifetime: the cell
/// it points at cannot be reclaimed while the handle lives, however long
/// that is. Dropping the handle releases the pin on the owning thread —
/// `ValueRef` is `!Send` for that reason. Forgetting it via `mem::forget`
/// leaks the cell (a leak, never a use-after-free).
pub struct ValueRef {
    ptr: *const u8,
    len: usize,
    _not_send: std::marker::PhantomData<*mut u8>,
}

impl ValueRef {
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn as_slice(&self) -> &[u8] {
        // SAFETY: the held pin keeps the cell unreclaimed while this handle
        // exists; the bytes are immutable after publish.
        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }
}

impl Drop for ValueRef {
    fn drop(&mut self) {
        crate::epoch::unpin();
    }
}

impl std::ops::Deref for ValueRef {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        self.as_slice()
    }
}

pub struct EngineStats {
    pub shards: u64,
    pub items: u64,
    pub hits: u64,
    pub misses: u64,
    pub hit_ratio: f64,
    pub expired: u64,
    pub evictions: u64,
    pub sets: u64,
    pub dels: u64,
    pub retries: u64,
    pub uptime_ms: u64,
}

enum Find {
    Found(u64),
    Expired,
    NotFound,
}

struct WriterState {
    /// Insertion-ordered (== time-ordered) retirement queue; drained from
    /// the front, so the common case is one timestamp + one comparison.
    retired: VecDeque<(NonNull<CellData>, u64)>,
    /// Expiry wheel (§ Phase 3): slot (deadline / tick) & mask.
    wheel: Vec<Vec<WheelEntry>>,
    /// Absolute tick index the cursor has consumed.
    next_tick: u64,
}

/// A pending expiry. Identity is the cell pointer: if the entry was
/// overwritten or deleted, `entries[way].cell` no longer equals `cell` and
/// the descriptor is stale — skipped, never acted on. The pointer is safe
/// to deref when processed because retirement grants 500 ms of grace.
#[derive(Clone, Copy)]
struct WheelEntry {
    expire_at: u64,
    cell: u64,
    bucket: u32,
    way: u8,
}

fn wheel_push(w: &mut WriterState, expire_ms: u64, cell: u64, bucket: usize, way: usize) {
    // Ceil: the cursor then arrives at or after the deadline — firing is up
    // to one tick late (standard approximate-wheel jitter), never early, so
    // the not-yet-due reinsert below stays a rare clock-edge safety net.
    let slot = ((expire_ms + WHEEL_TICK_MS - 1) / WHEEL_TICK_MS) as usize & (WHEEL_SLOTS - 1);
    w.wheel[slot].push(WheelEntry {
        expire_at: expire_ms,
        cell,
        bucket: bucket as u32,
        way: way as u8,
    });
}

// CellData is plain owned data (Boxes + one atomic), so tracking its ownership
// across threads is sound; NonNull is !Send by default only because it is a
// raw-pointer wrapper.
unsafe impl Send for WriterState {}

#[derive(Default)]
struct Stats {
    hits: AtomicU64,
    misses: AtomicU64,
    expired: AtomicU64,
    evictions: AtomicU64,
    sets: AtomicU64,
    dels: AtomicU64,
    retries: AtomicU64,
    gc: AtomicU64,
    l0_hits: AtomicU64,
}

struct Shard {
    /// Probe lines: one 64-byte-aligned line per bucket (§3.10 dual arena).
    probe: Box<[ProbeLine]>,
    /// Flat entry array, 4 entries per line: `entries[b * WAYS + way]`.
    /// Cold — touched only on a tag match.
    entries: Box<[Entry]>,
    /// Parallel Capability Side-Planes (spec §4.2) indexed by `slot_idx = b * WAYS + way`
    #[allow(dead_code)]
    bucket_mask: usize,
    write: Mutex<WriterState>,
    stats: Stats,
    items: AtomicU64,
    /// L0 validation counter (§3.4): bumped by writers BEFORE any change
    /// becomes visible, so a cached generation is valid exactly while no
    /// write has touched this shard.
    mutations: AtomicU64,
}

impl Shard {
    #[inline]
    fn entry(&self, b: usize, way: usize) -> &Entry {
        &self.entries[b * WAYS + way]
    }
}

pub struct Engine {
    /// unique instance id — L0 slots are tagged with it so engines sharing
    /// a thread never cross-contaminate
    id: u64,
    shards: Box<[Shard]>,
    shard_mask: usize,
    slo: RwLock<HashMap<String, Slo>>,
    created: Instant,
}

fn now_ms() -> u64 {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

fn cost_log(len: usize) -> u8 {
    (usize::BITS - 1 - len.max(1).leading_zeros()) as u8
}

/// Minimal glob matcher for SCAN MATCH: `*` and `?`, no escapes.
fn glob_match(pat: &[u8], text: &[u8]) -> bool {
    // iterative two-pointer with a single star backtracking point
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star, mut star_t) = (usize::MAX, 0usize);
    while ti < text.len() {
        if pi < pat.len() && (pat[pi] == b'?' || pat[pi] == text[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < pat.len() && pat[pi] == b'*' {
            star = pi;
            star_t = ti;
            pi += 1;
        } else if star != usize::MAX {
            pi = star + 1;
            ti = star_t + 1;
            star_t = ti;
        } else {
            return false;
        }
    }
    while pi < pat.len() && pat[pi] == b'*' {
        pi += 1;
    }
    pi == pat.len()
}

fn alloc_cell(key: &[u8], val: &[u8], expire_at: u64) -> u64 {
    Box::into_raw(Box::new(CellData::new(key, val, expire_at))) as u64
}

unsafe fn cell<'a>(p: u64) -> &'a CellData {
    &*(p as *const CellData)
}

impl Shard {
    fn begin(&self, b: usize) -> u32 {
        let v = self.probe[b].version.load(Ordering::Relaxed);
        self.probe[b].version.store(v + 1, Ordering::Relaxed);
        v
    }

    fn end(&self, b: usize, v: u32) {
        self.probe[b].version.store(v + 2, Ordering::Release);
    }

    fn bump_overflow(&self, home: usize, delta: i32) {
        let cur = self.probe[home].overflow_out.load(Ordering::Relaxed) as i32;
        let next = (cur + delta).max(0) as u32;
        self.probe[home].overflow_out.store(next, Ordering::Relaxed);
    }

    /// Free retired cells tagged below `barrier` (epoch-provably
    /// unreachable). Called from sweep with the freshly computed barrier.
    /// Bounded by the queue-length snapshot; each iteration frees one cell.
    fn drain_retired(&self, w: &mut WriterState, barrier: u64) {
        let len = w.retired.len();
        for _ in 0..len {
            let Some(&(p, tag)) = w.retired.front() else {
                break;
            };
            if tag >= barrier {
                break;
            }
            w.retired.pop_front();
            unsafe { drop(Box::from_raw(p.as_ptr())) };
            self.stats.gc.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Direct scan used by writers (under the shard mutex) and by the
    /// seqlock fallback path. Tag pre-filter, then byte-exact key compare.
    /// Returns (cell, bucket, way, expired).
    fn locked_find(&self, key: &[u8], kh: u32, b0: usize) -> Option<(u64, usize, usize, bool)> {
        let tag = (kh >> 24) as u8;
        for p in 0..PROBE_LIMIT {
            let b = (b0 + p) & self.bucket_mask;
            let bucket = &self.probe[b];
            for way in 0..WAYS {
                let c = self.entry(b, way).cell.load(Ordering::Relaxed);
                if c == 0 || bucket.tag(way) != tag {
                    continue;
                }
                let cd = unsafe { cell(c) };
                if cd.key() == key {
                    let exp = cd.expire_at.load(Ordering::Relaxed);
                    return Some((c, b, way, exp != 0 && now_ms() >= exp));
                }
            }
        }
        None
    }

    /// Bounded by locked_find (PROBE_LIMIT × WAYS).
    fn locked_lookup(&self, key: &[u8], kh: u32, b0: usize) -> Find {
        match self.locked_find(key, kh, b0) {
            Some((c, _, _, false)) => Find::Found(c),
            Some((_, _, _, true)) => Find::Expired,
            None => Find::NotFound,
        }
    }

    /// Lock-free lookup: one cache line per probe (version + tag words),
    /// entry lines touched only on a tag match, byte-exact key compare in
    /// the immutable cell. Zero RMW. A torn read retries the SAME bucket
    /// (the key lives in exactly one bucket — skipping would false-miss);
    /// after a bounded number of attempts the locked fallback guarantees
    /// progress, so a live key can never miss.
    /// Bound = PROBE_LIMIT × SEQLOCK_RETRIES × WAYS = 4 × 64 × 14.
    fn lookup(&self, key: &[u8], kh: u32, b0: usize) -> Find {
        let tag = (kh >> 24) as u8;
        for p in 0..PROBE_LIMIT {
            let b = (b0 + p) & self.bucket_mask;
            let bucket = &self.probe[b];
            'retry: for _ in 0..SEQLOCK_RETRIES {
                let v1 = bucket.version.load(Ordering::Acquire);
                if v1 & 1 == 0 {
                    let m0 = match_bytes(bucket.tags[0].load(Ordering::Relaxed), tag);
                    let m1 = match_bytes(bucket.tags[1].load(Ordering::Relaxed), tag);
                    // §3.3 pays for reads: if the home bucket never spilled,
                    // a validated miss here is a miss for the whole window —
                    // skip the remaining dependent line loads.
                    let home_clear = p == 0 && bucket.overflow_out.load(Ordering::Relaxed) == 0;
                    for w in 0..WAYS {
                        let m = if w < 8 { m0 } else { m1 };
                        if m & (0x80_u64 << (8 * (w & 7))) == 0 {
                            continue;
                        }
                        let c = self.entry(b, w).cell.load(Ordering::Relaxed);
                        if c == 0 {
                            continue; // stale tag from a deleted tenant
                        }
                        let cd = unsafe { cell(c) };
                        if cd.key() == key {
                            let exp = cd.expire_at.load(Ordering::Relaxed);
                            compiler_fence(Ordering::SeqCst);
                            if bucket.version.load(Ordering::Acquire) == v1 {
                                return if exp != 0 && now_ms() >= exp {
                                    Find::Expired
                                } else {
                                    Find::Found(c)
                                };
                            }
                            std::hint::spin_loop();
                            continue 'retry; // torn: retry THIS bucket
                        }
                    }
                    compiler_fence(Ordering::SeqCst);
                    if bucket.version.load(Ordering::Acquire) == v1 {
                        if home_clear {
                            return Find::NotFound;
                        }
                        break; // validated miss in this bucket → next probe
                    }
                }
                std::hint::spin_loop();
            }
            // bounded fallback: guaranteed progress on a contended bucket
            self.stats.retries.fetch_add(1, Ordering::Relaxed);
            let _w = self.write.lock().unwrap();
            return self.locked_lookup(key, kh, b0);
        }
        Find::NotFound
    }
}

impl Engine {
    pub fn new(opts: EngineOptions) -> Engine {
        let s = opts.shard_hint.unwrap_or_else(|| {
            let cores = opts.cores.unwrap_or_else(|| {
                std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(8)
            });
            derived_shard_count(cores, opts.memory_bytes, MIN_SHARD_BYTES)
        });
        assert!(s.is_power_of_two(), "shard count must be a power of two");
        // Frozen v1 hash layout (§3.1 12-bit shard field):
        //   bits  0..12  shard index      (S ≤ 4096, asserted here)
        //   bits 12..44  bucket index     (nb ≤ 2^32 per shard)
        //   bits 32..64  key_hash32; tag = bits 56..64 (top byte)
        assert!(s <= 4096, "shard field is 12 bits: S must be ≤ 4096");
        let per_shard = (opts.total_entries / s).max(WAYS);
        let nb = (per_shard / WAYS)
            .next_power_of_two()
            .clamp(opts.min_buckets.max(1), 1 << 20);
        let shards: Vec<Shard> = (0..s)
            .map(|_| Shard {
                probe: (0..nb)
                    .map(|_| ProbeLine::new())
                    .collect::<Vec<_>>()
                    .into_boxed_slice(),
                entries: (0..nb * WAYS)
                    .map(|_| Entry::new())
                    .collect::<Vec<_>>()
                    .into_boxed_slice(),
                bucket_mask: nb - 1,
                write: Mutex::new(WriterState {
                    retired: VecDeque::new(),
                    wheel: vec![Vec::new(); WHEEL_SLOTS],
                    // start at the creation tick: a fresh engine must not
                    // replay the process's wheel history from tick 0
                    next_tick: now_ms() / WHEEL_TICK_MS,
                }),
                stats: Stats::default(),
                items: AtomicU64::new(0),
                mutations: AtomicU64::new(0),
            })
            .collect();
        static ENGINE_SEQ: AtomicU64 = AtomicU64::new(1);
        Engine {
            id: ENGINE_SEQ.fetch_add(1, Ordering::Relaxed),
            shards: shards.into_boxed_slice(),
            shard_mask: s - 1,
            slo: RwLock::new(HashMap::new()),
            created: Instant::now(),
        }
    }

    fn shard_of(&self, key: &[u8]) -> (&Shard, u32, usize) {
        let h = hash_key(key);
        let sh = &self.shards[(h as usize) & self.shard_mask];
        (sh, (h >> 32) as u32, ((h >> 12) as usize) & sh.bucket_mask)
    }

    /// No loops of its own; bounded by lookup's 3584.
    pub fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        let (sh, kh, b0) = self.shard_of(key);
        let _pin = crate::epoch::Guard::new();
        match sh.lookup(key, kh, b0) {
            Find::Found(c) => {
                sh.stats.hits.fetch_add(1, Ordering::Relaxed);
                Some(unsafe { cell(c) }.val().to_vec())
            }
            Find::Expired => {
                sh.stats.misses.fetch_add(1, Ordering::Relaxed);
                sh.stats.expired.fetch_add(1, Ordering::Relaxed);
                None
            }
            Find::NotFound => {
                sh.stats.misses.fetch_add(1, Ordering::Relaxed);
                None
            }
        }
    }

    /// Zero-copy read: same lock-free path as `get`, no allocation. The
    /// returned handle holds an epoch pin — the cell stays unreclaimed for
    /// as long as the handle lives. No loops of its own; bounded by
    /// lookup's 3584.
    pub fn get_ref(&self, key: &[u8]) -> Option<ValueRef> {
        let (sh, kh, b0) = self.shard_of(key);
        crate::epoch::pin();
        match sh.lookup(key, kh, b0) {
            Find::Found(c) => {
                sh.stats.hits.fetch_add(1, Ordering::Relaxed);
                let v = unsafe { cell(c) }.val();
                Some(ValueRef {
                    ptr: v.as_ptr(),
                    len: v.len(),
                    _not_send: std::marker::PhantomData,
                })
            }
            Find::Expired => {
                sh.stats.misses.fetch_add(1, Ordering::Relaxed);
                sh.stats.expired.fetch_add(1, Ordering::Relaxed);
                crate::epoch::unpin();
                None
            }
            Find::NotFound => {
                sh.stats.misses.fetch_add(1, Ordering::Relaxed);
                crate::epoch::unpin();
                None
            }
        }
    }

    pub fn exists(&self, key: &[u8]) -> bool {
        let (sh, kh, b0) = self.shard_of(key);
        let _pin = crate::epoch::Guard::new();
        matches!(sh.lookup(key, kh, b0), Find::Found(_))
    }

    pub fn ttl(&self, key: &[u8]) -> TtlStatus {
        let (sh, kh, b0) = self.shard_of(key);
        let _pin = crate::epoch::Guard::new();
        match sh.lookup(key, kh, b0) {
            Find::Found(c) => {
                let exp = unsafe { cell(c) }.expire_at.load(Ordering::Relaxed);
                if exp == 0 {
                    TtlStatus::Persistent
                } else {
                    TtlStatus::Expires(exp.saturating_sub(now_ms()))
                }
            }
            _ => TtlStatus::Missing,
        }
    }

    pub fn set(&self, key: &[u8], val: &[u8]) {
        self.set_opts(key, val, &SetOpts::default());
    }

    /// Deepest nest: PROBE_LIMIT × WAYS = 56 (empty-way search and eviction
    /// scan are sequential siblings; the bound covers the deepest single
    /// nest — deferred MIR pass will sum sibling paths).
    pub fn set_opts(&self, key: &[u8], val: &[u8], o: &SetOpts) -> SetOutcome {
        let (sh, kh, b0) = self.shard_of(key);
        let mut w = sh.write.lock().unwrap();
        sh.mutations.fetch_add(1, Ordering::Relaxed);

        if let Some((oldc, b, way, expired)) = sh.locked_find(key, kh, b0) {
            // An expired entry is logically absent: NX may store, XX may not.
            if (!expired && o.nx) || (expired && o.xx) {
                return SetOutcome::NotStored;
            }
            let old = unsafe { cell(oldc) };
            let new_exp = if !expired && o.keepttl {
                old.expire_at.load(Ordering::Relaxed)
            } else {
                o.ttl
                    .map(|d| now_ms().saturating_add(d.as_millis() as u64))
                    .unwrap_or(0)
            };
            let nc = alloc_cell(key, val, new_exp);
            let v = sh.begin(b);
            sh.entry(b, way).cell.store(nc, Ordering::Relaxed);
            sh.probe[b].set_tag(way, (kh >> 24) as u8);
            sh.end(b, v);
            if new_exp > 0 {
                wheel_push(&mut w, new_exp, nc, b, way);
            }
            w.retired
                .push_back((NonNull::from(old), crate::epoch::retire_tag()));
            if w.retired.len() >= 64 {
                let barrier = crate::epoch::barrier();
                sh.drain_retired(&mut w, barrier);
            }
            sh.stats.sets.fetch_add(1, Ordering::Relaxed);
            let old_val = if o.get_old && !expired {
                Some(old.val().to_vec())
            } else {
                None
            };
            return SetOutcome::Stored(old_val);
        }

        if o.xx {
            return SetOutcome::NotStored;
        }

        // Find an empty way in the probe window, preferring the home bucket.
        let mut slot: Option<(usize, usize)> = None;
        for p in 0..PROBE_LIMIT {
            let b = (b0 + p) & sh.bucket_mask;
            for way in 0..WAYS {
                if sh.entry(b, way).cell.load(Ordering::Relaxed) == 0 {
                    slot = Some((b, way));
                    break;
                }
            }
            if slot.is_some() {
                break;
            }
        }

        let (b, way) = match slot {
            Some(s) => s,
            None => {
                // Window full: evict min (freq, ctl) over the window.
                // ctl bit 0 is the pin flag; nothing pins in v0, so a victim
                // always exists.
                let mut best: Option<(u32, u32, usize, usize)> = None;
                for p in 0..PROBE_LIMIT {
                    let bb = (b0 + p) & sh.bucket_mask;
                    for way in 0..WAYS {
                        let e = &sh.entry(bb, way);
                        let ctl = e.ctl.load(Ordering::Relaxed);
                        if ctl & 1 != 0 {
                            continue;
                        }
                        let freq = e.freq.load(Ordering::Relaxed) as u32;
                        let tick = (ctl >> 1) as u32;
                        let better = best.map_or(true, |(bf, bt, _, _)| (freq, tick) < (bf, bt));
                        if better {
                            best = Some((freq, tick, bb, way));
                        }
                    }
                }
                let (_, _, b, way) = best.expect("full probe window with no evictable entry");
                let victim = sh.entry(b, way).cell.load(Ordering::Relaxed);
                let v = sh.begin(b);
                sh.entry(b, way).cell.store(0, Ordering::Relaxed);
                sh.end(b, v);
                w.retired.push_back((
                    NonNull::from(unsafe { cell(victim) }),
                    crate::epoch::retire_tag(),
                ));
                if w.retired.len() >= 64 {
                    let barrier = crate::epoch::barrier();
                    sh.drain_retired(&mut w, barrier);
                }
                sh.items.fetch_sub(1, Ordering::Relaxed);
                sh.stats.evictions.fetch_add(1, Ordering::Relaxed);
                (b, way)
            }
        };

        let exp = o
            .ttl
            .map(|d| now_ms().saturating_add(d.as_millis() as u64))
            .unwrap_or(0);
        let nc = alloc_cell(key, val, exp);
        if b != b0 {
            // Bump the home bucket's overflow counter BEFORE the entry is
            // visible: a reader that can see the spilled entry must also see
            // a positive overflow, or the miss-skip would false-miss it.
            // (Undercount is impossible; evict/sweep only ever overcount.)
            let v0 = sh.begin(b0);
            sh.bump_overflow(b0, 1);
            sh.end(b0, v0);
        }
        let e = &sh.entry(b, way);
        let v = sh.begin(b);
        e.ctl
            .store(((now_ms() as u16) & 0x7fff) << 1, Ordering::Relaxed);
        e.cost_log.store(cost_log(val.len()), Ordering::Relaxed);
        e.freq.store(0, Ordering::Relaxed);
        e.cell.store(nc, Ordering::Relaxed);
        sh.probe[b].set_tag(way, (kh >> 24) as u8);
        sh.end(b, v);
        if exp > 0 {
            wheel_push(&mut w, exp, nc, b, way);
        }
        sh.items.fetch_add(1, Ordering::Relaxed);
        sh.stats.sets.fetch_add(1, Ordering::Relaxed);
        SetOutcome::Stored(None)
    }

    pub fn del(&self, key: &[u8]) -> bool {
        let (sh, kh, b0) = self.shard_of(key);
        let mut w = sh.write.lock().unwrap();
        sh.mutations.fetch_add(1, Ordering::Relaxed);
        if let Some((c, b, way, expired)) = sh.locked_find(key, kh, b0) {
            if expired {
                return false; // logically absent; the sweeper reclaims it
            }
            let v = sh.begin(b);
            sh.entry(b, way).cell.store(0, Ordering::Relaxed);
            sh.end(b, v);
            w.retired.push_back((
                NonNull::from(unsafe { cell(c) }),
                crate::epoch::retire_tag(),
            ));
            if b != b0 {
                sh.bump_overflow(b0, -1);
            }
            sh.items.fetch_sub(1, Ordering::Relaxed);
            sh.stats.dels.fetch_add(1, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    pub fn expire(&self, key: &[u8], ttl: Option<Duration>) -> bool {
        let (sh, kh, b0) = self.shard_of(key);
        let mut w = sh.write.lock().unwrap();
        sh.mutations.fetch_add(1, Ordering::Relaxed);
        match sh.locked_find(key, kh, b0) {
            Some((c, b, way, false)) => {
                let d = ttl
                    .map(|t| now_ms().saturating_add(t.as_millis() as u64))
                    .unwrap_or(0);
                unsafe { cell(c) }.expire_at.store(d, Ordering::Relaxed);
                if d > 0 {
                    wheel_push(&mut w, d, c, b, way);
                }
                true
            }
            _ => false,
        }
    }

    /// Cells freed by epoch reclamation (observability for the GC proof).
    pub fn reclaimed(&self) -> u64 {
        self.shards
            .iter()
            .map(|s| s.stats.gc.load(Ordering::Relaxed))
            .sum()
    }

    /// Cursor scan over the flat entry space (SCAN). Returns the next cursor
    /// (0 = done) and up to `count` keys matching `pattern` (glob: `*`, `?`).
    /// Weak guarantee, Redis-shaped: keys present for the entire scan are
    /// returned at least once; a concurrent resize may skip moved entries.
    pub fn scan_from(
        &self,
        cursor: u64,
        count: usize,
        pattern: Option<&[u8]>,
    ) -> (u64, Vec<Vec<u8>>) {
        let _pin = crate::epoch::Guard::new();
        let per_shard = self
            .shards
            .first()
            .map(|s| s.entries.len() as u64)
            .unwrap_or(0);
        if per_shard == 0 {
            return (0, Vec::new());
        }
        let total = self.shards.len() as u64 * per_shard;
        let mut pos = cursor.min(total.saturating_sub(1)).max(0);
        let mut keys = Vec::new();
        while pos < total && keys.len() < count {
            let sh_idx = (pos / per_shard) as usize;
            let idx = (pos % per_shard) as usize;
            pos += 1;
            let c = self.shards[sh_idx].entries[idx]
                .cell
                .load(Ordering::Relaxed);
            if c == 0 {
                continue;
            }
            let cd = unsafe { cell(c) };
            let k = cd.key();
            if let Some(p) = pattern {
                if !glob_match(p, k) {
                    continue;
                }
            }
            keys.push(k.to_vec());
        }
        (if pos >= total { 0 } else { pos }, keys)
    }

    /// L0 SPRINT hits across all shards (the hot tier's scoreboard).
    pub fn l0_hit_count(&self) -> u64 {
        self.shards
            .iter()
            .map(|s| s.stats.l0_hits.load(Ordering::Relaxed))
            .sum()
    }

    /// L0 SPRINT read (spec Phase 4): thread-private hot tier validated by
    /// the shard mutation counter; on miss, reads through the engine and
    /// fills. The closure receives the cached bytes zero-copy.
    pub fn with_l0<R>(&self, key: &[u8], f: impl FnOnce(&[u8]) -> R) -> Option<R> {
        let h = hash_key(key);
        let sh = &self.shards[(h as usize) & self.shard_mask];
        let gen = sh.mutations.load(Ordering::Acquire);
        let now = now_ms();
        let mut f = Some(f);
        if let Some(r) = crate::l0::with_hit(self.id, h, gen, now, |v| f.take().unwrap()(v)) {
            sh.stats.l0_hits.fetch_add(1, Ordering::Relaxed);
            return Some(r);
        }
        match self.get_with_expire(key) {
            Some((v, expire_at)) => {
                // fill only if the deadline has not already lapsed
                if expire_at == 0 || now_ms() < expire_at {
                    crate::l0::fill(self.id, h, gen, expire_at, &v);
                    Some(f.take().unwrap()(&v))
                } else {
                    None
                }
            }
            None => None,
        }
    }

    /// Owned value plus absolute deadline (L0 fill source).
    fn get_with_expire(&self, key: &[u8]) -> Option<(Vec<u8>, u64)> {
        let (sh, kh, b0) = self.shard_of(key);
        let _pin = crate::epoch::Guard::new();
        match sh.lookup(key, kh, b0) {
            Find::Found(c) => {
                sh.stats.hits.fetch_add(1, Ordering::Relaxed);
                let cd = unsafe { cell(c) };
                Some((cd.val().to_vec(), cd.expire_at.load(Ordering::Relaxed)))
            }
            _ => None,
        }
    }

    /// Owned-value convenience over `with_l0`.
    pub fn get_l0(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.with_l0(key, |v| v.to_vec())
    }

    pub fn item_count(&self) -> u64 {
        self.shards
            .iter()
            .map(|s| s.items.load(Ordering::Relaxed))
            .sum()
    }

    pub fn flush(&self) {
        for sh in self.shards.iter() {
            let mut w = sh.write.lock().unwrap();
            sh.mutations.fetch_add(1, Ordering::Relaxed);
            for b in 0..sh.probe.len() {
                let mut v = None;
                for way in 0..WAYS {
                    let c = sh.entry(b, way).cell.load(Ordering::Relaxed);
                    if c != 0 {
                        if v.is_none() {
                            v = Some(sh.begin(b));
                        }
                        sh.entry(b, way).cell.store(0, Ordering::Relaxed);
                        w.retired.push_back((
                            NonNull::from(unsafe { cell(c) }),
                            crate::epoch::retire_tag(),
                        ));
                    }
                }
                if let Some(v) = v {
                    sh.end(b, v);
                }
                sh.probe[b].overflow_out.store(0, Ordering::Relaxed);
            }
            sh.items.store(0, Ordering::Relaxed);
            // all descriptors became stale the moment their cells retired
            for slot in 0..WHEEL_SLOTS {
                w.wheel[slot].clear();
            }
        }
    }

    /// Maintenance tick: advance the expiry wheel by a bounded number of
    /// slots, unlinking entries whose deadline has passed (verified by cell
    /// identity), and free aged-out retired cells. Cost scales with pending
    /// expirations, not with table size.
    pub fn sweep(&self) {
        // Epoch maintenance: advance, then collect against the barrier —
        // garbage tagged below it is unreachable by every pinned reader.
        crate::epoch::advance();
        let barrier = crate::epoch::barrier();
        for sh in self.shards.iter() {
            let Ok(mut w) = sh.write.lock() else { continue };
            sh.drain_retired(&mut w, barrier);
            let start = w.next_tick;
            let now_tick = now_ms() / WHEEL_TICK_MS;
            // The cursor may never pass the clock: forcing it forward would
            // sprint into future slots and lap the ring, delaying every
            // pending expiry by a full cycle. Sweep calls faster than the
            // tick rate are simply no-ops.
            let target = (start + WHEEL_SLOTS_PER_CALL).min(now_tick + 1);
            if target > start {
                let mut unlinks = 0usize;
                for t in start..target {
                    let slot = (t as usize) & (WHEEL_SLOTS - 1);
                    if w.wheel[slot].is_empty() {
                        continue;
                    }
                    let due = std::mem::take(&mut w.wheel[slot]);
                    for i in 0..due.len() {
                        let e = &due[i];
                        if now_ms() < e.expire_at {
                            // wrapped insertion, not yet due → revisit next cycle
                            w.wheel[slot].push(*e);
                            continue;
                        }
                        let b = e.bucket as usize;
                        let way = e.way as usize;
                        // identity check: overwritten/deleted entries leave a
                        // stale descriptor, never a wrong unlink
                        if sh.entry(b, way).cell.load(Ordering::Relaxed) != e.cell {
                            continue;
                        }
                        let exp = unsafe { cell(e.cell) }.expire_at.load(Ordering::Relaxed);
                        if exp != e.expire_at {
                            continue;
                        }
                        // work-credit bound: overflow goes one slot ahead,
                        // spreading a burst across calls instead of
                        // monopolizing the writer mutex
                        if unlinks >= WHEEL_BURST_CAP {
                            let next_slot = (slot + 1) & (WHEEL_SLOTS - 1);
                            for rest in &due[i..] {
                                w.wheel[next_slot].push(*rest);
                            }
                            break;
                        }
                        unlinks += 1;
                        // L0 invalidation before the unlink becomes visible;
                        // per-unlink (not per-sweep) so idle sweeps don't
                        // needlessly flush every thread's hot tier
                        sh.mutations.fetch_add(1, Ordering::Relaxed);
                        let v = sh.begin(b);
                        sh.entry(b, way).cell.store(0, Ordering::Relaxed);
                        sh.end(b, v);
                        w.retired.push_back((
                            NonNull::from(unsafe { cell(e.cell) }),
                            crate::epoch::retire_tag(),
                        ));
                        sh.items.fetch_sub(1, Ordering::Relaxed);
                        sh.stats.expired.fetch_add(1, Ordering::Relaxed);
                    }
                }
                w.next_tick = target;
            }
        }
    }

    pub fn stats(&self) -> EngineStats {
        let mut hits = 0u64;
        let mut misses = 0u64;
        let mut expired = 0u64;
        let mut evictions = 0u64;
        let mut sets = 0u64;
        let mut dels = 0u64;
        let mut retries = 0u64;
        let mut items = 0u64;
        for sh in self.shards.iter() {
            hits += sh.stats.hits.load(Ordering::Relaxed);
            misses += sh.stats.misses.load(Ordering::Relaxed);
            expired += sh.stats.expired.load(Ordering::Relaxed);
            evictions += sh.stats.evictions.load(Ordering::Relaxed);
            sets += sh.stats.sets.load(Ordering::Relaxed);
            dels += sh.stats.dels.load(Ordering::Relaxed);
            retries += sh.stats.retries.load(Ordering::Relaxed);
            items += sh.items.load(Ordering::Relaxed);
        }
        let total = hits + misses;
        EngineStats {
            shards: self.shards.len() as u64,
            items,
            hits,
            misses,
            hit_ratio: if total > 0 {
                hits as f64 / total as f64
            } else {
                0.0
            },
            expired,
            evictions,
            sets,
            dels,
            retries,
            uptime_ms: self.created.elapsed().as_millis() as u64,
        }
    }

    pub fn slo_set(&self, slo: Slo) {
        self.slo.write().unwrap().insert(slo.class.clone(), slo);
    }

    pub fn slo_get(&self, class: &str) -> Option<Slo> {
        self.slo.read().unwrap().get(class).cloned()
    }

    pub fn slo_del(&self, class: &str) -> bool {
        self.slo.write().unwrap().remove(class).is_some()
    }

    pub fn slo_list(&self) -> Vec<Slo> {
        self.slo.read().unwrap().values().cloned().collect()
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        for sh in self.shards.iter() {
            let Ok(mut w) = sh.write.lock() else { continue };
            for e in sh.entries.iter() {
                let c = e.cell.load(Ordering::Relaxed);
                if c != 0 {
                    unsafe { drop(Box::from_raw(c as *mut CellData)) };
                }
            }
            for (p, _) in std::mem::take(&mut w.retired) {
                unsafe { drop(Box::from_raw(p.as_ptr())) };
            }
            // wheel descriptors reference freed cells via pointer identity;
            // after freeing the table they are all stale — drop them
            for slot in 0..WHEEL_SLOTS {
                w.wheel[slot].clear();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny() -> Engine {
        // 8 entries / 1 shard floors to a single 14-way bucket: nb = 1, so
        // the whole probe window is one bucket and capacity is exactly WAYS.
        Engine::new(EngineOptions {
            shard_hint: Some(1),
            total_entries: 8,
            min_buckets: 1,
            ..Default::default()
        })
    }

    #[test]
    fn set_get_del_roundtrip() {
        let e = tiny();
        assert_eq!(e.get(b"foo"), None);
        e.set(b"foo", b"bar");
        assert_eq!(e.get(b"foo"), Some(b"bar".to_vec()));
        assert!(e.exists(b"foo"));
        assert_eq!(e.item_count(), 1);
        assert!(e.del(b"foo"));
        assert!(!e.del(b"foo"));
        assert_eq!(e.get(b"foo"), None);
        assert_eq!(e.item_count(), 0);
    }

    #[test]
    fn ttl_lifecycle() {
        let e = tiny();
        e.set_opts(
            b"k",
            b"v",
            &SetOpts {
                ttl: Some(Duration::from_millis(80)),
                ..Default::default()
            },
        );
        match e.ttl(b"k") {
            TtlStatus::Expires(ms) => assert!((1..=80).contains(&ms)),
            other => panic!("expected Expires, got {other:?}"),
        }
        std::thread::sleep(Duration::from_millis(140));
        assert_eq!(e.get(b"k"), None);
        assert!(matches!(e.ttl(b"k"), TtlStatus::Missing));
        e.sweep();
        assert_eq!(e.item_count(), 0);
    }

    #[test]
    fn set_flags() {
        let e = tiny();
        // NX on missing key stores
        let o = SetOpts {
            nx: true,
            ..Default::default()
        };
        assert!(matches!(
            e.set_opts(b"a", b"1", &o),
            SetOutcome::Stored(None)
        ));
        // NX on present key is rejected
        assert!(matches!(e.set_opts(b"a", b"2", &o), SetOutcome::NotStored));
        assert_eq!(e.get(b"a"), Some(b"1".to_vec()));
        // XX on missing key is rejected
        let o = SetOpts {
            xx: true,
            ..Default::default()
        };
        assert!(matches!(e.set_opts(b"b", b"1", &o), SetOutcome::NotStored));
        // GET returns the previous value
        let o = SetOpts {
            get_old: true,
            ..Default::default()
        };
        match e.set_opts(b"a", b"3", &o) {
            SetOutcome::Stored(Some(old)) => assert_eq!(old, b"1".to_vec()),
            other => panic!("expected Stored(Some), got {other:?}"),
        }
        // KEEPTTL preserves an existing deadline
        e.set_opts(
            b"a",
            b"4",
            &SetOpts {
                ttl: Some(Duration::from_secs(60)),
                ..Default::default()
            },
        );
        e.set_opts(
            b"a",
            b"5",
            &SetOpts {
                keepttl: true,
                ..Default::default()
            },
        );
        match e.ttl(b"a") {
            TtlStatus::Expires(ms) => assert!(ms > 55_000, "keepttl lost the deadline: {ms}"),
            other => panic!("expected Expires, got {other:?}"),
        }
        assert_eq!(e.get(b"a"), Some(b"5".to_vec()));
    }

    #[test]
    fn eviction_at_capacity() {
        // 1 shard, 1 bucket → the probe window is a single 14-way bucket.
        let e = tiny();
        for i in 0..(3 * WAYS) as u64 {
            e.set(format!("k{i}").as_bytes(), b"v");
        }
        let st = e.stats();
        assert_eq!(st.items, WAYS as u64);
        assert_eq!(st.evictions, (2 * WAYS) as u64);
        let present = (0..(3 * WAYS) as u64)
            .filter(|i| e.get(format!("k{i}").as_bytes()).is_some())
            .count();
        assert_eq!(present, WAYS);
    }

    #[test]
    fn expire_command_semantics() {
        let e = tiny();
        e.set(b"k", b"v");
        assert!(matches!(e.ttl(b"k"), TtlStatus::Persistent));
        assert!(e.expire(b"k", Some(Duration::from_millis(60))));
        assert!(matches!(e.ttl(b"k"), TtlStatus::Expires(_)));
        std::thread::sleep(Duration::from_millis(120));
        assert_eq!(e.get(b"k"), None);
        // expire on a missing key fails
        assert!(!e.expire(b"nope", Some(Duration::from_secs(1))));
    }

    #[test]
    fn flush_clears_everything() {
        let e = tiny();
        for i in 0..50u64 {
            e.set(format!("k{i}").as_bytes(), b"v");
        }
        e.flush();
        assert_eq!(e.item_count(), 0);
        assert_eq!(e.get(b"k1"), None);
    }

    #[test]
    fn slo_roundtrip() {
        let e = tiny();
        e.slo_set(Slo {
            class: "dashboard".into(),
            freshness_p99_ms: 250,
            origin_qps_max: 2000,
            latency_p99_us: 2500,
            priority: 2,
        });
        assert_eq!(e.slo_get("dashboard").unwrap().freshness_p99_ms, 250);
        assert!(e.slo_del("dashboard"));
        assert!(e.slo_get("dashboard").is_none());
    }

    // Timing wheel: staggered deadlines expire only when due; wide margins
    // keep the test free of timing flakiness.
    #[test]
    fn wheel_expires_staggered_deadlines() {
        let e = Engine::new(EngineOptions {
            shard_hint: Some(1),
            total_entries: 1 << 12,
            min_buckets: 32,
            ..Default::default()
        });
        for i in 0..100u64 {
            let ttl = Duration::from_millis(100 + 20 * i); // 100 ms .. 2.1 s
            e.set_opts(
                format!("w{i}").as_bytes(),
                b"v",
                &SetOpts {
                    ttl: Some(ttl),
                    ..Default::default()
                },
            );
        }
        std::thread::sleep(Duration::from_millis(600));
        e.sweep();
        let present = (0..100u64)
            .filter(|i| e.get(format!("w{i}").as_bytes()).is_some())
            .count();
        // boundary at i ≈ 25 (100 + 20i = 600); allow scheduling slack
        assert!((68..=80).contains(&present), "present = {present}");
        // sweeping far past every deadline clears all of it
        for _ in 0..64 {
            e.sweep();
            std::thread::sleep(Duration::from_millis(64));
        }
        assert_eq!(e.item_count(), 0);
    }

    #[test]
    fn wheel_descriptor_is_inert_after_overwrite_or_delete() {
        let e = tiny();
        // overwrite cancels the old deadline
        e.set_opts(
            b"a",
            b"1",
            &SetOpts {
                ttl: Some(Duration::from_millis(80)),
                ..Default::default()
            },
        );
        e.set(b"a", b"2");
        std::thread::sleep(Duration::from_millis(200));
        e.sweep();
        assert_eq!(e.get(b"a"), Some(b"2".to_vec()));
        // delete leaves a stale descriptor that must be skipped harmlessly
        e.set_opts(
            b"d",
            b"1",
            &SetOpts {
                ttl: Some(Duration::from_millis(80)),
                ..Default::default()
            },
        );
        assert!(e.del(b"d"));
        std::thread::sleep(Duration::from_millis(200));
        for _ in 0..8 {
            e.sweep();
            std::thread::sleep(Duration::from_millis(64));
        }
        assert_eq!(e.item_count(), 1); // only "a" remains
        let st = e.stats();
        assert_eq!(
            st.expired, 0,
            "no expiry may fire for overwritten/deleted keys"
        );
    }

    // OPT-1 miss-skip regression: 52 keys into a 4-bucket window (capacity
    // 56, no eviction) forces home-bucket spills; the overflow-counter
    // shortcut must never false-miss a spilled key.
    #[test]
    fn spilled_keys_never_false_miss() {
        let e = Engine::new(EngineOptions {
            shard_hint: Some(1),
            total_entries: 56,
            min_buckets: 4,
            ..Default::default()
        });
        for i in 0..52u64 {
            e.set(format!("sp:{i}").as_bytes(), b"v");
        }
        for round in 0..3 {
            for i in 0..52u64 {
                assert_eq!(
                    e.get(format!("sp:{i}").as_bytes()),
                    Some(b"v".to_vec()),
                    "false miss on round {round} key {i}"
                );
            }
        }
    }

    // Epoch reclamation proof: a held ValueRef pins its cell against the
    // collector; dropping it releases the pin and the cell frees after the
    // epoch advances past the barrier.
    #[test]
    fn epoch_reclamation_waits_for_pinned_refs() {
        let e = tiny();
        e.set(b"k", b"v1");
        let r = e.get_ref(b"k").unwrap();
        assert_eq!(r.as_slice(), b"v1");
        e.set(b"k", b"v2"); // retires the old cell with the current tag
        for _ in 0..50 {
            e.sweep();
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(
            e.reclaimed(),
            0,
            "cell freed while the ref still holds its pin"
        );
        assert_eq!(r.as_slice(), b"v1", "the pinned cell must stay readable");
        drop(r);
        let mut freed = false;
        for _ in 0..200 {
            e.sweep();
            std::thread::sleep(Duration::from_millis(2));
            if e.reclaimed() >= 1 {
                freed = true;
                break;
            }
        }
        assert!(freed, "cell must free after unpin and epoch advance");
        assert_eq!(e.get(b"k"), Some(b"v2".to_vec()));
    }

    // L0 SPRINT: fill → hit without touching the engine (observable through
    // the engine hit counter staying flat) → write invalidates via the
    // mutation counter → miss path refills. Engine isolation included.
    #[test]
    fn l0_hits_and_invalidates() {
        let e = tiny();
        e.set(b"hot", b"v1");
        assert_eq!(e.get_l0(b"hot"), Some(b"v1".to_vec())); // fill
        let engine_hits = e.stats().hits;
        assert_eq!(e.get_l0(b"hot"), Some(b"v1".to_vec())); // L0 hit
        assert_eq!(
            e.stats().hits,
            engine_hits,
            "second read must come from L0, not the engine"
        );
        assert!(e.l0_hit_count() >= 1);
        e.set(b"hot", b"v2"); // bumps the mutation counter
        assert_eq!(e.get_l0(b"hot"), Some(b"v2".to_vec()));
        assert_eq!(e.stats().hits, engine_hits + 1);
        assert!(e.get_l0(b"missing").is_none());
    }

    // A lapsed TTL must not be served from L0 even when no shard write has
    // bumped the generation (the lazy expiry check in the slot).
    #[test]
    fn l0_never_serves_lapsed_ttl() {
        let e = tiny();
        e.set_opts(
            b"t",
            b"v",
            &SetOpts {
                ttl: Some(Duration::from_millis(60)),
                ..Default::default()
            },
        );
        assert_eq!(e.get_l0(b"t"), Some(b"v".to_vec())); // fill
        std::thread::sleep(Duration::from_millis(140));
        assert_eq!(e.get_l0(b"t"), None, "lapsed TTL served from L0");
        assert_eq!(e.get(b"t"), None);
    }

    #[test]
    fn l0_isolates_engines() {
        let a = tiny();
        let b = tiny();
        a.set(b"k", b"A");
        b.set(b"k", b"B");
        // alternating same-key reads on one thread: slots must never leak a
        // value across engine ids
        for _ in 0..8 {
            assert_eq!(a.get_l0(b"k"), Some(b"A".to_vec()));
            assert_eq!(b.get_l0(b"k"), Some(b"B".to_vec()));
        }
    }

    // Differential under churn: a writer flips one key while a reader loops
    // through L0 — only ever the two written values, never garbage.
    #[test]
    fn l0_never_serves_garbage_under_churn() {
        let e = std::sync::Arc::new(Engine::new(EngineOptions {
            total_entries: 1 << 14,
            ..Default::default()
        }));
        let va = vec![b'a'; 32];
        let vb = vec![b'b'; 32];
        e.set(b"hot", &va);
        let w = {
            let e = e.clone();
            let (va, vb) = (va.clone(), vb.clone());
            std::thread::spawn(move || {
                for i in 0..20_000u64 {
                    e.set(b"hot", if i & 1 == 0 { &va } else { &vb });
                }
            })
        };
        let r = {
            let e = e.clone();
            std::thread::spawn(move || {
                for _ in 0..100_000usize {
                    let v = e.get_l0(b"hot").expect("false miss on a live key");
                    assert_eq!(v.len(), 32);
                    assert!(v.iter().all(|&c| c == v[0]), "torn value: {:?}", &v[..8]);
                    assert!(v[0] == b'a' || v[0] == b'b');
                }
            })
        };
        w.join().unwrap();
        r.join().unwrap();
    }

    // Work-credit: a slot holding many same-deadline entries must not
    // unlink them all in one sweep call.
    #[test]
    fn wheel_burst_is_bounded() {
        let e = Engine::new(EngineOptions {
            shard_hint: Some(1),
            total_entries: 1 << 16,
            min_buckets: 64,
            ..Default::default()
        });
        for i in 0..20_000u64 {
            e.set_opts(
                format!("burst:{i}").as_bytes(),
                b"v",
                &SetOpts {
                    ttl: Some(Duration::from_millis(60)),
                    ..Default::default()
                },
            );
        }
        std::thread::sleep(Duration::from_millis(150));
        e.sweep();
        let st = e.stats();
        assert!(
            st.expired <= 1024,
            "single sweep unlinked {} > cap",
            st.expired
        );
        assert!(e.item_count() > 0, "burst must spread across calls");
        // and it still drains fully
        let mut guard = 0u32;
        while e.item_count() > 0 && guard < 500 {
            e.sweep();
            std::thread::sleep(Duration::from_millis(40));
            guard += 1;
        }
        assert_eq!(e.item_count(), 0);
    }

    // SCAN: full cursor cycle returns exactly the present keys; MATCH
    // filters by glob.
    #[test]
    fn scan_full_cycle_and_glob() {
        let e = Engine::new(EngineOptions {
            shard_hint: Some(2),
            total_entries: 1 << 14,
            min_buckets: 16,
            ..Default::default()
        });
        for i in 0..120u64 {
            e.set(format!("sc:a:{i}").as_bytes(), b"v");
        }
        for i in 0..60u64 {
            e.set(format!("sc:b:{i}").as_bytes(), b"v");
        }
        let mut cursor = 0u64;
        let mut got: Vec<Vec<u8>> = Vec::new();
        let mut rounds = 0;
        loop {
            let (next, keys) = e.scan_from(cursor, 37, None);
            got.extend(keys);
            cursor = next;
            rounds += 1;
            if cursor == 0 || rounds > 100 {
                break;
            }
        }
        assert_eq!(cursor, 0, "scan must terminate");
        assert_eq!(got.len(), 180);
        let set: std::collections::HashSet<Vec<u8>> = got.into_iter().collect();
        assert_eq!(set.len(), 180, "no duplicates across the cycle");
        assert!(set.contains(b"sc:a:0".as_slice()));

        let mut all_a = Vec::new();
        let mut cursor = 0u64;
        loop {
            let (next, keys) = e.scan_from(cursor, 50, Some(b"sc:a:*"));
            all_a.extend(keys);
            cursor = next;
            if cursor == 0 {
                break;
            }
        }
        assert_eq!(
            all_a.len(),
            120,
            "MATCH sc:a:* must return exactly the a-keys"
        );
        assert!(all_a.iter().all(|k| k.starts_with(b"sc:a:")));
    }

    #[test]
    fn glob_matcher_basics() {
        assert!(glob_match(b"abc", b"abc"));
        assert!(glob_match(b"a*c", b"abbbc"));
        assert!(glob_match(b"a?c", b"abc"));
        assert!(!glob_match(b"a?c", b"ac"));
        assert!(glob_match(b"*", b"anything"));
        assert!(glob_match(b"sc:a:*", b"sc:a:42"));
        assert!(!glob_match(b"sc:a:*", b"sc:b:42"));
        assert!(glob_match(b"**", b"x"));
        assert!(glob_match(b"", b""));
        assert!(!glob_match(b"", b"x"));
    }

    #[test]
    fn get_ref_is_zero_copy_and_correct() {
        let e = tiny();
        e.set(b"k", b"hello world");
        let r = e.get_ref(b"k").unwrap();
        assert_eq!(r.as_slice(), b"hello world");
        assert_eq!(r.len(), 11);
        assert_eq!(&*r, b"hello world");
        assert!(e.get_ref(b"missing").is_none());
        // expiry is honored on the zero-copy path too
        e.set_opts(
            b"t",
            b"v",
            &SetOpts {
                ttl: Some(Duration::from_millis(60)),
                ..Default::default()
            },
        );
        assert!(e.get_ref(b"t").is_some());
        std::thread::sleep(Duration::from_millis(130));
        assert!(e.get_ref(b"t").is_none());
    }

    // Differential check of the tag-probed read path against a HashMap
    // oracle: random set/del/get mixes, including key churn that leaves
    // stale tags behind on reused ways.
    #[test]
    fn tag_probe_matches_oracle() {
        struct X(u64);
        impl X {
            fn next(&mut self) -> u64 {
                let mut x = self.0;
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                self.0 = x;
                x
            }
        }
        let e = Engine::new(EngineOptions {
            shard_hint: Some(1),
            total_entries: 1 << 16,
            min_buckets: 32,
            ..Default::default()
        });
        let mut rng = X(0x853c_49e6_748f_ea9b);
        let mut model: std::collections::HashMap<Vec<u8>, Vec<u8>> =
            std::collections::HashMap::new();
        for _ in 0..30_000 {
            let k = format!("k{}", rng.next() % 300).into_bytes();
            match rng.next() % 3 {
                0 => {
                    let v = format!("v{}", rng.next() % 65_536).into_bytes();
                    e.set(&k, &v);
                    model.insert(k, v);
                }
                1 => {
                    assert_eq!(e.del(&k), model.remove(&k).is_some());
                }
                _ => {
                    assert_eq!(
                        e.get(&k),
                        model.get(&k).cloned(),
                        "key {:?}",
                        String::from_utf8_lossy(&k)
                    );
                }
            }
        }
    }
}
