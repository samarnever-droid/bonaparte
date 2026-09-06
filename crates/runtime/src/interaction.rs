//! Fast interaction planes are Rust-rendered cached pixels, not a replacement
//! compositor. The client warps the subject while dragging; release requests an
//! authoritative full composite. Adjustment layers above the subject settle then.
use crate::{DecodedImages, EditorSession};
use bonaparte_effects::EffectRegistry;
use bonaparte_engine::preview::{prepare_scene, render_scene_cpu, Resolution, Scene};
use bonaparte_engine::Affine2D;
use bonaparte_model::*;
use serde::Deserialize;
use serde_json::json;
use std::{collections::BTreeSet, sync::Arc};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InteractionRequest {
    pub comp_id: CompId,
    pub layer_id: LayerId,
    pub time: Time,
    #[serde(default)]
    pub bypass_effects: bool,
}
pub struct InteractionInput {
    project: Arc<Project>,
    images: DecodedImages,
    registry: EffectRegistry,
    cache: Arc<crate::PreviewRenderer>,
    revision: u64,
    request: InteractionRequest,
}
impl EditorSession {
    pub fn interaction_input(
        &self,
        request: InteractionRequest,
    ) -> Result<InteractionInput, String> {
        let layer = self
            .project
            .layer(request.comp_id, request.layer_id)
            .ok_or("Interaction layer not found")?;
        if layer.locked || matches!(layer.kind, LayerKind::Adjustment { .. }) {
            return Err("This layer does not support a transform proxy".into());
        }
        Ok(InteractionInput {
            project: self.preview_snapshot.clone(),
            images: self.images.clone(),
            registry: self.registry.clone(),
            cache: self.preview.clone(),
            revision: self.revision,
            request,
        })
    }
}
impl InteractionInput {
    pub fn packet(&self) -> Result<Vec<u8>, String> {
        let r = &self.request;
        let comp = self
            .project
            .comp(r.comp_id)
            .ok_or("Composition not found")?;
        let target = comp.layers.get(&r.layer_id).ok_or("Layer not found")?;
        let matrix =
            Affine2D::from_layer(comp, target, r.time).ok_or("Layer transform unavailable")?;
        let divisor = if comp.width.max(comp.height) > 1280 {
            4
        } else if comp.width.max(comp.height) > 640 {
            2
        } else {
            1
        };
        let mut scene = prepare_scene(
            &self.project,
            r.comp_id,
            r.time,
            &self.images,
            &self.registry,
            Resolution::new(divisor)?,
            &self.cache.sources,
        )
        .map_err(|e| e.to_string())?;
        if r.bypass_effects {
            fn clear(s: &mut Scene) {
                for l in &mut s.layers {
                    l.effects.clear();
                    if let bonaparte_engine::preview::Source::Composition(child) = &mut l.source {
                        clear(child);
                    }
                }
            }
            clear(&mut scene);
        }
        let mut affected = BTreeSet::new();
        affected.insert(r.layer_id);
        loop {
            let before = affected.len();
            for layer in comp.layers.values() {
                if layer.parent.is_some_and(|id| affected.contains(&id)) {
                    affected.insert(layer.id);
                }
            }
            if affected.len() == before {
                break;
            }
        }
        let positions: Vec<_> = scene
            .layers
            .iter()
            .enumerate()
            .filter(|(_, l)| affected.contains(&l.id))
            .map(|(i, _)| i)
            .collect();
        let first = *positions
            .first()
            .ok_or("No visible layer pixels at this time")?;
        let last = *positions.last().unwrap();
        if scene.layers[first..=last]
            .iter()
            .any(|l| !affected.contains(&l.id))
        {
            return Err("Interleaved parent/child stacks use the draft renderer rather than a detached proxy".into());
        }
        let mut base = scene.clone();
        base.layers = scene.layers[..first].to_vec();
        let mut subject = scene.clone();
        subject.background = [0.0; 4];
        subject.layers = scene.layers[first..=last].to_vec();
        let mut foreground = scene.clone();
        foreground.background = [0.0; 4];
        let mut adjusted = false;
        foreground.layers = scene.layers[last + 1..]
            .iter()
            .filter_map(|l| {
                if matches!(l.source, bonaparte_engine::preview::Source::Adjustment) {
                    adjusted = true;
                    None
                } else {
                    Some(l.clone())
                }
            })
            .collect();
        let non_normal = scene.layers.iter().any(|l| l.blend != BlendMode::Normal);
        let metadata=serde_json::to_vec(&json!({"compId":r.comp_id,"layerId":r.layer_id,"revision":self.revision,"time":r.time,"width":scene.width,"height":scene.height,"logicalWidth":comp.width,"logicalHeight":comp.height,"matrix":[matrix.a,matrix.b,matrix.c,matrix.d,matrix.tx,matrix.ty],"divisor":divisor,"approximate":adjusted||non_normal||!target.effects.is_empty(),"blendMode":target.blend_mode})).map_err(|e|e.to_string())?;
        let length = scene.width as usize * scene.height as usize * 4;
        if length * 3 > 24 * 1024 * 1024 {
            return Err("Interaction planes exceed 24 MiB".into());
        }
        let mut packet = Vec::with_capacity(8 + metadata.len() + length * 3);
        packet.extend_from_slice(b"BIP1");
        packet.extend_from_slice(&(metadata.len() as u32).to_le_bytes());
        packet.extend(metadata);
        for plane in [&base, &subject, &foreground] {
            packet.extend(
                render_scene_cpu(plane, &self.registry)
                    .map_err(|e| e.to_string())?
                    .rgba,
            );
        }
        Ok(packet)
    }
}
