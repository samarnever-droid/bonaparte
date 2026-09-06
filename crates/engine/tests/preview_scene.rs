use bonaparte_effects::EffectRegistry;
use bonaparte_engine::preview::{prepare_scene, render_scene_cpu, Resolution, SourceCache};
use bonaparte_engine::reference::render_comp_legacy as render_comp;
use bonaparte_engine::NoMedia;
use bonaparte_model::*;
fn fixture() -> (Project, CompId) {
    let mut p = Project::new("Preview");
    let c = p.create_comp("Main", 96, 64, FrameRate::FPS_30, Time(240000));
    p.comp_mut(c).unwrap().background = [0.03, 0.06, 0.1, 0.7];
    (p, c)
}
#[test]
fn prepared_full_scene_matches_original_reference_byte_for_byte() {
    let mut p: Project =
        serde_json::from_str(include_str!("../../../examples/orbit.bonaparte.json")).unwrap();
    let registry = EffectRegistry::new();
    let cache = SourceCache::default();
    for t in [0, 144000, 360000] {
        let original = render_comp(&p, CompId(1), Time(t), &NoMedia).unwrap();
        let scene = prepare_scene(
            &p,
            CompId(1),
            Time(t),
            &NoMedia,
            &registry,
            Resolution::FULL,
            &cache,
        )
        .unwrap();
        assert_eq!(
            render_scene_cpu(&scene, &registry).unwrap(),
            original,
            "time {t}"
        );
    }
    // A content edit invalidates the source independently of transforms/effects.
    if let LayerKind::Text { text, .. } = &mut p.layer_mut(CompId(1), LayerId(13)).unwrap().kind {
        *text = "Changed type".into();
    }
    let scene = prepare_scene(
        &p,
        CompId(1),
        Time(144000),
        &NoMedia,
        &registry,
        Resolution::FULL,
        &cache,
    )
    .unwrap();
    assert_eq!(
        render_scene_cpu(&scene, &registry).unwrap(),
        render_comp(&p, CompId(1), Time(144000), &NoMedia).unwrap()
    );
    assert!(cache.stats().hits > 0);
}
#[test]
fn resolution_changes_the_sampling_grid_not_document_coordinates() {
    let (mut p, c) = fixture();
    let mut l = Layer::new_rect(
        "Small rectangle",
        [1.0, 0.0, 0.0, 1.0],
        Time::ZERO,
        Time(240000),
    );
    if let LayerKind::Shape { style, .. } = &mut l.kind {
        style.size = Some([24.0, 16.0]);
    }
    l.transform.position = [24.0, 8.0];
    p.insert_layer(c, l);
    let before = serde_json::to_value(&p).unwrap();
    let registry = EffectRegistry::new();
    let cache = SourceCache::default();
    for d in [1, 2, 4] {
        let scene = prepare_scene(
            &p,
            c,
            Time::ZERO,
            &NoMedia,
            &registry,
            Resolution::new(d).unwrap(),
            &cache,
        )
        .unwrap();
        let f = render_scene_cpu(&scene, &registry).unwrap();
        assert_eq!((f.width, f.height), (96 / d, 64 / d));
        assert_eq!(f.pixel(72 / d, 40 / d), [1.0, 0.0, 0.0, 1.0]);
    }
    assert_eq!(serde_json::to_value(&p).unwrap(), before);
    assert!(Resolution::new(0).is_err());
    assert!(Resolution::new(3).is_err());
}
#[test]
fn pixel_effect_parameters_scale_but_normalized_parameters_do_not() {
    let registry = EffectRegistry::new();
    let mut effect = EffectInstance::new("blur", "builtin.blur");
    effect
        .params
        .insert("radius".into(), EffectValue::Float(24.0));
    assert_eq!(
        registry
            .instance_parameters(&effect, Time::ZERO, 0.25)
            .unwrap()["radius"],
        EffectValue::Float(6.0)
    );
    let effect = EffectInstance::new("vignette", "builtin.vignette");
    assert_eq!(
        registry
            .instance_parameters(&effect, Time::ZERO, 0.25)
            .unwrap()["radius"],
        EffectValue::Float(0.75)
    );
}
#[test]
fn cache_retention_is_bounded_and_clear_releases_entries() {
    let (mut p, c) = fixture();
    let id = p.insert_layer(
        c,
        Layer::new_text("Text", "A", 16.0, Time::ZERO, Time(240000)),
    );
    let registry = EffectRegistry::new();
    let cache = SourceCache::new(1024);
    for ch in ['A', 'B', 'C', 'D'] {
        if let LayerKind::Text { text, .. } = &mut p.layer_mut(c, id).unwrap().kind {
            *text = ch.to_string();
        }
        let _ = prepare_scene(
            &p,
            c,
            Time::ZERO,
            &NoMedia,
            &registry,
            Resolution::FULL,
            &cache,
        )
        .unwrap();
        assert!(cache.stats().bytes <= 1024);
    }
    cache.clear();
    assert_eq!(cache.stats().bytes, 0);
    assert_eq!(cache.stats().entries, 0);
}
#[test]
fn odd_dimensions_round_up_and_never_become_zero() {
    let r = Resolution::new(4).unwrap();
    assert_eq!(r.dimensions(33, 17), [9, 5]);
    assert_eq!(r.dimensions(1, 1), [1, 1]);
}
