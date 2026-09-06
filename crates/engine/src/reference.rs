//! The CPU reference renderer — the golden-frame contract made executable.
//!
//! Purpose: a *deterministic, slow, simple* compositor that produces the
//! same pixels the GPU path must produce. Golden-frame tests (RULES §3.5)
//! compare against this; when a GPU pass and the reference disagree, one of
//! them is wrong and CI says which. It also gives us a working renderer
//! before the GPU backend exists.
//!
//! Purity (RULES §1.4): decoded media pixels are INJECTED via the
//! [`MediaFrames`] trait — the engine never touches the filesystem.

use bonaparte_model::{
    BlendMode, Comp, CompId, Layer, LayerKind, MediaId, Project, Property, Time,
};

use crate::font::sample_glyph_subpixel;

/// Maximum allowed recursion depth for nested PreComp rendering.
pub const MAX_PRECOMP_DEPTH: usize = 32;

#[derive(Debug, Clone, thiserror::Error)]
pub enum RenderError {
    #[error("composition {0} not found")]
    CompNotFound(CompId),
    #[error("layer {0} uses a kind the reference renderer does not draw yet")]
    UnsupportedLayerKind(bonaparte_model::LayerId),
    #[error("media {0} has no decoded pixels available at this time")]
    MediaUnavailable(MediaId),
    #[error("cycle detected in nested precomp {0}")]
    PreCompCycle(CompId),
    #[error("maximum precomp nesting depth exceeded at comp {0}")]
    PreCompDepthLimit(CompId),
}

/// Injected decoded media pixels. Implemented natively by `bonaparte-media`
/// (file decode + cache); tests pass `NoMedia`. The engine stays pure.
pub trait MediaFrames {
    /// RGBA8 pixels (top-left origin, row-major) for `media` at `time`,
    /// or None if unavailable.
    fn frame_rgba(&self, media: MediaId, time: Time) -> Option<FrameView<'_>>;
}

pub struct FrameView<'a> {
    pub width: u32,
    pub height: u32,
    pub rgba: &'a [u8],
}

/// Always-empty source: for comps containing no footage layers.
pub struct NoMedia;

impl MediaFrames for NoMedia {
    fn frame_rgba(&self, _media: MediaId, _time: Time) -> Option<FrameView<'_>> {
        None
    }
}

/// An RGBA8 image, top-left origin, row-major.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Frame {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            rgba: vec![0; (width as usize * height as usize) * 4],
        }
    }

    pub fn filled(width: u32, height: u32, color: [f32; 4]) -> Self {
        let r = linear_to_srgb_byte(color[0]);
        let g = linear_to_srgb_byte(color[1]);
        let b = linear_to_srgb_byte(color[2]);
        let a = linear_to_srgb_byte(color[3]);
        let pixel_count = width as usize * height as usize;
        let mut rgba = Vec::with_capacity(pixel_count * 4);
        for _ in 0..pixel_count {
            rgba.push(r);
            rgba.push(g);
            rgba.push(b);
            rgba.push(a);
        }
        Self {
            width,
            height,
            rgba,
        }
    }

    /// Read a pixel as linear floats 0..1.
    pub fn pixel(&self, x: u32, y: u32) -> [f32; 4] {
        if x >= self.width || y >= self.height {
            return [0.0, 0.0, 0.0, 0.0];
        }
        let i = ((y as usize * self.width as usize) + x as usize) * 4;
        [
            srgb_byte_to_linear(self.rgba[i]),
            srgb_byte_to_linear(self.rgba[i + 1]),
            srgb_byte_to_linear(self.rgba[i + 2]),
            srgb_byte_to_linear(self.rgba[i + 3]),
        ]
    }

    /// Set a pixel directly as linear floats 0..1.
    pub fn set_pixel(&mut self, x: u32, y: u32, color: [f32; 4]) {
        if x >= self.width || y >= self.height {
            return;
        }
        let i = ((y as usize * self.width as usize) + x as usize) * 4;
        self.rgba[i] = linear_to_srgb_byte(color[0]);
        self.rgba[i + 1] = linear_to_srgb_byte(color[1]);
        self.rgba[i + 2] = linear_to_srgb_byte(color[2]);
        self.rgba[i + 3] = linear_to_srgb_byte(color[3]);
    }

    /// Blend source pixel onto destination using the specified BlendMode.
    pub fn blend_pixel(&mut self, x: u32, y: u32, src: [f32; 4], mode: BlendMode) {
        if x >= self.width || y >= self.height {
            return;
        }
        let dst = self.pixel(x, y);
        let out = blend_pixel_colors(mode, src, dst);
        self.set_pixel(x, y, out);
    }
}

static SRGB_TO_LINEAR_LUT: std::sync::LazyLock<[f32; 256]> = std::sync::LazyLock::new(|| {
    let mut lut = [0.0f32; 256];
    for i in 0..256 {
        lut[i] = (i as f32 / 255.0).powf(2.2);
    }
    lut
});

#[inline(always)]
pub fn linear_to_srgb_byte(c: f32) -> u8 {
    (c.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0).round() as u8
}

#[inline(always)]
pub fn srgb_byte_to_linear(b: u8) -> f32 {
    SRGB_TO_LINEAR_LUT[b as usize]
}

/// Evaluates one color channel blend function B(s, d).
pub fn blend_channel(mode: BlendMode, s: f32, d: f32) -> f32 {
    match mode {
        BlendMode::Normal => s,
        BlendMode::Multiply => s * d,
        BlendMode::Screen => 1.0 - (1.0 - s) * (1.0 - d),
        BlendMode::Overlay => {
            if d <= 0.5 {
                2.0 * s * d
            } else {
                1.0 - 2.0 * (1.0 - s) * (1.0 - d)
            }
        }
        BlendMode::Add => (s + d).min(1.0),
        BlendMode::Darken => s.min(d),
        BlendMode::Lighten => s.max(d),
        BlendMode::Difference => (s - d).abs(),
    }
}

/// Full 2D alpha blending according to the W3C Compositing and Blending specification.
/// Combines straight source and destination colors and alphas.
pub fn blend_pixel_colors(mode: BlendMode, src: [f32; 4], dst: [f32; 4]) -> [f32; 4] {
    let src_a = src[3].clamp(0.0, 1.0);
    let dst_a = dst[3].clamp(0.0, 1.0);

    if src_a <= 0.0 {
        return dst;
    }
    let out_a = src_a + dst_a * (1.0 - src_a);
    if out_a <= 1e-6 {
        return [0.0, 0.0, 0.0, 0.0];
    }

    let mut out = [0.0f32; 4];
    for c in 0..3 {
        let s = src[c];
        let d = dst[c];
        let b = blend_channel(mode, s, d);
        // Formula: (src_a * (1 - dst_a) * s + dst_a * (1 - src_a) * d + src_a * dst_a * b) / out_a
        let blended_color = (src_a * (1.0 - dst_a) * s
            + dst_a * (1.0 - src_a) * d
            + src_a * dst_a * b)
            / out_a;
        out[c] = blended_color.clamp(0.0, 1.0);
    }
    out[3] = out_a;
    out
}

/// 2D Affine transform [a, b, c, d, tx, ty] mapping (x, y) -> (a*x + c*y + tx, b*x + d*y + ty).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Affine2D {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub tx: f32,
    pub ty: f32,
}

impl Affine2D {
    pub fn identity() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            tx: 0.0,
            ty: 0.0,
        }
    }

    pub fn transform_point(&self, p: [f32; 2]) -> [f32; 2] {
        [
            self.a * p[0] + self.c * p[1] + self.tx,
            self.b * p[0] + self.d * p[1] + self.ty,
        ]
    }

    pub fn determinant(&self) -> f32 {
        self.a * self.d - self.b * self.c
    }

    pub fn inverse(&self) -> Option<Self> {
        let det = self.determinant();
        if det.abs() < 1e-6 {
            return None;
        }
        let inv_det = 1.0 / det;
        Some(Self {
            a: self.d * inv_det,
            b: -self.b * inv_det,
            c: -self.c * inv_det,
            d: self.a * inv_det,
            tx: (self.c * self.ty - self.d * self.tx) * inv_det,
            ty: (self.b * self.tx - self.a * self.ty) * inv_det,
        })
    }

    /// Build the canvas-space affine matrix for `layer` at `time`.
    /// Maps layer local centered coordinates [-w/2, w/2] x [-h/2, h/2] directly to comp canvas pixel coords.
    pub fn from_layer(comp: &Comp, layer: &Layer, time: Time) -> Option<Self> {
        let eff = comp.effective_transform(layer.id, time)?;
        // eff.matrix maps layer local coords to comp center coordinates.
        // Canvas pixel coordinates add (comp.width / 2.0, comp.height / 2.0).
        Some(Self {
            a: eff.matrix[0],
            b: eff.matrix[1],
            c: eff.matrix[2],
            d: eff.matrix[3],
            tx: eff.matrix[4] + comp.width as f32 / 2.0,
            ty: eff.matrix[5] + comp.height as f32 / 2.0,
        })
    }
}

/// Render one comp at one moment in time (ticks), bottom layer first.
///
/// Deterministic: same project + comp + time + frames = same bytes, always.
pub fn render_comp(
    project: &Project,
    comp_id: CompId,
    time: Time,
    frames: &dyn MediaFrames,
) -> Result<Frame, RenderError> {
    let mut active_comps = Vec::new();
    render_comp_internal(project, comp_id, time, frames, &mut active_comps, 0)
}

pub fn render_comp_internal(
    project: &Project,
    comp_id: CompId,
    time: Time,
    frames: &dyn MediaFrames,
    active_comps: &mut Vec<CompId>,
    depth: usize,
) -> Result<Frame, RenderError> {
    let comp = project.comp(comp_id).ok_or(RenderError::CompNotFound(comp_id))?;
    let mut frame = Frame::filled(comp.width, comp.height, comp.background);

    active_comps.push(comp_id);

    // Render in bottom-to-top order (layer_order[0] is bottom).
    for &layer_id in &comp.layer_order {
        let layer = &comp.layers[&layer_id];
        if !layer.visible_at(time) {
            continue;
        }
        draw_layer(
            &mut frame,
            project,
            comp,
            layer,
            time,
            frames,
            active_comps,
            depth,
            0,
            0,
            comp.width,
            comp.height,
            0,
            0,
        )?;
    }

    active_comps.pop();
    Ok(frame)
}

/// Draw a single layer within a sub-rectangle of the canvas (used by both full frame and tile rendering).
#[allow(clippy::too_many_arguments)]
pub fn draw_layer(
    frame: &mut Frame,
    project: &Project,
    comp: &Comp,
    layer: &Layer,
    time: Time,
    frames: &dyn MediaFrames,
    active_comps: &mut Vec<CompId>,
    depth: usize,
    clip_x: u32,
    clip_y: u32,
    clip_w: u32,
    clip_h: u32,
    offset_x: u32,
    offset_y: u32,
) -> Result<(), RenderError> {
    let opacity = match layer.evaluate(Property::Opacity, time) {
        bonaparte_model::PropValue::Scalar(v) => v.clamp(0.0, 1.0),
        _ => 1.0,
    };
    if opacity <= 1e-5 {
        return Ok(());
    }

    // Determine the natural dimensions (width, height) of the layer in local space.
    let (layer_w, layer_h) = match &layer.kind {
        LayerKind::Solid { .. } => (comp.width as f32, comp.height as f32),
        LayerKind::Shape { .. } => (comp.width as f32, comp.height as f32),
        LayerKind::Footage { media } => {
            if let Some(view) = frames.frame_rgba(*media, time) {
                (view.width as f32, view.height as f32)
            } else {
                return Err(RenderError::MediaUnavailable(*media));
            }
        }
        LayerKind::Text { text, size } => {
            let s = size / 8.0;
            let tw = text.len() as f32 * 8.0 * s;
            let th = *size;
            (tw.max(1.0), th.max(1.0))
        }
        LayerKind::PreComp { comp: child_id } => {
            let child = project.comp(*child_id).ok_or(RenderError::CompNotFound(*child_id))?;
            (child.width as f32, child.height as f32)
        }
    };

    let affine = match Affine2D::from_layer(comp, layer, time) {
        Some(m) => m,
        None => return Ok(()),
    };

    let inv_affine = match affine.inverse() {
        Some(m) => m,
        None => return Ok(()), // Singular / collapsed layer, invisible
    };

    // Calculate axis-aligned bounding box of the transformed layer in canvas coords.
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

    // Intersect layer AABB with canvas clip rectangle.
    let start_x = (min_x.floor().max(clip_x as f32) as i64).max(0) as u32;
    let end_x = (max_x.ceil().min((clip_x + clip_w) as f32) as i64)
        .max(0)
        .min(comp.width as i64) as u32;
    let start_y = (min_y.floor().max(clip_y as f32) as i64).max(0) as u32;
    let end_y = (max_y.ceil().min((clip_y + clip_h) as f32) as i64)
        .max(0)
        .min(comp.height as i64) as u32;

    if start_x >= end_x || start_y >= end_y {
        return Ok(());
    }

    // Pre-render PreComp if this is a nested composition.
    let precomp_frame = if let LayerKind::PreComp { comp: child_id } = &layer.kind {
        if active_comps.contains(child_id) {
            return Err(RenderError::PreCompCycle(*child_id));
        }
        if depth >= MAX_PRECOMP_DEPTH {
            return Err(RenderError::PreCompDepthLimit(*child_id));
        }
        let child_time = time - layer.start;
        Some(render_comp_internal(
            project,
            *child_id,
            child_time,
            frames,
            active_comps,
            depth + 1,
        )?)
    } else {
        None
    };

    let text_chars: Option<Vec<char>> = if let LayerKind::Text { text, .. } = &layer.kind {
        Some(text.chars().collect())
    } else {
        None
    };

    // Evaluate pixels across the intersected bounding box.
    for py in start_y..end_y {
        let dest_y = py.saturating_sub(offset_y);
        if dest_y >= frame.height {
            continue;
        }
        for px in start_x..end_x {
            let dest_x = px.saturating_sub(offset_x);
            if dest_x >= frame.width {
                continue;
            }

            // Map canvas pixel (px + 0.5, py + 0.5) to local layer space.
            let [lx, ly] = inv_affine.transform_point([px as f32 + 0.5, py as f32 + 0.5]);

            // Normalized local coordinates u, v in [0.0, 1.0].
            let u = (lx + hw) / layer_w;
            let v = (ly + hh) / layer_h;

            if !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
                continue;
            }

            let src_color = match &layer.kind {
                LayerKind::Solid { color } => {
                    [color[0], color[1], color[2], color[3] * opacity]
                }
                LayerKind::Shape { color, generator } => {
                    let mut cov = 1.0f32;

                    if let Some(gen) = generator {
                        if gen == "builtin.circle" {
                            let norm_x = lx / hw;
                            let norm_y = ly / hh;
                            let dist = (norm_x * norm_x + norm_y * norm_y).sqrt();

                            let min_dim = hw.min(hh);
                            let edge_w = if min_dim > 1e-3 { 1.0 / min_dim } else { 0.05 };

                            if dist > 1.0 + edge_w {
                                continue;
                            } else if dist > 1.0 - edge_w {
                                cov = ((1.0 + edge_w - dist) / (2.0 * edge_w)).clamp(0.0, 1.0);
                            }
                        }
                    }

                    if cov <= 1e-4 {
                        continue;
                    }

                    [
                        color[0],
                        color[1],
                        color[2],
                        color[3] * opacity * cov,
                    ]
                }
                LayerKind::Footage { media } => {
                    let view = frames
                        .frame_rgba(*media, time)
                        .ok_or(RenderError::MediaUnavailable(*media))?;
                    let sx = ((u * view.width as f32) as u32).min(view.width.saturating_sub(1));
                    let sy = ((v * view.height as f32) as u32).min(view.height.saturating_sub(1));
                    let idx = ((sy as usize * view.width as usize) + sx as usize) * 4;
                    [
                        srgb_byte_to_linear(view.rgba[idx]),
                        srgb_byte_to_linear(view.rgba[idx + 1]),
                        srgb_byte_to_linear(view.rgba[idx + 2]),
                        srgb_byte_to_linear(view.rgba[idx + 3]) * opacity,
                    ]
                }
                LayerKind::Text { size, .. } => {
                    let s = size / 8.0;
                    let local_x = u * layer_w;
                    let local_y = v * layer_h;
                    let char_w = 8.0 * s;
                    let char_idx = (local_x / char_w) as usize;
                    let chars = text_chars.as_ref().unwrap();
                    if char_idx < chars.len() {
                        let c = chars[char_idx];
                        let gx = (local_x - char_idx as f32 * char_w) / s;
                        let gy = local_y / s;
                        let cov = sample_glyph_subpixel(c, gx, gy);
                        if cov <= 1e-4 {
                            continue;
                        }
                        [1.0, 1.0, 1.0, opacity * cov]
                    } else {
                        continue;
                    }
                }
                LayerKind::PreComp { .. } => {
                    let pf = precomp_frame.as_ref().unwrap();
                    let sx = ((u * pf.width as f32) as u32).min(pf.width.saturating_sub(1));
                    let sy = ((v * pf.height as f32) as u32).min(pf.height.saturating_sub(1));
                    let p = pf.pixel(sx, sy);
                    [p[0], p[1], p[2], p[3] * opacity]
                }
            };

            frame.blend_pixel(dest_x, dest_y, src_color, layer.blend_mode);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bonaparte_model::{Easing, History, Keyframe, Op, StaticTransform, Track};

    fn solid(color: [f32; 4]) -> Layer {
        let mut l = Layer::new("solid", LayerKind::Solid { color }, Time::ZERO, Time(10 * bonaparte_model::TICKS_PER_SEC));
        l.transform = StaticTransform::default();
        l
    }

    fn commit_layer(p: &mut Project, c: bonaparte_model::CompId, layer: Layer) -> bonaparte_model::LayerId {
        let mut h = History::new();
        h.commit(p, Op::AddLayer { comp: c, layer }).unwrap();
        bonaparte_model::LayerId(p.next_layer.0 - 1)
    }

    #[test]
    fn renders_background_and_layering_is_bottom_to_top() {
        let mut p = Project::new("t");
        let c = p.create_comp("main", 4, 4, bonaparte_model::FrameRate::FPS_30, Time(10 * bonaparte_model::TICKS_PER_SEC));
        p.comps.get_mut(&c).unwrap().background = [0.0, 0.0, 0.0, 1.0];
        commit_layer(&mut p, c, solid([1.0, 0.0, 0.0, 1.0])); // red on top
        let f = render_comp(&p, c, Time::ZERO, &NoMedia).unwrap();
        let px = f.pixel(0, 0);
        assert!((px[0] - 1.0).abs() < 0.01); // red wins: it was added on top
    }

    #[test]
    fn opacity_track_wins_over_static() {
        let mut p = Project::new("t");
        let c = p.create_comp(
            "main",
            2,
            2,
            bonaparte_model::FrameRate::FPS_30,
            Time(10 * bonaparte_model::TICKS_PER_SEC),
        );
        p.comps.get_mut(&c).unwrap().background = [0.0, 0.0, 0.0, 1.0];
        let mut layer = solid([1.0, 0.0, 0.0, 1.0]);
        layer.transform.opacity = 1.0;
        // Animated: fade 0 → 1 over 0..1s.
        let mut track = Track::new();
        track.set_key(Keyframe {
            time: Time::ZERO,
            value: bonaparte_model::PropValue::Scalar(0.0),
            easing: Easing::Linear,
        });
        track.set_key(Keyframe {
            time: Time(bonaparte_model::TICKS_PER_SEC),
            value: bonaparte_model::PropValue::Scalar(1.0),
            easing: Easing::Linear,
        });
        layer.tracks.insert(Property::Opacity, track);

        p.insert_layer(c, layer);

        let f0 = render_comp(&p, c, Time::ZERO, &NoMedia).unwrap();
        let f1 = render_comp(&p, c, Time(bonaparte_model::TICKS_PER_SEC / 2), &NoMedia).unwrap();
        assert!(f0.pixel(0, 0)[0] < 0.05, "opacity 0 at t=0 must show background");
        let mid = f1.pixel(0, 0);
        assert!(mid[0] > 0.4 && mid[0] < 0.6, "half-faded red at t=0.5s");
    }

    struct OneImage;
    impl MediaFrames for OneImage {
        fn frame_rgba(&self, _media: MediaId, _time: Time) -> Option<FrameView<'_>> {
            Some(FrameView {
                width: 2,
                height: 2,
                rgba: &[255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255],
            })
        }
    }

    #[test]
    fn footage_layer_draws_injected_pixels() {
        let mut p = Project::new("t");
        let c = p.create_comp(
            "main",
            4,
            4,
            bonaparte_model::FrameRate::FPS_30,
            Time(10 * bonaparte_model::TICKS_PER_SEC),
        );
        let mut layer = Layer::new(
            "img",
            LayerKind::Footage {
                media: bonaparte_model::MediaId(1),
            },
            Time::ZERO,
            Time(10 * bonaparte_model::TICKS_PER_SEC),
        );
        layer.transform = StaticTransform {
            scale: [100.0, 100.0],
            ..StaticTransform::default()
        };
        commit_layer(&mut p, c, layer);
        let f = render_comp(&p, c, Time::ZERO, &OneImage).unwrap();
        let px = f.pixel(1, 1);
        assert!(px[0] > 0.9 && px[1] < 0.1, "injected red image drawn");
    }

    #[test]
    fn precomp_cycle_detection_stops_infinite_recursion() {
        let mut p = Project::new("PreComp Cycle");
        let c1 = p.create_comp("C1", 10, 10, bonaparte_model::FrameRate::FPS_30, Time(100));
        let c2 = p.create_comp("C2", 10, 10, bonaparte_model::FrameRate::FPS_30, Time(100));

        let l1 = Layer::new("pre1", LayerKind::PreComp { comp: c2 }, Time::ZERO, Time(100));
        let l2 = Layer::new("pre2", LayerKind::PreComp { comp: c1 }, Time::ZERO, Time(100));
        p.insert_layer(c1, l1);
        p.insert_layer(c2, l2);

        let res = render_comp(&p, c1, Time::ZERO, &NoMedia);
        assert!(matches!(res, Err(RenderError::PreCompCycle(_))));
    }
}
