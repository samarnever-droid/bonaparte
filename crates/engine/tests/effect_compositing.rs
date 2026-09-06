use bonaparte_effects::{CpuEvalError, CpuFrame, EffectRegistry, ParamValue};
use bonaparte_engine::reference::render_comp_with_registry;
use bonaparte_engine::{render_comp, render_tile, Frame, NoMedia, Tile};
use bonaparte_model::*;
use std::collections::HashMap;
fn scene(w: u32, h: u32) -> (Project, CompId) {
    let mut p = Project::new("Effects");
    let c = p.create_comp("Main", w, h, FrameRate::FPS_30, Time(120000));
    (p, c)
}
#[test]
fn alpha_is_not_gamma_encoded() {
    let frame = Frame::filled(1, 1, [0.25, 0.25, 0.25, 0.5]);
    assert_eq!(frame.rgba[3], 128);
    assert!((frame.pixel(0, 0)[3] - 0.5).abs() < 0.005);
}
#[test]
fn ordered_layer_effects_render_and_bypass_is_a_real_noop() {
    let (mut p, c) = scene(8, 8);
    let mut l = Layer::new_solid("Red", [1.0, 0.0, 0.0, 1.0], Time::ZERO, Time(120000));
    l.effects
        .push(EffectInstance::new("invert", "builtin.invert"));
    let id = p.insert_layer(c, l);
    let inverted = render_comp(&p, c, Time::ZERO, &NoMedia).unwrap();
    assert_eq!(&inverted.rgba[..4], &[0, 255, 255, 255]);
    let mut tint = EffectInstance::new("tint", "builtin.tint");
    tint.params.insert(
        "white_color".into(),
        EffectValue::Color([1.0, 0.0, 0.0, 1.0]),
    );
    p.layer_mut(c, id).unwrap().effects.push(tint);
    let a = render_comp(&p, c, Time::ZERO, &NoMedia).unwrap();
    p.layer_mut(c, id).unwrap().effects.reverse();
    let b = render_comp(&p, c, Time::ZERO, &NoMedia).unwrap();
    assert_ne!(a.rgba, b.rgba);
    for e in &mut p.layer_mut(c, id).unwrap().effects {
        e.enabled = false;
    }
    assert_eq!(
        &render_comp(&p, c, Time::ZERO, &NoMedia).unwrap().rgba[..4],
        &[255, 0, 0, 255]
    );
}
#[test]
fn adjustment_affects_layers_below_but_not_above_it() {
    let (mut p, c) = scene(8, 8);
    p.insert_layer(
        c,
        Layer::new_solid("Red", [1.0, 0.0, 0.0, 1.0], Time::ZERO, Time(120000)),
    );
    let mut adjustment = Layer::new(
        "Invert below",
        LayerKind::Adjustment {},
        Time::ZERO,
        Time(120000),
    );
    adjustment
        .effects
        .push(EffectInstance::new("invert", "builtin.invert"));
    p.insert_layer(c, adjustment);
    let mut top = Layer::new_rect(
        "Green foreground",
        [0.0, 1.0, 0.0, 1.0],
        Time::ZERO,
        Time(120000),
    );
    if let LayerKind::Shape { style, .. } = &mut top.kind {
        style.size = Some([2.0, 2.0]);
    }
    p.insert_layer(c, top);
    let f = render_comp(&p, c, Time::ZERO, &NoMedia).unwrap();
    assert_eq!(&f.rgba[..4], &[0, 255, 255, 255]);
    assert_eq!(f.pixel(4, 4), [0.0, 1.0, 0.0, 1.0]);
}
#[test]
fn effect_aware_tiles_match_full_frame_at_kernel_boundaries() {
    let (mut p, c) = scene(300, 40);
    let mut l = Layer::new_rect(
        "Blurred shape",
        [0.8, 0.2, 0.1, 1.0],
        Time::ZERO,
        Time(120000),
    );
    if let LayerKind::Shape { style, .. } = &mut l.kind {
        style.size = Some([15.0, 25.0]);
    }
    l.transform.position = [106.0, 0.0];
    let mut blur = EffectInstance::new("blur", "builtin.blur");
    blur.params.insert("radius".into(), EffectValue::Float(5.0));
    l.effects.push(blur);
    p.insert_layer(c, l);
    let full = render_comp(&p, c, Time::ZERO, &NoMedia).unwrap();
    let tile = render_tile(&p, c, Tile { x: 1, y: 0 }, Time::ZERO, &NoMedia).unwrap();
    for y in 0..tile.height {
        let src = ((y * full.width + 256) * 4) as usize;
        let dst = (y * tile.width * 4) as usize;
        assert_eq!(
            &full.rgba[src..src + tile.width as usize * 4],
            &tile.rgba[dst..dst + tile.width as usize * 4]
        );
    }
}
#[test]
fn renderer_accepts_third_party_registry_without_engine_id_hooks() {
    let (mut p, c) = scene(2, 2);
    let mut l = Layer::new_solid("Source", [1.0; 4], Time::ZERO, Time(120000));
    l.effects
        .push(EffectInstance::new("instance", "test.paint"));
    p.insert_layer(c, l);
    let manifest=EffectManifest::parse("api_version='1.0'\nid='test.paint'\nname='Paint'\ncost='light'\nshader='paint.wgsl'\ninputs=['input']").unwrap();
    fn paint(
        _: &str,
        f: &CpuFrame,
        _: &HashMap<String, ParamValue>,
    ) -> Result<CpuFrame, CpuEvalError> {
        Ok(CpuFrame::filled(f.width, f.height, [20, 80, 140, 255]))
    }
    let mut registry = EffectRegistry::empty();
    registry
        .register_cpu(manifest, String::new(), paint)
        .unwrap();
    let f = render_comp_with_registry(&p, c, Time::ZERO, &NoMedia, &registry).unwrap();
    assert_eq!(&f.rgba[..4], &[20, 80, 140, 255]);
}
#[test]
fn text_is_antialiased_and_content_changes_change_rendered_pixels() {
    let (mut p, c) = scene(140, 50);
    let id = p.insert_layer(
        c,
        Layer::new_text("Type", "Motion", 24.0, Time::ZERO, Time(120000)),
    );
    let a = render_comp(&p, c, Time::ZERO, &NoMedia).unwrap();
    if let LayerKind::Text { text, .. } = &mut p.layer_mut(c, id).unwrap().kind {
        *text = "Design".into();
    }
    let b = render_comp(&p, c, Time::ZERO, &NoMedia).unwrap();
    assert_ne!(a.rgba, b.rgba);
    assert!(a.rgba.chunks_exact(4).any(|p| p[0] > 0 && p[0] < 255));
}

#[test]
fn bilinear_resampling_is_premultiplied_linear_light() {
    let f = Frame {
        width: 2,
        height: 1,
        rgba: vec![255, 0, 0, 0, 0, 255, 0, 255],
    };
    let p = f.sample_bilinear(0.5, 0.5);
    assert_eq!(p, [0.0, 1.0, 0.0, 0.5]);
    let f = Frame {
        width: 2,
        height: 1,
        rgba: vec![0, 0, 0, 255, 255, 255, 255, 255],
    };
    assert_eq!(f.sample_bilinear(0.5, 0.5), [0.5, 0.5, 0.5, 1.0]);
}
#[test]
fn oversized_typography_returns_an_actionable_error() {
    let (mut p, c) = scene(8, 8);
    p.insert_layer(
        c,
        Layer::new_text("Huge", &"W".repeat(400), 2048.0, Time::ZERO, Time(120000)),
    );
    let error = render_comp(&p, c, Time::ZERO, &NoMedia)
        .unwrap_err()
        .to_string();
    assert!(error.contains("Text layout exceeds 16 megapixels"));
}
#[test]
fn adjustment_opacity_interpolates_premultiplied_backdrop_alpha() {
    let (mut p, c) = scene(2, 2);
    p.comp_mut(c).unwrap().background = [1.0, 0.0, 0.0, 0.5];
    let mut layer = Layer::new(
        "Transparent gradient",
        LayerKind::Adjustment {},
        Time::ZERO,
        Time(120000),
    );
    layer.transform.opacity = 0.5;
    let mut effect = EffectInstance::new("gradient", "builtin.gradient");
    effect.params.insert(
        "start_color".into(),
        EffectValue::Color([0.0, 0.0, 1.0, 0.0]),
    );
    effect
        .params
        .insert("end_color".into(), EffectValue::Color([0.0, 0.0, 1.0, 0.0]));
    layer.effects.push(effect);
    p.insert_layer(c, layer);
    let f = render_comp(&p, c, Time::ZERO, &NoMedia).unwrap();
    assert_eq!(&f.rgba[..4], &[255, 0, 0, 64]);
}
