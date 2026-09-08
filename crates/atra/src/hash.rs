//! Key hashing: FxHash (multiply-rotate) with a murmur3-style finalizer.
//!
//! The finalizer matters because shard index, bucket index and the 32-bit
//! key hash are all sliced from this one word (§3.1's 12-bit shard field);
//! raw FxHash output has weak high bits.

use std::hash::Hasher;

const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

pub struct FxHasher(u64);

impl FxHasher {
    pub fn new() -> Self {
        FxHasher(0)
    }

    #[inline]
    fn add(&mut self, w: u64) {
        self.0 = (self.0.rotate_left(5) ^ w).wrapping_mul(SEED);
    }
}

impl Default for FxHasher {
    fn default() -> Self {
        Self::new()
    }
}

impl Hasher for FxHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        let mut chunks = bytes.chunks_exact(8);
        for c in &mut chunks {
            self.add(u64::from_le_bytes(c.try_into().unwrap()));
        }
        let rem = chunks.remainder();
        if !rem.is_empty() {
            let mut b = [0u8; 8];
            b[..rem.len()].copy_from_slice(rem);
            self.add(u64::from_le_bytes(b));
        }
    }

    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.add(i);
    }

    #[inline]
    fn write_usize(&mut self, i: usize) {
        self.add(i as u64);
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }
}

#[inline]
fn mix(mut x: u64) -> u64 {
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    x = x.wrapping_mul(0xc4_ce_b9fe_1a85_ec53);
    x ^= x >> 33;
    x
}

/// Hash a key. Bit layout used by the engine (little-endian, lowest bits first):
/// shard index = bits 0..12, bucket index = bits 12.., key_hash32 = bits 32..64.
pub fn hash_key(key: &[u8]) -> u64 {
    let mut h = FxHasher::new();
    h.write(key);
    h.write_u64(0x9e37_79b9_7f4a_7c15);
    mix(h.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        assert_eq!(hash_key(b"foo:bar"), hash_key(b"foo:bar"));
    }

    #[test]
    fn distinct_keys_differ() {
        let a = hash_key(b"foo:bar");
        let b = hash_key(b"foo:baz");
        assert_ne!(a, b);
    }

    #[test]
    fn low_bits_spread_across_shards() {
        let mut cnt = [0usize; 16];
        for i in 0..10_000u64 {
            let h = hash_key(&i.to_le_bytes());
            cnt[(h as usize) & 15] += 1;
        }
        assert!(
            cnt.iter().all(|&c| c > 100),
            "shard bits not spread: {cnt:?}"
        );
    }

    #[test]
    fn middle_bits_spread_across_buckets() {
        let mut cnt = [0usize; 64];
        for i in 0..10_000u64 {
            let h = hash_key(&i.to_le_bytes());
            cnt[((h >> 12) as usize) & 63] += 1;
        }
        assert!(
            cnt.iter().all(|&c| c > 50),
            "bucket bits not spread: {cnt:?}"
        );
    }
}
