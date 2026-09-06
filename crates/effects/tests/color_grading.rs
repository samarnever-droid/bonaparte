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
    assert_eq!(manifest.params.len(), 12);
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
