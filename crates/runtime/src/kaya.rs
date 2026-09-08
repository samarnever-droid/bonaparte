//! The plugin host + **Kaya** — the first third-party plugin interface.
//!
//! Plugins register through a manifest (id, capabilities, settings the UI
//! must collect). The runtime validates commands against manifests, the UI
//! renders settings from them, and secrets (API keys) never touch the
//! project file — they ride per-call from the client's own storage.
//!
//! Kaya's capabilities:
//! - `transcribe`: word-level speech recognition (OpenAI Whisper, BYO key)
//!   persisted on the asset as `KayaWord`s.
//! - `annotate`: the Kaya line format — `Hi~~1:20+2~~` (word, start m:ss,
//!   duration seconds) — so any AI or human reads exactly what was said,
//!   when, and how long it took.
//! - `analyze`: offline scene sense — silence spans and rhythm (BPM,
//!   downbeats) straight from the decoded PCM. No keys, no guessing.
//! - `narrate`: text-to-speech via Sarvam AI Bulbul or ElevenLabs (BYO
//!   keys), placed on the timeline at a requested second.

use base64::{engine::general_purpose::STANDARD, Engine};
use bonaparte_audio::beats;
use bonaparte_model::{AudioArrangement, KayaWord};
use serde::Serialize;
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// Plugin manifests — the loader's source of truth
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct PluginSetting {
    pub key: &'static str,
    pub label: &'static str,
    /// "secret" fields are masked by the UI and stored client-side only.
    pub kind: &'static str,
    pub placeholder: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginManifest {
    pub id: &'static str,
    pub name: &'static str,
    pub version: &'static str,
    pub description: &'static str,
    pub capabilities: Vec<&'static str>,
    pub settings: Vec<PluginSetting>,
}

/// Every registered plugin. Built-ins ship with the binary; the manifest is
/// the contract third parties code against.
pub fn plugin_manifests() -> Vec<PluginManifest> {
    vec![PluginManifest {
        id: "kaya",
        name: "Kaya ⚡",
        version: "1.0.0",
        description: "Word-level transcription (Whisper), the Kaya timing \
format, offline silence/rhythm analysis, and the Narrator bumper (Sarvam \
Bulbul / ElevenLabs). Speech in, timed words out; text in, narration out.",
        capabilities: vec!["transcribe", "annotate", "analyze", "narrate"],
        settings: vec![
            PluginSetting {
                key: "openaiKey",
                label: "OpenAI API key (Whisper transcription)",
                kind: "secret",
                placeholder: "sk-…",
            },
            PluginSetting {
                key: "sarvamKey",
                label: "Sarvam AI key (Bulbul narrator)",
                kind: "secret",
                placeholder: "Sarvam subscription key",
            },
            PluginSetting {
                key: "elevenLabsKey",
                label: "ElevenLabs key (Narrator)",
                kind: "secret",
                placeholder: "xi-api-key",
            },
        ],
    }]
}

// ---------------------------------------------------------------------------
// The Kaya line format: Hi~~1:20+2~~ I~~1:22+1~~
// ---------------------------------------------------------------------------

/// Render words in Kaya's line format: `Word~~m:ss+durSecs~~`.
pub fn annotate(words: &[KayaWord]) -> String {
    words
        .iter()
        .map(|w| {
            let total_secs = (w.start_ms / 1000.0).floor().max(0.0) as u64;
            let m = total_secs / 60;
            let s = total_secs % 60;
            let dur = if w.dur_ms > 0.0 && w.dur_ms < 0.05 {
                0.1
            } else {
                (w.dur_ms / 1000.0 * 10.0).round() / 10.0
            };
            let dur = if dur.fract() == 0.0 {
                format!("{:.0}", dur)
            } else {
                format!("{:.1}", dur)
            };
            format!("{}~~{}:{}+{}~~", w.word, m, s, dur)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parse Kaya lines back into timed words (round-trip for AI agents).
pub fn parse_annotation(line: &str) -> Vec<KayaWord> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(start) = rest.find("~~") {
        let word = rest[..start].trim_end().to_owned();
        let after = &rest[start + 2..];
        let Some(end) = after.find("~~") else { break };
        let spec = &after[..end];
        if let Some((mss, dur)) = spec.split_once('+') {
            let parse_stamp = |stamp: &str| -> Option<f64> {
                let (m, s) = stamp.split_once(':')?;
                Some(m.parse::<f64>().ok()? * 60.0 + s.parse::<f64>().ok()?)
            };
            if let (Some(start_secs), Ok(dur_secs)) = (parse_stamp(mss), dur.parse::<f64>()) {
                if !word.is_empty() {
                    out.push(KayaWord {
                        word,
                        start_ms: start_secs * 1000.0,
                        dur_ms: dur_secs * 1000.0,
                    });
                }
            }
        }
        rest = &after[end + 2..];
    }
    out
}

// ---------------------------------------------------------------------------
// Offline analysis: silence + rhythm — what Kaya KNOWS without any key
// ---------------------------------------------------------------------------

pub struct SceneSense {
    /// Silent spans (start_ms, dur_ms) where the envelope stays near-flat.
    pub silence: Vec<(f64, f64)>,
    /// Words-shaped structure for silence, in Kaya format: silence~~0:12+3~~
    pub silence_annotation: String,
    /// Detected tempo (null when non-rhythmic).
    pub bpm: Option<f64>,
    /// Beat count inside the analyzed span.
    pub beats: usize,
}

/// Analyze the decoded PCM of a source: silence spans + rhythm feel.
/// Onset envelope straight off any AudioSource (no decode-cache needed).
pub fn envelope_of(
    source: &dyn bonaparte_audio::AudioSource,
    rate: u32,
    buckets_per_sec: u32,
) -> Vec<f32> {
    let bps = buckets_per_sec.max(1) as usize;
    let per_bucket = (rate as usize / bps).max(1);
    let total = source.frames() as usize;
    let channels = source.channels().max(1) as usize;
    let mut out = Vec::with_capacity(total.div_ceil(per_bucket));
    let mut frame = 0usize;
    while frame < total {
        let end = (frame + per_bucket).min(total);
        let mut acc = 0.0f64;
        for f in frame..end {
            for ch in 0..channels {
                let s = source.sample(f as i64, ch) as f64;
                acc += s * s;
            }
        }
        let n = ((end - frame) * channels).max(1) as f64;
        out.push((acc / n).sqrt() as f32);
        frame = end;
    }
    out
}

pub fn scene_sense(
    source: &dyn bonaparte_audio::AudioSource,
    rate: u32,
) -> Result<SceneSense, String> {
    let envelope = envelope_of(source, rate, beats::BUCKETS_PER_SEC);
    if envelope.is_empty() {
        return Err("No audio to analyze".into());
    }
    // Silence: buckets under 8% of the 90th-percentile energy.
    let mut sorted = envelope.clone();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let loud = sorted[(sorted.len() as f32 * 0.9) as usize % sorted.len()];
    let floor = loud * 0.08;
    let bps = beats::BUCKETS_PER_SEC as f64;
    let mut silence: Vec<(f64, f64)> = Vec::new();
    let mut run_start: Option<usize> = None;
    for (i, e) in envelope.iter().enumerate() {
        if *e < floor {
            if run_start.is_none() {
                run_start = Some(i);
            }
        } else if let Some(a) = run_start.take() {
            if i - a >= bps as usize / 5 {
                // ≥100 ms counts as a real gap.
                silence.push((a as f64 / bps * 1000.0, (i - a) as f64 / bps * 1000.0));
            }
        }
    }
    if let Some(a) = run_start {
        let i = envelope.len();
        if i - a >= bps as usize / 5 {
            silence.push((a as f64 / bps * 1000.0, (i - a) as f64 / bps * 1000.0));
        }
    }
    let silence_annotation = silence
        .iter()
        .map(|(start, dur)| {
            let total = (start / 1000.0).floor() as u64;
            format!(
                "silence~~{}:{}+{:.1}~~",
                total / 60,
                total % 60,
                (dur / 1000.0 * 10.0).round() / 10.0
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
    let grid = beats::detect(&envelope, beats::BUCKETS_PER_SEC).ok();
    Ok(SceneSense {
        bpm: grid.as_ref().map(|g| g.bpm),
        beats: grid.as_ref().map(|g| g.beats_ms.len()).unwrap_or(0),
        silence,
        silence_annotation,
    })
}

// ---------------------------------------------------------------------------
// HTTP: minimal multipart + JSON posting on ureq (rustls, no OpenSSL)
// ---------------------------------------------------------------------------

#[cfg(target_arch = "wasm32")]
pub fn transcribe(
    _source: &dyn bonaparte_audio::AudioSource,
    _api_key: &str,
) -> Result<Vec<KayaWord>, String> {
    Err("Kaya transcription runs on the desktop engine".into())
}

#[cfg(target_arch = "wasm32")]
pub fn narrate(
    _text: &str,
    _provider: &str,
    _api_key: &str,
    _voice: &str,
) -> Result<Narration, String> {
    Err("The narrator runs on the desktop engine".into())
}

#[cfg(not(target_arch = "wasm32"))]
fn post_multipart(url: &str, key: &str, wav: &[u8]) -> Result<Value, String> {
    let boundary = "bonaparte-kaya-7f3d9a2b";
    let mut body = Vec::new();
    let mut part = |name: &str, value: &str| {
        body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n").as_bytes());
    };
    part("model", "whisper-1");
    part("response_format", "verbose_json");
    part("timestamp_granularities[]", "word");
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"audio.wav\"\r\nContent-Type: audio/wav\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(wav);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let response = ureq::post(url)
        .set("Authorization", &format!("Bearer {key}"))
        .set(
            "Content-Type",
            &format!("multipart/form-data; boundary={boundary}"),
        )
        .send_bytes(&body)
        .map_err(|e| format!("Whisper request failed: {e}"))?;
    response
        .into_json::<Value>()
        .map_err(|e| format!("Whisper response unreadable: {e}"))
}

/// 16 kHz mono WAV of the source's first `max_secs` — Whisper's native diet.
pub fn whisper_wav(
    source: &dyn bonaparte_audio::AudioSource,
    max_secs: f64,
) -> Result<Vec<u8>, String> {
    let rate = 16_000u32;
    let channels = source.channels().max(1) as usize;
    let total_frames = source.frames().min((max_secs * rate as f64) as u64);
    let mut pcm = Vec::with_capacity(total_frames as usize);
    let step = source.frames() as f64 / total_frames as f64;
    for i in 0..total_frames as usize {
        // Nearest-neighbour downsample is plenty for speech recognition.
        let src_frame = (i as f64 * step) as i64;
        let mut acc = 0.0f32;
        for ch in 0..channels {
            acc += source.sample(src_frame, ch);
        }
        pcm.push(acc / channels as f32);
    }
    let data_len = pcm.len() * 2;
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
    for s in &pcm {
        let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        wav.extend_from_slice(&v.to_le_bytes());
    }
    Ok(wav)
}

/// Kaya transcription: OpenAI Whisper with word timestamps → `KayaWord`s.
#[cfg(not(target_arch = "wasm32"))]
pub fn transcribe(
    source: &dyn bonaparte_audio::AudioSource,
    api_key: &str,
) -> Result<Vec<KayaWord>, String> {
    if api_key.len() < 20 {
        return Err("Kaya needs an OpenAI API key (plugin settings) for transcription".into());
    }
    let wav = whisper_wav(source, 600.0)?;
    let reply = post_multipart(
        "https://api.openai.com/v1/audio/transcriptions",
        api_key,
        &wav,
    )?;
    let mut words = Vec::new();
    if let Some(list) = reply.get("words").and_then(|w| w.as_array()) {
        for w in list {
            let word = w.get("word").and_then(|x| x.as_str()).unwrap_or("").trim();
            let start = w.get("start").and_then(|x| x.as_f64()).unwrap_or(0.0);
            let end = w.get("end").and_then(|x| x.as_f64()).unwrap_or(start);
            if word.is_empty() {
                continue;
            }
            words.push(KayaWord {
                word: word.to_owned(),
                start_ms: start * 1000.0,
                dur_ms: ((end - start).max(0.05)) * 1000.0,
            });
        }
    }
    if words.is_empty() {
        // Fall back to plain text: one segment, the whole span.
        if let Some(text) = reply.get("text").and_then(|t| t.as_str()) {
            let span = source.frames() as f64 / 16_000.0 * 1000.0;
            let per = span / text.split_whitespace().count().max(1) as f64;
            for (i, word) in text.split_whitespace().enumerate() {
                words.push(KayaWord {
                    word: word.to_owned(),
                    start_ms: i as f64 * per,
                    dur_ms: per,
                });
            }
        }
    }
    if words.is_empty() {
        return Err("Whisper heard no words".into());
    }
    Ok(words)
}

// ---------------------------------------------------------------------------
// The Narrator: Sarvam AI Bulbul or ElevenLabs, BYO key
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct Narration {
    pub audio: Vec<u8>,
    pub provider: &'static str,
}

/// Speak `text` with the requested provider. Returns encoded audio
/// (wav from Sarvam, mp3 from ElevenLabs) — the normal decode pipeline
/// takes it from there.
#[cfg(not(target_arch = "wasm32"))]
pub fn narrate(
    text: &str,
    provider: &str,
    api_key: &str,
    voice: &str,
) -> Result<Narration, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("The narrator needs something to say".into());
    }
    match provider {
        "sarvam" => {
            if api_key.len() < 8 {
                return Err("Narrator needs a Sarvam AI key (plugin settings)".into());
            }
            let reply = ureq::post("https://api.sarvam.ai/text-to-speech")
                .set("api-subscription-key", api_key)
                .send_json(json!({
                    "inputs": [text],
                    "target_language_code": "en-IN",
                    "speaker": if voice.is_empty() { "anushka" } else { voice },
                    "speech_sample_rate": 48000,
                    "model": "bulbul:v2",
                }))
                .map_err(|e| format!("Sarvam request failed: {e}"))?
                .into_json::<Value>()
                .map_err(|e| format!("Sarvam response unreadable: {e}"))?;
            let b64 = reply
                .get("audios")
                .and_then(|a| a.get(0))
                .and_then(|a| a.as_str())
                .ok_or("Sarvam returned no audio")?;
            Ok(Narration {
                audio: STANDARD.decode(b64).map_err(|e| e.to_string())?,
                provider: "sarvam-bulbul",
            })
        }
        "elevenlabs" => {
            if api_key.len() < 8 {
                return Err("Narrator needs an ElevenLabs key (plugin settings)".into());
            }
            let voice_id = if voice.is_empty() {
                "21m00Tcm4TlvDq8ikWAM"
            } else {
                voice
            };
            let url = format!("https://api.elevenlabs.io/v1/text-to-speech/{voice_id}");
            let response = ureq::post(&url)
                .set("xi-api-key", api_key)
                .set("Accept", "audio/mpeg")
                .send_json(json!({
                    "text": text,
                    "model_id": "eleven_multilingual_v2",
                }))
                .map_err(|e| format!("ElevenLabs request failed: {e}"))?;
            let mut audio = Vec::new();
            use std::io::Read as _;
            response
                .into_reader()
                .take(64 * 1024 * 1024)
                .read_to_end(&mut audio)
                .map_err(|e| format!("ElevenLabs audio unreadable: {e}"))?;
            Ok(Narration {
                audio,
                provider: "elevenlabs",
            })
        }
        other => Err(format!(
            "Unknown narrator provider '{other}' — sarvam or elevenlabs"
        )),
    }
}

/// The narration timeline entry: words the narrator will say, timed evenly
/// across the clip (word-level timing when the provider doesn't align).
pub fn narration_plan(text: &str, start_ms: f64, dur_ms: f64) -> Vec<KayaWord> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let per = dur_ms / words.len().max(1) as f64;
    words
        .iter()
        .enumerate()
        .map(|(i, w)| KayaWord {
            word: (*w).to_owned(),
            start_ms: start_ms + i as f64 * per,
            dur_ms: per,
        })
        .collect()
}

/// True when the arrangement already carries a clip at this position —
/// used to keep the Narrator from stacking takes on the same lane entry.
pub fn lane_conflict(arrangement: &AudioArrangement, track_id: &str, frame: i64) -> bool {
    arrangement
        .tracks
        .iter()
        .find(|t| t.id == track_id)
        .is_some_and(|t| {
            t.clips
                .iter()
                .any(|c| frame >= c.start_frame && frame < c.start_frame + c.duration_frames as i64)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(w: &str, start_ms: f64, dur_ms: f64) -> KayaWord {
        KayaWord {
            word: w.into(),
            start_ms,
            dur_ms,
        }
    }

    #[test]
    fn annotation_matches_the_kaya_format() {
        let words = vec![word("Hi", 80_000.0, 2_000.0), word("I", 87_000.0, 400.0)];
        assert_eq!(annotate(&words), "Hi~~1:20+2~~ I~~1:27+0.4~~");
        // And it parses back losslessly (within format precision).
        let parsed = parse_annotation(&annotate(&words));
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].word, "Hi");
        assert!((parsed[0].start_ms - 80_000.0).abs() < 1.0);
        assert!((parsed[0].dur_ms - 2_000.0).abs() < 100.0);
    }

    #[test]
    fn parse_survives_stray_text_and_bad_blocks() {
        let parsed = parse_annotation("intro Hi~~1:20+2~~ ~~garbage~~ out");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].word, "intro Hi");
    }

    #[test]
    fn narration_plan_times_every_word() {
        let plan = narration_plan("hi i am ansh", 10_000.0, 4_000.0);
        assert_eq!(plan.len(), 4);
        assert_eq!(plan[0].word, "hi");
        assert!((plan[0].start_ms - 10_000.0).abs() < 1.0);
        assert!((plan[3].start_ms - 10_000.0 - 3_000.0).abs() < 1.0);
        assert!((plan[3].dur_ms - 1_000.0).abs() < 1.0);
    }

    #[test]
    fn manifests_declare_kaya_and_its_settings() {
        let manifests = plugin_manifests();
        let kaya = manifests.iter().find(|m| m.id == "kaya").expect("kaya");
        assert!(kaya.capabilities.contains(&"transcribe"));
        assert!(kaya.capabilities.contains(&"narrate"));
        assert!(kaya
            .settings
            .iter()
            .any(|s| s.key == "openaiKey" && s.kind == "secret"));
        assert!(kaya.settings.iter().any(|s| s.key == "sarvamKey"));
        assert!(kaya.settings.iter().any(|s| s.key == "elevenLabsKey"));
    }

    #[test]
    fn narrate_demands_keys_before_network() {
        let err = narrate("hello", "sarvam", "", "").unwrap_err();
        assert!(err.contains("Sarvam"), "{err}");
        let err = narrate("hello", "elevenlabs", "x", "").unwrap_err();
        assert!(err.contains("ElevenLabs"), "{err}");
        let err = narrate("hello", "nope", "key", "").unwrap_err();
        assert!(err.contains("Unknown narrator provider"), "{err}");
    }

    #[test]
    fn transcribe_demands_keys_before_network() {
        let source = bonaparte_audio::PcmSource {
            channels: 1,
            samples: vec![0.0; 16_000],
        };
        let err = transcribe(&source, "short").unwrap_err();
        assert!(err.contains("OpenAI"), "{err}");
    }
}
