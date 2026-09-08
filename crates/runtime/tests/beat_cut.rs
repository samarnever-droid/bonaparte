//! Beat Cut end-to-end: import a click track, detect the grid, cut the
//! video to it, and prove the segments are real jump cuts.

use base64::{engine::general_purpose::STANDARD, Engine};
use bonaparte_model::*;
use bonaparte_runtime::audio::prepare_import;
use bonaparte_runtime::EditorSession;
use serde_json::json;

/// 48 kHz mono 16-bit PCM click track: decaying bursts at `bpm`, quiet
/// sine bed between — exactly what beat detection keys on.
fn click_wav(rate: u32, seconds: usize, bpm: f64) -> Vec<u8> {
    let frames = rate as usize * seconds;
    let period = (rate as f64 * 60.0 / bpm) as usize;
    let mut pcm = vec![0i16; frames];
    for start in (0..frames).step_by(period) {
        for (d, s) in pcm[start..(start + 2400).min(frames)]
            .iter_mut()
            .enumerate()
        {
            let decay = (-(d as f64) / 700.0).exp();
            *s = (0.9 * decay * (d as f64 * 0.05).sin() * i16::MAX as f64) as i16;
        }
    }
    for (i, s) in pcm.iter_mut().enumerate() {
        if *s == 0 {
            *s = (0.01 * (i as f64 * 0.01).sin() * i16::MAX as f64) as i16;
        }
    }
    let data_len = frames * 2;
    let mut wav = Vec::with_capacity(44 + data_len);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(data_len as u32).to_le_bytes());
    for v in pcm {
        wav.extend_from_slice(&v.to_le_bytes());
    }
    wav
}

/// Session with a 2-second 8×8 video imported (one footage layer).
fn session_with_video() -> EditorSession {
    let mut p = Project::new("Beat Cut");
    p.create_comp("Main", 32, 32, FrameRate::FPS_30, Time(4 * TICKS_PER_SEC));
    let mut s = EditorSession::new(p).unwrap();
    let poster = STANDARD.encode([200u8; 64 * 4]);
    s.command(
        "import_video",
        json!({
            "name": "take.mp4",
            "compId": 1,
            "width": 8,
            "height": 8,
            "fpsNum": 4,
            "fpsDen": 1,
            "durationTicks": 2 * TICKS_PER_SEC,
            "posterRgbaBase64": poster,
            "frames": (0..8).map(|i| json!({
                "timeMs": i as u32 * 250,
                "rgbaBase64": STANDARD.encode([(100 + i * 15) as u8; 64 * 4]),
            })).collect::<Vec<_>>(),
        }),
    )
    .expect("video import");
    s
}

fn video_asset_id(s: &EditorSession) -> u64 {
    s.project
        .media
        .iter()
        .find_map(|(id, m)| m.video.is_some().then_some(id.0))
        .expect("video asset present")
}

fn audio_asset_id(s: &EditorSession) -> u64 {
    s.project
        .media
        .iter()
        .find_map(|(id, m)| m.audio.is_some().then_some(id.0))
        .expect("audio asset present")
}

fn import_clicks(s: &mut EditorSession) {
    let click = click_wav(48_000, 10, 120.0);
    let prepared = prepare_import(json!({
        "compId": 1,
        "name": "drums.wav",
        "dataBase64": STANDARD.encode(&click),
        "startFrame": 0,
    }))
    .unwrap();
    s.import_audio(prepared).unwrap();
    s.audio.refresh(&s.project).unwrap();
}

fn peak_scale(k: &Keyframe) -> f32 {
    match k.value {
        PropValue::Vec2([x, _]) => x,
        PropValue::Scalar(x) => x,
    }
}

#[test]
fn detect_cut_and_prove_jump_cuts() {
    let mut s = session_with_video();
    import_clicks(&mut s);

    // 1) Detect.
    let reply = s
        .command(
            "analyze_beats",
            json!({"compId": 1, "assetId": audio_asset_id(&s)}),
        )
        .expect("analyze");
    assert!(
        (115.0..=125.0).contains(&reply["bpm"].as_f64().unwrap()),
        "bpm: {}",
        reply["bpm"]
    );
    assert!(
        reply["beats"].as_u64().unwrap() >= 18,
        "10 s of 120 BPM ≈ 20 beats"
    );

    // Grid persisted, rotated so index 0 is a downbeat.
    let asset = s.project.media[&MediaId(audio_asset_id(&s))]
        .audio
        .as_ref()
        .unwrap();
    let grid = asset.beat_grid.as_ref().expect("grid persisted");
    assert_eq!(reply["beats"].as_u64().unwrap() as usize, grid.len());
    assert!(grid[0].abs() < 100.0, "grid starts at the first beat");

    // Markers painted on the arrangement ruler (comp is 4 s → ~8 beats).
    let markers = &s.project.comp(CompId(1)).unwrap().audio.markers;
    assert!(
        markers.iter().filter(|m| m.id.starts_with("beat-")).count() >= 6,
        "ruler markers painted"
    );

    // 2) Cut. The video layer is the only footage layer and spans 2 s.
    let video_layer = *s
        .project
        .comp(CompId(1))
        .unwrap()
        .layer_order
        .last()
        .unwrap();
    let layers_before = s.project.comp(CompId(1)).unwrap().layer_order.len();
    let history_before = s.snapshot().history.len();
    let reply = s
        .command(
            "cut_to_beat",
            json!({"compId": 1, "audioAsset": audio_asset_id(&s), "videoLayer": video_layer.0, "style": "remix"}),
        )
        .expect("cut");
    let segments = reply["segments"].as_u64().unwrap();
    assert!(
        (3..=6).contains(&segments),
        "2 s of 120 BPM ≈ 4 beats, got {segments}"
    );

    // 3) Segments are real jump cuts: source offsets differ between
    //    clips and downbeat clips pop 8% bigger.
    let comp = s.project.comp(CompId(1)).unwrap();
    assert_eq!(comp.layer_order.len(), layers_before + segments as usize);
    let mut offsets = Vec::new();
    let mut pops = 0;
    for id in &comp.layer_order {
        let layer = &comp.layers[id];
        if let LayerKind::Footage {
            media,
            source_start,
        } = &layer.kind
        {
            if !layer.name.starts_with("Beat ") {
                continue; // the original video layer is footage too
            }
            assert_eq!(media.0, video_asset_id(&s));
            offsets.push(source_start.0);
            if layer.transform.scale == [108.0, 108.0] {
                pops += 1;
            }
        }
    }
    assert_eq!(offsets.len(), segments as usize);
    assert!(offsets.iter().any(|o| *o > 0), "jump cuts shift the source");
    assert!(
        offsets.windows(2).any(|w| w[0] != w[1]),
        "windows actually bounce"
    );
    assert!(pops >= 1, "downbeat clips pop, got {pops}");

    // 4) Undo restores the pre-cut timeline in ONE step.
    s.command("undo", json!({})).unwrap();
    let comp = s.project.comp(CompId(1)).unwrap();
    assert_eq!(comp.layer_order.len(), layers_before);
    assert_eq!(s.snapshot().history.len(), history_before);
}

#[test]
fn pulse_style_animates_scale_without_cutting() {
    let mut s = session_with_video();
    import_clicks(&mut s);
    s.command(
        "analyze_beats",
        json!({"compId": 1, "assetId": audio_asset_id(&s)}),
    )
    .unwrap();

    let video_layer = *s
        .project
        .comp(CompId(1))
        .unwrap()
        .layer_order
        .last()
        .unwrap();
    let layers_before = s.project.comp(CompId(1)).unwrap().layer_order.len();
    let reply = s
        .command(
            "cut_to_beat",
            json!({"compId": 1, "audioAsset": audio_asset_id(&s), "videoLayer": video_layer.0, "style": "pulse"}),
        )
        .unwrap();
    assert_eq!(reply["style"], "pulse");
    let comp = s.project.comp(CompId(1)).unwrap();
    assert_eq!(
        comp.layer_order.len(),
        layers_before,
        "pulse adds no layers"
    );
    let track = comp.layers[&video_layer]
        .tracks
        .get(&Property::Scale)
        .expect("scale track animated");
    // One pop key + one snap-back per beat inside the 2 s layer.
    assert!(track.keys.len() >= 8, "got {} keys", track.keys.len());
    let peaks: Vec<f32> = track
        .keys
        .iter()
        .map(peak_scale)
        .filter(|p| *p > 100.0)
        .collect();
    assert!(peaks.len() >= 4, "a pop per beat");
    assert!(peaks.iter().any(|p| *p > 110.0), "downbeats pop harder");
}

#[test]
fn cutting_without_a_grid_fails_helpfully() {
    let mut s = session_with_video();
    let video_layer = *s
        .project
        .comp(CompId(1))
        .unwrap()
        .layer_order
        .last()
        .unwrap();
    let err = s
        .command(
            "cut_to_beat",
            json!({"compId": 1, "audioAsset": 999, "videoLayer": video_layer.0}),
        )
        .unwrap_err();
    assert!(
        err.contains("Detect beats"),
        "error must point at the fix: {err}"
    );
}
