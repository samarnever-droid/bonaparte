//! Derived shard count (spec §3.1):
//!
//! S = clamp(2^ceil(log2(4C)), 16, 2^floor(log2(M / M_min)))
//!
//! The upper bound is memory-derived, not a magic constant: a shard costs at
//! least a table, a sweeper cursor and retirement state, so memory floors the
//! shard count just as core count raises it.

/// Minimum viable shard (spec §3.1 uses 64 MiB).
pub const MIN_SHARD_BYTES: usize = 64 << 20;

fn floor_pow2(x: usize) -> usize {
    if x == 0 {
        0
    } else {
        1 << (usize::BITS - 1 - x.leading_zeros())
    }
}

pub fn derived_shard_count(cores: usize, memory_bytes: usize, min_shard_bytes: usize) -> usize {
    let base = (cores.max(1) * 4).next_power_of_two();
    let upper = floor_pow2(memory_bytes / min_shard_bytes.max(1));
    base.clamp(16, upper.max(16))
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: usize = 1 << 30;

    #[test]
    fn spec_examples() {
        // 4-core VM: minimum 16 shards
        assert_eq!(derived_shard_count(4, 8 * GIB, MIN_SHARD_BYTES), 16);
        // 64 cores, ample memory: 4x oversubscription
        assert_eq!(derived_shard_count(64, 64 * GIB, MIN_SHARD_BYTES), 256);
        // 512 cores, 4 TiB: memory cap is not binding
        assert_eq!(
            derived_shard_count(512, 4 * 1024 * GIB, MIN_SHARD_BYTES),
            2048
        );
    }

    #[test]
    fn memory_derived_upper_bound() {
        // 256 cores would ask for 1024 shards, but 8 GiB / 64 MiB floors it at 128
        assert_eq!(derived_shard_count(256, 8 * GIB, MIN_SHARD_BYTES), 128);
    }
}
