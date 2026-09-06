//! # bonaparte-effects
//!
//! The built-in (first-party) effect packs — ordinary plugins that ship in
//! the box (microkernel architecture). Every pack is a folder:
//! manifest.toml + WGSL + README.
//!
//! Owns: pack loading, plugin registry, and deterministic CPU reference evaluators.
//! Must never depend on: engine internals or UI.

pub mod cpu_reference;
pub mod grading;
pub mod registry;

pub use cpu_reference::{
    evaluate_blur, evaluate_chromatic_aberration, evaluate_circle, evaluate_color_adjust,
    evaluate_directional_blur, evaluate_drop_shadow, evaluate_effect, evaluate_glow,
    evaluate_invert, evaluate_tint, evaluate_transform, evaluate_vignette, CpuEvalError, CpuFrame,
    ParamValue,
};
pub use registry::{
    builtin_manifests, builtin_packs, EffectRegistry, RegisteredEffect, RegistryError,
    BLUR_MANIFEST, BLUR_SHADER, CHROMATIC_ABERRATION_MANIFEST, CHROMATIC_ABERRATION_SHADER,
    CIRCLE_MANIFEST, CIRCLE_SHADER, COLOR_ADJUST_MANIFEST, COLOR_ADJUST_SHADER,
    DIRECTIONAL_BLUR_MANIFEST, DIRECTIONAL_BLUR_SHADER, DROP_SHADOW_MANIFEST, DROP_SHADOW_SHADER,
    GLOW_MANIFEST, GLOW_SHADER, INVERT_MANIFEST, INVERT_SHADER, TINT_MANIFEST, TINT_SHADER,
    TRANSFORM_MANIFEST, TRANSFORM_SHADER, VIGNETTE_MANIFEST, VIGNETTE_SHADER,
};

#[cfg(test)]
mod tests {
    use super::*;

    /// The microkernel proof, executed: every built-in effect must be a
    /// valid public-API plugin. If this test fails, the API drifted.
    #[test]
    fn every_builtin_is_a_valid_public_api_plugin() {
        let manifests = builtin_manifests();
        assert_eq!(
            manifests.len(),
            bonaparte_effects_count(),
            "must have all 11 core built-in packs"
        );
        for m in &manifests {
            m.validate().expect("builtin packs must validate");
            assert!(
                m.params.iter().all(|p| !p.doc.is_empty()),
                "param doc required (RULES §7.2)"
            );
        }
    }

    #[test]
    fn effect_registry_contains_all_11_builtins() {
        let registry = EffectRegistry::new();
        assert_eq!(registry.len(), bonaparte_effects_count());
        let expected_ids = [
            "builtin.glow",
            "builtin.blur",
            "builtin.drop_shadow",
            "builtin.color_adjust",
            "builtin.transform",
            "builtin.vignette",
            "builtin.chromatic_aberration",
            "builtin.invert",
            "builtin.tint",
            "builtin.directional_blur",
            "builtin.circle",
        ];
        for id in expected_ids {
            assert!(registry.contains(id), "registry must contain {id}");
            assert!(registry.get_manifest(id).is_some());
            assert!(registry.get_shader(id).is_some());
        }
    }
}

#[cfg(test)]
fn bonaparte_effects_count() -> usize {
    registry::builtin_packs().len()
}
