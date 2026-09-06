//! # bonaparte-media
//!
//! NATIVE-ONLY crate (RULES §1.5): Crash-isolated FFmpeg process orchestration,
//! media asset probing, PerceptionCard generation, proxy transcoding,
//! two-tier disk-backed frame cache with Flat RAM guarantee, and streaming MP4 export.
//!
//! Owns: everything that touches media files on disk or spawns media child processes.
//! Must never: be compiled into the WASM target, or be depended on by the engine core.

pub mod cache;
pub mod decode;
pub mod export;
pub mod probe;
pub mod proxy;

pub use cache::DiskPlaybackCache;
pub use decode::{decode_frame, DecodeError, DecodedFrame};
pub use export::{export_mp4_stream, ExportConfig, ExportError, ExportStats};
pub use probe::{compute_perception_card, parse_rational_framerate, probe_asset, ProbeError};
pub use proxy::{generate_proxy, ProxyError, ProxyInfo};

// Re-export engine MediaFrames trait for convenience
pub use bonaparte_engine::reference::{FrameView, MediaFrames};
