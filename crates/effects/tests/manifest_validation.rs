use bonaparte_effects::registry::{builtin_manifests, EffectRegistry};
use bonaparte_model::manifest::{EffectManifest, ParamKind};
use std::path::Path;

#[test]
fn test_all_10_builtin_manifests_are_valid() {
    let manifests = builtin_manifests();
    assert_eq!(manifests.len(), 11, "Must have exactly 11 first-party manifests");

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

    for (manifest, expected_id) in manifests.iter().zip(expected_ids.iter()) {
        assert_eq!(&manifest.id, expected_id);
        assert!(!manifest.name.is_empty());
        assert!(!manifest.shader.is_empty());
        assert!(manifest.shader.ends_with(".wgsl"));
        manifest.validate().expect("manifest must be valid");

        for p in &manifest.params {
            assert!(!p.id.is_empty(), "param id required for {}", manifest.id);
            assert!(!p.name.is_empty(), "param name required for {}", p.id);
            assert!(
                !p.doc.is_empty(),
                "param doc required for {} in {}",
                p.id,
                manifest.id
            );

            match &p.kind {
                ParamKind::Slider { min, max, default } => {
                    assert!(
                        min <= max,
                        "param {}: min ({}) must be <= max ({})",
                        p.id,
                        min,
                        max
                    );
                    assert!(
                        *default >= *min && *default <= *max,
                        "param {}: default ({}) must be within [{}, {})",
                        p.id,
                        default,
                        min,
                        max
                    );
                }
                ParamKind::Color { default } => {
                    assert_eq!(default.len(), 4);
                    for c in default {
                        assert!(*c >= 0.0 && *c <= 1.0);
                    }
                }
                ParamKind::Point { default } => {
                    assert_eq!(default.len(), 2);
                }
                ParamKind::Checkbox { .. } => {}
                ParamKind::Dropdown { options, default } => {
                    assert!(!options.is_empty());
                    assert!(*default < options.len());
                }
            }
        }
    }
}

#[test]
fn test_dynamic_pack_directory_scanner() {
    let packs_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("packs");
    assert!(packs_dir.is_dir(), "packs dir must exist at {:?}", packs_dir);

    let mut registry = EffectRegistry::empty();
    let loaded_ids = registry
        .scan_directory(&packs_dir)
        .expect("scan_directory must succeed");

    assert_eq!(loaded_ids.len(), 11);
    assert_eq!(registry.len(), 11);

    for id in loaded_ids {
        let effect = registry.get(&id).expect("effect must be present in registry");
        assert!(!effect.shader_source.is_empty());
        assert!(!effect.is_builtin);
        assert!(effect.directory.is_some());
    }
}

#[test]
fn test_scanner_rejects_invalid_manifest() {
    let bad_toml = "api_version = '9.9'\nid = 'broken'\nname = 'Broken'\ncost = 'light'\nshader = 'broken.wgsl'\n";

    let res = EffectManifest::parse(bad_toml);
    assert!(res.is_err(), "unsupported API version must be rejected");
}
