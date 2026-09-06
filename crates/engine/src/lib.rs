//! # bonaparte-engine
//!
//! The pure compositor core: render graph, tile scheduler, and the CPU
//! reference renderer used by golden-frame tests.
//!
//! Owns: dependency-sorted render passes, tile culling, deterministic
//! evaluation of the document at a point in time.
//! Must never depend on: tokio, `std::fs`, threads, system time, or any host
//! capability (RULES §1.4 — CI enforces this by building on wasm32).
//! Consequence by design: every API here is synchronous and pure.
//!
//! The GPU backend (wgpu, WGSL effect passes) mounts behind the `gpu`
//! feature using the same graph; this crate's non-GPU core is what makes
//! golden-frame tests and wasm preview possible.

pub mod font;
pub mod gpu;
pub mod graph;
pub mod reference;
pub mod tiles;

pub use font::{sample_glyph_subpixel, FONT_8X8};
pub use gpu::{COMPOSITOR_WGSL, BLEND_WGSL};
pub use graph::{EffectParamValue, GraphError, Node, NodeId, NodeKind, RenderGraph};
pub use reference::{
    blend_channel, blend_pixel_colors, draw_layer, linear_to_srgb_byte, render_comp,
    srgb_byte_to_linear, Affine2D, Frame, FrameView, MediaFrames, NoMedia, RenderError,
};
pub use tiles::{
    render_tile, render_viewport, render_viewport_tiles, Rect, Tile, TileFrame, TileGrid,
    TILE_SIZE,
};
