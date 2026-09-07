use bonaparte_model::*;
use bonaparte_runtime::{EditorSession, PreviewRenderer, PreviewRequest, RenderRequest};
use serde_json::{json, Value};
use std::sync::Arc;
fn project(color: [f32; 4]) -> Project {
    let mut p = Project::new("Preview test");
    let c = p.create_comp("Main", 64, 36, FrameRate::FPS_30, Time(240000));
    p.insert_layer(
        c,
        Layer::new_solid("Source", color, Time::ZERO, Time(240000)),
    );
    p
}
fn session() -> EditorSession {
    EditorSession::new(project([0.25, 0.1, 0.05, 1.0])).unwrap()
}
fn request(divisor: u32, time: i64) -> PreviewRequest {
    serde_json::from_value(json!({"compId":1,"time":time,"divisor":divisor,"backend":"cpu"}))
        .unwrap()
}
#[test]
fn repeated_frames_share_pixels_and_packet_metadata_describes_actual_execution() {
    let session = session();
    let first = session
        .preview_input(request(2, 0))
        .unwrap()
        .render()
        .unwrap();
    let second = session
        .preview_input(request(2, 0))
        .unwrap()
        .render()
        .unwrap();
    assert!(!first.metadata.cache_hit);
    assert!(second.metadata.cache_hit);
    assert!(Arc::ptr_eq(&first.pixels, &second.pixels));
    assert_eq!(second.metadata.backend, "cpu");
    assert_eq!((second.pixels.width, second.pixels.height), (32, 18));
    let bytes = second.packet().unwrap();
    assert_eq!(&bytes[..4], b"BPF3");
    let n = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    let metadata: Value = serde_json::from_slice(&bytes[8..8 + n]).unwrap();
    assert_eq!(metadata["cacheHit"], true);
    assert_eq!(metadata["divisor"], 2);
    assert_eq!(bytes.len(), 8 + n + 32 * 18 * 4);
}
#[test]
fn mutation_undo_open_and_late_old_jobs_cannot_repopulate_stale_caches() {
    let mut session = session();
    let old = session.preview_input(request(1, 0)).unwrap();
    let first = old.render().unwrap();
    session.command("apply",json!({"op":{"type":"setLayerContent","comp":1,"layer":1,"kind":{"Solid":{"color":[0,1,0,1]}}}})).unwrap();
    let late = old.render().unwrap();
    assert_eq!(late.pixels.rgba, first.pixels.rgba);
    assert_eq!(session.preview.status().frames.entries, 0);
    let green = session
        .preview_input(request(1, 0))
        .unwrap()
        .render()
        .unwrap();
    assert!(!green.metadata.cache_hit);
    assert_eq!(&green.pixels.rgba[..4], &[0, 255, 0, 255]);
    session.command("undo", json!({})).unwrap();
    let undo = session
        .preview_input(request(1, 0))
        .unwrap()
        .render()
        .unwrap();
    assert_eq!(undo.pixels.rgba, first.pixels.rgba);
    assert!(!undo.metadata.cache_hit);
    session
        .command(
            "open_project",
            json!({"json":serde_json::to_string(&project([0.0,0.0,1.0,1.0])).unwrap()}),
        )
        .unwrap();
    let blue = session
        .preview_input(request(1, 0))
        .unwrap()
        .render()
        .unwrap();
    assert_eq!(&blue.pixels.rgba[..4], &[0, 0, 255, 255]);
    assert!(!blue.metadata.cache_hit);
}
#[test]
fn temporary_gestures_are_never_persistent_frame_cache_entries() {
    let session = session();
    let first = session
        .preview_input(request(1, 0))
        .unwrap()
        .render()
        .unwrap();
    let before = serde_json::to_value(session.snapshot()).unwrap();
    let mut layer = session
        .project
        .layer(CompId(1), LayerId(1))
        .unwrap()
        .clone();
    layer.kind = LayerKind::Solid {
        color: [1.0, 0.0, 1.0, 1.0],
    };
    for _ in 0..2 {
        let req: PreviewRequest = serde_json::from_value(
            json!({"compId":1,"time":0,"backend":"cpu","layerOverride":layer}),
        )
        .unwrap();
        let f = session.preview_input(req).unwrap().render().unwrap();
        assert!(!f.metadata.cache_hit);
        assert_eq!(&f.pixels.rgba[..4], &[255, 0, 255, 255]);
    }
    let restored = session
        .preview_input(request(1, 0))
        .unwrap()
        .render()
        .unwrap();
    assert!(restored.metadata.cache_hit);
    assert_eq!(restored.pixels.rgba, first.pixels.rgba);
    assert_eq!(serde_json::to_value(session.snapshot()).unwrap(), before);
}
#[test]
fn resolution_time_and_bypass_are_distinct_and_exports_stay_full_size() {
    let mut p = project([1.0, 0.0, 0.0, 1.0]);
    p.layer_mut(CompId(1), LayerId(1))
        .unwrap()
        .effects
        .push(EffectInstance::new("invert", "builtin.invert"));
    let session = EditorSession::new(p).unwrap();
    let before = serde_json::to_value(session.snapshot()).unwrap();
    for d in [1, 2, 4] {
        let f = session
            .preview_input(request(d, 0))
            .unwrap()
            .render()
            .unwrap();
        assert!(!f.metadata.cache_hit);
        assert_eq!((f.pixels.width, f.pixels.height), (64 / d, 36 / d));
        assert_eq!(&f.pixels.rgba[..4], &[0, 255, 255, 255]);
    }
    let bypass: PreviewRequest = serde_json::from_value(
        json!({"compId":1,"time":0,"divisor":4,"backend":"cpu","bypassEffects":true}),
    )
    .unwrap();
    assert_eq!(
        &session
            .preview_input(bypass)
            .unwrap()
            .render()
            .unwrap()
            .pixels
            .rgba[..4],
        &[255, 0, 0, 255]
    );
    assert!(
        !session
            .preview_input(request(4, 4000))
            .unwrap()
            .render()
            .unwrap()
            .metadata
            .cache_hit
    );
    let req: RenderRequest =
        serde_json::from_value(json!({"compId":1,"time":0,"divisor":4})).unwrap();
    let png = session.render_input(req).unwrap().png().unwrap();
    assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), 64);
    assert_eq!(u32::from_be_bytes(png[20..24].try_into().unwrap()), 36);
    assert_eq!(serde_json::to_value(session.snapshot()).unwrap(), before);
}
#[test]
fn clear_is_not_an_edit_and_invalid_requests_do_not_render() {
    let mut session = session();
    session
        .preview_input(request(4, 0))
        .unwrap()
        .render()
        .unwrap();
    let before = serde_json::to_value(session.snapshot()).unwrap();
    session.command("clear_preview_cache", json!({})).unwrap();
    assert_eq!(session.preview.status().frames.entries, 0);
    assert_eq!(serde_json::to_value(session.snapshot()).unwrap(), before);
    assert!(session.preview_input(request(3, 0)).is_err());
    assert!(session.preview_input(request(1, -1)).is_err());
    let req: PreviewRequest = serde_json::from_value(json!({"compId":999,"time":0})).unwrap();
    assert!(session.preview_input(req).is_err());
}
#[test]
fn unavailable_gpu_falls_back_to_correct_cpu_pixels_with_a_reason() {
    let mut session = session();
    session.preview = Arc::new(PreviewRenderer::new(true));
    let req: PreviewRequest =
        serde_json::from_value(json!({"compId":1,"time":0,"backend":"gpu"})).unwrap();
    let frame = session.preview_input(req).unwrap().render().unwrap();
    assert_eq!(frame.metadata.backend, "cpu");
    assert!(frame
        .metadata
        .fallback_reason
        .as_deref()
        .unwrap()
        .contains("disabled"));
    assert!(!session.preview.status().gpu.available);
}
#[cfg(feature = "gpu")]
#[test]
fn gpu_requests_report_real_submissions_and_unsupported_effects_fall_back() {
    if let Err(error) = bonaparte_gpu::probe() {
        assert!(
            std::env::var("BONAPARTE_REQUIRE_GPU_TESTS").as_deref() != Ok("1"),
            "{error}"
        );
        return;
    }
    let mut session = session();
    let req = || {
        serde_json::from_value::<PreviewRequest>(json!({"compId":1,"time":0,"backend":"gpu"}))
            .unwrap()
    };
    let frame = session.preview_input(req()).unwrap().render().unwrap();
    assert!(frame.metadata.backend.starts_with("gpu"));
    assert!(session.preview.status().gpu.submitted_frames > 0);
    // Glow and drop shadow now execute natively as multi-pass GPU programs:
    // stacking them must stay on the GPU with no fallback reason at all.
    let glow = EffectInstance::new("glow", "builtin.glow");
    let shadow = EffectInstance::new("shadow", "builtin.drop_shadow");
    let shadow_params = |mut effect: EffectInstance, color: [f32; 4]| {
        for (id, value) in [
            ("offset_x", 3.0),
            ("offset_y", 2.0),
            ("radius", 5.0),
            ("opacity", 0.8),
        ] {
            effect.params.insert(id.into(), EffectValue::Float(value));
        }
        effect
            .params
            .insert("color".into(), EffectValue::Color(color));
        effect
    };
    let shadow = shadow_params(shadow, [0.0, 0.0, 0.1, 0.9]);
    session
        .command(
            "apply",
            json!({"op":Op::SetLayerEffects{comp:CompId(1),layer:LayerId(1),effects:vec![glow, shadow]}}),
        )
        .unwrap();
    let frame = session.preview_input(req()).unwrap().render().unwrap();
    assert!(frame.metadata.backend.starts_with("gpu"));
    assert_eq!(frame.metadata.fallback_reason.as_deref(), None);
    session.command("undo", json!({})).unwrap();
    assert!(session
        .preview_input(req())
        .unwrap()
        .render()
        .unwrap()
        .metadata
        .backend
        .starts_with("gpu"));
}
