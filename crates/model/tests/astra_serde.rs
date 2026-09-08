//! EmbeddedAudio × Astra extents: old projects load unchanged, new
//! store-backed sources round-trip, and extents never bloat small saves.

use bonaparte_model::audio::EmbeddedAudio;
use std::sync::Arc;

fn sample(astra_chunks: Option<Arc<[String]>>) -> EmbeddedAudio {
    EmbeddedAudio {
        data_base64: Arc::from(""),
        astra_chunks,
        beat_grid: None,
        sha256: "a".repeat(64),
        frames: 48_000,
        channels: 2,
        sample_rate: 48_000,
        original_sample_rate: 44_100,
        original_channels: 1,
        codec: "wav".into(),
        peak: 0.81,
    }
}

#[test]
fn legacy_documents_without_extents_load_unchanged() {
    // A pre-Astra save: no astra_chunks key anywhere.
    let doc = serde_json::json!({
        "data_base64": "AAAA",
        "sha256": "b".repeat(64),
        "frames": 1_024,
        "channels": 1,
        "sample_rate": 44_100,
        "original_sample_rate": 44_100,
        "original_channels": 1,
        "codec": "wav",
        "peak": 1.0,
    });
    let audio: EmbeddedAudio = serde_json::from_value(doc).unwrap();
    assert_eq!(audio.astra_chunks, None);
    // And small inline sources must not grow a serialized extent key.
    let out = serde_json::to_value(sample(None)).unwrap();
    assert!(out.get("astra_chunks").is_none());
    assert_eq!(out["data_base64"], "");
}

#[test]
fn store_backed_sources_round_trip() {
    let extent: Arc<[String]> = vec!["deadbeef".to_string(), "0000".repeat(16)]
        .into_iter()
        .collect::<Vec<_>>()
        .into();
    let audio = sample(Some(extent));
    let value = serde_json::to_value(&audio).unwrap();
    let chunks = value["astra_chunks"].as_array().expect("extent serialized");
    assert_eq!(chunks.len(), 2);
    assert_eq!(value["data_base64"], "");
    let back: EmbeddedAudio = serde_json::from_value(value).unwrap();
    assert_eq!(back.astra_chunks.as_ref().unwrap().len(), 2);
    assert_eq!(back.sha256, audio.sha256);
    assert_eq!(back.frames, audio.frames);
}
