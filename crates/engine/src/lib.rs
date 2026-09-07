//! # bonaparte-engine
//!
//! Pure synchronous CPU compositor, render graph and tile APIs.
//! Rendering receives decoded media buffers; it performs no file or network I/O.
//! The browser development bridge and native desktop both use this implementation.
//!
//! Native GPU execution lives in bonaparte-gpu and consumes preview::Scene.
//! The legacy gpu module contains shader/descriptor fixtures only.
//! Effect-bearing tile requests currently render a full composition then crop;
//! there is no O(tile) memory guarantee for that path.

pub mod font;
pub mod gpu;
pub mod graph;
pub mod import_obj;
pub mod import_svg;
pub mod preview;
pub mod reference;
pub mod tiles;
pub mod trace;
pub mod typography;
pub mod vector;

pub mod cancel;
pub use font::{sample_glyph_subpixel, FONT_8X8};
pub use gpu::{BLEND_WGSL, COMPOSITOR_WGSL};
pub use graph::{EffectParamValue, GraphError, Node, NodeId, NodeKind, RenderGraph};
pub use reference::{
    blend_channel, blend_pixel_colors, draw_layer, linear_to_srgb_byte, render_comp,
    srgb_byte_to_linear, Affine2D, Frame, FrameView, MediaFrames, NoMedia, RenderError,
    MAX_PRECOMP_DEPTH,
};
pub use tiles::{
    render_tile, render_viewport, render_viewport_tiles, Rect, Tile, TileFrame, TileGrid, TILE_SIZE,
};
