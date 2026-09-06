//! Serializable effect instances. The document knows values, not implementations.
//! Manifests and evaluators remain owned by the public effects registry.
use crate::{PropValue, Time, Track};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EffectValue {
    Float(f32),
    Bool(bool),
    Color([f32; 4]),
    Point([f32; 2]),
    Index(usize),
}

impl EffectValue {
    pub fn is_finite(&self) -> bool {
        match self {
            Self::Float(v) => v.is_finite(),
            Self::Color(v) => v.iter().all(|x| x.is_finite()),
            Self::Point(v) => v.iter().all(|x| x.is_finite()),
            _ => true,
        }
    }
}

fn enabled() -> bool {
    true
}

/// `id` identifies the instance, so a layer can contain the same plugin twice.
/// Numeric/point tracks use composition ticks, just like transform tracks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectInstance {
    pub id: String,
    pub effect_id: String,
    #[serde(default = "enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub params: BTreeMap<String, EffectValue>,
    #[serde(default)]
    pub tracks: BTreeMap<String, Track>,
}

impl EffectInstance {
    pub fn new(id: impl Into<String>, effect_id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            effect_id: effect_id.into(),
            enabled: true,
            params: BTreeMap::new(),
            tracks: BTreeMap::new(),
        }
    }

    pub fn evaluated_params(&self, time: Time) -> BTreeMap<String, EffectValue> {
        let mut params = self.params.clone();
        for (id, track) in &self.tracks {
            if let Some(value) = track.evaluate(time) {
                params.insert(
                    id.clone(),
                    match value {
                        PropValue::Scalar(v) => EffectValue::Float(v),
                        PropValue::Vec2(v) => EffectValue::Point(v),
                    },
                );
            }
        }
        params
    }
}

pub fn validate_effect_stack(effects: &[EffectInstance]) -> Result<(), String> {
    if effects.len() > 32 {
        return Err("A layer supports at most 32 effects".into());
    }
    let mut ids = BTreeSet::new();
    for effect in effects {
        if effect.id.is_empty() || effect.effect_id.is_empty() || !ids.insert(&effect.id) {
            return Err("Effect instance IDs must be nonempty and unique within a layer".into());
        }
        if effect.params.len() > 128 || effect.tracks.len() > 128 {
            return Err("Too many effect parameters".into());
        }
        if effect.params.values().any(|v| !v.is_finite()) {
            return Err("Effect parameters must be finite".into());
        }
        for track in effect.tracks.values() {
            crate::validation::validate_track(track, None)?;
        }
    }
    Ok(())
}
