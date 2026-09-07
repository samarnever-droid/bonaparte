//! # bonaparte-model
//!
//! The spine of Bonaparte: the project document, the [`ops::Op`] enum (the only
//! way state ever changes), keyframe tracks, media/slot definitions, and the
//! plugin manifest schema.
//!
//! Owns: document types, `Op` vocabulary, undo/redo, interpolation.
//! Must never depend on: the engine, the UI, media I/O, or any host capability.
//! Consumers: `bonaparte-engine` (renders it), `bonaparte-app` (edits it via
//! generated TS types), `bonaparte-mcp` (drives it headless).
//!
//! Everything here is deterministic: same document + same `Op` sequence =
//! same document. This is what makes undo, golden-frame tests, and MCP
//! automation all trustworthy.

pub mod document;
pub mod effect;
pub mod ids;
pub mod keyframe;
pub mod manifest;
pub mod ops;
pub mod time;
pub mod validation;

pub use document::{
    AssetRole, BlendMode, Camera, Comp, EffectiveTransform, EmbeddedImage, Layer, LayerKind,
    MediaAsset, MediaKind, PerceptionCard, Project, Property, ShapeStyle, SlotDef, StaticTransform,
    TextStyle, Turntable,
};
pub use ids::{CompId, LayerId, MediaId};
pub use keyframe::{Easing, Keyframe, PropValue, Track};
pub use manifest::{
    EffectManifest, GpuCost, ManifestError, ParamDef, ParamKind, SUPPORTED_API_MAJOR,
};
pub use ops::{History, ModelError, Op};
pub use time::{FrameRate, Time, TimecodeError, TICKS_PER_SEC};

pub use effect::{EffectInstance, EffectValue};

pub mod audio;
pub use audio::*;

pub mod interaction;
pub use interaction::TransformOverride;

pub mod audio_fx;
pub use audio_fx::*;
