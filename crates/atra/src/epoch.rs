//! Epoch-based reclamation (spec Phase 3): unlinked cells are freed only
//! once every reader that pinned before the unlink has unpinned — replacing
//! the 500 ms time-grace guess with a proof.
//!
//! Mechanics: a global epoch advances on every maintenance sweep. Readers
//! announce the current epoch in a thread-owned registry slot while pinned
//! (plain loads and stores — no RMW on the read path, no cross-thread
//! contention: the slot line is written only by its owning thread). Writers
//! tag retired cells with the current epoch. Garbage tagged below the
//! barrier — the minimum of the global epoch and every announced pin — is
//! provably unreachable: a reader can only touch garbage unlinked during
//! its own pinned section, and such unlinks are tagged at or above the
//! epoch the reader announced.
//!
//! Pinning is depth-counted per thread. `ValueRef` carries one pin for its
//! lifetime, which is why it is `!Send`: the pin must be released on the
//! thread that took it. Forgetting a `ValueRef` (via `mem::forget`) leaks
//! its cell — a memory leak on misuse, never a use-after-free.

use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

static GLOBAL_EPOCH: AtomicU64 = AtomicU64::new(1);

static REGISTRY: Mutex<Vec<&'static AtomicU64>> = Mutex::new(Vec::new());

struct PinState {
    slot: &'static AtomicU64,
    depth: u32,
}

impl PinState {
    fn register() -> PinState {
        let slot: &'static AtomicU64 = Box::leak(Box::new(AtomicU64::new(0)));
        REGISTRY.lock().unwrap().push(slot);
        PinState { slot, depth: 0 }
    }
}

impl Drop for PinState {
    fn drop(&mut self) {
        // a dying thread cannot hold references
        self.slot.store(0, Ordering::Release);
    }
}

thread_local! {
    static PIN: RefCell<PinState> = RefCell::new(PinState::register());
}

/// Pin the current thread for a read section: announces the current global
/// epoch so the collector will not free anything this section can still
/// touch. Nestable (depth counted); plain stores, no RMW.
#[inline]
pub fn pin() {
    PIN.with(|p| {
        let mut p = p.borrow_mut();
        if p.depth == 0 {
            let e = GLOBAL_EPOCH.load(Ordering::Acquire);
            p.slot.store(e, Ordering::Release);
        }
        p.depth += 1;
    });
}

/// Release one pin level; clears the announcement at depth zero.
#[inline]
pub fn unpin() {
    PIN.with(|p| {
        let mut p = p.borrow_mut();
        p.depth = p.depth.saturating_sub(1);
        if p.depth == 0 {
            p.slot.store(0, Ordering::Release);
        }
    });
}

/// RAII pin for scoped reads.
pub struct Guard;

impl Guard {
    #[inline]
    pub fn new() -> Guard {
        pin();
        Guard
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        unpin();
    }
}

/// Epoch tag stamped on a retire (writer side).
#[inline]
pub fn retire_tag() -> u64 {
    GLOBAL_EPOCH.load(Ordering::Relaxed)
}

/// Advance the global epoch (maintenance tick).
pub fn advance() {
    GLOBAL_EPOCH.fetch_add(1, Ordering::Relaxed);
}

/// Garbage tagged below this value is provably unreachable: every pinned
/// reader announced a higher epoch, and future readers will pin at or above
/// the current global epoch.
pub fn barrier() -> u64 {
    let g = GLOBAL_EPOCH.load(Ordering::Acquire);
    let reg = REGISTRY.lock().unwrap();
    let mut m = g;
    for slot in reg.iter() {
        let v = slot.load(Ordering::Acquire);
        if v != 0 && v < m {
            m = v;
        }
    }
    m
}
