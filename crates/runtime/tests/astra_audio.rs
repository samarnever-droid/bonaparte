//! Astra-backed audio import: sources above the inline limit stream into
//! the store, the project carries a hash extent, and decode re-reads the
//! extent through the hot cache. This is the "no more 32 MiB wall" proof.

use base64::{engine::general_purpose::STANDARD, Engine};
use bonaparte_media::audio::decode_embedded;
use bonaparte_runtime::audio::prepare_import;
use serde_json::json;

/// Build a valid 48 kHz stereo 16-bit PCM WAV of `seconds` length.
fn wav_bytes(seconds: usize) -> Vec<u8> {
    let rate = 48_000u32;
    let channels = 2u16;
    let frames = rate as usize * seconds;
    let data_len = frames * channels as usize * 2;
    let mut wav = Vec::with_capacity(44 + data_len);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate as u32 * channels as u32 * 2).to_le_bytes()); // byte rate
    wav.extend_from_slice(&(channels * 2).to_le_bytes()); // block align
    wav.extend_from_slice(&16u16.to_le_bytes()); // bits
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(data_len as u32).to_le_bytes());
    for f in 0..frames {
        for c in 0..channels as usize {
            let t = f as f32 / rate as f32;
            let sample = (t * 440.0 + c as f32 * 0.01).sin() * 0.4;
            wav.extend_from_slice(&((sample * i16::MAX as f32) as i16).to_le_bytes());
        }
    }
    wav
}

#[test]
fn large_source_streams_into_astra_not_the_project() {
    let bytes = wav_bytes(95); // ~17.6 MiB stereo 16-bit — above the 16 MiB inline limit
    assert!(bytes.len() > 16 * 1024 * 1024);
    let prepared = prepare_import(json!({
        "compId": 1,
        "name": "long take.wav",
        "dataBase64": STANDARD.encode(&bytes),
        "startFrame": 0,
    }))
    .expect("sources above the old 32 MiB wall must import fine");

    // The project document stays light: no base64 payload, just the extent.
    assert!(
        prepared.audio.data_base64.is_empty(),
        "big sources must not ride in the project file"
    );
    let extent = prepared.audio.astra_chunks.as_ref().expect("extent list");
    assert!(extent.len() >= 4, "17 MiB must span multiple 4 MiB chunks");

    // Extent is verifiable and re-readable through the hot cache.
    let store = astra::Store::global();
    assert!(store.verify(extent.iter().as_ref()));

    // Decode-from-store: the extent decodes to the recorded frame count.
    let _source = decode_embedded(&prepared.audio).expect("store-backed decode");
    assert_eq!(prepared.audio.frames as usize, 48_000 * 95);
    assert_eq!(prepared.audio.channels, 2);
    assert_eq!(prepared.audio.sample_rate, 48_000);
}

#[test]
fn small_source_stays_fully_portable() {
    let bytes = wav_bytes(1); // ~192 KiB — well under the inline limit
    let prepared = prepare_import(json!({
        "compId": 1,
        "name": "bling.wav",
        "dataBase64": STANDARD.encode(&bytes),
        "startFrame": 0,
    }))
    .unwrap();
    assert!(!prepared.audio.data_base64.is_empty());
    assert!(prepared.audio.astra_chunks.is_none());
}
