//! Kaya ⚡ plugin: the annotation format, offline scene sense, key-gated
//! network paths, and the plugin manifest contract.

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

fn session_with_clicks() -> (EditorSession, u64) {
    let mut p = Project::new("Kaya");
    p.create_comp("Main", 320, 180, FrameRate::FPS_30, Time(4 * TICKS_PER_SEC));
    let mut s = EditorSession::new(p).unwrap();
    let prepared = prepare_import(json!({
        "compId": 1,
        "name": "drums.wav",
        "dataBase64": STANDARD.encode(click_wav(48_000, 10, 120.0)),
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
    (s, id)
}

#[test]
fn the_kaya_format_is_exactly_word_start_plus_duration() {
    // The user's example: "Hi i am ansh" → Hi~~1:20+2~~ I~~1:27+4~~ …
    let words = vec![
        KayaWord {
            word: "Hi".into(),
            start_ms: 80_000.0,
            dur_ms: 2_000.0,
        },
        KayaWord {
            word: "I".into(),
            start_ms: 87_000.0,
            dur_ms: 4_000.0,
        },
    ];
    let line = bonaparte_runtime::kaya::annotate(&words);
    assert_eq!(line, "Hi~~1:20+2~~ I~~1:27+4~~");
    let back = bonaparte_runtime::kaya::parse_annotation(&line);
    assert_eq!(back.len(), 2);
    assert!((back[1].start_ms - 87_000.0).abs() < 1.0);
}

#[test]
fn offline_scene_sense_finds_rhythm_and_silence() {
    let (mut s, id) = session_with_clicks();
    let reply = s.command("kaya.analyze", json!({"assetId": id})).unwrap();
    let bpm = reply["bpm"].as_f64().expect("tempo detected");
    assert!((115.0..=125.0).contains(&bpm), "bpm {bpm}");
    assert!(reply["beats"].as_u64().unwrap() >= 10);
    // The click track's quarter-second gaps read as rhythm, not silence —
    // but the quiet bed between bursts is above the floor only barely; the
    // annotation must exist either way.
    let annotation = reply["annotation"]["silence"].as_str().unwrap();
    assert!(!annotation.is_empty() || reply["silence"].as_array().unwrap().is_empty());
    // With a transcript absent, the words annotation is empty, not garbage.
    assert_eq!(reply["annotation"]["words"], "");
}

#[test]
fn transcribe_without_a_key_fails_helpfully() {
    let (mut s, id) = session_with_clicks();
    let err = s
        .command(
            "kaya.transcribe",
            json!({"assetId": id, "openaiKey": "nope"}),
        )
        .unwrap_err();
    assert!(err.contains("OpenAI"), "{err}");
}

#[test]
fn narrator_without_a_key_fails_helpfully() {
    let (mut s, _) = session_with_clicks();
    let err = s
        .command(
            "narrator.speak",
            json!({"compId": 1, "text": "Hi i am ansh", "startSecs": 0.5, "provider": "sarvam", "apiKey": "x"}),
        )
        .unwrap_err();
    assert!(err.contains("Sarvam"), "{err}");
}

#[test]
fn plugin_manifests_expose_kaya_to_the_ui() {
    let (mut s, _) = session_with_clicks();
    let reply = s.command("plugins.list", json!({})).unwrap();
    let plugins = reply["plugins"].as_array().expect("manifests");
    let kaya = plugins
        .iter()
        .find(|m| m["id"] == "kaya")
        .expect("kaya registered");
    assert_eq!(kaya["name"], "Kaya ⚡");
    let caps: Vec<&str> = kaya["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap())
        .collect();
    assert!(caps.contains(&"transcribe") && caps.contains(&"narrate") && caps.contains(&"analyze"));
    let settings = kaya["settings"].as_array().unwrap();
    assert!(settings
        .iter()
        .any(|x| x["key"] == "openaiKey" && x["kind"] == "secret"));
}
