//! # Atra — the cache engine core.
//!
//! A sharded, seqlock-read, epoch-reclaimed in-RAM cache engine: the
//! project owner's Meridian engine core, vendored here (no third-party
//! notice required) and integrated as Bonaparte's hot media-frame cache.
//!
//! Read path: hash → shard → bounded probe window → per-bucket seqlock
//! validation. No locks on reads. Writes take one mutex per shard.
//! Reclamation: epoch-based; TTL via a timing wheel; eviction by shard
//! memory budget. `sweep()` is the single maintenance entry point.
pub mod engine;
pub mod epoch;
pub mod flash;
pub mod hash;
pub mod l0;
pub mod shard_count;
pub mod types;

pub use engine::{
    Engine, EngineOptions, EngineStats, SetOpts, SetOutcome, Slo, TtlStatus, ValueRef, PROBE_LIMIT,
};
pub use flash::FlashTier;
pub use types::WAYS;
