use bonaparte_effects::EffectRegistry;
use bonaparte_engine::preview::{prepare_scene, Resolution, Source, SourceCache};
use bonaparte_engine::typography::measure_text;
use bonaparte_engine::{render_comp, NoMedia};
use bonaparte_model::*;
fn project() -> (Project, CompId) {
    let mut p = Project::new("Raster quality");
    let c = p.create_comp("Main", 480, 320, FrameRate::FPS_30, Time(120000));
    p.comp_mut(c).unwrap().background = [0.0; 4];
    (p, c)
}
#[test]
fn scaled_type_is_rasterized_at_output_density_not_the_old_glyph_mask() {
    let (mut p, c) = project();
    let mut layer = Layer::new_text("Type", "Sharp", 14.0, Time::ZERO, Time(120000));
    if let LayerKind::Text { style, .. } = &mut layer.kind {
        style.bold = false;
    }
    layer.transform.scale = [500.0, 500.0];
    let id = p.insert_layer(c, layer);
    let scene = prepare_scene(
        &p,
        c,
        Time::ZERO,
        &NoMedia,
        &EffectRegistry::new(),
        Resolution::FULL,
        &SourceCache::default(),
    )
    .unwrap();
    let l = scene.layers.iter().find(|l| l.id == id).unwrap();
    let Source::Raster { pixels, .. } = &l.source else {
        panic!("expected type raster")
    };
    let bounds = measure_text("Sharp", 14.0, false, 0.0);
    assert_eq!(l.size, bounds);
    assert!(pixels.width >= bounds[0] as u32 * 5);
    assert!(pixels.height >= bounds[1] as u32 * 5);
    let sharp = render_comp(&p, c, Time::ZERO, &NoMedia).unwrap();
    let old = bonaparte_engine::reference::render_comp_legacy(&p, c, Time::ZERO, &NoMedia).unwrap();
    assert_ne!(sharp.rgba, old.rgba);
    assert!(sharp.rgba.chunks_exact(4).any(|p| p[3] > 0 && p[3] < 255));
}
#[test]
fn enlarged_precomps_and_generators_get_larger_render_grids() {
    let (mut p, c) = project();
    let child = p.create_comp("Child", 60, 40, FrameRate::FPS_30, Time(120000));
    let mut shape = Layer::new_circle("Ellipse", [1.0; 4], Time::ZERO, Time(120000));
    if let LayerKind::Shape { style, .. } = &mut shape.kind {
        style.size = Some([30.0, 20.0]);
        style.stroke_width = 4.0;
    }
    p.insert_layer(child, shape);
    let mut pre = Layer::new(
        "Nested",
        LayerKind::PreComp { comp: child },
        Time::ZERO,
        Time(120000),
    );
    pre.transform.scale = [400.0, 400.0];
    p.insert_layer(c, pre);
    let scene = prepare_scene(
        &p,
        c,
        Time::ZERO,
        &NoMedia,
        &EffectRegistry::new(),
        Resolution::FULL,
        &SourceCache::default(),
    )
    .unwrap();
    let Source::Composition(child) = &scene.layers[0].source else {
        panic!()
    };
    assert_eq!((child.width, child.height), (240, 160));
    assert_eq!(child.logical_size, [60.0, 40.0]);
    let Source::Raster { pixels, .. } = &child.layers[0].source else {
        panic!()
    };
    assert_eq!((pixels.width, pixels.height), (120, 80));
}
#[test]
fn high_quality_preview_and_export_use_the_same_sampling_path() {
    let (mut p, c) = project();
    let mut layer = Layer::new_text("Tilted", "VECTOR", 18.0, Time::ZERO, Time(120000));
    layer.transform.rotation = 17.0;
    layer.transform.scale = [320.0, 250.0];
    p.insert_layer(c, layer);
    let registry = EffectRegistry::new();
    let scene = prepare_scene(
        &p,
        c,
        Time::ZERO,
        &NoMedia,
        &registry,
        Resolution::FULL,
        &SourceCache::default(),
    )
    .unwrap();
    let preview = bonaparte_engine::preview::render_scene_cpu(&scene, &registry).unwrap();
    assert_eq!(preview, render_comp(&p, c, Time::ZERO, &NoMedia).unwrap());
}
