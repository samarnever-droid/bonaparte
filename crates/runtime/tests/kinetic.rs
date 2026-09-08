//! Kinetic ⚡ end-to-end: lyric-video generation on the beat grid plus the
//! synthesized sound-design track. This is the "type lyrics, get a finished
//! sequence" proof.

use base64::{engine::general_purpose::STANDARD, Engine};
use bonaparte_model::*;
use bonaparte_runtime::audio::prepare_import;
use bonaparte_runtime::EditorSession;
use serde_json::json;

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
    wav.extend_from_slice(b"WAVEfmt ");
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

fn session() -> EditorSession {
    let mut p = Project::new("Kinetic");
    p.create_comp("Main", 640, 360, FrameRate::FPS_30, Time(4 * TICKS_PER_SEC));
    EditorSession::new(p).unwrap()
}

fn import_clicks_and_detect(s: &mut EditorSession) -> u64 {
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
    let id = s
        .project
        .media
        .iter()
        .find_map(|(id, m)| m.audio.is_some().then_some(id.0))
        .unwrap();
    s.command("analyze_beats", json!({"compId": 1, "assetId": id}))
        .unwrap();
    id
}

#[test]
fn lyrics_lay_out_on_the_grid_and_animate() {
    let mut s = session();
    let audio = import_clicks_and_detect(&mut s);

    let reply = s
        .command(
            "kinetic_lyrics",
            json!({
                "compId": 1,
                "audioAsset": audio,
                "style": "pop",
                "lines": ["we light the sky", "hold the beat down"],
            }),
        )
        .expect("kinetic");
    assert_eq!(reply["lines"].as_u64().unwrap(), 2);
    assert_eq!(reply["words"].as_u64().unwrap(), 8); // 4 + 4

    let comp = s.project.comp(CompId(1)).unwrap();
    let lyric_layers: Vec<&Layer> = comp
        .layer_order
        .iter()
        .map(|id| &comp.layers[id])
        .filter(|l| l.name.starts_with("Lyric: "))
        .collect();
    assert_eq!(lyric_layers.len(), 8);

    // Every word layer is a text layer with its own window inside the comp.
    for l in &lyric_layers {
        let LayerKind::Text { text, size, style } = &l.kind else {
            panic!("not a text layer");
        };
        assert!(!text.is_empty());
        assert!(style.bold);
        assert!(*size > 10.0 && *size < 200.0);
        assert!(l.start.0 >= 0);
        assert!(l.start.0 + l.duration.0 <= 4 * TICKS_PER_SEC);
    }

    // Pop words carry a scale track: tiny → peak → settle.
    for l in &lyric_layers {
        let track = l.tracks.get(&Property::Scale).expect("scale keys");
        assert_eq!(track.keys.len(), 3, "pop = 3 scale keys");
        let first = match track.keys[0].value {
            PropValue::Vec2([x, _]) => x,
            PropValue::Scalar(x) => x,
        };
        assert!(first < 20.0, "starts tiny: {first}");
        let peak = match track.keys[1].value {
            PropValue::Vec2([x, _]) => x,
            PropValue::Scalar(x) => x,
        };
        assert!((100.0..125.0).contains(&peak), "peak {peak}");
        assert_eq!(track.keys[2].value, PropValue::Vec2([100.0, 100.0]));
    }

    // Words in the same line start staggered, lines don't overlap.
    let mut starts: Vec<i64> = lyric_layers.iter().map(|l| l.start.0).collect();
    starts.sort();
    assert!(
        starts.windows(2).all(|w| w[0] < w[1]),
        "staggered, no overlaps"
    );

    // ONE undo removes the whole generated sequence.
    let history = s.snapshot().history.len();
    s.command("undo", json!({})).unwrap();
    let comp = s.project.comp(CompId(1)).unwrap();
    assert!(comp.layer_order.is_empty());
    assert_eq!(s.snapshot().history.len(), history - 1);
}

#[test]
fn lyrics_fall_back_to_even_spread_without_a_grid() {
    let mut s = session();
    let reply = s
        .command(
            "kinetic_lyrics",
            json!({
                "compId": 1,
                "style": "rise",
                "lines": ["no music", "still moves"],
            }),
        )
        .expect("kinetic without grid");
    assert_eq!(reply["words"].as_u64().unwrap(), 4);
    let comp = s.project.comp(CompId(1)).unwrap();
    let spans: Vec<(i64, i64)> = comp
        .layer_order
        .iter()
        .map(|id| {
            let l = &comp.layers[id];
            (l.start.0, l.start.0 + l.duration.0)
        })
        .collect();
    // Two lines × 2 s each inside a 4 s comp, words ride their line's span.
    assert!(spans
        .iter()
        .all(|(a, b)| *a >= 0 && *b <= 4 * TICKS_PER_SEC));
    let first_half = spans.iter().filter(|(a, _)| *a < 2 * TICKS_PER_SEC).count();
    assert_eq!(first_half, 2, "line 1 in the first half");
}

#[test]
fn sound_design_lands_on_downbeats_and_is_one_undo() {
    let mut s = session();
    let audio = import_clicks_and_detect(&mut s);
    let tracks_before = s.project.comp(CompId(1)).unwrap().audio.tracks.len();
    let media_before = s.project.media.len();

    let reply = s
        .command(
            "sound_design",
            json!({"compId": 1, "audioAsset": audio, "flavor": "impact"}),
        )
        .expect("sound design");
    assert_eq!(reply["flavor"], "impact");
    let events = reply["events"].as_u64().unwrap();
    assert!(
        events >= 2,
        "4 s of 120 BPM holds ≥2 downbeats… got {events}"
    );

    // New synthesized asset + a dedicated sound-design lane.
    assert_eq!(s.project.media.len(), media_before + 1);
    let asset = s
        .project
        .media
        .get(&MediaId(reply["mediaId"].as_u64().unwrap()))
        .expect("synth asset");
    let synth = asset.audio.as_ref().expect("audio payload");
    assert_eq!(synth.sample_rate, 48_000);
    assert!(synth.frames > 10_000, "0.45 s of audio");
    let comp = s.project.comp(CompId(1)).unwrap();
    assert_eq!(comp.audio.tracks.len(), tracks_before + 1);
    let lane = comp.audio.tracks.last().unwrap();
    assert_eq!(lane.clips.len() as u64, events);
    assert!(lane.clips.iter().all(|c| c.gain_db < 0.0));

    // Decoded source registered: playback works without a re-decode.
    let media_id = MediaId(reply["mediaId"].as_u64().unwrap());
    assert!(s.audio.sources.contains_key(&media_id));

    // One undo removes asset + lane + clips together.
    s.command("undo", json!({})).unwrap();
    assert_eq!(s.project.media.len(), media_before);
    assert_eq!(
        s.project.comp(CompId(1)).unwrap().audio.tracks.len(),
        tracks_before
    );
    assert!(!s.audio.sources.contains_key(&media_id));
}

#[test]
fn sound_design_without_a_grid_fails_helpfully() {
    let mut s = session();
    let click = click_wav(48_000, 6, 120.0);
    let prepared = prepare_import(json!({
        "compId": 1,
        "name": "plain.wav",
        "dataBase64": STANDARD.encode(&click),
        "startFrame": 0,
    }))
    .unwrap();
    s.import_audio(prepared).unwrap();
    let id = s
        .project
        .media
        .iter()
        .find_map(|(id, m)| m.audio.is_some().then_some(id.0))
        .unwrap();
    let err = s
        .command("sound_design", json!({"compId": 1, "audioAsset": id}))
        .unwrap_err();
    assert!(err.contains("Detect beats"), "{err}");
}
