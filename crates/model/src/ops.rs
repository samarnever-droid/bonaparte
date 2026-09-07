//! The `Op` enum: the ONLY way project state changes (RULES §1.3).
//!
//! Everything derives from this one decision:
//! - Undo/redo = applying stored inverses (`History`).
//! - The MCP server and the AI command palette = sending `Op`s.
//! - The AI-diff preview = showing the `Op`s before applying them.
//! - The history list = the sequence of `Op`s, rendered with their docs.
//!
//! Every `Op` implements `apply` (mutate) and `invert` (compute the inverse
//! from the pre-state). Round-trip property: for any document D and op O,
//! `O.apply(D); O.invert(D').apply(D') == D`. The History tests below gate
//! every new `Op` variant.

use serde::{Deserialize, Serialize};

use crate::document::{BlendMode, Comp, Layer, LayerKind, MediaAsset, Project, Property};
use crate::effect::EffectInstance;
use crate::ids::{CompId, LayerId, MediaId};
use crate::keyframe::{Easing, Keyframe, PropValue};
use crate::time::{FrameRate, Time};

/// A single, undoable mutation of the project document.
///
/// Tagged JSON (`{"type": "addLayer", ...}`) so the TS UI, the MCP tools,
/// and the AI-diff all speak one readable wire format.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Op {
    /// One sample-accurate audio arrangement edit: clips, tracks, markers and mixer.
    SetCompAudio {
        comp: CompId,
        audio: crate::AudioArrangement,
    },
    /// Set the comp's 3D perspective camera. `fov <= 0` disables projection
    /// (pure 2D rendering, exactly the legacy pipeline).
    SetCamera {
        comp: CompId,
        position: [f32; 2],
        z: f32,
        fov: f32,
        #[serde(default)]
        focus: f32,
        #[serde(default)]
        dof: f32,
    },
    /// Set the comp's turntable auto-orbit (camera spins around the center).
    SetTurntable {
        comp: CompId,
        enabled: bool,
        period: f64,
    },
    /// One transaction and one history entry. Nested batches are deliberately refused.
    Batch {
        label: String,
        ops: Vec<Op>,
    },
    RenameProject {
        name: String,
    },
    /// Move a layer and every transform/effect keyframe by the same tick delta.
    ShiftLayer {
        comp: CompId,
        layer: LayerId,
        delta: Time,
    },
    /// Replace typed layer content without disturbing transforms or keyframes.
    SetLayerContent {
        comp: CompId,
        layer: LayerId,
        kind: LayerKind,
    },
    /// Replace the small ordered stack as one gesture, including parameter animation.
    SetLayerEffects {
        comp: CompId,
        layer: LayerId,
        effects: Vec<EffectInstance>,
    },
    /// Create a composition (ID allocated on apply; see `History::commit`).
    CreateComp {
        name: String,
        width: u32,
        height: u32,
        fps: FrameRate,
        duration: Time,
    },
    /// Restore an exact composition (used for undoing `RemoveComp`).
    RestoreComp {
        comp: Box<Comp>,
    },
    /// Delete a composition and everything in it.
    RemoveComp {
        comp: CompId,
    },
    /// Update composition properties.
    SetCompProps {
        comp: CompId,
        name: String,
        width: u32,
        height: u32,
        fps: FrameRate,
        duration: Time,
        background: [f32; 4],
    },
    /// Insert a new layer on top of the comp's stack (ID allocated on apply).
    AddLayer {
        comp: CompId,
        layer: Layer,
    },
    /// Restore an exact layer and its position in layer_order (used for undoing `RemoveLayer`).
    RestoreLayer {
        comp: CompId,
        layer: Box<Layer>,
        index: usize,
    },
    /// Remove a layer and its tracks.
    RemoveLayer {
        comp: CompId,
        layer: LayerId,
    },
    /// Change a layer's display name.
    RenameLayer {
        comp: CompId,
        layer: LayerId,
        name: String,
    },
    /// Set a layer's timeline start time and duration.
    SetLayerTime {
        comp: CompId,
        layer: LayerId,
        start: Time,
        duration: Time,
    },
    /// Set a layer's parent for transform inheritance.
    SetLayerParent {
        comp: CompId,
        layer: LayerId,
        parent: Option<LayerId>,
    },
    /// Set a layer's alpha blend mode.
    SetLayerBlendMode {
        comp: CompId,
        layer: LayerId,
        blend_mode: BlendMode,
    },
    /// Set a layer's visibility.
    SetLayerVisible {
        comp: CompId,
        layer: LayerId,
        visible: bool,
    },
    /// Set a layer's locked status.
    SetLayerLocked {
        comp: CompId,
        layer: LayerId,
        locked: bool,
    },
    /// Set the static value of a property. Fails on type mismatch. This does
    /// NOT touch tracks; while a track exists, it wins at evaluation time.
    SetValue {
        comp: CompId,
        layer: LayerId,
        property: Property,
        value: PropValue,
    },
    /// Insert or replace the keyframe at `key.time` on the property's track
    /// (creating the track if absent).
    AddKeyframe {
        comp: CompId,
        layer: LayerId,
        property: Property,
        key: Keyframe,
    },
    /// Remove the keyframe at `time` from the property's track.
    RemoveKeyframe {
        comp: CompId,
        layer: LayerId,
        property: Property,
        time: Time,
    },
    /// Move a keyframe in time (a drag in the timeline).
    MoveKeyframe {
        comp: CompId,
        layer: LayerId,
        property: Property,
        from: Time,
        to: Time,
    },
    /// Change the easing out of the keyframe at `time` (a graph-editor edit).
    SetEasing {
        comp: CompId,
        layer: LayerId,
        property: Property,
        time: Time,
        easing: Easing,
    },
    /// Move a layer to a new index in the stack (a timeline drag).
    ReorderLayer {
        comp: CompId,
        layer: LayerId,
        new_index: usize,
    },
    /// Register a media asset (ID allocated on apply). Perception cards and
    /// aliases ride on the asset.
    AddMedia {
        asset: MediaAsset,
    },
    /// Restore an exact media asset (used for undoing `RemoveMedia`).
    RestoreMedia {
        asset: MediaAsset,
    },
    /// Unregister a media asset.
    RemoveMedia {
        media: MediaId,
    },
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ModelError {
    #[error("{0}")]
    Invalid(String),
    #[error("composition {0} not found")]
    CompNotFound(CompId),
    #[error("layer {0} not found")]
    LayerNotFound(LayerId),
    #[error("media asset {0} not found")]
    MediaNotFound(MediaId),
    #[error("property {0:?} does not accept this value type")]
    TypeMismatch(Property),
    #[error("no keyframe at {1} on layer {0}")]
    KeyframeNotFound(LayerId, Time),
    #[error("no keyframe track on property {0:?}")]
    TrackMissing(Property),
    #[error("cannot move keyframe {1} to occupied time {2}")]
    KeyframeOccupied(LayerId, Time, Time),
    #[error("parenting layer {0} to {1} creates a circular dependency")]
    ParentCycle(LayerId, LayerId),
}

impl Op {
    /// Apply a primitive mutation. Persisted edits must go through `History::commit`
    /// (or the shared runtime commit) for whole-document validation and atomicity.
    /// A Batch also stages its own changes before replacing the document.
    ///
    /// Note: `CreateComp`/`AddLayer`/`AddMedia` allocate IDs during apply;
    /// use `History::commit` to record inverses with the assigned IDs.
    pub fn apply(self, project: &mut Project) -> Result<(), ModelError> {
        match self {
            Op::Batch { ops, .. } => {
                if ops.len() > 256 || ops.iter().any(|o| matches!(o, Op::Batch { .. })) {
                    return Err(ModelError::Invalid(
                        "Transactions allow at most 256 operations and cannot be nested".into(),
                    ));
                }
                let mut candidate = project.clone();
                for op in ops {
                    op.apply(&mut candidate)?;
                }
                candidate.validate().map_err(ModelError::Invalid)?;
                *project = candidate;
                Ok(())
            }
            Op::ShiftLayer { comp, layer, delta } => {
                let current = project
                    .layer(comp, layer)
                    .ok_or(ModelError::LayerNotFound(layer))?;
                let mut shifted = current.clone();
                let shift = |t: Time| {
                    t.0.checked_add(delta.0).map(Time).ok_or_else(|| {
                        ModelError::Invalid("Timeline shift overflows time range".into())
                    })
                };
                shifted.start = shift(shifted.start)?;
                for track in shifted.tracks.values_mut().chain(
                    shifted
                        .effects
                        .iter_mut()
                        .flat_map(|effect| effect.tracks.values_mut()),
                ) {
                    for key in &mut track.keys {
                        key.time = shift(key.time)?;
                    }
                }
                *project
                    .layer_mut(comp, layer)
                    .ok_or(ModelError::LayerNotFound(layer))? = shifted;
                Ok(())
            }
            Op::SetCompAudio { comp, audio } => {
                project
                    .comp_mut(comp)
                    .ok_or(ModelError::CompNotFound(comp))?
                    .audio = audio;
                Ok(())
            }
            Op::SetCamera {
                comp,
                position,
                z,
                fov,
                focus,
                dof,
            } => {
                let c = project
                    .comp_mut(comp)
                    .ok_or(ModelError::CompNotFound(comp))?;
                c.camera = crate::document::Camera {
                    position,
                    z,
                    fov: fov.max(0.0),
                    focus,
                    dof: dof.clamp(0.0, 1.0),
                };
                Ok(())
            }
            Op::SetTurntable {
                comp,
                enabled,
                period,
            } => {
                let c = project
                    .comp_mut(comp)
                    .ok_or(ModelError::CompNotFound(comp))?;
                c.turntable = crate::document::Turntable {
                    enabled,
                    period: period.max(0.5),
                };
                Ok(())
            }
            Op::RenameProject { name } => {
                project.name = name;
                Ok(())
            }
            Op::SetLayerContent { comp, layer, kind } => {
                crate::validation::validate_content(&kind).map_err(ModelError::Invalid)?;
                project
                    .layer_mut(comp, layer)
                    .ok_or(ModelError::LayerNotFound(layer))?
                    .kind = kind;
                Ok(())
            }
            Op::SetLayerEffects {
                comp,
                layer,
                effects,
            } => {
                crate::effect::validate_effect_stack(&effects).map_err(ModelError::Invalid)?;
                project
                    .layer_mut(comp, layer)
                    .ok_or(ModelError::LayerNotFound(layer))?
                    .effects = effects;
                Ok(())
            }
            Op::CreateComp {
                name,
                width,
                height,
                fps,
                duration,
            } => {
                project.create_comp(name, width, height, fps, duration);
                Ok(())
            }
            Op::RestoreComp { comp } => {
                if project.comps.contains_key(&comp.id) {
                    return Err(ModelError::Invalid(
                        "Cannot restore over an existing composition".into(),
                    ));
                }
                project.comps.insert(comp.id, *comp);
                Ok(())
            }
            Op::RemoveComp { comp } => {
                if project.comps.remove(&comp).is_none() {
                    return Err(ModelError::CompNotFound(comp));
                }
                Ok(())
            }
            Op::SetCompProps {
                comp,
                name,
                width,
                height,
                fps,
                duration,
                background,
            } => {
                let c = project
                    .comps
                    .get_mut(&comp)
                    .ok_or(ModelError::CompNotFound(comp))?;
                c.name = name;
                c.width = width;
                c.height = height;
                c.fps = fps;
                c.duration = duration;
                c.background = background;
                Ok(())
            }
            Op::AddLayer { comp, layer } => {
                if !project.comps.contains_key(&comp) {
                    return Err(ModelError::CompNotFound(comp));
                }
                project.insert_layer(comp, layer);
                Ok(())
            }
            Op::RestoreLayer { comp, layer, index } => {
                let c = project
                    .comps
                    .get_mut(&comp)
                    .ok_or(ModelError::CompNotFound(comp))?;
                let layer_id = layer.id;
                if c.layers.contains_key(&layer_id) {
                    return Err(ModelError::Invalid(
                        "Cannot restore over an existing layer".into(),
                    ));
                }
                c.layers.insert(layer_id, *layer);
                let insert_idx = index.min(c.layer_order.len());
                c.layer_order.insert(insert_idx, layer_id);
                Ok(())
            }
            Op::RemoveLayer { comp, layer } => {
                let c = project
                    .comps
                    .get_mut(&comp)
                    .ok_or(ModelError::CompNotFound(comp))?;
                c.layers
                    .remove(&layer)
                    .ok_or(ModelError::LayerNotFound(layer))?;
                c.layer_order.retain(|&l| l != layer);
                Ok(())
            }
            Op::RenameLayer { comp, layer, name } => {
                project
                    .layer_mut(comp, layer)
                    .ok_or(ModelError::LayerNotFound(layer))?
                    .name = name;
                Ok(())
            }
            Op::SetLayerTime {
                comp,
                layer,
                start,
                duration,
            } => {
                let l = project
                    .layer_mut(comp, layer)
                    .ok_or(ModelError::LayerNotFound(layer))?;
                l.start = start;
                l.duration = duration;
                Ok(())
            }
            Op::SetLayerParent {
                comp,
                layer,
                parent,
            } => {
                let c = project
                    .comps
                    .get_mut(&comp)
                    .ok_or(ModelError::CompNotFound(comp))?;
                if !c.layers.contains_key(&layer) {
                    return Err(ModelError::LayerNotFound(layer));
                }
                if let Some(pid) = parent {
                    if !c.layers.contains_key(&pid) {
                        return Err(ModelError::LayerNotFound(pid));
                    }
                    if c.has_parent_cycle(layer, parent) {
                        return Err(ModelError::ParentCycle(layer, pid));
                    }
                }
                c.layers.get_mut(&layer).unwrap().parent = parent;
                Ok(())
            }
            Op::SetLayerBlendMode {
                comp,
                layer,
                blend_mode,
            } => {
                project
                    .layer_mut(comp, layer)
                    .ok_or(ModelError::LayerNotFound(layer))?
                    .blend_mode = blend_mode;
                Ok(())
            }
            Op::SetLayerVisible {
                comp,
                layer,
                visible,
            } => {
                project
                    .layer_mut(comp, layer)
                    .ok_or(ModelError::LayerNotFound(layer))?
                    .visible = visible;
                Ok(())
            }
            Op::SetLayerLocked {
                comp,
                layer,
                locked,
            } => {
                project
                    .layer_mut(comp, layer)
                    .ok_or(ModelError::LayerNotFound(layer))?
                    .locked = locked;
                Ok(())
            }
            Op::SetValue {
                comp,
                layer,
                property,
                value,
            } => {
                let l = project
                    .layer_mut(comp, layer)
                    .ok_or(ModelError::LayerNotFound(layer))?;
                l.transform
                    .set(property, value)
                    .map_err(ModelError::TypeMismatch)?;
                Ok(())
            }
            Op::AddKeyframe {
                comp,
                layer,
                property,
                key,
            } => {
                if property.value_kind() != key.value.kind() {
                    return Err(ModelError::TypeMismatch(property));
                }
                let l = project
                    .layer_mut(comp, layer)
                    .ok_or(ModelError::LayerNotFound(layer))?;
                l.tracks.entry(property).or_default().set_key(key);
                Ok(())
            }
            Op::RemoveKeyframe {
                comp,
                layer,
                property,
                time,
            } => {
                let l = project
                    .layer_mut(comp, layer)
                    .ok_or(ModelError::LayerNotFound(layer))?;
                let track = l
                    .tracks
                    .get_mut(&property)
                    .ok_or(ModelError::TrackMissing(property))?;
                track
                    .remove_key(time)
                    .ok_or(ModelError::KeyframeNotFound(layer, time))?;
                Ok(())
            }
            Op::MoveKeyframe {
                comp,
                layer,
                property,
                from,
                to,
            } => {
                let l = project
                    .layer_mut(comp, layer)
                    .ok_or(ModelError::LayerNotFound(layer))?;
                let track = l
                    .tracks
                    .get_mut(&property)
                    .ok_or(ModelError::TrackMissing(property))?;
                if track.keys.iter().any(|k| k.time == to && k.time != from) {
                    return Err(ModelError::KeyframeOccupied(layer, from, to));
                }
                track
                    .move_key(from, to)
                    .ok_or(ModelError::KeyframeNotFound(layer, from))?;
                Ok(())
            }
            Op::SetEasing {
                comp,
                layer,
                property,
                time,
                easing,
            } => {
                let l = project
                    .layer_mut(comp, layer)
                    .ok_or(ModelError::LayerNotFound(layer))?;
                let track = l
                    .tracks
                    .get_mut(&property)
                    .ok_or(ModelError::TrackMissing(property))?;
                let key = track
                    .keys
                    .iter_mut()
                    .find(|k| k.time == time)
                    .ok_or(ModelError::KeyframeNotFound(layer, time))?;
                key.easing = easing;
                Ok(())
            }
            Op::ReorderLayer {
                comp,
                layer,
                new_index,
            } => {
                let c = project
                    .comps
                    .get_mut(&comp)
                    .ok_or(ModelError::CompNotFound(comp))?;
                let from = c
                    .layer_order
                    .iter()
                    .position(|&l| l == layer)
                    .ok_or(ModelError::LayerNotFound(layer))?;
                let item = c.layer_order.remove(from);
                c.layer_order
                    .insert(new_index.min(c.layer_order.len()), item);
                Ok(())
            }
            Op::AddMedia { asset } => {
                project.insert_media(asset);
                Ok(())
            }
            Op::RestoreMedia { asset } => {
                if project.media.contains_key(&asset.id) {
                    return Err(ModelError::Invalid(
                        "Cannot restore over an existing media asset".into(),
                    ));
                }
                project.media.insert(asset.id, asset);
                Ok(())
            }
            Op::RemoveMedia { media } => {
                if project.media.remove(&media).is_none() {
                    return Err(ModelError::MediaNotFound(media));
                }
                Ok(())
            }
        }
    }

    /// Compute the inverse of this op from the document's CURRENT state.
    /// Must be called BEFORE `apply`. Round-trip property:
    /// `inv = o.invert(d); o.apply(d); inv.apply(d)` restores `d`.
    /// For ID-allocating ops (`CreateComp`/`AddLayer`/`AddMedia`) the inverse
    /// is patched with the assigned ID by `History::commit`.
    pub fn invert(&self, project: &Project) -> Result<Op, ModelError> {
        match self {
            Op::Batch { label, ops } => {
                if ops.len() > 256 || ops.iter().any(|o| matches!(o, Op::Batch { .. })) {
                    return Err(ModelError::Invalid(
                        "Transactions allow at most 256 operations and cannot be nested".into(),
                    ));
                }
                let mut candidate = project.clone();
                let mut history = History::new();
                let mut inverses = Vec::with_capacity(ops.len());
                for op in ops {
                    history.commit(&mut candidate, op.clone())?;
                    // A transaction may contain 256 primitives, but user history
                    // retains 200 transactions. Drain each temporary entry now so
                    // the history cap cannot silently discard part of an inverse.
                    inverses.push(history.undo_stack.pop().expect("just committed").op);
                }
                inverses.reverse();
                Ok(Op::Batch {
                    label: label.clone(),
                    ops: inverses,
                })
            }
            Op::ShiftLayer { comp, layer, delta } => Ok(Op::ShiftLayer {
                comp: *comp,
                layer: *layer,
                delta: Time(delta.0.checked_neg().ok_or_else(|| {
                    ModelError::Invalid("Timeline shift overflows time range".into())
                })?),
            }),
            Op::SetCamera { comp, .. } => {
                let c = project.comp(*comp).ok_or(ModelError::CompNotFound(*comp))?;
                Ok(Op::SetCamera {
                    comp: *comp,
                    position: c.camera.position,
                    z: c.camera.z,
                    fov: c.camera.fov,
                    focus: c.camera.focus,
                    dof: c.camera.dof,
                })
            }
            Op::SetTurntable { comp, .. } => {
                let c = project.comp(*comp).ok_or(ModelError::CompNotFound(*comp))?;
                Ok(Op::SetTurntable {
                    comp: *comp,
                    enabled: c.turntable.enabled,
                    period: c.turntable.period,
                })
            }
            Op::SetCompAudio { comp, .. } => Ok(Op::SetCompAudio {
                comp: *comp,
                audio: project
                    .comp(*comp)
                    .ok_or(ModelError::CompNotFound(*comp))?
                    .audio
                    .clone(),
            }),
            Op::RenameProject { .. } => Ok(Op::RenameProject {
                name: project.name.clone(),
            }),
            Op::SetLayerContent { comp, layer, .. } => Ok(Op::SetLayerContent {
                comp: *comp,
                layer: *layer,
                kind: project
                    .layer(*comp, *layer)
                    .ok_or(ModelError::LayerNotFound(*layer))?
                    .kind
                    .clone(),
            }),
            Op::SetLayerEffects { comp, layer, .. } => Ok(Op::SetLayerEffects {
                comp: *comp,
                layer: *layer,
                effects: project
                    .layer(*comp, *layer)
                    .ok_or(ModelError::LayerNotFound(*layer))?
                    .effects
                    .clone(),
            }),
            Op::CreateComp { .. } => Ok(Op::RemoveComp { comp: CompId(0) }), // patched post-apply
            Op::RestoreComp { comp } => Ok(Op::RemoveComp { comp: comp.id }),
            Op::RemoveComp { comp } => {
                let c = project
                    .comp(*comp)
                    .ok_or(ModelError::CompNotFound(*comp))?
                    .clone();
                Ok(Op::RestoreComp { comp: Box::new(c) })
            }
            Op::SetCompProps { comp, .. } => {
                let c = project.comp(*comp).ok_or(ModelError::CompNotFound(*comp))?;
                Ok(Op::SetCompProps {
                    comp: *comp,
                    name: c.name.clone(),
                    width: c.width,
                    height: c.height,
                    fps: c.fps,
                    duration: c.duration,
                    background: c.background,
                })
            }
            Op::AddLayer { comp, .. } => Ok(Op::RemoveLayer {
                comp: *comp,
                layer: LayerId(0), // patched post-apply by History::commit
            }),
            Op::RestoreLayer { comp, layer, .. } => Ok(Op::RemoveLayer {
                comp: *comp,
                layer: layer.id,
            }),
            Op::RemoveLayer { comp, layer } => {
                let c = project.comp(*comp).ok_or(ModelError::CompNotFound(*comp))?;
                let l = c
                    .layers
                    .get(layer)
                    .ok_or(ModelError::LayerNotFound(*layer))?
                    .clone();
                let idx = c
                    .layer_order
                    .iter()
                    .position(|&x| x == *layer)
                    .unwrap_or(c.layer_order.len());
                Ok(Op::RestoreLayer {
                    comp: *comp,
                    layer: Box::new(l),
                    index: idx,
                })
            }
            Op::RenameLayer { comp, layer, .. } => {
                let old = project
                    .layer(*comp, *layer)
                    .ok_or(ModelError::LayerNotFound(*layer))?
                    .name
                    .clone();
                Ok(Op::RenameLayer {
                    comp: *comp,
                    layer: *layer,
                    name: old,
                })
            }
            Op::SetLayerTime { comp, layer, .. } => {
                let l = project
                    .layer(*comp, *layer)
                    .ok_or(ModelError::LayerNotFound(*layer))?;
                Ok(Op::SetLayerTime {
                    comp: *comp,
                    layer: *layer,
                    start: l.start,
                    duration: l.duration,
                })
            }
            Op::SetLayerParent { comp, layer, .. } => {
                let l = project
                    .layer(*comp, *layer)
                    .ok_or(ModelError::LayerNotFound(*layer))?;
                Ok(Op::SetLayerParent {
                    comp: *comp,
                    layer: *layer,
                    parent: l.parent,
                })
            }
            Op::SetLayerBlendMode { comp, layer, .. } => {
                let l = project
                    .layer(*comp, *layer)
                    .ok_or(ModelError::LayerNotFound(*layer))?;
                Ok(Op::SetLayerBlendMode {
                    comp: *comp,
                    layer: *layer,
                    blend_mode: l.blend_mode,
                })
            }
            Op::SetLayerVisible { comp, layer, .. } => {
                let l = project
                    .layer(*comp, *layer)
                    .ok_or(ModelError::LayerNotFound(*layer))?;
                Ok(Op::SetLayerVisible {
                    comp: *comp,
                    layer: *layer,
                    visible: l.visible,
                })
            }
            Op::SetLayerLocked { comp, layer, .. } => {
                let l = project
                    .layer(*comp, *layer)
                    .ok_or(ModelError::LayerNotFound(*layer))?;
                Ok(Op::SetLayerLocked {
                    comp: *comp,
                    layer: *layer,
                    locked: l.locked,
                })
            }
            Op::SetValue {
                comp,
                layer,
                property,
                ..
            } => {
                let l = project
                    .layer(*comp, *layer)
                    .ok_or(ModelError::LayerNotFound(*layer))?;
                Ok(Op::SetValue {
                    comp: *comp,
                    layer: *layer,
                    property: *property,
                    value: property.static_value(&l.transform),
                })
            }
            Op::AddKeyframe {
                comp,
                layer,
                property,
                key,
            } => {
                let l = project
                    .layer(*comp, *layer)
                    .ok_or(ModelError::LayerNotFound(*layer))?;
                let old_key = l
                    .tracks
                    .get(property)
                    .and_then(|t| t.keys.iter().find(|k| k.time == key.time).cloned());
                if let Some(prev) = old_key {
                    Ok(Op::AddKeyframe {
                        comp: *comp,
                        layer: *layer,
                        property: *property,
                        key: prev,
                    })
                } else {
                    Ok(Op::RemoveKeyframe {
                        comp: *comp,
                        layer: *layer,
                        property: *property,
                        time: key.time,
                    })
                }
            }
            Op::RemoveKeyframe {
                comp,
                layer,
                property,
                time,
            } => {
                let l = project
                    .layer(*comp, *layer)
                    .ok_or(ModelError::LayerNotFound(*layer))?;
                let track = l
                    .tracks
                    .get(property)
                    .ok_or(ModelError::TrackMissing(*property))?;
                let key = track
                    .keys
                    .iter()
                    .find(|k| k.time == *time)
                    .ok_or(ModelError::KeyframeNotFound(*layer, *time))?
                    .clone();
                Ok(Op::AddKeyframe {
                    comp: *comp,
                    layer: *layer,
                    property: *property,
                    key,
                })
            }
            Op::MoveKeyframe {
                comp,
                layer,
                property,
                from,
                to,
            } => Ok(Op::MoveKeyframe {
                comp: *comp,
                layer: *layer,
                property: *property,
                from: *to,
                to: *from,
            }),
            Op::SetEasing {
                comp,
                layer,
                property,
                time,
                ..
            } => {
                let l = project
                    .layer(*comp, *layer)
                    .ok_or(ModelError::LayerNotFound(*layer))?;
                let track = l
                    .tracks
                    .get(property)
                    .ok_or(ModelError::TrackMissing(*property))?;
                let key = track
                    .keys
                    .iter()
                    .find(|k| k.time == *time)
                    .ok_or(ModelError::KeyframeNotFound(*layer, *time))?;
                Ok(Op::SetEasing {
                    comp: *comp,
                    layer: *layer,
                    property: *property,
                    time: *time,
                    easing: key.easing,
                })
            }
            Op::ReorderLayer { comp, layer, .. } => {
                let c = project.comp(*comp).ok_or(ModelError::CompNotFound(*comp))?;
                let old_index = c
                    .layer_order
                    .iter()
                    .position(|&l| l == *layer)
                    .ok_or(ModelError::LayerNotFound(*layer))?;
                Ok(Op::ReorderLayer {
                    comp: *comp,
                    layer: *layer,
                    new_index: old_index,
                })
            }
            Op::AddMedia { .. } => Ok(Op::RemoveMedia { media: MediaId(0) }), // patched post-apply
            Op::RestoreMedia { asset } => Ok(Op::RemoveMedia { media: asset.id }),
            Op::RemoveMedia { media } => {
                let a = project
                    .media
                    .get(media)
                    .ok_or(ModelError::MediaNotFound(*media))?
                    .clone();
                Ok(Op::RestoreMedia { asset: a })
            }
        }
    }

    /// Human-readable summary for the history list and AI proposals —
    /// the UI and MCP render from this (RULES §7).
    pub fn describe(&self) -> String {
        match self {
            Op::Batch { label, .. } => label.clone(),
            Op::ShiftLayer { layer, delta, .. } => format!(
                "Moved layer {layer} and its animation by {:.3}s",
                delta.as_secs_f64()
            ),
            Op::SetCompAudio { .. } => "Edited audio timeline".into(),
            Op::SetCamera { .. } => "Moved the 3D camera".into(),
            Op::SetTurntable { enabled, .. } => {
                if *enabled {
                    "Started the turntable".into()
                } else {
                    "Stopped the turntable".into()
                }
            }
            Op::RenameProject { name } => format!("Renamed project to “{name}”"),
            Op::SetLayerContent { layer, .. } => format!("Edited layer {layer} content"),
            Op::SetLayerEffects { layer, .. } => format!("Edited layer {layer} effects"),
            Op::CreateComp { name, .. } => format!("Created comp “{name}”"),
            Op::RestoreComp { comp } => format!("Restored comp “{}” ({})", comp.name, comp.id),
            Op::RemoveComp { comp } => format!("Deleted comp {comp}"),
            Op::SetCompProps { comp, name, .. } => {
                format!("Updated comp {comp} properties ({name})")
            }
            Op::AddLayer { layer, .. } => format!("Added layer “{}”", layer.name),
            Op::RestoreLayer { layer, .. } => {
                format!("Restored layer “{}” ({})", layer.name, layer.id)
            }
            Op::RemoveLayer { layer, .. } => format!("Removed layer {layer}"),
            Op::RenameLayer { name, .. } => format!("Renamed layer to “{name}”"),
            Op::SetLayerTime {
                layer,
                start,
                duration,
                ..
            } => {
                format!("Set layer {layer} timing: start={start}, duration={duration}")
            }
            Op::SetLayerParent { layer, parent, .. } => match parent {
                Some(p) => format!("Parented layer {layer} to {p}"),
                None => format!("Unparented layer {layer}"),
            },
            Op::SetLayerBlendMode {
                layer, blend_mode, ..
            } => {
                format!("Set layer {layer} blend mode to {blend_mode}")
            }
            Op::SetLayerVisible { layer, visible, .. } => {
                format!("Set layer {layer} visibility to {visible}")
            }
            Op::SetLayerLocked { layer, locked, .. } => {
                format!("Set layer {layer} locked to {locked}")
            }
            Op::SetValue {
                property, value, ..
            } => format!("Set {property:?} to {value:?}"),
            Op::AddKeyframe { property, key, .. } => {
                format!(
                    "Added {property:?} keyframe at t={:.2}s",
                    key.time.as_secs_f64()
                )
            }
            Op::RemoveKeyframe { property, time, .. } => {
                format!(
                    "Removed {property:?} keyframe at t={:.2}s",
                    time.as_secs_f64()
                )
            }
            Op::MoveKeyframe {
                property, from, to, ..
            } => format!(
                "Moved {property:?} keyframe {:.2}s → {:.2}s",
                from.as_secs_f64(),
                to.as_secs_f64()
            ),
            Op::SetEasing { property, time, .. } => {
                format!(
                    "Changed {property:?} easing at t={:.2}s",
                    time.as_secs_f64()
                )
            }
            Op::ReorderLayer {
                layer, new_index, ..
            } => {
                format!("Reordered layer {layer} to position {new_index}")
            }
            Op::AddMedia { asset } => format!("Imported media “{}”", asset.name),
            Op::RestoreMedia { asset } => format!("Restored media “{}” ({})", asset.name, asset.id),
            Op::RemoveMedia { media } => format!("Removed media {media}"),
        }
    }
}

/// Bounded history with atomic commit, undo and redo. Failed operations never
/// modify the document, allocators, or either history stack.
#[derive(Debug, Clone)]
struct HistoryEntry {
    op: Op,
    label: String,
    edit_group: Option<String>,
}

#[derive(Debug, Default, Clone)]
pub struct History {
    undo_stack: Vec<HistoryEntry>,
    redo_stack: Vec<HistoryEntry>,
}

fn merge_targets(op: &Op) -> Option<Vec<String>> {
    let mut result = match op {
        Op::SetLayerContent { comp, layer, .. } => vec![format!("content:{comp}:{layer}")],
        Op::SetLayerEffects { comp, layer, .. } => vec![format!("effects:{comp}:{layer}")],
        Op::SetValue {
            comp,
            layer,
            property,
            ..
        } => vec![format!("value:{comp}:{layer}:{property:?}")],
        Op::AddKeyframe {
            comp,
            layer,
            property,
            key,
        } => vec![format!("key:{comp}:{layer}:{property:?}:{}", key.time.0)],
        Op::RemoveKeyframe {
            comp,
            layer,
            property,
            time,
        } => vec![format!("key:{comp}:{layer}:{property:?}:{}", time.0)],
        Op::RenameLayer { comp, layer, .. } => vec![format!("name:{comp}:{layer}")],
        Op::SetCamera { comp, .. } => vec![format!("camera:{comp}")],
        Op::SetTurntable { comp, .. } => vec![format!("turntable:{comp}")],
        Op::Batch { ops, .. } => {
            let mut all = vec![];
            for op in ops {
                all.extend(merge_targets(op)?);
            }
            all
        }
        _ => return None,
    };
    result.sort();
    Some(result)
}

impl History {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of undoable entries currently retained (bounded at 200).
    pub fn undo_len(&self) -> usize {
        self.undo_stack.len()
    }

    /// Number of redoable entries currently retained.
    pub fn redo_len(&self) -> usize {
        self.redo_stack.len()
    }

    pub fn commit(&mut self, project: &mut Project, op: Op) -> Result<(), ModelError> {
        self.commit_grouped(project, op, None)
    }
    pub fn commit_grouped(
        &mut self,
        project: &mut Project,
        op: Op,
        group: Option<String>,
    ) -> Result<(), ModelError> {
        if group
            .as_ref()
            .is_some_and(|g| g.is_empty() || g.len() > 128)
        {
            return Err(ModelError::Invalid("Invalid live edit group".into()));
        }
        let merge = group.as_ref().is_some_and(|g| {
            self.redo_stack.is_empty()
                && self.undo_stack.last().is_some_and(|last| {
                    last.edit_group.as_ref() == Some(g)
                        && merge_targets(&last.op) == merge_targets(&op)
                        && merge_targets(&op).is_some()
                })
        });
        project.validate().map_err(ModelError::Invalid)?;
        let label = op.describe();
        let inverse = match &op {
            Op::CreateComp { .. } => Op::RemoveComp {
                comp: project.next_comp,
            },
            Op::AddLayer { comp, .. } => Op::RemoveLayer {
                comp: *comp,
                layer: project.next_layer,
            },
            Op::AddMedia { .. } => Op::RemoveMedia {
                media: project.next_media,
            },
            _ => op.invert(project)?,
        };
        let mut candidate = project.clone();
        op.apply(&mut candidate)?;
        candidate.validate().map_err(ModelError::Invalid)?;
        *project = candidate;
        if merge {
            self.undo_stack.last_mut().expect("merge target").label = label;
        } else {
            self.undo_stack.push(HistoryEntry {
                op: inverse,
                label,
                edit_group: group,
            });
        }
        if self.undo_stack.len() > 200 {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
        Ok(())
    }

    pub fn undo(&mut self, project: &mut Project) -> Result<bool, ModelError> {
        let Some(entry) = self.undo_stack.last().cloned() else {
            return Ok(false);
        };
        let redo = entry.op.invert(project)?;
        let mut candidate = project.clone();
        entry.op.apply(&mut candidate)?;
        candidate.validate().map_err(ModelError::Invalid)?;
        *project = candidate;
        self.undo_stack.pop();
        if let Some(last) = self.undo_stack.last_mut() {
            last.edit_group = None;
        }
        self.redo_stack.push(HistoryEntry {
            op: redo,
            label: entry.label,
            edit_group: None,
        });
        Ok(true)
    }

    pub fn redo(&mut self, project: &mut Project) -> Result<bool, ModelError> {
        let Some(entry) = self.redo_stack.last().cloned() else {
            return Ok(false);
        };
        let inverse = entry.op.invert(project)?;
        let mut candidate = project.clone();
        entry.op.apply(&mut candidate)?;
        candidate.validate().map_err(ModelError::Invalid)?;
        *project = candidate;
        self.redo_stack.pop();
        self.undo_stack.push(HistoryEntry {
            op: inverse,
            label: entry.label,
            edit_group: None,
        });
        Ok(true)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }
    /// Label of the entry a `redo` would re-apply — i.e. the one `undo` just
    /// reverted. Feeds "Undid: …" feedback in the UI and MCP responses.
    pub fn redo_top_label(&self) -> Option<&str> {
        self.redo_stack.last().map(|e| e.label.as_str())
    }

    /// Label of the entry an `undo` would revert — i.e. the one `redo` just
    /// re-applied.
    pub fn undo_top_label(&self) -> Option<&str> {
        self.undo_stack.last().map(|e| e.label.as_str())
    }

    pub fn undo_descriptions(&self) -> Vec<String> {
        self.undo_stack
            .iter()
            .map(|entry| entry.label.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{LayerKind, MediaKind};

    fn make_test_project() -> (Project, CompId, LayerId) {
        let mut p = Project::new("Test Project");
        let comp = p.create_comp("Comp 1", 1920, 1080, FrameRate::FPS_30, Time(120_000 * 5));
        let layer = p.insert_layer(
            comp,
            Layer::new(
                "Layer 1",
                LayerKind::Solid {
                    color: [1.0, 0.0, 0.0, 1.0],
                },
                Time::ZERO,
                Time(120_000 * 5),
            ),
        );
        (p, comp, layer)
    }

    #[test]
    fn roundtrip_set_value() {
        let (mut p, comp, layer) = make_test_project();
        let mut history = History::new();

        let initial_pos = p.layer(comp, layer).unwrap().transform.position;
        assert_eq!(initial_pos, [0.0, 0.0]);

        let op = Op::SetValue {
            comp,
            layer,
            property: Property::Position,
            value: PropValue::Vec2([150.0, -75.0]),
        };

        history.commit(&mut p, op).unwrap();
        assert_eq!(
            p.layer(comp, layer).unwrap().transform.position,
            [150.0, -75.0]
        );

        assert!(history.undo(&mut p).unwrap());
        assert_eq!(p.layer(comp, layer).unwrap().transform.position, [0.0, 0.0]);

        assert!(history.redo(&mut p).unwrap());
        assert_eq!(
            p.layer(comp, layer).unwrap().transform.position,
            [150.0, -75.0]
        );
    }

    #[test]
    fn roundtrip_layer_parenting_and_cycle_guard() {
        let (mut p, comp, layer1) = make_test_project();
        let layer2 = p.insert_layer(
            comp,
            Layer::new(
                "Layer 2",
                LayerKind::Solid {
                    color: [0.0, 1.0, 0.0, 1.0],
                },
                Time::ZERO,
                Time(120_000 * 5),
            ),
        );
        let layer3 = p.insert_layer(
            comp,
            Layer::new(
                "Layer 3",
                LayerKind::Solid {
                    color: [0.0, 0.0, 1.0, 1.0],
                },
                Time::ZERO,
                Time(120_000 * 5),
            ),
        );

        let mut history = History::new();

        // 2 -> 1
        history
            .commit(
                &mut p,
                Op::SetLayerParent {
                    comp,
                    layer: layer2,
                    parent: Some(layer1),
                },
            )
            .unwrap();
        assert_eq!(p.layer(comp, layer2).unwrap().parent, Some(layer1));

        // 3 -> 2
        history
            .commit(
                &mut p,
                Op::SetLayerParent {
                    comp,
                    layer: layer3,
                    parent: Some(layer2),
                },
            )
            .unwrap();
        assert_eq!(p.layer(comp, layer3).unwrap().parent, Some(layer2));

        // Attempt cycle: 1 -> 3
        let cycle_op = Op::SetLayerParent {
            comp,
            layer: layer1,
            parent: Some(layer3),
        };
        let err = history.commit(&mut p, cycle_op);
        assert!(
            matches!(err, Err(ModelError::ParentCycle(l1, l3)) if l1 == layer1 && l3 == layer3)
        );

        // Undo 3 -> 2
        assert!(history.undo(&mut p).unwrap());
        assert_eq!(p.layer(comp, layer3).unwrap().parent, None);

        // Redo 3 -> 2
        assert!(history.redo(&mut p).unwrap());
        assert_eq!(p.layer(comp, layer3).unwrap().parent, Some(layer2));
    }

    #[test]
    fn roundtrip_remove_comp_exact_restoration() {
        let (mut p, comp, layer) = make_test_project();
        let mut history = History::new();

        // Set something unique on the layer
        history
            .commit(
                &mut p,
                Op::RenameLayer {
                    comp,
                    layer,
                    name: "Special Layer".into(),
                },
            )
            .unwrap();

        // Remove comp
        history.commit(&mut p, Op::RemoveComp { comp }).unwrap();
        assert!(p.comp(comp).is_none());

        // Undo RemoveComp -> comp and all layers restored exactly with identical IDs
        assert!(history.undo(&mut p).unwrap());
        assert!(p.comp(comp).is_some());
        assert_eq!(p.layer(comp, layer).unwrap().name, "Special Layer");

        // Redo RemoveComp
        assert!(history.redo(&mut p).unwrap());
        assert!(p.comp(comp).is_none());
    }

    #[test]
    fn roundtrip_remove_layer_exact_restoration() {
        let (mut p, comp, layer1) = make_test_project();
        let layer2 = p.insert_layer(
            comp,
            Layer::new(
                "Layer 2",
                LayerKind::Solid {
                    color: [0.0, 1.0, 0.0, 1.0],
                },
                Time::ZERO,
                Time(120_000 * 5),
            ),
        );

        let mut history = History::new();

        // Remove layer1
        history
            .commit(
                &mut p,
                Op::RemoveLayer {
                    comp,
                    layer: layer1,
                },
            )
            .unwrap();
        assert_eq!(p.comp(comp).unwrap().layer_order, vec![layer2]);

        // Undo -> layer1 restored at its original index (index 0)
        assert!(history.undo(&mut p).unwrap());
        assert_eq!(p.comp(comp).unwrap().layer_order, vec![layer1, layer2]);
        assert_eq!(p.layer(comp, layer1).unwrap().name, "Layer 1");
    }

    #[test]
    fn roundtrip_all_layer_property_ops() {
        let (mut p, comp, layer) = make_test_project();
        let mut history = History::new();

        // SetLayerTime
        history
            .commit(
                &mut p,
                Op::SetLayerTime {
                    comp,
                    layer,
                    start: Time(10_000),
                    duration: Time(50_000),
                },
            )
            .unwrap();
        assert_eq!(p.layer(comp, layer).unwrap().start, Time(10_000));
        assert_eq!(p.layer(comp, layer).unwrap().duration, Time(50_000));

        // SetLayerBlendMode
        history
            .commit(
                &mut p,
                Op::SetLayerBlendMode {
                    comp,
                    layer,
                    blend_mode: BlendMode::Multiply,
                },
            )
            .unwrap();
        assert_eq!(
            p.layer(comp, layer).unwrap().blend_mode,
            BlendMode::Multiply
        );

        // SetLayerVisible
        history
            .commit(
                &mut p,
                Op::SetLayerVisible {
                    comp,
                    layer,
                    visible: false,
                },
            )
            .unwrap();
        assert!(!p.layer(comp, layer).unwrap().visible);

        // SetLayerLocked
        history
            .commit(
                &mut p,
                Op::SetLayerLocked {
                    comp,
                    layer,
                    locked: true,
                },
            )
            .unwrap();
        assert!(p.layer(comp, layer).unwrap().locked);

        // Undo all 4 ops
        assert!(history.undo(&mut p).unwrap()); // locked
        assert!(!p.layer(comp, layer).unwrap().locked);

        assert!(history.undo(&mut p).unwrap()); // visible
        assert!(p.layer(comp, layer).unwrap().visible);

        assert!(history.undo(&mut p).unwrap()); // blend_mode
        assert_eq!(p.layer(comp, layer).unwrap().blend_mode, BlendMode::Normal);

        assert!(history.undo(&mut p).unwrap()); // time
        assert_eq!(p.layer(comp, layer).unwrap().start, Time::ZERO);
        assert_eq!(p.layer(comp, layer).unwrap().duration, Time(120_000 * 5));
    }

    #[test]
    fn roundtrip_media_ops() {
        let mut p = Project::new("Media Project");
        let mut history = History::new();

        let asset = MediaAsset {
            id: MediaId(0),
            name: "test.png".into(),
            path: Some("/path/to/test.png".into()),
            kind: MediaKind::Image,
            embedded: None,
            audio: None,
            slot: None,
            alias: Some("logo".into()),
            perception: None,
        };

        history.commit(&mut p, Op::AddMedia { asset }).unwrap();
        assert_eq!(p.media.len(), 1);
        let media_id = *p.media.keys().next().unwrap();

        history
            .commit(&mut p, Op::RemoveMedia { media: media_id })
            .unwrap();
        assert!(p.media.is_empty());

        assert!(history.undo(&mut p).unwrap());
        assert_eq!(p.media.len(), 1);
        assert_eq!(p.media.get(&media_id).unwrap().name, "test.png");
    }
}
