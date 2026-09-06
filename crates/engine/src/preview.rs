//! Resolution-aware, prepared render scenes shared by preview backends.
//! Geometry stays in composition coordinates. Only the raster grid changes;
//! previewing never edits the project or changes export dimensions.
use crate::reference::{Affine2D, Frame, FrameView, MediaFrames, RenderError};
use crate::typography::{measure_text, rasterize_text_scaled};
use bonaparte_effects::{CpuFrame, EffectRegistry, ParamValue};
use bonaparte_model::*;
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Resolution(u32);
impl Resolution {
    pub const FULL: Self = Self(1);
    pub fn new(divisor: u32) -> Result<Self, String> {
        if matches!(divisor, 1 | 2 | 4) {
            Ok(Self(divisor))
        } else {
            Err("Preview resolution must be full, half or quarter".into())
        }
    }
    pub fn divisor(self) -> u32 {
        self.0
    }
    pub fn dimensions(self, width: u32, height: u32) -> [u32; 2] {
        [width.div_ceil(self.0), height.div_ceil(self.0)]
    }
    pub fn effect_scale(self) -> f32 {
        1.0 / self.0 as f32
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum SourceKey {
    Text {
        text: String,
        size: u32,
        bold: bool,
        tracking: u32,
        density: u32,
    },
    Generator {
        id: String,
        width: u32,
        height: u32,
        values: String,
        callback: usize,
    },
}
struct CachedSource {
    key: SourceKey,
    pixels: Arc<CpuFrame>,
}
struct SourceState {
    entries: VecDeque<CachedSource>,
    bytes: usize,
    hits: u64,
    misses: u64,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceStats {
    pub bytes: usize,
    pub budget_bytes: usize,
    pub entries: usize,
    pub hits: u64,
    pub misses: u64,
}
/// Bounded retained glyph/generator pixels. In-flight scenes hold their own Arcs;
/// the budget describes cache retention, not total process memory.
pub struct SourceCache {
    state: Mutex<SourceState>,
    budget: usize,
}
impl SourceCache {
    pub fn new(budget: usize) -> Self {
        Self {
            state: Mutex::new(SourceState {
                entries: VecDeque::new(),
                bytes: 0,
                hits: 0,
                misses: 0,
            }),
            budget,
        }
    }
    pub fn clear(&self) {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        s.entries.clear();
        s.bytes = 0;
    }
    pub fn stats(&self) -> SourceStats {
        let s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        SourceStats {
            bytes: s.bytes,
            budget_bytes: self.budget,
            entries: s.entries.len(),
            hits: s.hits,
            misses: s.misses,
        }
    }
    fn get_or_make(
        &self,
        key: SourceKey,
        make: impl FnOnce() -> Result<CpuFrame, RenderError>,
    ) -> Result<Arc<CpuFrame>, RenderError> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(index) = state.entries.iter().position(|e| e.key == key) {
            let entry = state.entries.remove(index).expect("located entry");
            let pixels = entry.pixels.clone();
            state.entries.push_back(entry);
            state.hits += 1;
            return Ok(pixels);
        }
        state.misses += 1;
        let pixels = Arc::new(make()?);
        let bytes = pixels.rgba.len();
        if bytes <= self.budget {
            while state.bytes + bytes > self.budget || state.entries.len() >= 1024 {
                if let Some(old) = state.entries.pop_front() {
                    state.bytes -= old.pixels.rgba.len();
                } else {
                    break;
                }
            }
            state.bytes += bytes;
            state.entries.push_back(CachedSource {
                key,
                pixels: pixels.clone(),
            });
        }
        Ok(pixels)
    }
    fn text(
        &self,
        text: &str,
        size: f32,
        bold: bool,
        tracking: f32,
        density: f32,
    ) -> Result<Arc<CpuFrame>, RenderError> {
        let key = SourceKey::Text {
            text: text.into(),
            size: size.to_bits(),
            bold,
            tracking: tracking.to_bits(),
            density: density.to_bits(),
        };
        self.get_or_make(key, || {
            let mask = rasterize_text_scaled(text, size, bold, tracking, density)
                .map_err(RenderError::Invalid)?;
            let mut rgba = Vec::with_capacity(mask.coverage.len() * 4);
            for alpha in mask.coverage {
                rgba.extend_from_slice(&[255, 255, 255, alpha]);
            }
            Ok(CpuFrame::from_rgba(mask.width, mask.height, rgba))
        })
    }
    fn generator(
        &self,
        registry: &EffectRegistry,
        id: &str,
        color: [f32; 4],
        style: &ShapeStyle,
        size: [f32; 2],
        density: f32,
    ) -> Result<Arc<CpuFrame>, RenderError> {
        let pack = registry
            .get(id)
            .ok_or_else(|| RenderError::Effect(format!("Unknown generator {id}")))?;
        if !pack.manifest.inputs.is_empty() {
            return Err(RenderError::Effect(format!("{id} is not a generator")));
        }
        let encode = |c: [f32; 4]| {
            [
                bonaparte_effects::grading::linear_to_srgb(c[0]),
                bonaparte_effects::grading::linear_to_srgb(c[1]),
                bonaparte_effects::grading::linear_to_srgb(c[2]),
                c[3],
            ]
        };
        let mut params = HashMap::new();
        for p in &pack.manifest.params {
            let value = match p.id.as_str() {
                "color" => Some(ParamValue::Color(encode(color))),
                "stroke_color" => Some(ParamValue::Color(encode(style.stroke_color))),
                "stroke_width" => Some(ParamValue::Float(style.stroke_width)),
                _ => None,
            };
            if let Some(v) = value {
                params.insert(p.id.clone(), v);
            }
        }
        let width = (size[0] * density).ceil() as u32;
        let height = (size[1] * density).ceil() as u32;
        let key = SourceKey::Generator {
            id: id.into(),
            width,
            height,
            values: format!("{:?}:{:?}:{:?}", color, style, pack.manifest),
            callback: pack.evaluator.map(|f| f as usize).unwrap_or(0),
        };
        self.get_or_make(key, || {
            registry
                .evaluate_scaled(id, &CpuFrame::new(width, height), &params, density)
                .map_err(|e| RenderError::Effect(e.to_string()))
        })
    }
}
impl Default for SourceCache {
    fn default() -> Self {
        Self::new(32 * 1024 * 1024)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sampling {
    Nearest,
    Linear,
}
#[derive(Clone)]
pub enum Source {
    Solid([f32; 4]),
    Rectangle {
        color: [f32; 4],
        style: ShapeStyle,
    },
    Raster {
        pixels: Arc<CpuFrame>,
        sampling: Sampling,
        tint: Option<[f32; 4]>,
    },
    Composition(Box<Scene>),
    Adjustment,
}
#[derive(Clone)]
pub struct SceneLayer {
    pub id: LayerId,
    pub inverse: Affine2D,
    pub size: [f32; 2],
    /// Raster-space x,y,width,height, already clipped to the output.
    pub bounds: [u32; 4],
    pub opacity: f32,
    pub blend: BlendMode,
    pub source: Source,
    pub effects: Vec<EffectInstance>,
}
#[derive(Clone)]
pub struct Scene {
    pub width: u32,
    pub height: u32,
    pub logical_size: [f32; 2],
    pub background: [f32; 4],
    pub time: Time,
    pub resolution: Resolution,
    pub raster_scale: f32,
    pub layers: Vec<SceneLayer>,
}
impl Scene {
    pub fn pixel_size(&self) -> [f32; 2] {
        [
            self.logical_size[0] / self.width as f32,
            self.logical_size[1] / self.height as f32,
        ]
    }
}

/// Largest singular value of the local-to-composition transform.
fn magnification(m: &Affine2D) -> f32 {
    let x = m.a * m.a + m.b * m.b;
    let y = m.c * m.c + m.d * m.d;
    let z = m.a * m.c + m.b * m.d;
    ((x + y + ((x - y) * (x - y) + 4.0 * z * z).sqrt()) * 0.5).sqrt()
}
fn bounded_density(size: [f32; 2], wanted: f32) -> f32 {
    let maximum = (16_777_216.0 / (size[0] * size[1]).max(1.0))
        .sqrt()
        .min(8192.0 / size[0].max(1.0))
        .min(8192.0 / size[1].max(1.0))
        .min(8.0);
    wanted.max(0.125).min(maximum.max(0.125))
}
fn source_density(size: [f32; 2], wanted: f32) -> f32 {
    bounded_density(size, (wanted.max(1.0) * 2.0).ceil() / 2.0).max(1.0)
}

pub fn prepare_scene(
    project: &Project,
    comp_id: CompId,
    time: Time,
    frames: &dyn MediaFrames,
    registry: &EffectRegistry,
    resolution: Resolution,
    cache: &SourceCache,
) -> Result<Scene, RenderError> {
    prepare_scene_with_transform(
        project, comp_id, time, frames, registry, resolution, cache, None,
    )
}

pub fn prepare_scene_with_transform(
    project: &Project,
    comp_id: CompId,
    time: Time,
    frames: &dyn MediaFrames,
    registry: &EffectRegistry,
    resolution: Resolution,
    cache: &SourceCache,
    transient: Option<&bonaparte_model::TransformOverride>,
) -> Result<Scene, RenderError> {
    fn visit(
        project: &Project,
        comp_id: CompId,
        time: Time,
        frames: &dyn MediaFrames,
        registry: &EffectRegistry,
        resolution: Resolution,
        cache: &SourceCache,
        active: &mut Vec<CompId>,
        seen: &mut std::collections::HashSet<usize>,
        source_bytes: &mut usize,
        transient: Option<&bonaparte_model::TransformOverride>,
        density: f32,
        canvas_pixels: &mut u64,
    ) -> Result<Scene, RenderError> {
        if active.contains(&comp_id) {
            return Err(RenderError::PreCompCycle(comp_id));
        }
        if active.len() >= 32 {
            return Err(RenderError::PreCompDepthLimit(comp_id));
        }
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
        let density = bounded_density([comp.width as f32, comp.height as f32], density);
        let width = (comp.width as f32 * density).ceil().max(1.0) as u32;
        let height = (comp.height as f32 * density).ceil().max(1.0) as u32;
        *canvas_pixels += u64::from(width) * u64::from(height);
        if *canvas_pixels > 67_108_864 {
            return Err(RenderError::Invalid("Composition intermediates exceed the 64-megapixel render budget; reduce nested sizes or preview resolution".into()));
        }
        let mut scene = Scene {
            width,
            height,
            logical_size: [comp.width as f32, comp.height as f32],
            background: comp.background,
            time,
            resolution,
            raster_scale: density,
            layers: vec![],
        };
        let [sx, sy] = scene.pixel_size();
        active.push(comp_id);
        for id in &comp.layer_order {
            let layer = comp
                .layers
                .get(id)
                .ok_or_else(|| RenderError::Invalid("Missing layer in render order".into()))?;
            if !layer.visible_at(time) {
                continue;
            }
            let opacity = comp
                .effective_transform_with(*id, time, transient)
                .map(|t| t.opacity.clamp(0.0, 1.0))
                .unwrap_or(1.0);
            if opacity <= 0.00001 {
                continue;
            }
            let adjustment = matches!(layer.kind, LayerKind::Adjustment {});
            let affine = Affine2D::from_layer_with(comp, layer, time, transient)
                .unwrap_or(Affine2D::identity());
            let inverse = if adjustment {
                Affine2D::identity()
            } else {
                match affine.inverse() {
                    Some(m) => m,
                    None => continue,
                }
            };
            let (source, size) = match &layer.kind {
                LayerKind::Solid { color } => (Source::Solid(*color), scene.logical_size),
                LayerKind::Adjustment {} => (Source::Adjustment, scene.logical_size),
                LayerKind::Shape {
                    color,
                    generator,
                    style,
                } => {
                    let size = style.size.unwrap_or(scene.logical_size);
                    let source = if let Some(id) = generator {
                        Source::Raster {
                            pixels: cache.generator(
                                registry,
                                id,
                                *color,
                                style,
                                size,
                                source_density(size, density * magnification(&affine)),
                            )?,
                            sampling: Sampling::Linear,
                            tint: None,
                        }
                    } else {
                        Source::Rectangle {
                            color: *color,
                            style: style.clone(),
                        }
                    };
                    (source, size)
                }
                LayerKind::Text { text, size, style } => {
                    let logical = measure_text(text, *size, style.bold, style.tracking);
                    if logical[0] * logical[1] > 16_777_216.0 {
                        return Err(RenderError::Invalid("Text layout exceeds 16 megapixels. Reduce font size, tracking or line length.".into()));
                    }
                    let raster_density = source_density(logical, density * magnification(&affine));
                    let pixels =
                        cache.text(text, *size, style.bold, style.tracking, raster_density)?;
                    let size = logical;
                    (
                        Source::Raster {
                            pixels,
                            sampling: if density == 1.0
                                && raster_density == 1.0
                                && affine.b.abs() < 1e-6
                                && affine.c.abs() < 1e-6
                            {
                                Sampling::Nearest
                            } else {
                                Sampling::Linear
                            },
                            tint: Some(style.color),
                        },
                        size,
                    )
                }
                LayerKind::Footage { media } => {
                    let pixels = if let Some(pixels) = frames.shared_frame(*media, time) {
                        pixels
                    } else {
                        let f = frames
                            .frame_rgba(*media, time)
                            .ok_or(RenderError::MediaUnavailable(*media))?;
                        Arc::new(CpuFrame::from_rgba(f.width, f.height, f.rgba.to_vec()))
                    };
                    let size = [pixels.width as f32, pixels.height as f32];
                    (
                        Source::Raster {
                            pixels,
                            sampling: Sampling::Linear,
                            tint: None,
                        },
                        size,
                    )
                }
                LayerKind::PreComp { comp: child } => {
                    let nested = visit(
                        project,
                        *child,
                        time - layer.start,
                        frames,
                        registry,
                        resolution,
                        cache,
                        active,
                        seen,
                        source_bytes,
                        transient,
                        density * magnification(&affine).max(1.0),
                        canvas_pixels,
                    )?;
                    let size = nested.logical_size;
                    (Source::Composition(Box::new(nested)), size)
                }
            };
            if let Source::Raster { pixels, .. } = &source {
                if seen.insert(Arc::as_ptr(pixels) as usize) {
                    *source_bytes += pixels.rgba.len();
                    if *source_bytes > 128 * 1024 * 1024 {
                        return Err(RenderError::Invalid("Prepared source rasters exceed the 128 MiB preview budget. Reduce image/text sizes or hide layers.".into()));
                    }
                }
            }
            let bounds = if adjustment {
                [0, 0, width, height]
            } else {
                let hw = size[0] / 2.0;
                let hh = size[1] / 2.0;
                let corners = [
                    affine.transform_point([-hw, -hh]),
                    affine.transform_point([hw, -hh]),
                    affine.transform_point([hw, hh]),
                    affine.transform_point([-hw, hh]),
                ];
                let left = (corners
                    .iter()
                    .map(|p| p[0] / sx)
                    .fold(f32::INFINITY, f32::min)
                    .floor()
                    .max(0.0) as u32)
                    .min(width);
                let top = (corners
                    .iter()
                    .map(|p| p[1] / sy)
                    .fold(f32::INFINITY, f32::min)
                    .floor()
                    .max(0.0) as u32)
                    .min(height);
                let right = (corners
                    .iter()
                    .map(|p| p[0] / sx)
                    .fold(f32::NEG_INFINITY, f32::max)
                    .ceil()
                    .max(0.0) as u32)
                    .min(width);
                let bottom = (corners
                    .iter()
                    .map(|p| p[1] / sy)
                    .fold(f32::NEG_INFINITY, f32::max)
                    .ceil()
                    .max(0.0) as u32)
                    .min(height);
                [
                    left,
                    top,
                    right.saturating_sub(left),
                    bottom.saturating_sub(top),
                ]
            };
            // Effects can extend a source outside its bounds, but a completely
            // off-canvas source still needs evaluation (e.g. a generator filter).
            scene.layers.push(SceneLayer {
                id: *id,
                inverse,
                size,
                bounds,
                opacity,
                blend: layer.blend_mode,
                source,
                effects: layer
                    .effects
                    .iter()
                    .filter(|e| e.enabled)
                    .cloned()
                    .collect(),
            });
        }
        active.pop();
        Ok(scene)
    }
    visit(
        project,
        comp_id,
        time,
        frames,
        registry,
        resolution,
        cache,
        &mut vec![],
        &mut std::collections::HashSet::new(),
        &mut 0,
        transient,
        resolution.effect_scale(),
        &mut 0,
    )
}

fn source_at(
    layer: &SceneLayer,
    point: [f32; 2],
    footprint: [f32; 2],
    nested: Option<&Frame>,
) -> [f32; 4] {
    let [lx, ly] = layer.inverse.transform_point(point);
    let [w, h] = layer.size;
    let hw = w / 2.0;
    let hh = h / 2.0;
    let u = (lx + hw) / w;
    let v = (ly + hh) / h;
    if !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
        return [0.0; 4];
    }
    match &layer.source {
        Source::Solid(color) => *color,
        Source::Rectangle { color, style } => {
            let r = style.corner_radius.min(hw.min(hh));
            let qx = lx.abs() - hw + r;
            let qy = ly.abs() - hh + r;
            let distance =
                (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r;
            let aa = (layer.inverse.a.hypot(layer.inverse.b) * footprint[0])
                .max(layer.inverse.c.hypot(layer.inverse.d) * footprint[1])
                .max(0.01);
            let coverage = (0.5 - distance / aa).clamp(0.0, 1.0);
            let stroke = if style.stroke_width > 0.0 {
                (0.5 + (distance + style.stroke_width) / aa).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let mut out = [0.0; 4];
            for c in 0..4 {
                out[c] = color[c] * (1.0 - stroke) + style.stroke_color[c] * stroke;
            }
            out[3] *= coverage;
            out
        }
        Source::Raster {
            pixels,
            sampling,
            tint,
        } => {
            let view = FrameView {
                width: pixels.width,
                height: pixels.height,
                rgba: &pixels.rgba,
            };
            let p = match sampling {
                Sampling::Nearest => view.pixel(
                    ((u * pixels.width as f32) as u32).min(pixels.width - 1),
                    ((v * pixels.height as f32) as u32).min(pixels.height - 1),
                ),
                Sampling::Linear => view.sample_bilinear(u, v),
            };
            if let Some(color) = tint {
                [color[0], color[1], color[2], color[3] * p[3]]
            } else {
                p
            }
        }
        Source::Composition(_) => nested.expect("rendered child").sample_bilinear(u, v),
        Source::Adjustment => [0.0; 4],
    }
}

/// Execute a prepared scene on the CPU. Full-resolution output is compared
/// byte-for-byte with the independent original reference renderer in tests.
pub fn render_scene_cpu(scene: &Scene, registry: &EffectRegistry) -> Result<Frame, RenderError> {
    let mut frame = Frame::filled(scene.width, scene.height, scene.background);
    let pixel_size = scene.pixel_size();
    for layer in &scene.layers {
        let adjustment = matches!(layer.source, Source::Adjustment);
        if adjustment && layer.effects.is_empty() {
            continue;
        }
        let nested = if let Source::Composition(child) = &layer.source {
            Some(render_scene_cpu(child, registry)?)
        } else {
            None
        };
        let mut source = if adjustment {
            frame.clone()
        } else if !layer.effects.is_empty() {
            Frame::new(scene.width, scene.height)
        } else {
            Frame::new(0, 0)
        };
        if !adjustment {
            let source_only = !layer.effects.is_empty();
            let dest = if source_only { &mut source } else { &mut frame };
            let [x0, y0, w, h] = layer.bounds;
            let opaque_flat = match &layer.source {
                Source::Solid(color)
                    if color[3] * (if source_only { 1.0 } else { layer.opacity }) >= 1.0
                        && (source_only || layer.blend == BlendMode::Normal) =>
                {
                    Some(Frame::filled(1, 1, *color).rgba)
                }
                _ => None,
            };
            for y in y0..y0 + h {
                for x in x0..x0 + w {
                    let mut pixel = source_at(
                        layer,
                        [
                            (x as f32 + 0.5) * pixel_size[0],
                            (y as f32 + 0.5) * pixel_size[1],
                        ],
                        pixel_size,
                        nested.as_ref(),
                    );
                    if !source_only {
                        pixel[3] *= layer.opacity;
                    }
                    if pixel[3] >= 1.0 {
                        if let Some(color) = &opaque_flat {
                            let i = ((y * dest.width + x) * 4) as usize;
                            dest.rgba[i..i + 4].copy_from_slice(color);
                            continue;
                        }
                    }
                    dest.blend_pixel(
                        x,
                        y,
                        pixel,
                        if source_only {
                            BlendMode::Normal
                        } else {
                            layer.blend
                        },
                    );
                }
            }
        }
        if layer.effects.is_empty() {
            continue;
        }
        let mut filtered = CpuFrame::from_rgba(source.width, source.height, source.rgba);
        for instance in &layer.effects {
            filtered = registry
                .evaluate_instance_scaled(instance, &filtered, scene.time, scene.raster_scale)
                .map_err(|e| RenderError::Effect(e.to_string()))?;
        }
        if adjustment && layer.opacity == 1.0 {
            frame.rgba = filtered.rgba;
            continue;
        }
        let filtered = Frame {
            width: filtered.width,
            height: filtered.height,
            rgba: filtered.rgba,
        };
        for y in 0..frame.height {
            for x in 0..frame.width {
                if !adjustment && layer.blend == BlendMode::Normal {
                    let i = ((y * frame.width + x) * 4) as usize;
                    if filtered.rgba[i + 3] == 0 {
                        continue;
                    }
                    if layer.opacity == 1.0 && filtered.rgba[i + 3] == 255 {
                        frame.rgba[i..i + 4].copy_from_slice(&filtered.rgba[i..i + 4]);
                        continue;
                    }
                }
                let mut p = filtered.pixel(x, y);
                if adjustment {
                    let old = frame.pixel(x, y);
                    let alpha = old[3] * (1.0 - layer.opacity) + p[3] * layer.opacity;
                    for c in 0..3 {
                        p[c] = if alpha > 0.000001 {
                            (old[c] * old[3] * (1.0 - layer.opacity) + p[c] * p[3] * layer.opacity)
                                / alpha
                        } else {
                            0.0
                        };
                    }
                    p[3] = alpha;
                    frame.set_pixel(x, y, p);
                } else {
                    p[3] *= layer.opacity;
                    frame.blend_pixel(x, y, p, layer.blend);
                }
            }
        }
    }
    Ok(frame)
}
