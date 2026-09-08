//! Layout primitives (spec §3).
//!
//! v0 keeps atomic fields so the seqlock read path is sound without `unsafe`
//! volatile tricks; the packed 16 B entry (u48 cell ref, exactly four per
//! cache line) is the Phase 1 gate — see PHASES.md.

use std::sync::atomic::{AtomicU16, AtomicU32, AtomicU64, AtomicU8, Ordering};

/// Ways per bucket (spec §3.2: 14-way SIMD-tag-probed bucket).
pub const WAYS: usize = 14;

/// SWAR byte-match: returns a word with 0x80 set in each byte position where
/// `word`'s byte equals `byte`. This is the pcmpeqb+movmskb pair expressed as
/// ALU ops so it works directly on the atomic tag words; true intrinsics land
/// with the packed COMBO layout (Phase 1 remainder).
#[inline]
pub fn match_bytes(word: u64, byte: u8) -> u64 {
    const LO: u64 = 0x0101_0101_0101_0101;
    const HI: u64 = 0x8080_8080_8080_8080;
    let x = word ^ (byte as u64).wrapping_mul(LO);
    x.wrapping_sub(LO) & !x & HI
}

/// Heap cell for one cached item. Key and value share a single allocation
/// (OPT-1: 3 allocs → 1 per set, and the key compare and value read land in
/// the same cache lines). Immutable after publish except `expire_at`, which
/// is why readers may dereference a pointer they validated under the bucket
/// seqlock and then keep using it inside the retirement grace window.
pub struct CellData {
    buf: Box<[u8]>,
    key_len: u32,
    /// absolute coarse-ms deadline; 0 = persistent
    pub expire_at: AtomicU64,
}

impl CellData {
    pub fn new(key: &[u8], val: &[u8], expire_at: u64) -> Self {
        let mut buf = Vec::with_capacity(key.len() + val.len());
        buf.extend_from_slice(key);
        buf.extend_from_slice(val);
        CellData {
            buf: buf.into_boxed_slice(),
            key_len: key.len() as u32,
            expire_at: AtomicU64::new(expire_at),
        }
    }

    #[inline]
    pub fn key(&self) -> &[u8] {
        &self.buf[..self.key_len as usize]
    }

    #[inline]
    pub fn val(&self) -> &[u8] {
        &self.buf[self.key_len as usize..]
    }
}

/// 16 B exactly (spec §3.5): cell at offset 0 (8-aligned), ctl/wheel/cost/
/// freq packed after it. The u48-packed cell ref of the full COMBO form is
/// deferred with the arena split; the size target is already met.
#[repr(C)]
pub struct Entry {
    /// `*mut CellData`; 0 = empty way
    pub cell: AtomicU64,
    /// bit 0 = pinned; bits 1..=15 = insert tick (age proxy for eviction)
    pub ctl: AtomicU16,
    /// reserved: timing-wheel cookie (Phase 3)
    pub wheel_cookie: AtomicU16,
    pub cost_log: AtomicU8,
    /// admission/eviction frequency; maintained off the read path in v0
    pub freq: AtomicU8,
}

impl Entry {
    pub fn new() -> Self {
        Entry {
            cell: AtomicU64::new(0),
            ctl: AtomicU16::new(0),
            wheel_cookie: AtomicU16::new(0),
            cost_log: AtomicU8::new(0),
            freq: AtomicU8::new(0),
        }
    }
}

impl Default for Entry {
    fn default() -> Self {
        Self::new()
    }
}

/// One cache line of probe metadata (spec §3.2/§3.10): seqlock version,
/// overflow counter and 16 tag bytes on a 64-byte-aligned line of their own,
/// with entries in a separate flat array — the probe path touches exactly
/// one line, and writer traffic on entry lines never dirties probe lines.
#[repr(C, align(64))]
pub struct ProbeLine {
    /// seqlock version: even = stable, odd = writer active. One writer per
    /// shard at a time (shard mutex), so the version never races itself.
    pub version: AtomicU32,
    /// §3.3 guaranteed-space invariant: entries this home bucket spilled
    /// outbound. Overcounting is safe (readers probe more); undercounting
    /// never happens (spill bumps publish before the entry becomes visible).
    pub overflow_out: AtomicU32,
    /// 16 tag bytes as two words: bytes 14..16 unused (the way loop is
    /// bounded by WAYS). Ways 0..8 live in `tags[0]`.
    pub tags: [AtomicU64; 2],
}

impl ProbeLine {
    pub fn new() -> Self {
        ProbeLine {
            version: AtomicU32::new(0),
            overflow_out: AtomicU32::new(0),
            tags: [AtomicU64::new(0), AtomicU64::new(0)],
        }
    }

    /// Writer-side (shard mutex held, inside the odd-version section).
    pub fn set_tag(&self, way: usize, tag: u8) {
        let word = &self.tags[way / 8];
        let sh = 8 * (way % 8);
        let cur = word.load(Ordering::Relaxed);
        let next = (cur & !(0xff_u64 << sh)) | ((tag as u64) << sh);
        word.store(next, Ordering::Relaxed);
    }

    /// Reader-side helper (valid under seqlock validation like any field).
    pub fn tag(&self, way: usize) -> u8 {
        ((self.tags[way / 8].load(Ordering::Relaxed) >> (8 * (way % 8))) & 0xff) as u8
    }
}

impl Default for ProbeLine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(all(test, target_pointer_width = "64"))]
mod tests {
    use super::*;

    #[test]
    fn entry_layout_is_locked() {
        // The spec §3.5 16 B entry: cell@0, ctl@8, wheel@10, cost@12,
        // freq@13. The packed-u48 form lands with the COMBO arena.
        assert_eq!(std::mem::size_of::<Entry>(), 16);
    }

    #[test]
    fn probe_line_is_exactly_one_cache_line() {
        // §3.2/§3.10: version + overflow + 16 tag bytes on one aligned line —
        // a probe touches exactly one line, by construction.
        assert_eq!(std::mem::size_of::<ProbeLine>(), 64);
        assert_eq!(std::mem::align_of::<ProbeLine>(), 64);
    }

    #[test]
    fn swar_match_bytes() {
        let w = 0x1212_3412_1212_1212_u64;
        let m = match_bytes(w, 0x34);
        assert_eq!(m, 0x0000_8000_0000_0000);
        assert_eq!(match_bytes(w, 0x12), 0x8080_0080_8080_8080);
        assert_eq!(match_bytes(w, 0x00), 0);
        assert_eq!(match_bytes(0, 0), 0x8080_8080_8080_8080);
    }

    #[test]
    fn set_tag_places_bytes() {
        let b = ProbeLine::new();
        b.set_tag(0, 0xaa);
        b.set_tag(7, 0xbb);
        b.set_tag(8, 0xcc);
        b.set_tag(13, 0xdd);
        assert_eq!(b.tags[0].load(Ordering::Relaxed), 0xbb << 56 | 0xaa);
        assert_eq!(b.tags[1].load(Ordering::Relaxed), 0xdd << 40 | 0xcc);
        // overwrite in place
        b.set_tag(7, 0x11);
        assert_eq!(b.tags[0].load(Ordering::Relaxed) >> 56, 0x11);
    }
}
