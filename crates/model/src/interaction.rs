//! Lightweight transient transforms. These never enter history/project data.
use crate::{CompId, LayerId, Project, PropValue, Property};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransformOverride {
    pub comp_id: CompId,
    pub layer_id: LayerId,
    pub property: Property,
    pub value: PropValue,
}
impl TransformOverride {
    pub fn validate(&self, project: &Project) -> Result<(), String> {
        let layer = project
            .layer(self.comp_id, self.layer_id)
            .ok_or("Interaction layer not found")?;
        if layer.locked {
            return Err("Interaction layer is locked".into());
        }
        if self.property.value_kind() != self.value.kind() {
            return Err("Interaction property type mismatch".into());
        }
        let valid = |v: f32| v.is_finite() && v.abs() <= 1_000_000.0;
        if !match self.value {
            PropValue::Scalar(v) => valid(v),
            PropValue::Vec2(v) => v.into_iter().all(valid),
        } {
            return Err("Interaction transform is not finite or exceeds limits".into());
        }
        Ok(())
    }
}
