//! Tile grid and viewport culling utilities.
//!
//! Unfiltered compositions can render individual 256×256 regions. Compositions
//! containing effects use a full-frame reference render followed by a crop, so
//! neighborhood filters do not produce tile seams. ROI/halo propagation, a shared
//! filtered-frame cache, and bounded GPU memory scheduling remain future work.

use bonaparte_model::{BlendMode, Comp, CompId, Layer, LayerKind, Project, Time};
use serde::{Deserialize, Serialize};

use crate::reference::{draw_layer, Affine2D, Frame, MediaFrames, RenderError};

/// Tile edge length in pixels. 256 balances upload overhead vs eviction
/// granularity on integrated GPUs (shared memory, hard budgets).
pub const TILE_SIZE: u32 = 256;

/// A tile in tile-coordinates (multiply by TILE_SIZE for pixels).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Tile {
    pub x: u32,
    pub y: u32,
}

/// 2D rectangle with floating point coordinates and dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn intersects(&self, other: &Rect) -> bool {
        self.x < other.x + other.width
            && self.x + self.width > other.x
            && self.y < other.y + other.height
            && self.y + self.height > other.y
    }

    pub fn contains_point(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.x + self.width && py >= self.y && py < self.y + self.height
    }
}

/// An RGBA8 tile buffer for a 256×256 region (or smaller on comp edges).
#[derive(Debug, Clone, PartialEq)]
pub struct TileFrame {
    pub tile: Tile,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl TileFrame {
    pub fn new(tile: Tile, width: u32, height: u32) -> Self {
        Self {
            tile,
            width,
            height,
            rgba: vec![0; (width as usize * height as usize) * 4],
        }
    }

    pub fn filled(tile: Tile, width: u32, height: u32, color: [f32; 4]) -> Self {
        let frame = Frame::filled(width, height, color);
        Self {
            tile,
            width,
            height,
            rgba: frame.rgba,
        }
    }

    pub fn pixel(&self, x: u32, y: u32) -> [f32; 4] {
        let frame = Frame {
            width: self.width,
            height: self.height,
            rgba: self.rgba.clone(),
        };
        frame.pixel(x, y)
    }

    pub fn set_pixel(&mut self, x: u32, y: u32, color: [f32; 4]) {
        let mut frame = Frame {
            width: self.width,
            height: self.height,
            rgba: std::mem::take(&mut self.rgba),
        };
        frame.set_pixel(x, y, color);
        self.rgba = frame.rgba;
    }

    pub fn blend_pixel(&mut self, x: u32, y: u32, src: [f32; 4], mode: BlendMode) {
        let mut frame = Frame {
            width: self.width,
            height: self.height,
            rgba: std::mem::take(&mut self.rgba),
        };
        frame.blend_pixel(x, y, src, mode);
        self.rgba = frame.rgba;
    }
}

/// The tile grid covering one comp's canvas.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileGrid {
    pub width: u32,
    pub height: u32,
}

impl TileGrid {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    /// Grid dimensions in tiles (rounded up — edge tiles are partial).
    pub fn grid_width(&self) -> u32 {
        self.width.div_ceil(TILE_SIZE)
    }

    pub fn grid_height(&self) -> u32 {
        self.height.div_ceil(TILE_SIZE)
    }

    pub fn tile_count(&self) -> u32 {
        self.grid_width() * self.grid_height()
    }

    /// Pixel rectangle covering this tile on the comp canvas.
    pub fn tile_rect(&self, tile: Tile) -> Rect {
        let px = (tile.x * TILE_SIZE) as f32;
        let py = (tile.y * TILE_SIZE) as f32;
        let pw = (TILE_SIZE.min(self.width.saturating_sub(tile.x * TILE_SIZE))) as f32;
        let ph = (TILE_SIZE.min(self.height.saturating_sub(tile.y * TILE_SIZE))) as f32;
        Rect::new(px, py, pw, ph)
    }

    /// The tiles intersecting a viewport rectangle, clamped to the comp
    /// bounds. Deterministic order: row-major, top-left first — the same
    /// order the compositor evicts from (first-requested, first-cached).
    pub fn tiles_for_viewport(&self, vx: u32, vy: u32, vw: u32, vh: u32) -> Vec<Tile> {
        if vx >= self.width || vy >= self.height {
            return Vec::new();
        }
        let x0 = vx / TILE_SIZE;
        let y0 = vy / TILE_SIZE;
        let x1 = vx.saturating_add(vw).min(self.width).div_ceil(TILE_SIZE);
        let y1 = vy.saturating_add(vh).min(self.height).div_ceil(TILE_SIZE);
        let mut tiles = Vec::new();
        for ty in y0..y1 {
            for tx in x0..x1 {
                tiles.push(Tile { x: tx, y: ty });
            }
        }
        tiles
    }

    /// Calculate the axis-aligned bounding box of `layer` in comp canvas coordinates at `time`.
    pub fn layer_bounding_box(
        &self,
        comp: &Comp,
        layer: &Layer,
        time: Time,
        frames: &dyn MediaFrames,
    ) -> Option<Rect> {
        if !layer.visible_at(time) {
            return None;
        }

        let (layer_w, layer_h) = match &layer.kind {
            LayerKind::Adjustment {} | LayerKind::Solid { .. } => {
                (comp.width as f32, comp.height as f32)
            }
            LayerKind::Shape { style, .. } => {
                let size = style
                    .size
                    .unwrap_or([comp.width as f32, comp.height as f32]);
                (size[0], size[1])
            }
            LayerKind::Footage { media } => {
                if let Some(view) = frames.frame_rgba(*media, time) {
                    (view.width as f32, view.height as f32)
                } else {
                    (comp.width as f32, comp.height as f32)
                }
            }
            LayerKind::Text { text, size, style } => {
                let size = crate::typography::measure_text(text, *size, style.bold, style.tracking);
                (size[0], size[1])
            }
            LayerKind::PreComp { .. } => (comp.width as f32, comp.height as f32),
        };

        let affine = Affine2D::from_layer(comp, layer, time)?;
        let hw = layer_w / 2.0;
        let hh = layer_h / 2.0;
        let corners = [
            affine.transform_point([-hw, -hh]),
            affine.transform_point([hw, -hh]),
            affine.transform_point([hw, hh]),
            affine.transform_point([-hw, hh]),
        ];

        let mut min_x = f32::MAX;
        let mut max_x = f32::MIN;
        let mut min_y = f32::MAX;
        let mut max_y = f32::MIN;
        for &[cx, cy] in &corners {
            min_x = min_x.min(cx);
            max_x = max_x.max(cx);
            min_y = min_y.min(cy);
            max_y = max_y.max(cy);
        }

        Some(Rect::new(min_x, min_y, max_x - min_x, max_y - min_y))
    }

    /// True if `layer`'s bounding box intersects the given `tile`.
    pub fn layer_intersects_tile(
        &self,
        comp: &Comp,
        layer: &Layer,
        tile: Tile,
        time: Time,
        frames: &dyn MediaFrames,
    ) -> bool {
        let tile_rect = self.tile_rect(tile);
        if let Some(layer_box) = self.layer_bounding_box(comp, layer, time, frames) {
            tile_rect.intersects(&layer_box)
        } else {
            false
        }
    }

    /// Returns the active visible layers intersecting `tile` in bottom-to-top rendering order.
    pub fn active_layers_for_tile<'a>(
        &self,
        comp: &'a Comp,
        tile: Tile,
        time: Time,
        frames: &dyn MediaFrames,
    ) -> Vec<&'a Layer> {
        let mut layers = Vec::new();
        for &layer_id in &comp.layer_order {
            if let Some(layer) = comp.layers.get(&layer_id) {
                if self.layer_intersects_tile(comp, layer, tile, time, frames) {
                    layers.push(layer);
                }
            }
        }
        layers
    }
}

/// Render a single 256×256 tile of a composition at `time`.
///
/// Unfiltered scenes allocate a tile. Filtered scenes use a full-frame fallback
/// to preserve neighborhood sampling and adjustment-layer correctness.
/// Effect-aware region-of-interest scheduling is a future optimization.
pub fn render_tile(
    project: &Project,
    comp_id: CompId,
    tile: Tile,
    time: Time,
    frames: &dyn MediaFrames,
) -> Result<TileFrame, RenderError> {
    let comp = project
        .comp(comp_id)
        .ok_or(RenderError::CompNotFound(comp_id))?;
    let grid = TileGrid::new(comp.width, comp.height);

    if tile.x >= grid.grid_width() || tile.y >= grid.grid_height() {
        return Ok(TileFrame::new(tile, 0, 0));
    }

    if needs_full_frame_effects(project) {
        let frame = crate::reference::render_comp(project, comp_id, time, frames)?;
        return Ok(crop_tile(&frame, tile));
    }
    let rect = grid.tile_rect(tile);
    let tile_w = rect.width as u32;
    let tile_h = rect.height as u32;

    let mut frame = Frame::filled(tile_w, tile_h, comp.background);
    let mut active_comps = vec![comp_id];

    let clip_x = tile.x * TILE_SIZE;
    let clip_y = tile.y * TILE_SIZE;

    // Render each intersecting layer into the tile frame.
    for &layer_id in &comp.layer_order {
        let layer = &comp.layers[&layer_id];
        if !grid.layer_intersects_tile(comp, layer, tile, time, frames) {
            continue;
        }

        draw_layer(
            &mut frame,
            project,
            comp,
            layer,
            time,
            frames,
            &mut active_comps,
            0,
            clip_x,
            clip_y,
            tile_w,
            tile_h,
            clip_x,
            clip_y,
        )?;
    }

    Ok(TileFrame {
        tile,
        width: tile_w,
        height: tile_h,
        rgba: frame.rgba,
    })
}

/// Render only the tiles intersecting a viewport rectangle at `time`.
/// Memory footprint is strictly O(viewport) RAM.
#[allow(clippy::too_many_arguments)]
pub fn render_viewport_tiles(
    project: &Project,
    comp_id: CompId,
    vx: u32,
    vy: u32,
    vw: u32,
    vh: u32,
    time: Time,
    frames: &dyn MediaFrames,
) -> Result<Vec<TileFrame>, RenderError> {
    let comp = project
        .comp(comp_id)
        .ok_or(RenderError::CompNotFound(comp_id))?;
    let grid = TileGrid::new(comp.width, comp.height);
    let tiles = grid.tiles_for_viewport(vx, vy, vw, vh);

    if needs_full_frame_effects(project) {
        let frame = crate::reference::render_comp(project, comp_id, time, frames)?;
        return Ok(tiles
            .into_iter()
            .map(|tile| crop_tile(&frame, tile))
            .collect());
    }
    let mut result = Vec::with_capacity(tiles.len());
    for tile in tiles {
        let tile_frame = render_tile(project, comp_id, tile, time, frames)?;
        result.push(tile_frame);
    }
    Ok(result)
}

/// Render a viewport region by stitching the rendered tiles into an O(viewport) `Frame`.
#[allow(clippy::too_many_arguments)]
pub fn render_viewport(
    project: &Project,
    comp_id: CompId,
    vx: u32,
    vy: u32,
    vw: u32,
    vh: u32,
    time: Time,
    frames: &dyn MediaFrames,
) -> Result<Frame, RenderError> {
    let comp = project
        .comp(comp_id)
        .ok_or(RenderError::CompNotFound(comp_id))?;
    let mut out_frame = Frame::filled(vw, vh, comp.background);
    let tiles = render_viewport_tiles(project, comp_id, vx, vy, vw, vh, time, frames)?;

    for tile_frame in tiles {
        let tile_origin_x = tile_frame.tile.x * TILE_SIZE;
        let tile_origin_y = tile_frame.tile.y * TILE_SIZE;

        for ty in 0..tile_frame.height {
            let canvas_y = tile_origin_y + ty;
            if canvas_y < vy || canvas_y >= vy + vh {
                continue;
            }
            let out_y = canvas_y - vy;

            for tx in 0..tile_frame.width {
                let canvas_x = tile_origin_x + tx;
                if canvas_x < vx || canvas_x >= vx + vw {
                    continue;
                }
                let out_x = canvas_x - vx;

                let px = tile_frame.pixel(tx, ty);
                out_frame.set_pixel(out_x, out_y, px);
            }
        }
    }

    Ok(out_frame)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reference::NoMedia;
    use bonaparte_model::{LayerKind, StaticTransform};

    #[test]
    fn full_viewport_on_1080p_costs_40_tiles_not_a_4k_buffer() {
        let grid = TileGrid::new(1920, 1080);
        assert_eq!((grid.grid_width(), grid.grid_height()), (8, 5));
        assert_eq!(grid.tile_count(), 40);
        let visible = grid.tiles_for_viewport(0, 0, 1920, 1080);
        assert_eq!(visible.len(), 40);
    }

    #[test]
    fn zoomed_viewport_culls_to_intersecting_tiles_only() {
        let grid = TileGrid::new(1920, 1080);
        let visible = grid.tiles_for_viewport(200, 150, 300, 300);
        assert_eq!(visible.len(), 4);
        assert!(visible.contains(&Tile { x: 0, y: 0 }));
        assert!(visible.contains(&Tile { x: 1, y: 1 }));
    }

    #[test]
    fn viewports_outside_bounds_clamp_without_panicking() {
        let grid = TileGrid::new(1000, 1000);
        let visible = grid.tiles_for_viewport(990, 990, 5000, 5000);
        assert_eq!(visible.len(), 1);
        assert_eq!(grid.tiles_for_viewport(2000, 2000, 10, 10), vec![]);
    }

    #[test]
    fn render_tile_matches_render_comp_pixel_for_pixel() {
        let mut p = Project::new("test");
        let c = p.create_comp("c", 300, 300, bonaparte_model::FrameRate::FPS_30, Time(100));
        p.comps.get_mut(&c).unwrap().background = [0.1, 0.2, 0.3, 1.0];

        let mut red_layer = Layer::new(
            "red",
            LayerKind::Solid {
                color: [1.0, 0.0, 0.0, 1.0],
            },
            Time::ZERO,
            Time(100),
        );
        red_layer.transform = StaticTransform::default();
        p.insert_layer(c, red_layer);

        let full = crate::reference::render_comp(&p, c, Time::ZERO, &NoMedia).unwrap();
        let tile0 = render_tile(&p, c, Tile { x: 0, y: 0 }, Time::ZERO, &NoMedia).unwrap();

        assert_eq!(tile0.width, 256);
        assert_eq!(tile0.height, 256);
        assert_eq!(tile0.pixel(10, 10), full.pixel(10, 10));
    }
}

fn needs_full_frame_effects(project: &Project) -> bool {
    project
        .comps
        .values()
        .flat_map(|c| c.layers.values())
        .any(|l| {
            l.effects.iter().any(|e| e.enabled)
                || matches!(
                    l.kind,
                    LayerKind::Text { .. }
                        | LayerKind::PreComp { .. }
                        | LayerKind::Shape {
                            generator: Some(_),
                            ..
                        }
                )
        })
}
fn crop_tile(frame: &Frame, tile: Tile) -> TileFrame {
    let x = tile.x * TILE_SIZE;
    let y = tile.y * TILE_SIZE;
    let w = frame.width.saturating_sub(x).min(TILE_SIZE);
    let h = frame.height.saturating_sub(y).min(TILE_SIZE);
    let mut out = TileFrame::new(tile, w, h);
    for row in 0..h {
        let i = (((y + row) * frame.width + x) * 4) as usize;
        let d = (row * w * 4) as usize;
        out.rgba[d..d + w as usize * 4].copy_from_slice(&frame.rgba[i..i + w as usize * 4]);
    }
    out
}
