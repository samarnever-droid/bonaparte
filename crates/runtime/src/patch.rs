//! Authoritative, opt-in UI deltas. A client never reimplements Op semantics;
//! it receives the actual changed model objects after validation/history commit.
use bonaparte_model::*;
use serde_json::{json, Value};
use std::sync::Arc;
fn media_value(asset: &MediaAsset) -> Value {
    let mut asset = asset.clone();
    if let Some(audio) = &mut asset.audio {
        audio.data_base64 = Arc::from("");
    }
    serde_json::to_value(asset).expect("validated media")
}
pub fn delta(
    before: &Project,
    after: &Project,
    base: u64,
    revision: u64,
    audio_revision: u64,
    history: &History,
) -> Value {
    let mut comps = serde_json::Map::new();
    for (id, new) in &after.comps {
        let Some(old) = before.comps.get(id) else {
            comps.insert(id.0.to_string(), json!({"full":new}));
            continue;
        };
        if old == new {
            continue;
        }
        let mut change = serde_json::Map::new();
        if old.name != new.name
            || old.width != new.width
            || old.height != new.height
            || old.fps != new.fps
            || old.duration != new.duration
            || old.background != new.background
            || old.camera != new.camera
            || old.turntable != new.turntable
        {
            change.insert("props".into(),json!({"name":new.name,"width":new.width,"height":new.height,"fps":new.fps,"duration":new.duration,"background":new.background,"camera":new.camera,"turntable":new.turntable}));
        }
        if old.layer_order != new.layer_order {
            change.insert("layerOrder".into(), json!(new.layer_order));
        }
        if old.audio != new.audio {
            change.insert("audio".into(), json!(new.audio));
        }
        let mut layers = serde_json::Map::new();
        for (id, layer) in &new.layers {
            if old.layers.get(id) != Some(layer) {
                layers.insert(id.0.to_string(), json!(layer));
            }
        }
        if !layers.is_empty() {
            change.insert("layers".into(), Value::Object(layers));
        }
        let removed: Vec<_> = old
            .layers
            .keys()
            .filter(|id| !new.layers.contains_key(id))
            .collect();
        if !removed.is_empty() {
            change.insert("removedLayers".into(), json!(removed));
        }
        comps.insert(id.0.to_string(), Value::Object(change));
    }
    let mut media = serde_json::Map::new();
    for (id, asset) in &after.media {
        if before.media.get(id) != Some(asset) {
            media.insert(id.0.to_string(), media_value(asset));
        }
    }
    json!({"kind":"patch","baseRevision":base,"revision":revision,"audioRevision":audio_revision,"canUndo":history.can_undo(),"canRedo":history.can_redo(),"history":history.undo_descriptions(),"historyDepth":history.undo_len(),"historyOverflow":history.undo_overflow(),"name":after.name,"nextComp":after.next_comp,"nextLayer":after.next_layer,"nextMedia":after.next_media,"comps":comps,"removedComps":before.comps.keys().filter(|id|!after.comps.contains_key(id)).collect::<Vec<_>>(),"media":media,"removedMedia":before.media.keys().filter(|id|!after.media.contains_key(id)).collect::<Vec<_>>()})
}
/// Audio and visual revisions are independent. Position/scale/rotation, image
/// changes, marker edits and tempo-grid edits do not invalidate audible PCM.
pub fn audio_changed(before: &Project, after: &Project) -> bool {
    if before.comps.len() != after.comps.len() {
        return true;
    }
    for (id, comp) in &after.comps {
        let Some(old) = before.comps.get(id) else {
            return true;
        };
        if old.duration != comp.duration
            || old.audio.tracks != comp.audio.tracks
            || old.audio.gain_db != comp.audio.gain_db
            || old.audio.muted != comp.audio.muted
            || old.audio.processing != comp.audio.processing
            || old.audio.limiter != comp.audio.limiter
            || old.audio.buses != comp.audio.buses
        {
            return true;
        }
        let nested = |c: &Comp| {
            c.layers
                .values()
                .filter_map(|l| {
                    if let LayerKind::PreComp { comp } = l.kind {
                        Some((l.id, comp, l.start, l.duration, l.visible))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        };
        if nested(old) != nested(comp) {
            return true;
        }
    }
    let sources = |p: &Project| {
        p.media
            .iter()
            .filter_map(|(id, m)| {
                m.audio
                    .as_ref()
                    .map(|a| (*id, a.sha256.clone(), a.frames, a.channels))
            })
            .collect::<Vec<_>>()
    };
    sources(before) != sources(after)
}
