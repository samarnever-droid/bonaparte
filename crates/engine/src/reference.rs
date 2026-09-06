//! The CPU reference renderer — the golden-frame contract made executable.
//!
//! Executed by the editor today and covered by pixel/behavior regression tests.
//! The future GPU renderer should be compared against this implementation; no
//! GPU execution or CPU/GPU parity testing is claimed by this milestone.
//!
//! Purity (RULES §1.4): decoded media pixels are INJECTED via the
//! [`MediaFrames`] trait — the engine never touches the filesystem.

use bonaparte_model::{BlendMode, Comp, CompId, Layer, LayerKind, MediaId, Project, Time};

use crate::typography::{measure_text, rasterize_text};
use bonaparte_effects::registry::builtin_registry;
use bonaparte_effects::{CpuFrame, EffectRegistry};

/// Maximum allowed recursion depth for nested PreComp rendering.
pub const MAX_PRECOMP_DEPTH: usize = 32;

#[derive(Debug, Clone, thiserror::Error)]
pub enum RenderError {
    #[error("{0}")]
    Invalid(String),
    #[error("Effect evaluation failed: {0}")]
    Effect(String),
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
    /// Immutable shared pixels allow preview backends to reuse uploads safely.
    /// Streaming providers may return None and keep using borrowed frame_rgba.
    fn shared_frame(&self, _media: MediaId, _time: Time) -> Option<std::sync::Arc<CpuFrame>> {
        None
    }
}

pub struct FrameView<'a> {
    pub width: u32,
    pub height: u32,
    pub rgba: &'a [u8],
}

impl FrameView<'_> {
    pub fn pixel(&self, x: u32, y: u32) -> [f32; 4] {
        if x >= self.width || y >= self.height {
            return [0.0, 0.0, 0.0, 0.0];
        }
        let i = ((y as usize * self.width as usize) + x as usize) * 4;
        [
            srgb_byte_to_linear(self.rgba[i]),
            srgb_byte_to_linear(self.rgba[i + 1]),
            srgb_byte_to_linear(self.rgba[i + 2]),
            self.rgba[i + 3] as f32 / 255.0,
        ]
    }
    pub fn sample_bilinear(&self, u: f32, v: f32) -> [f32; 4] {
        if self.width == 0 || self.height == 0 {
            return [0.0; 4];
        }
        let fx = (u * self.width as f32 - 0.5).clamp(0.0, self.width.saturating_sub(1) as f32);
        let fy = (v * self.height as f32 - 0.5).clamp(0.0, self.height.saturating_sub(1) as f32);
        let x0 = fx.floor() as u32;
        let y0 = fy.floor() as u32;
        let x1 = (x0 + 1).min(self.width.saturating_sub(1));
        let y1 = (y0 + 1).min(self.height.saturating_sub(1));
        let tx = fx - x0 as f32;
        let ty = fy - y0 as f32;
        let mut out = [0.0f32; 4];
        for (x, y, weight) in [
            (x0, y0, (1.0 - tx) * (1.0 - ty)),
            (x1, y0, tx * (1.0 - ty)),
            (x0, y1, (1.0 - tx) * ty),
            (x1, y1, tx * ty),
        ] {
            let i = ((y * self.width + x) * 4) as usize;
            let alpha_weight = self.rgba[i + 3] as f32 / 255.0 * weight;
            if alpha_weight <= 0.0 {
                continue;
            }
            for c in 0..3 {
                out[c] += srgb_byte_to_linear(self.rgba[i + c]) * alpha_weight;
            }
            out[3] += alpha_weight;
        }
        if out[3] > 0.000001 {
            for c in 0..3 {
                out[c] /= out[3];
            }
        }
        out
    }
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
        let a = (color[3].clamp(0.0, 1.0) * 255.0).round() as u8;
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
        FrameView {
            width: self.width,
            height: self.height,
            rgba: &self.rgba,
        }
        .pixel(x, y)
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
        self.rgba[i + 3] = (color[3].clamp(0.0, 1.0) * 255.0).round() as u8;
    }

    pub fn sample_bilinear(&self, u: f32, v: f32) -> [f32; 4] {
        FrameView {
            width: self.width,
            height: self.height,
            rgba: &self.rgba,
        }
        .sample_bilinear(u, v)
    }

    /// Blend source pixel onto destination using the specified BlendMode.
    pub fn blend_pixel(&mut self, x: u32, y: u32, src: [f32; 4], mode: BlendMode) {
        if x >= self.width || y >= self.height {
            return;
        }
        if src[3] <= 0.0 {
            return;
        }
        if src[3] >= 1.0 && mode == BlendMode::Normal {
            self.set_pixel(x, y, src);
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
        lut[i] = bonaparte_effects::grading::srgb_to_linear(i as f32 / 255.0);
    }
    lut
});

#[inline(always)]
pub fn linear_to_srgb_byte(c: f32) -> u8 {
    (bonaparte_effects::grading::linear_to_srgb(c.clamp(0.0, 1.0)) * 255.0).round() as u8
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
        let blended_color =
            (src_a * (1.0 - dst_a) * s + dst_a * (1.0 - src_a) * d + src_a * dst_a * b) / out_a;
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
        Self::from_layer_with(comp, layer, time, None)
    }
    pub fn from_layer_with(
        comp: &Comp,
        layer: &Layer,
        time: Time,
        transient: Option<&bonaparte_model::TransformOverride>,
    ) -> Option<Self> {
        let eff = comp.effective_transform_with(layer.id, time, transient)?;
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
    render_comp_with_registry(project, comp_id, time, frames, builtin_registry())
}

/// Render with a caller-owned registry, including third-party CPU plugins.
pub fn render_comp_with_registry(
    project: &Project,
    comp_id: CompId,
    time: Time,
    frames: &dyn MediaFrames,
    registry: &EffectRegistry,
) -> Result<Frame, RenderError> {
    static CACHE: std::sync::LazyLock<crate::preview::SourceCache> =
        std::sync::LazyLock::new(crate::preview::SourceCache::default);
    let scene = crate::preview::prepare_scene(
        project,
        comp_id,
        time,
        frames,
        registry,
        crate::preview::Resolution::FULL,
        &CACHE,
    )?;
    crate::preview::render_scene_cpu(&scene, registry)
}

/// Historical fixed-density implementation for compatibility comparisons only.
pub fn render_comp_legacy(
    project: &Project,
    comp_id: CompId,
    time: Time,
    frames: &dyn MediaFrames,
) -> Result<Frame, RenderError> {
    render_comp_registered(
        project,
        comp_id,
        time,
        frames,
        &mut Vec::new(),
        0,
        builtin_registry(),
    )
}

pub fn render_comp_internal(
    project: &Project,
    comp_id: CompId,
    time: Time,
    frames: &dyn MediaFrames,
    active_comps: &mut Vec<CompId>,
    depth: usize,
) -> Result<Frame, RenderError> {
    render_comp_registered(
        project,
        comp_id,
        time,
        frames,
        active_comps,
        depth,
        builtin_registry(),
    )
}

fn render_comp_registered(
    project: &Project,
    comp_id: CompId,
    time: Time,
    frames: &dyn MediaFrames,
    active_comps: &mut Vec<CompId>,
    depth: usize,
    registry: &EffectRegistry,
) -> Result<Frame, RenderError> {
    let comp = project
        .comp(comp_id)
        .ok_or(RenderError::CompNotFound(comp_id))?;
    bonaparte_model::validation::validate_comp_size(
        comp.width,
        comp.height,
        comp.fps,
        comp.duration,
    )
    .map_err(RenderError::Invalid)?;
    let mut frame = Frame::filled(comp.width, comp.height, comp.background);

    active_comps.push(comp_id);

    // Render in bottom-to-top order (layer_order[0] is bottom).
    for &layer_id in &comp.layer_order {
        let layer = comp
            .layers
            .get(&layer_id)
            .ok_or_else(|| RenderError::Invalid("Missing layer in render order".into()))?;
        if !layer.visible_at(time) {
            continue;
        }
        draw_layer_registered(
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
            registry,
        )?;
    }

    active_comps.pop();
    Ok(frame)
}

/// Draw a layer with built-in plugins. Custom registries use render_comp_with_registry.
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
    draw_layer_registered(
        frame,
        project,
        comp,
        layer,
        time,
        frames,
        active_comps,
        depth,
        clip_x,
        clip_y,
        clip_w,
        clip_h,
        offset_x,
        offset_y,
        builtin_registry(),
    )
}

#[allow(clippy::too_many_arguments)]
fn draw_layer_registered(
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
    registry: &EffectRegistry,
) -> Result<(), RenderError> {
    let adjustment = matches!(layer.kind, LayerKind::Adjustment {});
    if !layer.effects.iter().any(|e| e.enabled) {
        if adjustment {
            return Ok(());
        }
        return draw_layer_pixels(
            frame,
            project,
            comp,
            layer,
            time,
            frames,
            active_comps,
            depth,
            clip_x,
            clip_y,
            clip_w,
            clip_h,
            offset_x,
            offset_y,
            registry,
            false,
        );
    }
    let opacity = comp
        .effective_transform(layer.id, time)
        .map(|t| t.opacity.clamp(0.0, 1.0))
        .unwrap_or(1.0);
    if opacity <= 0.00001 {
        return Ok(());
    }
    // Correctness-first full-frame intermediates avoid seams in neighborhood filters.
    // The tile API explicitly falls back to this path until effect ROI propagation lands.
    let mut source = if adjustment {
        frame.clone()
    } else {
        Frame::new(comp.width, comp.height)
    };
    if !adjustment {
        draw_layer_pixels(
            &mut source,
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
            registry,
            true,
        )?;
    }
    let mut filtered = CpuFrame::from_rgba(source.width, source.height, source.rgba);
    for instance in &layer.effects {
        if !instance.enabled {
            continue;
        }
        filtered = registry
            .evaluate_instance(instance, &filtered, time)
            .map_err(|e| RenderError::Effect(e.to_string()))?;
    }
    let filtered = Frame {
        width: filtered.width,
        height: filtered.height,
        rgba: filtered.rgba,
    };
    // A full-strength adjustment replaces the backdrop byte-for-byte. Avoid
    // an unnecessary decode/encode pass (and preserve RGB under zero alpha).
    if adjustment
        && opacity == 1.0
        && offset_x == 0
        && offset_y == 0
        && frame.width == filtered.width
        && frame.height == filtered.height
    {
        frame.rgba = filtered.rgba;
        return Ok(());
    }
    for y in 0..frame.height {
        for x in 0..frame.width {
            if !adjustment && layer.blend_mode == BlendMode::Normal {
                let i = (((y + offset_y) * filtered.width + x + offset_x) * 4) as usize;
                if filtered.rgba[i + 3] == 0 {
                    continue;
                }
                if opacity == 1.0 && filtered.rgba[i + 3] == 255 {
                    let dest = ((y * frame.width + x) * 4) as usize;
                    frame.rgba[dest..dest + 4].copy_from_slice(&filtered.rgba[i..i + 4]);
                    continue;
                }
            }
            let mut pixel = filtered.pixel(x + offset_x, y + offset_y);
            if adjustment {
                let original = frame.pixel(x, y);
                let alpha = original[3] * (1.0 - opacity) + pixel[3] * opacity;
                for c in 0..3 {
                    pixel[c] = if alpha > 0.000001 {
                        (original[c] * original[3] * (1.0 - opacity)
                            + pixel[c] * pixel[3] * opacity)
                            / alpha
                    } else {
                        0.0
                    };
                }
                pixel[3] = alpha;
                frame.set_pixel(x, y, pixel);
            } else {
                pixel[3] *= opacity;
                frame.blend_pixel(x, y, pixel, layer.blend_mode);
            }
        }
    }
    Ok(())
}

/// Draw a single layer within a sub-rectangle of the canvas (used by both full frame and tile rendering).
#[allow(clippy::too_many_arguments)]
fn draw_layer_pixels(
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
    registry: &EffectRegistry,
    source_only: bool,
) -> Result<(), RenderError> {
    let opacity = if source_only {
        1.0
    } else {
        comp.effective_transform(layer.id, time)
            .map(|t| t.opacity.clamp(0.0, 1.0))
            .unwrap_or(1.0)
    };
    if opacity <= 1e-5 {
        return Ok(());
    }

    // Determine the natural dimensions (width, height) of the layer in local space.
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
                return Err(RenderError::MediaUnavailable(*media));
            }
        }
        LayerKind::Text { text, size, style } => {
            let bounds = measure_text(text, *size, style.bold, style.tracking);
            (bounds[0], bounds[1])
        }
        LayerKind::PreComp { comp: child_id } => {
            let child = project
                .comp(*child_id)
                .ok_or(RenderError::CompNotFound(*child_id))?;
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
        Some(render_comp_registered(
            project,
            *child_id,
            child_time,
            frames,
            active_comps,
            depth + 1,
            registry,
        )?)
    } else {
        None
    };

    let text_bitmap = if let LayerKind::Text { text, size, style } = &layer.kind {
        Some(
            rasterize_text(text, *size, style.bold, style.tracking)
                .map_err(RenderError::Invalid)?,
        )
    } else {
        None
    };

    // Shape generators use the public registry, never an engine-side plugin-ID switch.
    let generated_shape = if let LayerKind::Shape {
        color,
        generator: Some(id),
        style,
    } = &layer.kind
    {
        let manifest = registry
            .get_manifest(id)
            .ok_or_else(|| RenderError::Effect(format!("Unknown shape generator {id}")))?;
        if !manifest.inputs.is_empty() {
            return Err(RenderError::Effect(format!(
                "{id} is a filter, not a shape generator"
            )));
        }
        let mut params = std::collections::HashMap::new();
        for p in &manifest.params {
            let value = match p.id.as_str() {
                "color" => Some(bonaparte_model::EffectValue::Color([
                    bonaparte_effects::grading::linear_to_srgb(color[0]),
                    bonaparte_effects::grading::linear_to_srgb(color[1]),
                    bonaparte_effects::grading::linear_to_srgb(color[2]),
                    color[3],
                ])),
                "stroke_color" => Some(bonaparte_model::EffectValue::Color([
                    bonaparte_effects::grading::linear_to_srgb(style.stroke_color[0]),
                    bonaparte_effects::grading::linear_to_srgb(style.stroke_color[1]),
                    bonaparte_effects::grading::linear_to_srgb(style.stroke_color[2]),
                    style.stroke_color[3],
                ])),
                "stroke_width" => Some(bonaparte_model::EffectValue::Float(style.stroke_width)),
                _ => None,
            };
            if let Some(value) = value {
                params.insert(p.id.clone(), value);
            }
        }
        let source = CpuFrame::new(layer_w.ceil() as u32, layer_h.ceil() as u32);
        let generated = registry
            .evaluate(id, &source, &params)
            .map_err(|e| RenderError::Effect(e.to_string()))?;
        Some(Frame {
            width: generated.width,
            height: generated.height,
            rgba: generated.rgba,
        })
    } else {
        None
    };

    let flat_opaque = match &layer.kind {
        LayerKind::Solid { color }
            if color[3] * opacity >= 1.0
                && (source_only || layer.blend_mode == BlendMode::Normal) =>
        {
            Some([
                linear_to_srgb_byte(color[0]),
                linear_to_srgb_byte(color[1]),
                linear_to_srgb_byte(color[2]),
                255,
            ])
        }
        _ => None,
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

            if let Some(color) = flat_opaque {
                let i = ((dest_y * frame.width + dest_x) * 4) as usize;
                frame.rgba[i..i + 4].copy_from_slice(&color);
                continue;
            }
            let src_color = match &layer.kind {
                LayerKind::Solid { color } => [color[0], color[1], color[2], color[3] * opacity],
                LayerKind::Shape { color, style, .. } => {
                    if let Some(generated) = &generated_shape {
                        let p = generated.sample_bilinear(u, v);
                        [p[0], p[1], p[2], p[3] * opacity]
                    } else {
                        let r = style.corner_radius.min(hw.min(hh));
                        let qx = lx.abs() - hw + r;
                        let qy = ly.abs() - hh + r;
                        let distance = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt()
                            + qx.max(qy).min(0.0)
                            - r;
                        let aa = (inv_affine.a.hypot(inv_affine.b))
                            .max(inv_affine.c.hypot(inv_affine.d))
                            .max(0.01);
                        let coverage = (0.5 - distance / aa).clamp(0.0, 1.0);
                        let stroke = if style.stroke_width > 0.0 {
                            (0.5 + (distance + style.stroke_width) / aa).clamp(0.0, 1.0)
                        } else {
                            0.0
                        };
                        let mut out = [0.0; 4];
                        for ch in 0..4 {
                            out[ch] = color[ch] * (1.0 - stroke) + style.stroke_color[ch] * stroke;
                        }
                        out[3] *= coverage * opacity;
                        out
                    }
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
                        view.rgba[idx + 3] as f32 / 255.0 * opacity,
                    ]
                }
                LayerKind::Text { style, .. } => {
                    let coverage = text_bitmap.as_ref().expect("text bitmap").sample(u, v);
                    [
                        style.color[0],
                        style.color[1],
                        style.color[2],
                        style.color[3] * opacity * coverage,
                    ]
                }
                LayerKind::Adjustment {} => continue,
                LayerKind::PreComp { .. } => {
                    let pf = precomp_frame.as_ref().unwrap();
                    let p = pf.sample_bilinear(u, v);
                    [p[0], p[1], p[2], p[3] * opacity]
                }
            };

            frame.blend_pixel(
                dest_x,
                dest_y,
                src_color,
                if source_only {
                    BlendMode::Normal
                } else {
                    layer.blend_mode
                },
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bonaparte_model::{Easing, History, Keyframe, Op, Property, StaticTransform, Track};

    fn solid(color: [f32; 4]) -> Layer {
        let mut l = Layer::new(
            "solid",
            LayerKind::Solid { color },
            Time::ZERO,
            Time(10 * bonaparte_model::TICKS_PER_SEC),
        );
        l.transform = StaticTransform::default();
        l
    }

    fn commit_layer(
        p: &mut Project,
        c: bonaparte_model::CompId,
        layer: Layer,
    ) -> bonaparte_model::LayerId {
        let mut h = History::new();
        h.commit(p, Op::AddLayer { comp: c, layer }).unwrap();
        bonaparte_model::LayerId(p.next_layer.0 - 1)
    }

    #[test]
    fn renders_background_and_layering_is_bottom_to_top() {
        let mut p = Project::new("t");
        let c = p.create_comp(
            "main",
            4,
            4,
            bonaparte_model::FrameRate::FPS_30,
            Time(10 * bonaparte_model::TICKS_PER_SEC),
        );
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
        assert!(
            f0.pixel(0, 0)[0] < 0.05,
            "opacity 0 at t=0 must show background"
        );
        let mid = f1.pixel(0, 0);
        assert!(mid[0] > 0.4 && mid[0] < 0.6, "half-faded red at t=0.5s");
    }

    struct OneImage;
    impl MediaFrames for OneImage {
        fn frame_rgba(&self, _media: MediaId, _time: Time) -> Option<FrameView<'_>> {
            Some(FrameView {
                width: 2,
                height: 2,
                rgba: &[
                    255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255,
                ],
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
        p.insert_media(bonaparte_model::MediaAsset {
            id: MediaId(0),
            name: "Injected fixture".into(),
            path: None,
            kind: bonaparte_model::MediaKind::Image,
            embedded: None,
            audio: None,
            slot: None,
            alias: None,
            perception: None,
        });
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

        let l1 = Layer::new(
            "pre1",
            LayerKind::PreComp { comp: c2 },
            Time::ZERO,
            Time(100),
        );
        let l2 = Layer::new(
            "pre2",
            LayerKind::PreComp { comp: c1 },
            Time::ZERO,
            Time(100),
        );
        p.insert_layer(c1, l1);
        p.insert_layer(c2, l2);

        let res = render_comp(&p, c1, Time::ZERO, &NoMedia);
        assert!(matches!(res, Err(RenderError::PreCompCycle(_))));
    }
}
