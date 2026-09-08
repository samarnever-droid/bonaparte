//! Native audio acquisition and disk-backed PCM. No browser decodeAudioData
//! timing differences: every source is decoded once through the same FFmpeg path.
use base64::{engine::general_purpose::STANDARD, Engine};
use bonaparte_audio::AudioSource;
use bonaparte_model::{EmbeddedAudio, AUDIO_RATE, MAX_AUDIO_FRAMES};
use memmap2::{Mmap, MmapOptions};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex, OnceLock, Weak},
    time::{Duration, Instant},
};
/// Sanity bound only — the real sizing policy is Astra's: sources at or
/// under `EMBED_INLINE_LIMIT` ride inside the project file (portable),
/// larger ones stream from the store. 2 GiB per source.
pub const MAX_AUDIO_FILE_BYTES: usize = 2 * 1024 * 1024 * 1024;
/// Sources at or under this size embed directly in the project file so
/// single-file portability stays the default. Anything bigger is stored
/// as an Astra extent.
pub const EMBED_INLINE_LIMIT: usize = 16 * 1024 * 1024;
const FORMATS: &str = "wav,mp3,flac,ogg,aiff,mov,aac";
#[derive(Clone, Serialize, Deserialize)]
struct SourceInfo {
    frames: u64,
    channels: u16,
    original_sample_rate: u32,
    original_channels: u16,
    codec: String,
    peak: f32,
}
struct PeakLevel {
    span: u64,
    values: Vec<[f32; 4]>,
}
pub struct DecodedAudio {
    pcm: Mmap,
    info: SourceInfo,
    levels: Vec<PeakLevel>,
    _path: PathBuf,
}
impl DecodedAudio {
    /// Onset envelope for beat detection: RMS energy per time bucket,
    /// read straight off the decoded PCM (interleaved f32, machine-local
    /// decode cache). `buckets_per_sec` sets the analysis resolution —
    /// `bonaparte_audio::beats::BUCKETS_PER_SEC` (200) is the default.
    pub fn onset_envelope(&self, buckets_per_sec: u32) -> Vec<f32> {
        let ch = self.info.channels.max(1) as usize;
        let bps = buckets_per_sec.max(1) as usize;
        let per_bucket = (AUDIO_RATE as usize / bps).max(1);
        let total = self.info.frames as usize;
        let mut out = Vec::with_capacity(total.div_ceil(per_bucket));
        let mut frame = 0usize;
        while frame < total {
            let end = (frame + per_bucket).min(total);
            let mut acc = 0.0f64;
            let mut i = frame * ch;
            let stop = end * ch;
            while i < stop {
                let s = f32::from_le_bytes(self.pcm[i * 4..i * 4 + 4].try_into().unwrap_or([0; 4]))
                    as f64;
                acc += s * s;
                i += 1;
            }
            let n = (stop - frame * ch).max(1) as f64;
            out.push((acc / n).sqrt() as f32);
            frame = end;
        }
        out
    }
}

impl AudioSource for DecodedAudio {
    fn frames(&self) -> u64 {
        self.info.frames
    }
    fn channels(&self) -> u16 {
        self.info.channels
    }
    fn sample(&self, frame: i64, channel: usize) -> f32 {
        if frame < 0 || frame as u64 >= self.info.frames || channel >= self.info.channels as usize {
            return 0.0;
        }
        let i = (frame as usize * self.info.channels as usize + channel) * 4;
        f32::from_le_bytes(
            self.pcm[i..i + 4]
                .try_into()
                .expect("validated float offset"),
        )
    }
}
fn cache_root() -> Result<PathBuf, String> {
    let root = std::env::current_dir()
        .map_err(|e| e.to_string())?
        .join(".cache/audio-v1");
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    Ok(root)
}
const DISK_BUDGET: u64 = 4 * 1024 * 1024 * 1024;
fn reserve_cache(
    root: &Path,
    needed: u64,
    active: &HashMap<String, Weak<DecodedAudio>>,
) -> Result<(), String> {
    let mut files = Vec::new();
    let mut total = 0u64;
    for entry in std::fs::read_dir(root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.extension().and_then(|v| v.to_str()) != Some("f32") {
            continue;
        }
        let meta = entry.metadata().map_err(|e| e.to_string())?;
        total = total.saturating_add(meta.len());
        let key = path
            .file_stem()
            .and_then(|v| v.to_str())
            .unwrap_or("")
            .to_string();
        if active.get(&key).is_some_and(|v| v.strong_count() > 0) {
            continue;
        }
        files.push((
            meta.modified().unwrap_or(std::time::UNIX_EPOCH),
            path,
            meta.len(),
        ));
    }
    files.sort_by_key(|v| v.0);
    for (_, path, bytes) in files {
        if total.saturating_add(needed) <= DISK_BUDGET {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            let _ = std::fs::remove_file(path.with_extension("json"));
            total = total.saturating_sub(bytes);
        }
    }
    if total.saturating_add(needed) > DISK_BUDGET {
        return Err("The 4 GiB decoded-audio cache is full. Close unused audio projects before importing more sources.".into());
    }
    Ok(())
}

fn loaded() -> &'static Mutex<HashMap<String, Weak<DecodedAudio>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Weak<DecodedAudio>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}
fn run(mut cmd: Command, timeout: Duration) -> Result<Vec<u8>, String> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("FFmpeg tools are required for audio import: {e}"))?;
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    fn drain(mut stream: impl Read, cap: usize) -> Vec<u8> {
        let mut saved = Vec::new();
        let mut buf = [0u8; 8192];
        loop {
            match stream.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let keep = n.min(cap.saturating_sub(saved.len()));
                    saved.extend_from_slice(&buf[..keep]);
                }
            }
        }
        saved
    }
    let out = std::thread::spawn(move || drain(stdout, 1024 * 1024));
    let err = std::thread::spawn(move || drain(stderr, 65536));
    let start = Instant::now();
    let result = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err("Audio decoding exceeded its time limit".to_string());
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(e.to_string());
            }
        }
    };
    let stdout = out.join().map_err(|_| "Audio probe reader failed")?;
    let stderr = err.join().map_err(|_| "Audio decoder reader failed")?;
    let status = result?;
    if !status.success() {
        return Err(format!(
            "Audio decoding failed: {}",
            String::from_utf8_lossy(&stderr)
        ));
    }
    Ok(stdout)
}
fn analyze(pcm: Mmap, mut info: SourceInfo, path: PathBuf) -> Result<DecodedAudio, String> {
    let mut levels = vec![];
    let mut values = Vec::with_capacity(info.frames.div_ceil(256) as usize);
    let mut peak = 0.0f32;
    for block in 0..info.frames.div_ceil(256) {
        let mut p = [0.0f32; 4];
        let start = block * 256;
        let end = (start + 256).min(info.frames);
        for frame in start..end {
            for channel in 0..info.channels as usize {
                let i = (frame as usize * info.channels as usize + channel) * 4;
                let v = f32::from_le_bytes(pcm[i..i + 4].try_into().unwrap());
                if !v.is_finite() || v.abs() > 64.0 {
                    return Err(
                        "Audio contains non-finite or excessively high floating-point samples"
                            .into(),
                    );
                }
                peak = peak.max(v.abs());
                p[channel * 2] = p[channel * 2].min(v);
                p[channel * 2 + 1] = p[channel * 2 + 1].max(v);
            }
        }
        if info.channels == 1 {
            p[2] = p[0];
            p[3] = p[1];
        }
        values.push(p);
    }
    levels.push(PeakLevel { span: 256, values });
    while levels.last().unwrap().values.len() > 1 {
        let prev = levels.last().unwrap();
        let values = prev
            .values
            .chunks(4)
            .map(|part| {
                let mut p = [0.0f32; 4];
                for v in part {
                    p[0] = p[0].min(v[0]);
                    p[1] = p[1].max(v[1]);
                    p[2] = p[2].min(v[2]);
                    p[3] = p[3].max(v[3]);
                }
                p
            })
            .collect();
        let span = prev.span * 4;
        levels.push(PeakLevel { span, values });
    }
    info.peak = peak;
    Ok(DecodedAudio {
        pcm,
        info,
        levels,
        _path: path,
    })
}
fn mapped(path: &Path, info: SourceInfo) -> Result<DecodedAudio, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let length = file.metadata().map_err(|e| e.to_string())?.len();
    if info.frames == 0
        || info.frames > MAX_AUDIO_FRAMES
        || !(1..=2).contains(&info.channels)
        || length != info.frames * info.channels as u64 * 4
    {
        return Err("Decoded audio cache dimensions are invalid".into());
    }
    // The content-addressed file is complete, immutable, and retained for the
    // lifetime of this map. No code writes/truncates an existing valid PCM file.
    let pcm = unsafe { MmapOptions::new().map(&file) }.map_err(|e| e.to_string())?;
    analyze(pcm, info, path.into())
}
pub fn decode(bytes: &[u8]) -> Result<(EmbeddedAudio, Arc<DecodedAudio>), String> {
    if bytes.is_empty() || bytes.len() > MAX_AUDIO_FILE_BYTES {
        return Err("Audio files must be nonempty and at most 2 GiB".into());
    }
    // Small sources embed inline (portable project by default); bigger ones
    // stream through the Astra store and ride the project as a hash extent.
    if bytes.len() > EMBED_INLINE_LIMIT {
        let report = astra()
            .put_bytes(bytes)
            .map_err(|e| format!("Astra store write failed: {e}"))?;
        let (audio, source) = decode_plain(bytes)?;
        let external = EmbeddedAudio {
            data_base64: Arc::from(""),
            astra_chunks: Some(Arc::from(report.chunks.into_boxed_slice())),
            sha256: audio.sha256,
            frames: audio.frames,
            channels: audio.channels,
            sample_rate: audio.sample_rate,
            original_sample_rate: audio.original_sample_rate,
            original_channels: audio.original_channels,
            codec: audio.codec,
            peak: audio.peak,
            beat_grid: audio.beat_grid,
            kaya_words: audio.kaya_words,
        };
        return Ok((external, source));
    }
    decode_plain(bytes)
}

/// The Astra global store accessor (media stays engine-agnostic).
fn astra() -> &'static astra::Store {
    astra::Store::global()
}

fn decode_plain(bytes: &[u8]) -> Result<(EmbeddedAudio, Arc<DecodedAudio>), String> {
    if bytes.is_empty() {
        return Err("Audio files must be nonempty".into());
    }
    let hash = format!("{:x}", Sha256::digest(bytes));
    // Serialize cache creation, not playback. The original bytes are immutable.
    let mut cache = loaded()
        .lock()
        .map_err(|_| "Audio decoder cache unavailable")?;
    if let Some(source) = cache.get(&hash).and_then(Weak::upgrade) {
        return Ok((metadata(bytes, &hash, &source.info), source));
    }
    let root = cache_root()?;
    let path = root.join(format!("{hash}.f32"));
    let meta = root.join(format!("{hash}.json"));
    if path.is_file() && meta.is_file() {
        if let Ok(info) = std::fs::read(&meta)
            .ok()
            .and_then(|v| serde_json::from_slice::<SourceInfo>(&v).ok())
            .ok_or(())
        {
            if let Ok(source) = mapped(&path, info) {
                let source = Arc::new(source);
                cache.insert(hash.clone(), Arc::downgrade(&source));
                return Ok((metadata(bytes, &hash, &source.info), source));
            }
        }
    }
    let mut encoded = tempfile::NamedTempFile::new_in(&root).map_err(|e| e.to_string())?;
    encoded.write_all(bytes).map_err(|e| e.to_string())?;
    let mut probe = Command::new("ffprobe");
    probe
        .args([
            "-v",
            "error",
            "-protocol_whitelist",
            "file,pipe",
            "-format_whitelist",
            FORMATS,
            "-select_streams",
            "a:0",
            "-show_entries",
            "stream=sample_rate,channels,codec_name,duration:format=duration",
            "-of",
            "json",
        ])
        .arg(encoded.path());
    let data = run(probe, Duration::from_secs(30))?;
    let json: serde_json::Value =
        serde_json::from_slice(&data).map_err(|e| format!("Invalid audio probe: {e}"))?;
    let stream = json["streams"]
        .as_array()
        .and_then(|s| s.first())
        .ok_or("No decodable audio stream was found")?;
    let sample_rate = stream["sample_rate"]
        .as_str()
        .and_then(|v| v.parse::<u32>().ok())
        .ok_or("Invalid source sample rate")?;
    let channels = stream["channels"]
        .as_u64()
        .ok_or("Invalid source channel count")? as u16;
    if !(1..=2).contains(&channels) {
        return Err("This audio milestone supports mono and stereo; multichannel sources are not silently downmixed".into());
    }
    if !(8000..=384000).contains(&sample_rate) {
        return Err("Source sample rate must be 8–384 kHz".into());
    }
    let duration = stream["duration"]
        .as_str()
        .or_else(|| json["format"]["duration"].as_str())
        .and_then(|v| v.parse::<f64>().ok());
    if duration.is_some_and(|d| !d.is_finite() || d > 3600.01) {
        return Err("Audio sources are limited to one hour".into());
    }
    let limit = MAX_AUDIO_FRAMES * channels as u64 * 4;
    let reserved = duration
        .map(|d| ((d.max(0.0) + 1.0) * AUDIO_RATE as f64 * channels as f64 * 4.0) as u64)
        .unwrap_or(limit)
        .min(limit);
    reserve_cache(&root, reserved, &cache)?;
    let pcm = tempfile::NamedTempFile::new_in(&root).map_err(|e| e.to_string())?;
    let mut decode = Command::new("ffmpeg");
    decode
        .args([
            "-y",
            "-v",
            "error",
            "-nostdin",
            "-protocol_whitelist",
            "file,pipe",
            "-format_whitelist",
            FORMATS,
            "-i",
        ])
        .arg(encoded.path())
        .args([
            "-map",
            "0:a:0",
            "-vn",
            "-sn",
            "-dn",
            "-af",
            "aresample=48000:resampler=soxr:precision=28",
            "-ar",
            "48000",
            "-ac",
        ])
        .arg(channels.to_string())
        .args(["-c:a", "pcm_f32le", "-f", "f32le", "-fs"])
        .arg((limit + 16).to_string())
        .arg(pcm.path());
    run(decode, Duration::from_secs(180))?;
    let length = pcm.as_file().metadata().map_err(|e| e.to_string())?.len();
    if length == 0 || length > limit || length % (channels as u64 * 4) != 0 {
        return Err(
            "Decoded audio is empty, malformed or exceeds the one-hour source limit".into(),
        );
    }
    reserve_cache(&root, length, &cache)?;
    let info = SourceInfo {
        frames: length / (channels as u64 * 4),
        channels,
        original_sample_rate: sample_rate,
        original_channels: channels,
        codec: stream["codec_name"].as_str().unwrap_or("audio").to_string(),
        peak: 0.0,
    };
    // Validate samples before publishing the immutable cache file.
    let check = mapped(pcm.path(), info)?;
    let info = check.info.clone();
    drop(check);
    pcm.persist(&path).map_err(|e| e.to_string())?;
    let source = Arc::new(mapped(&path, info.clone())?);
    let mut temp_meta = tempfile::NamedTempFile::new_in(&root).map_err(|e| e.to_string())?;
    temp_meta
        .write_all(&serde_json::to_vec(&info).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    temp_meta.persist(&meta).map_err(|e| e.to_string())?;
    cache.retain(|_, value| value.strong_count() > 0);
    cache.insert(hash.clone(), Arc::downgrade(&source));
    Ok((metadata(bytes, &hash, &source.info), source))
}
fn metadata(bytes: &[u8], hash: &str, info: &SourceInfo) -> EmbeddedAudio {
    EmbeddedAudio {
        data_base64: Arc::from(STANDARD.encode(bytes)),
        astra_chunks: None,
        beat_grid: None,
        kaya_words: None,
        sha256: hash.into(),
        frames: info.frames,
        channels: info.channels,
        sample_rate: AUDIO_RATE,
        original_sample_rate: info.original_sample_rate,
        original_channels: info.original_channels,
        codec: info.codec.clone(),
        peak: info.peak,
    }
}
pub fn decode_embedded(audio: &EmbeddedAudio) -> Result<Arc<DecodedAudio>, String> {
    // Store-backed source: reassemble the extent through Astra's hot cache.
    if audio.data_base64.is_empty() {
        let Some(extent) = &audio.astra_chunks else {
            return Err("Audio source has neither inline data nor an Astra extent".into());
        };
        let mut bytes = Vec::new();
        for chunk in extent.iter() {
            let Some(data) = astra().read_chunk(chunk) else {
                return Err(format!("Astra chunk {chunk} is missing from the store"));
            };
            bytes.extend_from_slice(&data);
        }
        let (actual, source) = decode_plain(&bytes)?;
        if actual.sha256 != audio.sha256 {
            return Err("Astra extent does not match the recorded source hash".into());
        }
        return Ok(source);
    }
    if audio.data_base64.len() > MAX_AUDIO_FILE_BYTES.div_ceil(3) * 4 {
        return Err("Encoded audio exceeds the import limit".into());
    }
    let bytes = STANDARD
        .decode(audio.data_base64.as_bytes())
        .map_err(|e| format!("Invalid audio encoding: {e}"))?;
    let (actual, source) = decode(&bytes)?;
    if actual.sha256 != audio.sha256
        || actual.frames != audio.frames
        || actual.channels != audio.channels
        || actual.sample_rate != audio.sample_rate
        || actual.original_sample_rate != audio.original_sample_rate
        || actual.original_channels != audio.original_channels
        || actual.codec != audio.codec
        || (actual.peak - audio.peak).abs() > 0.00001
    {
        return Err("Embedded audio metadata does not match its decoded source".into());
    }
    Ok(source)
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Waveform {
    pub channels: u16,
    pub start_frame: u64,
    pub end_frame: u64,
    pub peaks: Vec<[f32; 4]>,
}
impl DecodedAudio {
    pub fn waveform(&self, start: u64, end: u64, points: usize) -> Result<Waveform, String> {
        if start >= end || end > self.info.frames || points == 0 || points > 4096 {
            return Err("Waveform requests need a valid source range and 1–4096 points".into());
        }
        let mut peaks = Vec::with_capacity(points);
        for i in 0..points {
            let from = start + ((end - start) as u128 * i as u128 / points as u128) as u64;
            let to = (start + ((end - start) as u128 * (i + 1) as u128 / points as u128) as u64)
                .max(from + 1)
                .min(end);
            let mut peak = [0.0f32; 4];
            let mut frame = from;
            while frame < to {
                if let Some(level) = self
                    .levels
                    .iter()
                    .rev()
                    .find(|l| frame % l.span == 0 && frame + l.span <= to)
                {
                    if let Some(p) = level.values.get((frame / level.span) as usize) {
                        for c in 0..2 {
                            peak[c * 2] = peak[c * 2].min(p[c * 2]);
                            peak[c * 2 + 1] = peak[c * 2 + 1].max(p[c * 2 + 1]);
                        }
                    }
                    frame += level.span;
                } else {
                    let stop = ((frame / 256 + 1) * 256).min(to);
                    for f in frame..stop {
                        for c in 0..self.info.channels as usize {
                            let v = self.sample(f as i64, c);
                            peak[c * 2] = peak[c * 2].min(v);
                            peak[c * 2 + 1] = peak[c * 2 + 1].max(v);
                        }
                    }
                    frame = stop;
                }
            }
            if self.info.channels == 1 {
                peak[2] = peak[0];
                peak[3] = peak[1];
            }
            peaks.push(peak);
        }
        Ok(Waveform {
            channels: self.info.channels,
            start_frame: start,
            end_frame: end,
            peaks,
        })
    }
}
/// 32-bit float WAV retains mix headroom. AAC/device playback may clip if the
/// meters exceed 0 dBFS; no undisclosed normalization/limiter is applied.
pub fn write_mix_wav(
    plan: &bonaparte_audio::MixPlan,
    path: &Path,
    start_frame: u64,
    frames: u64,
) -> Result<(), String> {
    let bytes = frames.checked_mul(8).ok_or("Audio export is too long")?;
    if bytes > u32::MAX as u64 - 56 {
        return Err("Float WAV export exceeds the RIFF 4 GiB limit".into());
    }
    let mut file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    file.write_all(b"RIFF")
        .and_then(|_| file.write_all(&(bytes as u32 + 48).to_le_bytes()))
        .and_then(|_| file.write_all(b"WAVEfmt "))
        .and_then(|_| file.write_all(&16u32.to_le_bytes()))
        .and_then(|_| file.write_all(&3u16.to_le_bytes()))
        .and_then(|_| file.write_all(&2u16.to_le_bytes()))
        .and_then(|_| file.write_all(&AUDIO_RATE.to_le_bytes()))
        .and_then(|_| file.write_all(&(AUDIO_RATE * 8).to_le_bytes()))
        .and_then(|_| file.write_all(&8u16.to_le_bytes()))
        .and_then(|_| file.write_all(&32u16.to_le_bytes()))
        .and_then(|_| file.write_all(b"fact"))
        .and_then(|_| file.write_all(&4u32.to_le_bytes()))
        .and_then(|_| file.write_all(&(frames as u32).to_le_bytes()))
        .and_then(|_| file.write_all(b"data"))
        .and_then(|_| file.write_all(&(bytes as u32).to_le_bytes()))
        .map_err(|e| e.to_string())?;
    let mut offset = 0;
    while offset < frames {
        let n = (frames - offset).min(AUDIO_RATE as u64) as usize;
        let block = loop {
            match plan.render((start_frame + offset) as i64, n, AUDIO_RATE) {
                Ok(block) => break block,
                Err(e) if e.starts_with("DSP_WARMUP:") => continue,
                Err(e) => return Err(e),
            }
        };
        let mut out = Vec::with_capacity(n * 8);
        for sample in block.samples {
            out.extend_from_slice(&sample.to_le_bytes());
        }
        file.write_all(&out).map_err(|e| e.to_string())?;
        offset += n as u64;
    }
    file.sync_all().map_err(|e| e.to_string())
}
