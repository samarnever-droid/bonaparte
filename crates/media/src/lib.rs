//! # bonaparte-media
//!
//! Native-only media I/O: bounded FFmpeg subprocess orchestration, probing,
//! image/video utilities, decoded audio acquisition and streaming exports.
//! Audio PCM is disk-backed. Resource budgets are not a flat-RAM guarantee;
//! child processes inherit caller privileges and are not a security sandbox.
//! The pure engine/audio crates do not depend on this crate.

pub mod cache;
pub mod decode;
pub mod export;
pub mod probe;
pub mod proxy;

pub use cache::DiskPlaybackCache;
pub use decode::{decode_frame, DecodeError, DecodedFrame};
pub use export::{export_mp4_stream, ExportConfig, ExportError, ExportStats};
pub use probe::{
    compute_perception_card, parse_rational_framerate, probe_asset, video_dimensions, ProbeError,
};
pub use proxy::{generate_proxy, ProxyError, ProxyInfo};

// Re-export engine MediaFrames trait for convenience
pub use bonaparte_engine::reference::{FrameView, MediaFrames};

pub mod audio;
