//! L0 SPRINT (spec Phase 4): a thread-private, direct-mapped hot cache in
//! front of the engine — the ~ns read tier.
//!
//! Validation is the §3.4 counter scheme: every shard carries a mutation
//! counter that writers bump BEFORE any change becomes visible, so a cached
//! (key, generation, value) triple is valid exactly while no write has
//! touched the shard since it was filled. Coarse — any shard write
//! invalidates that shard's whole L0 population — but read-heavy hot keys
//! with rare writes are precisely the workload it exists for; the invalida-
//! tion Bloom from the spec is the deferred refinement.
//!
//! Values are owned copies, not `ValueRef`s: holding many long-lived pins
//! would stall the epoch collector (a thread's announced epoch is that of
//! its oldest pin), so the hot tier trades one copy per fill for GC liveness.
//! Slots are tagged with the engine's unique id, so two engines sharing a
//! thread never cross-contaminate.

use std::cell::RefCell;

/// Must be a power of two. 512 slots ≈ 32 KiB per thread.
pub const L0_SLOTS: usize = 512;

struct Slot {
    engine: u64,
    hash: u64,
    gen: u64,
    /// absolute coarse-ms deadline; 0 = persistent. Checked lazily so a
    /// lapsed TTL is never served even when no shard write has bumped the
    /// generation (the sweeper's unlink bump is the backstop, but library
    /// users may not sweep).
    expire_at: u64,
    val: Vec<u8>,
}

thread_local! {
    static CACHE: RefCell<Vec<Slot>> =
        RefCell::new((0..L0_SLOTS).map(|_| Slot { engine: 0, hash: 0, gen: 0, expire_at: 0, val: Vec::new() }).collect());
}

#[inline]
fn fold(hash: u64) -> usize {
    // mid bits: the low bits already select the shard
    ((hash >> 12) as usize) & (L0_SLOTS - 1)
}

/// Serve from the cache if the slot matches this engine, key hash, and the
/// generation the caller just observed, and its TTL has not lapsed.
pub fn with_hit<R>(
    engine: u64,
    hash: u64,
    gen: u64,
    now: u64,
    f: impl FnOnce(&[u8]) -> R,
) -> Option<R> {
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        let s = &mut c[fold(hash)];
        if s.engine == engine && s.hash == hash && s.gen == gen {
            if s.expire_at != 0 && now >= s.expire_at {
                s.hash = 0; // lapsed: drop the slot
                return None;
            }
            Some(f(&s.val))
        } else {
            None
        }
    })
}

/// Fill the slot for this hash. The generation is the one observed before
/// the engine read that produced `val`: if a write raced the fill, the
/// stored generation is stale and the next probe misses (the safe direction).
pub fn fill(engine: u64, hash: u64, gen: u64, expire_at: u64, val: &[u8]) {
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        let s = &mut c[fold(hash)];
        s.engine = engine;
        s.hash = hash;
        s.gen = gen;
        s.expire_at = expire_at;
        s.val.clear();
        s.val.extend_from_slice(val);
    });
}
