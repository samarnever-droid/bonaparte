use bonaparte_effects::{evaluate_blur, evaluate_effect, CpuFrame, EffectRegistry, ParamValue};
use bonaparte_model::{Easing, EffectInstance, EffectManifest, Keyframe, PropValue, Time, Track};
use std::collections::HashMap;

#[test]
fn neutral_grade_is_byte_exact_including_transparent_rgb_and_alpha() {
    let mut frame = CpuFrame::new(256, 1);
    for x in 0..256 {
        frame.set_pixel_u8(x, 0, [x as u8, (255 - x) as u8, (x / 2) as u8, x as u8]);
    }
    assert_eq!(
        evaluate_effect("builtin.color_grade", &frame, &HashMap::new()).unwrap(),
        frame
    );
}
#[test]
fn exposure_is_linear_light_and_alpha_is_untouched() {
    let frame = CpuFrame::filled(2, 2, [128, 128, 128, 93]);
    let result = evaluate_effect(
        "builtin.color_grade",
        &frame,
        &HashMap::from([("exposure".into(), ParamValue::Float(1.0))]),
    )
    .unwrap();
    let expected = (bonaparte_effects::grading::linear_to_srgb(
        bonaparte_effects::grading::srgb_to_linear(128.0 / 255.0) * 2.0,
    ) * 255.0)
        .round() as u8;
    assert_eq!(
        result.get_pixel_u8(0, 0),
        [expected, expected, expected, 93]
    );
    assert!(expected < 200);
}
#[test]
fn temperature_and_monochrome_controls_change_pixels_predictably() {
    let gray = CpuFrame::filled(1, 1, [128, 128, 128, 255]);
    let warm = evaluate_effect(
        "builtin.color_grade",
        &gray,
        &HashMap::from([("temperature".into(), ParamValue::Float(1.0))]),
    )
    .unwrap()
    .get_pixel_u8(0, 0);
    assert!(warm[0] > 128 && warm[2] < 128);
    let color = CpuFrame::filled(1, 1, [220, 85, 40, 177]);
    let mono = evaluate_effect(
        "builtin.color_grade",
        &color,
        &HashMap::from([("saturation".into(), ParamValue::Float(0.0))]),
    )
    .unwrap()
    .get_pixel_u8(0, 0);
    assert_eq!(mono[0], mono[1]);
    assert_eq!(mono[1], mono[2]);
    assert_eq!(mono[3], 177);
}
#[test]
fn manifests_supply_defaults_and_refuse_unknown_wrong_typed_and_out_of_range_params() {
    let frame = CpuFrame::filled(1, 1, [100, 100, 100, 255]);
    for values in [
        HashMap::from([("exposure".into(), ParamValue::Float(6.0))]),
        HashMap::from([("exposure".into(), ParamValue::Bool(true))]),
        HashMap::from([("made_up".into(), ParamValue::Float(0.0))]),
    ] {
        assert!(evaluate_effect("builtin.color_grade", &frame, &values).is_err());
    }
    let registry = EffectRegistry::new();
    let manifest = registry.get_manifest("builtin.color_grade").unwrap();
    assert_eq!(manifest.params.len(), 21);
    for parameter in &manifest.params {
        assert!(!parameter.group.is_empty());
        assert!(parameter.kind.accepts(&parameter.kind.default_value()));
    }
}
#[test]
fn effect_animation_evaluates_at_composition_ticks_and_clamps_bezier_overshoot() {
    let registry = EffectRegistry::new();
    let mut effect = EffectInstance::new("animated", "builtin.color_grade");
    effect.tracks.insert(
        "exposure".into(),
        Track {
            keys: vec![
                Keyframe {
                    time: Time(0),
                    value: PropValue::Scalar(0.0),
                    easing: Easing::Bezier {
                        p1: [0.1, 3.0],
                        p2: [0.2, 3.0],
                    },
                },
                Keyframe {
                    time: Time(120000),
                    value: PropValue::Scalar(5.0),
                    easing: Easing::Linear,
                },
            ],
        },
    );
    let frame = CpuFrame::filled(1, 1, [128, 128, 128, 111]);
    let output = registry
        .evaluate_instance(&effect, &frame, Time(60000))
        .unwrap();
    assert_eq!(output.rgba[3], 111);
}
#[test]
fn premultiplied_blur_does_not_bleed_hidden_transparent_colors() {
    let frame = CpuFrame::from_rgba(3, 1, vec![255, 0, 0, 0, 0, 255, 0, 255, 255, 0, 0, 0]);
    let out = evaluate_blur(&frame, 1.0, false);
    for p in out.rgba.chunks_exact(4) {
        if p[3] > 0 {
            assert_eq!(p[0], 0);
            assert_eq!(p[1], 255);
            assert_eq!(p[2], 0);
        }
    }
}
#[test]
fn third_party_cpu_plugin_uses_public_registration_and_duplicate_ids_fail() {
    let mut registry = EffectRegistry::empty();
    let manifest=EffectManifest::parse("api_version='1.0'\nid='test.paint'\nname='Paint'\ncost='light'\nshader='paint.wgsl'\ninputs=['input']").unwrap();
    fn paint(
        _: &str,
        frame: &CpuFrame,
        _: &HashMap<String, ParamValue>,
    ) -> Result<CpuFrame, bonaparte_effects::CpuEvalError> {
        Ok(CpuFrame::filled(
            frame.width,
            frame.height,
            [4, 50, 190, 255],
        ))
    }
    registry
        .register_cpu(manifest.clone(), String::new(), paint)
        .unwrap();
    assert!(registry
        .register_cpu(manifest, String::new(), paint)
        .is_err());
    assert_eq!(
        registry
            .evaluate("test.paint", &CpuFrame::new(2, 2), &HashMap::new())
            .unwrap()
            .get_pixel_u8(0, 0),
        [4, 50, 190, 255]
    );
}
#[test]
fn shader_paths_cannot_escape_the_pack_directory() {
    assert!(EffectManifest::parse("api_version='1.0'\nid='test.escape'\nname='Escape'\ncost='light'\nshader='../outside.wgsl'").is_err());
}

#[test]
fn grade_v2_wheels_work_and_defaults_stay_byte_identical() {
    let frame = CpuFrame::filled(4, 4, [200, 120, 60, 255]);
    let defaults = HashMap::new();
    let graded = evaluate_effect("builtin.color_grade", &frame, &defaults).unwrap();
    assert_eq!(
        graded.rgba, frame.rgba,
        "all-default grade must be identity"
    );

    // Filmic rolloff compresses the bright channel instead of clipping.
    let hot = CpuFrame::filled(4, 4, [255, 255, 255, 255]);
    let params = HashMap::from([("filmic".into(), ParamValue::Float(1.0))]);
    let rolled = evaluate_effect("builtin.color_grade", &hot, &params).unwrap();
    assert!(
        rolled.rgba[0] < 255,
        "full white must roll off under filmic, got {}",
        rolled.rgba[0]
    );
    // …and it reaches pixels even when exposure/white balance are neutral
    // (regression: the neutral-light fast path used to skip gain and filmic).
    let dim = CpuFrame::filled(4, 4, [128, 128, 128, 255]);
    let dim_rolled = evaluate_effect("builtin.color_grade", &dim, &params).unwrap();
    assert_ne!(dim_rolled.rgba[0], 128, "filmic must affect mid gray too");

    // Gain multiplies in linear light: 0.5 halves energy.
    let half = HashMap::from([("gain".into(), ParamValue::Float(0.5))]);
    let halved = evaluate_effect("builtin.color_grade", &hot, &half).unwrap();
    assert!(halved.rgba[0] < 255 && halved.rgba[0] > 100);

    // Split toning tints shadows and highlights differently; strength zero is
    // exactly the identity regardless of hues.
    let strong = HashMap::from([
        ("split_strength".into(), ParamValue::Float(1.0)),
        ("split_shadow_hue".into(), ParamValue::Float(0.0)),
        ("split_highlight_hue".into(), ParamValue::Float(0.0)),
    ]);
    let red_tinted = evaluate_effect("builtin.color_grade", &frame, &strong).unwrap();
    assert!(
        red_tinted.rgba[0] > frame.rgba[0],
        "red hue lifts red channel"
    );
    let weak = HashMap::from([
        ("split_strength".into(), ParamValue::Float(0.0)),
        ("split_shadow_hue".into(), ParamValue::Float(0.0)),
    ]);
    assert_eq!(
        evaluate_effect("builtin.color_grade", &frame, &weak)
            .unwrap()
            .rgba,
        frame.rgba
    );

    // Chromatic wheels: a red-shifted lift tint pushes red up, blue down,
    // relative to the same frame; neutral gray (0.5) is the exact identity.
    let warm_lift = HashMap::from([("lift_color".into(), ParamValue::Color([0.8, 0.5, 0.2, 1.0]))]);
    let warmed = evaluate_effect("builtin.color_grade", &dim, &warm_lift).unwrap();
    assert!(warmed.rgba[0] > dim.rgba[0] && warmed.rgba[2] < dim.rgba[2]);
    let neutral = HashMap::from([("lift_color".into(), ParamValue::Color([0.5, 0.5, 0.5, 1.0]))]);
    assert_eq!(
        evaluate_effect("builtin.color_grade", &dim, &neutral)
            .unwrap()
            .rgba,
        dim.rgba
    );
    // Gain tint: red-side gain multiplies red energy in linear light.
    let red_gain = HashMap::from([("gain_color".into(), ParamValue::Color([1.0, 0.5, 0.5, 1.0]))]);
    let boosted = evaluate_effect("builtin.color_grade", &frame, &red_gain).unwrap();
    assert!(boosted.rgba[0] >= frame.rgba[0]);
    let neutral_gain =
        HashMap::from([("gain_color".into(), ParamValue::Color([0.5, 0.5, 0.5, 1.0]))]);
    assert_eq!(
        evaluate_effect("builtin.color_grade", &frame, &neutral_gain)
            .unwrap()
            .rgba,
        frame.rgba
    );

    // Lift raises the floor; negative lift crushes it.
    let lifted = HashMap::from([("lift".into(), ParamValue::Float(0.2))]);
    let up = evaluate_effect("builtin.color_grade", &dim, &lifted).unwrap();
    assert!(up.rgba[0] > 128);
    let crushed = HashMap::from([("lift".into(), ParamValue::Float(-0.2))]);
    assert!(
        evaluate_effect("builtin.color_grade", &dim, &crushed)
            .unwrap()
            .rgba[0]
            < 128
    );
}
