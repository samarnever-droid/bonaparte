//! Vision-Native MCP: the frame viewer returns real pixels inline and the
//! audio extractor returns the real mix — a model's eyes and ears on the
//! edit, no disk round-trip, no guessing from metadata.

use base64::{engine::general_purpose::STANDARD, Engine};
use bonaparte_mcp::protocol::ToolCallResult;
use bonaparte_mcp::session::McpSession;
use bonaparte_mcp::tools::execute_tool;
use bonaparte_model::*;
use serde_json::{json, Value};

fn project_with_motion() -> Project {
    let mut p = Project::new("Vision");
    let comp = p.create_comp("Main", 320, 180, FrameRate::FPS_30, Time(2 * TICKS_PER_SEC));
    let mut layer = Layer::new(
        "Card",
        LayerKind::Shape {
            color: [0.9, 0.2, 0.1, 1.0],
            generator: Some("builtin.circle".into()),
            style: Default::default(),
            points: Vec::new(),
        },
        Time::ZERO,
        Time(2 * TICKS_PER_SEC),
    );
    layer.transform.position = [-40.0, 0.0];
    p.insert_layer(comp, layer);
    p
}

fn decode_blocks(result: &ToolCallResult) -> Vec<Value> {
    serde_json::to_value(result)
        .unwrap()
        .get("content")
        .unwrap()
        .as_array()
        .unwrap()
        .clone()
}

#[test]
fn frame_view_returns_inline_png_pixels() {
    let mut session = McpSession {
        project: project_with_motion(),
        ..McpSession::default()
    };
    let result = execute_tool(
        &mut session,
        "frame.view",
        json!({"time": 0.5, "width": 128}),
    );
    assert!(!result.is_error, "{:?}", result.content);
    let blocks = decode_blocks(&result);
    // Leading text meta, then the actual image block.
    let meta: Value = serde_json::from_str(blocks[0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(meta["frames"][0]["width"], 128);
    let data = blocks[1]["data"].as_str().expect("inline image data");
    assert_eq!(blocks[1]["type"], "image");
    assert_eq!(blocks[1]["mimeType"], "image/png");
    let png = STANDARD.decode(data).unwrap();
    assert_eq!(&png[1..4], b"PNG", "real PNG magic");
    assert!(png.len() > 500, "non-trivial pixels: {}", png.len());
}

#[test]
fn frame_view_storyboard_spans_the_comp() {
    let mut session = McpSession {
        project: project_with_motion(),
        ..McpSession::default()
    };
    let result = execute_tool(&mut session, "frame.view", json!({"count": 4, "width": 64}));
    assert!(!result.is_error);
    let meta: Value =
        serde_json::from_str(decode_blocks(&result)[0]["text"].as_str().unwrap()).unwrap();
    let frames = meta["frames"].as_array().unwrap();
    assert_eq!(frames.len(), 4);
    let times: Vec<f64> = frames
        .iter()
        .map(|f| f["time_secs"].as_f64().unwrap())
        .collect();
    assert!(times[0] < times[1] && times[2] < times[3], "ascending");
    assert!(
        (times[3] - 1.5).abs() < 0.01,
        "last storyboard frame at 3/4 of 2s"
    );
}

#[test]
fn audio_extract_returns_inline_wav_of_the_mix() {
    // Build a project with an imported (decoded) click so the mix is non-silent.
    let mut session = McpSession::default();
    let wav_bytes = {
        let rate = 48_000u32;
        let frames = rate as usize; // 1 s
        let mut wav = Vec::new();
        let data_len = frames * 2;
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
        for i in 0..frames {
            let v = (0.5 * (i as f32 * 0.05).sin() * i16::MAX as f32) as i16;
            wav.extend_from_slice(&v.to_le_bytes());
        }
        wav
    };
    session.project = {
        let mut p = project_with_motion();
        // Import through the real pipeline so the arrangement has a clip.
        let prepared = bonaparte_runtime::audio::prepare_import(json!({
            "compId": 1,
            "name": "tone.wav",
            "dataBase64": STANDARD.encode(&wav_bytes),
            "startFrame": 0,
        }))
        .unwrap();
        let mut editor = bonaparte_runtime::EditorSession::new(p.clone()).unwrap();
        editor.import_audio(prepared).unwrap();
        editor.audio.refresh(&editor.project).unwrap();
        editor.project
    };
    let result = execute_tool(
        &mut session,
        "audio.extract",
        json!({"start_secs": 0.0, "duration_secs": 0.5, "rate": 16000}),
    );
    assert!(!result.is_error, "{:?}", result.content);
    let blocks = decode_blocks(&result);
    let meta: Value = serde_json::from_str(blocks[0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(meta["sampleRate"], 16000);
    assert_eq!(meta["channels"], 1);
    let data = blocks[1]["data"].as_str().expect("inline audio data");
    assert_eq!(blocks[1]["type"], "audio");
    assert_eq!(blocks[1]["mimeType"], "audio/wav");
    let wav = STANDARD.decode(data).unwrap();
    assert_eq!(&wav[0..4], b"RIFF");
    // Non-silent: a sine at 440 Hz through the mix must exceed −40 dB peak.
    let peak_db = meta["peakDb"].as_f64().unwrap();
    assert!(peak_db > -40.0, "audible mix, got {peak_db} dB");
}

#[test]
fn audio_extract_rejects_out_of_range_start() {
    let mut session = McpSession::default();
    session.project = project_with_motion();
    let result = execute_tool(&mut session, "audio.extract", json!({"start_secs": 999.0}));
    assert!(result.is_error);
}
