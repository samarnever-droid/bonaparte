//! The project document: comps, layers, media, perception cards, and
//! template placeholder slots.
//!
//! Invariants:
//! - `comp.layer_order[0]` is the BOTTOM layer (rendered first); the last entry
//!   is the top layer (rendered last, drawn over everything below).
//! - Every `LayerId` in `layer_order` has an entry in `layers`, and vice versa.
//! - All time is integer ticks (see `time.rs`); comps declare a rational
//!   FrameRate for frame snapping.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::ids::{CompId, LayerId, MediaId};
use crate::keyframe::{PropValue, Track};
use crate::time::{FrameRate, Time};

fn default_visible() -> bool {
    true
}

/// The whole project: what the UI displays, the engine renders, and the MCP
/// server edits. Serializes to the versioned JSON project file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub comps: BTreeMap<CompId, Comp>,
    pub media: BTreeMap<MediaId, MediaAsset>,
    /// Monotonic ID allocators. Serialized so reopened projects never
    /// re-issue a live ID.
    pub next_comp: CompId,
    pub next_layer: LayerId,
    pub next_media: MediaId,
}

/// Blend modes for compositing layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum BlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Add,
    Darken,
    Lighten,
    Difference,
}

impl std::fmt::Display for BlendMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BlendMode::Normal => write!(f, "Normal"),
            BlendMode::Multiply => write!(f, "Multiply"),
            BlendMode::Screen => write!(f, "Screen"),
            BlendMode::Overlay => write!(f, "Overlay"),
            BlendMode::Add => write!(f, "Add"),
            BlendMode::Darken => write!(f, "Darken"),
            BlendMode::Lighten => write!(f, "Lighten"),
            BlendMode::Difference => write!(f, "Difference"),
        }
    }
}

/// One composition: a canvas plus a stack of layers over time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comp {
    pub id: CompId,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub fps: FrameRate,
    /// Duration in ticks.
    pub duration: Time,
    pub background: [f32; 4],
    /// Bottom-to-top render order. Index 0 renders first (behind everything).
    pub layer_order: Vec<LayerId>,
    pub layers: BTreeMap<LayerId, Layer>,
}

/// One layer on the timeline. Static property values live in `transform`;
/// animated values live in `tracks` and win over the static value wherever a
/// track exists.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layer {
    pub id: LayerId,
    pub name: String,
    pub kind: LayerKind,
    /// Layer start time in ticks, relative to the comp.
    pub start: Time,
    /// Layer duration in ticks.
    pub duration: Time,
    /// Static (non-animated) values. A track on the same property overrides
    /// these at evaluation time.
    pub transform: StaticTransform,
    #[serde(default)]
    pub tracks: BTreeMap<Property, Track>,
    /// Optional parent layer for transform inheritance.
    #[serde(default)]
    pub parent: Option<LayerId>,
    /// Alpha blend mode when compositing onto underlying layers.
    #[serde(default)]
    pub blend_mode: BlendMode,
    /// Visibility toggle.
    #[serde(default = "default_visible")]
    pub visible: bool,
    /// Lock toggle preventing accidental edits.
    #[serde(default)]
    pub locked: bool,
}

impl Layer {
    pub fn new(name: impl Into<String>, kind: LayerKind, start: Time, duration: Time) -> Self {
        Self {
            id: LayerId(0),
            name: name.into(),
            kind,
            start,
            duration,
            transform: StaticTransform::default(),
            tracks: BTreeMap::new(),
            parent: None,
            blend_mode: BlendMode::Normal,
            visible: true,
            locked: false,
        }
    }

    pub fn new_circle(name: impl Into<String>, color: [f32; 4], start: Time, duration: Time) -> Self {
        Self::new(
            name,
            LayerKind::Shape {
                color,
                generator: Some("builtin.circle".into()),
            },
            start,
            duration,
        )
    }

    pub fn new_rect(name: impl Into<String>, color: [f32; 4], start: Time, duration: Time) -> Self {
        Self::new(
            name,
            LayerKind::Shape {
                color,
                generator: None,
            },
            start,
            duration,
        )
    }

    pub fn new_solid(name: impl Into<String>, color: [f32; 4], start: Time, duration: Time) -> Self {
        Self::new(name, LayerKind::Solid { color }, start, duration)
    }

    pub fn new_text(name: impl Into<String>, text: impl Into<String>, size: f32, start: Time, duration: Time) -> Self {
        Self::new(name, LayerKind::Text { text: text.into(), size }, start, duration)
    }
}

/// The animatable built-in properties. Plugins add more via their own param
/// paths; these are the core every layer type shares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Property {
    Position,
    Scale,
    Rotation,
    Opacity,
    AnchorPoint,
}

impl Property {
    /// The value type this property accepts, for type-checking `Op`s.
    pub fn value_kind(&self) -> ValueKind {
        match self {
            Property::Position | Property::Scale | Property::AnchorPoint => ValueKind::Vec2,
            Property::Rotation | Property::Opacity => ValueKind::Scalar,
        }
    }

    /// Read the static value for this property.
    pub fn static_value(&self, t: &StaticTransform) -> PropValue {
        match self {
            Property::Position => PropValue::Vec2(t.position),
            Property::Scale => PropValue::Vec2(t.scale),
            Property::Rotation => PropValue::Scalar(t.rotation),
            Property::Opacity => PropValue::Scalar(t.opacity),
            Property::AnchorPoint => PropValue::Vec2(t.anchor_point),
        }
    }
}

/// Discriminator for property value types, used for friendly type errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    Scalar,
    Vec2,
}

/// Static transform values shared by every layer kind.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StaticTransform {
    /// Pixels relative to comp center.
    pub position: [f32; 2],
    /// Percent (100 = natural size).
    pub scale: [f32; 2],
    /// Degrees, clockwise.
    pub rotation: f32,
    /// 0.0 – 1.0.
    pub opacity: f32,
    /// Anchor pivot point relative to layer origin [x, y].
    #[serde(default)]
    pub anchor_point: [f32; 2],
}

impl Default for StaticTransform {
    fn default() -> Self {
        Self {
            position: [0.0, 0.0],
            scale: [100.0, 100.0],
            rotation: 0.0,
            opacity: 1.0,
            anchor_point: [0.0, 0.0],
        }
    }
}

impl StaticTransform {
    /// Set a static property value. Fails on type mismatch rather than
    /// silently coercing (RULES §2.5).
    pub fn set(&mut self, property: Property, value: PropValue) -> Result<(), Property> {
        match (property, value) {
            (Property::Position, PropValue::Vec2(v)) => {
                self.position = v;
                Ok(())
            }
            (Property::Scale, PropValue::Vec2(v)) => {
                self.scale = v;
                Ok(())
            }
            (Property::Rotation, PropValue::Scalar(v)) => {
                self.rotation = v;
                Ok(())
            }
            (Property::Opacity, PropValue::Scalar(v)) => {
                self.opacity = v.clamp(0.0, 1.0);
                Ok(())
            }
            (Property::AnchorPoint, PropValue::Vec2(v)) => {
                self.anchor_point = v;
                Ok(())
            }
            (mismatched, _) => Err(mismatched),
        }
    }
}

/// Computed compound transform in comp space with inheritance.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EffectiveTransform {
    /// Position of the layer anchor in comp coordinates.
    pub position: [f32; 2],
    /// Compound scale percentage (100.0 = 1.0x).
    pub scale: [f32; 2],
    /// Compound rotation in degrees.
    pub rotation: f32,
    /// Compound combined opacity (0.0 to 1.0).
    pub opacity: f32,
    /// Local anchor point.
    pub anchor_point: [f32; 2],
    /// 2D affine transformation matrix [a, b, c, d, tx, ty] mapping layer coordinates to comp space:
    /// | a  c  tx |
    /// | b  d  ty |
    /// | 0  0   1 |
    pub matrix: [f32; 6],
}

impl EffectiveTransform {
    /// Transform a point in local layer coordinates to composition coordinates.
    pub fn transform_point(&self, point: [f32; 2]) -> [f32; 2] {
        [
            self.matrix[0] * point[0] + self.matrix[2] * point[1] + self.matrix[4],
            self.matrix[1] * point[0] + self.matrix[3] * point[1] + self.matrix[5],
        ]
    }
}

/// What a layer draws.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LayerKind {
    /// A flat colored rectangle filling the comp.
    Solid { color: [f32; 4] },
    /// A vector shape (first-party generator plugin renders it, e.g. "builtin.circle").
    Shape {
        color: [f32; 4],
        #[serde(default)]
        generator: Option<String>,
    },
    Text {
        text: String,
        size: f32,
    },
    /// A clip or still from the media library.
    Footage { media: MediaId },
    /// A nested composition ("Group into Comp").
    PreComp { comp: CompId },
}

/// An asset in the media library. `slot` marks it as a template placeholder
/// (template ecosystem, ARCHITECTURE.md); `perception` is the import-time
/// measurement card that gives no-vision AI quantitative sight
/// (ARCHITECTURE.md "Asset perception").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaAsset {
    pub id: MediaId,
    pub name: String,
    /// Where the media lives on disk. `None` for unfilled template slots.
    pub path: Option<String>,
    pub kind: MediaKind,
    /// If present, this asset is a template placeholder slot the user fills
    /// with their own media.
    pub slot: Option<SlotDef>,
    /// Human-confirmed binding ("logo") — resolves references deterministically
    /// after one confirmation (ARCHITECTURE.md asset grounding).
    #[serde(default)]
    pub alias: Option<String>,
    /// Import-time measurements. `None` until first acquisition/scan.
    #[serde(default)]
    pub perception: Option<PerceptionCard>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MediaKind {
    Image,
    Video { fps: FrameRate, duration: Time },
    Audio { duration: Time },
}

/// Role classification computed from perception heuristics at import —
/// "logo-candidate" regardless of what the file is called.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetRole {
    LogoCandidate,
    Photo,
    Graphic,
    Video,
    Audio,
    Placeholder,
    Unknown,
}

/// Deterministic measurements of a media asset, computed once at import by
/// sidecar tools (RULES: perception is a service, not an AI sense).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerceptionCard {
    pub role: AssetRole,
    pub width: u32,
    pub height: u32,
    /// Up to 5 dominant colors as RGB bytes (histogram bucketing).
    pub palette: Vec<[u8; 3]>,
    /// Mean luminance 0..1.
    pub mean_luma: f32,
    /// Histogram entropy 0..8 bits (photographic textures are high).
    pub entropy: f32,
    /// Fraction of fully-transparent pixels (images with alpha only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alpha_fraction: Option<f32>,
    /// Evidence strings for resolve_reference scoring ("OCR: ACME", etc.).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<String>,
}

/// A template placeholder: "drop your logo here".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlotDef {
    pub label: String,
}

impl Project {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            comps: BTreeMap::new(),
            media: BTreeMap::new(),
            next_comp: CompId(1),
            next_layer: LayerId(1),
            next_media: MediaId(1),
        }
    }

    /// Allocate a fresh composition ID and insert an empty comp.
    pub fn create_comp(
        &mut self,
        name: impl Into<String>,
        width: u32,
        height: u32,
        fps: FrameRate,
        duration: Time,
    ) -> CompId {
        let id = self.next_comp;
        self.next_comp = id.next();
        self.comps.insert(
            id,
            Comp {
                id,
                name: name.into(),
                width,
                height,
                fps,
                duration,
                background: [0.0, 0.0, 0.0, 1.0],
                layer_order: Vec::new(),
                layers: BTreeMap::new(),
            },
        );
        id
    }

    /// Allocate a fresh layer ID and insert the layer on top of `comp`'s stack.
    pub fn insert_layer(&mut self, comp: CompId, mut layer: Layer) -> LayerId {
        let id = self.next_layer;
        self.next_layer = id.next();
        layer.id = id;
        let c = self
            .comps
            .get_mut(&comp)
            .expect("insert_layer: comp must exist (ops validate before calling)");
        c.layer_order.push(id);
        c.layers.insert(id, layer);
        id
    }

    /// Allocate a fresh media ID and register the asset.
    pub fn insert_media(&mut self, mut asset: MediaAsset) -> MediaId {
        let id = self.next_media;
        self.next_media = id.next();
        asset.id = id;
        self.media.insert(id, asset);
        id
    }

    pub fn comp(&self, id: CompId) -> Option<&Comp> {
        self.comps.get(&id)
    }

    pub fn comp_mut(&mut self, id: CompId) -> Option<&mut Comp> {
        self.comps.get_mut(&id)
    }

    pub fn layer(&self, comp: CompId, id: LayerId) -> Option<&Layer> {
        self.comps.get(&comp)?.layers.get(&id)
    }

    pub fn layer_mut(&mut self, comp: CompId, id: LayerId) -> Option<&mut Layer> {
        self.comps.get_mut(&comp)?.layers.get_mut(&id)
    }
}

impl Comp {
    /// True if `time` (ticks) falls inside this comp's duration.
    pub fn contains_time(&self, time: Time) -> bool {
        time >= Time::ZERO && time < self.duration
    }

    /// Detect whether setting `layer_id`'s parent to `candidate_parent` introduces a cycle.
    pub fn has_parent_cycle(&self, layer_id: LayerId, candidate_parent: Option<LayerId>) -> bool {
        let mut curr = candidate_parent;
        let mut visited = HashSet::new();
        visited.insert(layer_id);
        while let Some(pid) = curr {
            if !visited.insert(pid) {
                return true;
            }
            curr = self.layers.get(&pid).and_then(|l| l.parent);
        }
        false
    }

    /// Compute the compound effective transform (inherited translation, rotation, scale, opacity)
    /// for `layer_id` at `time`, resolving through parent chains with cycle protection.
    pub fn effective_transform(&self, layer_id: LayerId, time: Time) -> Option<EffectiveTransform> {
        let target_layer = self.layers.get(&layer_id)?;

        // Build hierarchy chain from target_layer up to root
        let mut chain = Vec::new();
        let mut visited = HashSet::new();
        let mut curr_id = Some(layer_id);

        while let Some(id) = curr_id {
            if !visited.insert(id) {
                // Cycle detected in existing data; stop climbing
                break;
            }
            if let Some(l) = self.layers.get(&id) {
                chain.push((id, l));
                curr_id = l.parent;
            } else {
                break;
            }
        }
        // Reverse to get [root, ..., parent, target]
        chain.reverse();

        let mut matrix = [1.0f32, 0.0, 0.0, 1.0, 0.0, 0.0];
        let mut opacity = 1.0f32;
        let mut compound_rot = 0.0f32;
        let mut compound_scale = [1.0f32, 1.0f32];

        for (_, layer) in chain {
            let pos = match layer.evaluate(Property::Position, time) {
                PropValue::Vec2(v) => v,
                _ => [0.0, 0.0],
            };
            let sc = match layer.evaluate(Property::Scale, time) {
                PropValue::Vec2(v) => v,
                _ => [100.0, 100.0],
            };
            let rot = match layer.evaluate(Property::Rotation, time) {
                PropValue::Scalar(v) => v,
                _ => 0.0,
            };
            let op = match layer.evaluate(Property::Opacity, time) {
                PropValue::Scalar(v) => v.clamp(0.0, 1.0),
                _ => 1.0,
            };
            let anchor = match layer.evaluate(Property::AnchorPoint, time) {
                PropValue::Vec2(v) => v,
                _ => layer.transform.anchor_point,
            };

            opacity *= op;
            compound_rot += rot;
            compound_scale[0] *= sc[0] / 100.0;
            compound_scale[1] *= sc[1] / 100.0;

            // Local 2D affine matrix for this layer:
            // Translate by -anchor, scale by (sc/100), rotate by rot (radians), translate by +pos
            let rad = rot.to_radians();
            let cos_r = rad.cos();
            let sin_r = rad.sin();
            let sx = sc[0] / 100.0;
            let sy = sc[1] / 100.0;

            let local_a = sx * cos_r;
            let local_b = sx * sin_r;
            let local_c = -sy * sin_r;
            let local_d = sy * cos_r;
            let local_tx = pos[0] - (anchor[0] * local_a + anchor[1] * local_c);
            let local_ty = pos[1] - (anchor[0] * local_b + anchor[1] * local_d);
            let local_m = [local_a, local_b, local_c, local_d, local_tx, local_ty];

            // Multiply matrix = matrix * local_m
            matrix = mul_affine(matrix, local_m);
        }

        let target_anchor = match target_layer.evaluate(Property::AnchorPoint, time) {
            PropValue::Vec2(v) => v,
            _ => target_layer.transform.anchor_point,
        };

        let effective_pos = [
            matrix[4] + target_anchor[0] * matrix[0] + target_anchor[1] * matrix[2],
            matrix[5] + target_anchor[0] * matrix[1] + target_anchor[1] * matrix[3],
        ];

        Some(EffectiveTransform {
            position: effective_pos,
            scale: [compound_scale[0] * 100.0, compound_scale[1] * 100.0],
            rotation: compound_rot,
            opacity,
            anchor_point: target_anchor,
            matrix,
        })
    }
}

fn mul_affine(m1: [f32; 6], m2: [f32; 6]) -> [f32; 6] {
    let [a1, b1, c1, d1, tx1, ty1] = m1;
    let [a2, b2, c2, d2, tx2, ty2] = m2;
    [
        a1 * a2 + c1 * b2,
        b1 * a2 + d1 * b2,
        a1 * c2 + c1 * d2,
        b1 * c2 + d1 * d2,
        a1 * tx2 + c1 * ty2 + tx1,
        b1 * tx2 + d1 * ty2 + ty1,
    ]
}

impl Layer {
    /// True if this layer is on screen and visible at `time` (ticks).
    pub fn visible_at(&self, time: Time) -> bool {
        self.visible && time >= self.start && time < self.start + self.duration
    }

    /// Evaluate a property at `time`: the track wins if one exists,
    /// otherwise the static value.
    pub fn evaluate(&self, property: Property, time: Time) -> PropValue {
        match self.tracks.get(&property) {
            Some(track) => track
                .evaluate(time)
                .unwrap_or_else(|| property.static_value(&self.transform)),
            None => property.static_value(&self.transform),
        }
    }
}
