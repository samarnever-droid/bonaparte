//! Static-prefix compositing cache — "scene resume".
//!
//! Most real scenes are a large stack of unchanging layers with one or two
//! animated elements on top. This cache keeps the last composite of the
//! unchanged bottom of that stack: every layer gets an exact content token,
//! the first layer whose token changed (or that is inherently volatile —
//! nested compositions, effect stacks, adjustments) marks the boundary, and
//! rendering resumes painting from the stored prefix instead of the whole
//! stack.
//!
//! Reuse is decided by *token equality only* — never by frame adjacency —
//! so scrub jumps, seeks and live edits lower the boundary but can never
//! produce wrong pixels. Volatile-proof rasters keep their `Arc` alive in
//! the token, which makes pointer identity a sound content proof: the held
//! reference pins the address against allocator reuse while it is cached.
use crate::preview::{Sampling, Scene, SceneLayer, Source};
use crate::reference::{Frame, RenderError};
use bonaparte_effects::{CpuFrame, EffectRegistry};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
enum Content {
    Solid([u32; 4]),
    Rect {
        color: [u32; 4],
        style_bits: u64,
    },
    /// The held Arc both proves and pins the exact pixel content.
    Raster {
        pixels: Arc<CpuFrame>,
        sampling: u8,
        tint: Option<[u32; 4]>,
    },
    /// Depends on time, a child render, or the backdrop: never a base.
    Volatile,
}

fn bits4(v: &[f32; 4]) -> [u32; 4] {
    [
        v[0].to_bits(),
        v[1].to_bits(),
        v[2].to_bits(),
        v[3].to_bits(),
    ]
}

#[derive(Clone)]
struct Token {
    layer: u64,
    inverse: [u32; 6],
    size: [u32; 2],
    bounds: [u32; 4],
    opacity: u32,
    blend: u8,
    content: Content,
}

fn blend_code(mode: bonaparte_model::BlendMode) -> u8 {
    use bonaparte_model::BlendMode::*;
    match mode {
        Normal => 0,
        Multiply => 1,
        Screen => 2,
        Overlay => 3,
        Add => 4,
        Darken => 5,
        Lighten => 6,
        Difference => 7,
    }
}

/// FNV over the Debug form: shape styles are tiny, and a stable string is a
/// conservative exact token (same string ⇒ same render inputs).
fn shape_style_bits(style: &bonaparte_model::ShapeStyle) -> u64 {
    let text = format!("{style:?}");
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        h ^= byte as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

fn token_for(layer: &SceneLayer) -> Token {
    let m = &layer.inverse;
    let has_effects = !layer.effects.is_empty();
    let content = match &layer.source {
        Source::Solid(color) if !has_effects => Content::Solid(bits4(color)),
        Source::Rectangle { color, style } if !has_effects => Content::Rect {
            color: bits4(color),
            style_bits: shape_style_bits(style),
        },
        Source::Raster {
            pixels,
            sampling,
            tint,
        } if !has_effects => Content::Raster {
            pixels: pixels.clone(),
            sampling: match sampling {
                Sampling::Nearest => 0,
                Sampling::Linear => 1,
            },
            tint: tint.map(|t| bits4(&t)),
        },
        _ => Content::Volatile,
    };
    Token {
        layer: layer.id.0,
        inverse: [
            m.a.to_bits(),
            m.b.to_bits(),
            m.c.to_bits(),
            m.d.to_bits(),
            m.tx.to_bits(),
            m.ty.to_bits(),
        ],
        size: [layer.size[0].to_bits(), layer.size[1].to_bits()],
        bounds: layer.bounds,
        opacity: layer.opacity.to_bits(),
        blend: blend_code(layer.blend),
        content,
    }
}

fn same(a: &Token, other: &Token) -> bool {
    if a.layer != other.layer
        || a.inverse != other.inverse
        || a.size != other.size
        || a.bounds != other.bounds
        || a.opacity != other.opacity
        || a.blend != other.blend
    {
        return false;
    }
    match (&a.content, &other.content) {
        (Content::Solid(x), Content::Solid(y)) => x == y,
        (
            Content::Rect {
                color: c1,
                style_bits: s1,
            },
            Content::Rect {
                color: c2,
                style_bits: s2,
            },
        ) => c1 == c2 && s1 == s2,
        (
            Content::Raster {
                pixels: p1,
                sampling: g1,
                tint: t1,
            },
            Content::Raster {
                pixels: p2,
                sampling: g2,
                tint: t2,
            },
        ) => Arc::ptr_eq(p1, p2) && g1 == g2 && t1 == t2,
        _ => false,
    }
}

/// One reusable prefix per (comp, output-size) render stream.
#[derive(Default)]
pub struct PrefixCache {
    tokens: Vec<Token>,
    snapshot: Vec<u8>,
    boundary: usize,
    count: usize,
    width: u32,
    height: u32,
    background: [u32; 4],
}

impl PrefixCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Identical output to `render_scene_cpu`, resuming from the cached prefix.
    pub fn render(
        &mut self,
        scene: &Scene,
        registry: &EffectRegistry,
    ) -> Result<Frame, RenderError> {
        let n = scene.layers.len();
        let background = bits4(&scene.background);
        let fresh = self.count != n
            || self.width != scene.width
            || self.height != scene.height
            || self.background != background;
        let current: Vec<Token> = scene.layers.iter().map(token_for).collect();
        let mut boundary = n;
        if fresh {
            boundary = 0;
        } else {
            for i in 0..n {
                if !same(&current[i], &self.tokens[i]) {
                    boundary = i;
                    break;
                }
            }
        }
        // Nothing moved and the snapshot is the full frame: memcpy out.
        if !fresh && boundary == n && self.boundary == n && self.snapshot.len() == n_bytes(scene) {
            return Ok(Frame {
                width: scene.width,
                height: scene.height,
                rgba: self.snapshot.clone(),
            });
        }
        let resume = !fresh
            && self.boundary <= boundary
            && self.snapshot.len() == n_bytes(scene)
            && self.boundary > 0;
        let start = if resume { self.boundary } else { 0 };
        let mut frame = if resume {
            Frame {
                width: scene.width,
                height: scene.height,
                rgba: self.snapshot.clone(),
            }
        } else {
            Frame::filled(scene.width, scene.height, scene.background)
        };
        // Snapshot bookkeeping: the stored buffer must always equal "first
        // `boundary` layers painted" for the tokens we save below.
        let mut snap_k = if resume { self.boundary } else { 0 };
        let mut snap_buf = if resume {
            self.snapshot.clone()
        } else {
            Vec::new()
        };
        for i in start..n {
            crate::cancel::check()?;
            crate::preview::paint_layer(&mut frame, scene, &scene.layers[i], registry)?;
            if i + 1 == boundary && boundary > snap_k {
                snap_buf = frame.rgba.clone();
                snap_k = boundary;
            }
        }
        if boundary == n && n > snap_k {
            snap_buf = frame.rgba.clone();
            snap_k = n;
        }
        self.snapshot = snap_buf;
        self.boundary = snap_k;
        self.tokens = current;
        self.count = n;
        self.width = scene.width;
        self.height = scene.height;
        self.background = background;
        Ok(frame)
    }
}

fn n_bytes(scene: &Scene) -> usize {
    scene.width as usize * scene.height as usize * 4
}

/// Render with a process-wide cache keyed by (comp, output size). Export
/// and preview streams for the same comp and size share one prefix; other
/// sizes and comps get their own slot, and a bounded slot table keeps
/// memory flat no matter how many compositions churn through a session.
pub fn render_global(
    comp: bonaparte_model::CompId,
    scene: &Scene,
    registry: &EffectRegistry,
) -> Result<Frame, RenderError> {
    static SLOTS: std::sync::LazyLock<
        Mutex<std::collections::HashMap<(u64, u32, u32), Arc<Mutex<PrefixCache>>>>,
    > = std::sync::LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));
    let key = (comp.0, scene.width, scene.height);
    let slot = {
        let mut map = SLOTS.lock().unwrap_or_else(|e| e.into_inner());
        if map.len() > 4 {
            map.clear();
        }
        map.entry(key)
            .or_insert_with(|| Arc::new(Mutex::new(PrefixCache::new())))
            .clone()
    };
    if *DISABLED {
        // Escape hatch for A/B benchmarking and emergency rollback:
        // BONAPARTE_NO_PREFIX_CACHE=1 restores the plain full-composite path.
        return crate::preview::render_scene_cpu(scene, registry);
    }
    let mut cache = slot.lock().unwrap_or_else(|e| e.into_inner());
    cache.render(scene, registry)
}

static DISABLED: std::sync::LazyLock<bool> =
    std::sync::LazyLock::new(|| std::env::var("BONAPARTE_NO_PREFIX_CACHE").is_ok());
