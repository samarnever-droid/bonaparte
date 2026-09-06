use bonaparte_model::*;
use bonaparte_runtime::{EditorSession, PreviewRequest};
use serde_json::{json, Value};
fn session() -> EditorSession {
    let mut p = Project::new("Interactions");
    let c = p.create_comp("Main", 64, 48, FrameRate::FPS_24, Time(120000));
    let mut layer = Layer::new_rect("Box", [1.0, 0.0, 1.0, 1.0], Time::ZERO, Time(120000));
    if let LayerKind::Shape { style, .. } = &mut layer.kind {
        style.size = Some([16.0, 12.0]);
    }
    p.insert_layer(c, layer);
    EditorSession::new(p).unwrap()
}
#[test]
fn authoritative_deltas_roundtrip_undo_and_fall_back_for_wrong_revision() {
    let mut s = session();
    let op =
        json!({"type":"setValue","comp":1,"layer":1,"property":"Position","value":{"Vec2":[10,5]}});
    let result = s
        .command("apply", json!({"op":op,"delta":true,"baseRevision":0}))
        .unwrap();
    assert_eq!(result["kind"], "patch");
    assert!(result.get("project").is_none());
    assert_eq!(
        result["comps"]["1"]["layers"]["1"]["transform"]["position"],
        json!([10.0, 5.0])
    );
    assert!(serde_json::to_vec(&result).unwrap().len() < 2000);
    let result = s
        .command("undo", json!({"delta":true,"baseRevision":1}))
        .unwrap();
    assert_eq!(
        result["comps"]["1"]["layers"]["1"]["transform"]["position"],
        json!([0.0, 0.0])
    );
    let full = s
        .command("redo", json!({"delta":true,"baseRevision":999}))
        .unwrap();
    assert!(full.get("project").is_some());
}
#[test]
fn visual_edits_and_markers_do_not_invalidate_audio_clock() {
    let mut s = session();
    let before = s.snapshot().audio_revision;
    s.command("apply",json!({"op":{"type":"setValue","comp":1,"layer":1,"property":"Scale","value":{"Vec2":[125,80]}}})).unwrap();
    assert_eq!(s.snapshot().audio_revision, before);
    let mut audio = s.project.comp(CompId(1)).unwrap().audio.clone();
    audio.bpm = 96.0;
    audio.markers.push(AudioMarker {
        id: "marker-1".into(),
        frame: 10,
        name: "Cue".into(),
    });
    s.command(
        "apply",
        json!({"op":Op::SetCompAudio{comp:CompId(1),audio:audio.clone()}}),
    )
    .unwrap();
    assert_eq!(s.snapshot().audio_revision, before);
    audio.gain_db = -3.0;
    s.command(
        "apply",
        json!({"op":Op::SetCompAudio{comp:CompId(1),audio}}),
    )
    .unwrap();
    assert_eq!(s.snapshot().audio_revision, before + 1);
}
#[test]
fn lightweight_transform_preview_matches_a_real_edit_without_mutation() {
    let mut s = session();
    let before = serde_json::to_value(s.snapshot()).unwrap();
    let req:PreviewRequest=serde_json::from_value(json!({"compId":1,"time":0,"backend":"cpu","transformOverride":{"compId":1,"layerId":1,"property":"Position","value":{"Vec2":[11,4]}}})).unwrap();
    let transient = s.preview_input(req).unwrap().render().unwrap();
    assert_eq!(serde_json::to_value(s.snapshot()).unwrap(), before);
    s.command("apply",json!({"op":{"type":"setValue","comp":1,"layer":1,"property":"Position","value":{"Vec2":[11,4]}}})).unwrap();
    let real = s
        .preview_input(
            serde_json::from_value(json!({"compId":1,"time":0,"backend":"cpu"})).unwrap(),
        )
        .unwrap()
        .render()
        .unwrap();
    assert_eq!(transient.pixels.rgba, real.pixels.rgba);
}
#[test]
fn interaction_planes_are_real_rgba_and_bad_transforms_are_rejected() {
    let s = session();
    let bytes = s
        .interaction_input(
            serde_json::from_value(json!({"compId":1,"layerId":1,"time":0})).unwrap(),
        )
        .unwrap()
        .packet()
        .unwrap();
    assert_eq!(&bytes[..4], b"BIP1");
    let n = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    let meta: Value = serde_json::from_slice(&bytes[8..8 + n]).unwrap();
    assert_eq!(meta["width"], 64);
    assert_eq!(bytes.len(), 8 + n + 64 * 48 * 4 * 3);
    let plane = &bytes[8 + n + 64 * 48 * 4..8 + n + 64 * 48 * 8];
    assert!(plane.chunks_exact(4).any(|p| p == [255, 0, 255, 255]));
    let req:PreviewRequest=serde_json::from_value(json!({"compId":1,"time":0,"transformOverride":{"compId":1,"layerId":1,"property":"Scale","value":{"Scalar":1}}})).unwrap();
    assert!(s.preview_input(req).is_err());
}
