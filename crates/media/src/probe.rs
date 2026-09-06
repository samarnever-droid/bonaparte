//! Media asset probing and PerceptionCard generator for bonaparte-media.
//!
//! Owns:
//! - Spawning ffprobe child processes in crash-isolated fashion.
//! - Parsing JSON metadata (duration, width, height, fps, codec).
//! - Computing import-time PerceptionCards (dominant palette, mean luma, entropy, role).

use bonaparte_model::{AssetRole, FrameRate, MediaAsset, MediaId, MediaKind, PerceptionCard, Time};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    #[error("I/O error for {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("ffprobe execution failed for {path}: {message}")]
    ProcessFailed {
        path: PathBuf,
        message: String,
    },
    #[error("JSON metadata parse error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("No video or audio stream found in {0}")]
    NoMediaStream(PathBuf),
    #[error("Perception frame extraction failed: {0}")]
    PerceptionDecodeFailed(String),
}

#[derive(Debug, Deserialize)]
struct FfprobeOutput {
    streams: Option<Vec<FfprobeStream>>,
    format: Option<FfprobeFormat>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct FfprobeStream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    r_frame_rate: Option<String>,
    avg_frame_rate: Option<String>,
    duration: Option<String>,
    pix_fmt: Option<String>,
    sample_rate: Option<String>,
    channels: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct FfprobeFormat {
    duration: Option<String>,
    size: Option<String>,
    format_name: Option<String>,
}

/// Helper to parse rational frame rates such as "30/1" or "24000/1001".
pub fn parse_rational_framerate(s: &str) -> Option<FrameRate> {
    let trimmed = s.trim();
    if let Some((num_str, den_str)) = trimmed.split_once('/') {
        let num: u32 = num_str.parse().ok()?;
        let den: u32 = den_str.parse().ok()?;
        if num > 0 && den > 0 {
            return Some(FrameRate { num, den });
        }
    } else if let Ok(fps) = trimmed.parse::<f64>() {
        if (fps - 24.0).abs() < 0.01 {
            return Some(FrameRate::FPS_24);
        } else if (fps - 25.0).abs() < 0.01 {
            return Some(FrameRate::FPS_25);
        } else if (fps - 30.0).abs() < 0.01 {
            return Some(FrameRate::FPS_30);
        } else if (fps - 60.0).abs() < 0.01 {
            return Some(FrameRate::FPS_60);
        } else if (fps - 23.976).abs() < 0.01 {
            return Some(FrameRate::NTSC_FILM);
        } else {
            let num = (fps * 1000.0).round() as u32;
            return Some(FrameRate { num, den: 1000 });
        }
    }
    None
}

/// Compute a quantitative PerceptionCard from raw RGBA8 pixels.
pub fn compute_perception_card(
    width: u32,
    height: u32,
    rgba: &[u8],
    is_video: bool,
    is_audio_only: bool,
) -> PerceptionCard {
    let total_pixels = (width * height) as usize;
    if total_pixels == 0 || rgba.is_empty() {
        let role = if is_audio_only {
            AssetRole::Audio
        } else if is_video {
            AssetRole::Video
        } else {
            AssetRole::Unknown
        };
        return PerceptionCard {
            role,
            width,
            height,
            palette: Vec::new(),
            mean_luma: 0.0,
            entropy: 0.0,
            alpha_fraction: None,
            evidence: vec!["Empty or audio-only asset".to_string()],
        };
    }

    // 1. Histogram bucketing for dominant palette (16x16x16 color space = 4096 bins)
    let mut color_bins: BTreeMap<u16, (u32, u64, u64, u64)> = BTreeMap::new();
    let mut luma_hist = [0u32; 256];
    let mut luma_sum = 0.0f64;
    let mut transparent_pixels = 0usize;
    let mut has_any_alpha = false;

    for chunk in rgba.chunks_exact(4) {
        let r = chunk[0];
        let g = chunk[1];
        let b = chunk[2];
        let a = chunk[3];

        if a < 250 {
            has_any_alpha = true;
        }
        if a < 10 {
            transparent_pixels += 1;
            continue; // Exclude fully transparent pixels from dominant visible palette
        }

        // Color quantization: 4 bits per channel
        let bin_key = (((r as u16 >> 4) & 0x0F) << 8)
            | (((g as u16 >> 4) & 0x0F) << 4)
            | ((b as u16 >> 4) & 0x0F);

        let entry = color_bins.entry(bin_key).or_insert((0, 0, 0, 0));
        entry.0 += 1;
        entry.1 += r as u64;
        entry.2 += g as u64;
        entry.3 += b as u64;

        // Rec. 601 luma
        let luma = (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32).round() as u8;
        luma_hist[luma as usize] += 1;
        luma_sum += luma as f64;
    }

    let visible_pixels = total_pixels.saturating_sub(transparent_pixels);
    let mean_luma = if visible_pixels > 0 {
        (luma_sum / (visible_pixels as f64 * 255.0)) as f32
    } else {
        0.0
    };

    // 2. Shannon Entropy (0.0..8.0 bits)
    let mut entropy = 0.0f32;
    if visible_pixels > 0 {
        let n = visible_pixels as f32;
        for &count in &luma_hist {
            if count > 0 {
                let p = count as f32 / n;
                entropy -= p * p.log2();
            }
        }
    }

    // 3. Top 5 dominant colors
    let mut sorted_bins: Vec<_> = color_bins.into_iter().collect();
    sorted_bins.sort_by_key(|b| std::cmp::Reverse(b.1 .0)); // Descending by count

    let mut palette = Vec::new();
    for (_bin, (count, r_sum, g_sum, b_sum)) in sorted_bins.into_iter().take(5) {
        if count > 0 {
            let avg_r = (r_sum / count as u64).min(255) as u8;
            let avg_g = (g_sum / count as u64).min(255) as u8;
            let avg_b = (b_sum / count as u64).min(255) as u8;
            palette.push([avg_r, avg_g, avg_b]);
        }
    }

    // 4. Alpha fraction
    let alpha_fraction = if has_any_alpha {
        Some(transparent_pixels as f32 / total_pixels as f32)
    } else {
        None
    };

    // 5. Role Classification Heuristic
    let role = if is_audio_only {
        AssetRole::Audio
    } else if is_video {
        AssetRole::Video
    } else {
        let aspect = width as f32 / height.max(1) as f32;
        let is_squareish = (0.75..=1.33).contains(&aspect);
        let is_icon_size = width <= 1024 && height <= 1024;
        let trans_frac = alpha_fraction.unwrap_or(0.0);

        if trans_frac > 0.12 || (is_squareish && is_icon_size && trans_frac > 0.05) {
            AssetRole::LogoCandidate
        } else if entropy > 5.2 {
            AssetRole::Photo
        } else {
            AssetRole::Graphic
        }
    };

    let mut evidence = Vec::new();
    evidence.push(format!("Dimensions: {}x{}", width, height));
    evidence.push(format!(
        "Mean Luma: {:.2}, Entropy: {:.2} bits",
        mean_luma, entropy
    ));
    if let Some(af) = alpha_fraction {
        evidence.push(format!("Alpha fraction: {:.1}%", af * 100.0));
    }
    evidence.push(format!("Role classification: {:?}", role));

    PerceptionCard {
        role,
        width,
        height,
        palette,
        mean_luma,
        entropy,
        alpha_fraction,
        evidence,
    }
}

/// Run ffprobe as a child process and parse media asset metadata.
pub fn probe_asset(
    path: impl AsRef<Path>,
    id: MediaId,
    name: impl Into<String>,
) -> Result<MediaAsset, ProbeError> {
    let path = path.as_ref();
    if !path.exists() {
        return Err(ProbeError::Io {
            path: path.to_path_buf(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "File not found"),
        });
    }

    let output = Command::new("ffprobe")
        .arg("-v")
        .arg("quiet")
        .arg("-print_format")
        .arg("json")
        .arg("-show_format")
        .arg("-show_streams")
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| ProbeError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ProbeError::ProcessFailed {
            path: path.to_path_buf(),
            message: stderr.into_owned(),
        });
    }

    let json_text = String::from_utf8_lossy(&output.stdout);
    let probe_data: FfprobeOutput = serde_json::from_str(&json_text)?;

    let streams = probe_data
        .streams
        .ok_or_else(|| ProbeError::NoMediaStream(path.to_path_buf()))?;

    let video_stream = streams
        .iter()
        .find(|s| s.codec_type.as_deref() == Some("video"));
    let audio_stream = streams
        .iter()
        .find(|s| s.codec_type.as_deref() == Some("audio"));

    if video_stream.is_none() && audio_stream.is_none() {
        return Err(ProbeError::NoMediaStream(path.to_path_buf()));
    }

    let format_duration_sec: Option<f64> = probe_data
        .format
        .as_ref()
        .and_then(|f| f.duration.as_deref())
        .and_then(|d| d.parse().ok());

    let (kind, width, height, is_video, is_audio_only) = if let Some(vs) = video_stream {
        let w = vs.width.unwrap_or(0);
        let h = vs.height.unwrap_or(0);

        let fps = vs
            .r_frame_rate
            .as_deref()
            .and_then(parse_rational_framerate)
            .or_else(|| {
                vs.avg_frame_rate
                    .as_deref()
                    .and_then(parse_rational_framerate)
            })
            .unwrap_or(FrameRate::FPS_30);

        let stream_dur_sec: Option<f64> = vs.duration.as_deref().and_then(|d| d.parse().ok());
        let dur_sec = stream_dur_sec.or(format_duration_sec).unwrap_or(0.0);
        let duration = Time::from_secs_f64(dur_sec);

        // Detect static image vs moving video
        let is_image = vs.codec_name.as_deref() == Some("png")
            || vs.codec_name.as_deref() == Some("mjpeg")
            || vs.codec_name.as_deref() == Some("webp")
            || (dur_sec <= 0.04 && fps.to_frame(duration) <= 1);

        if is_image {
            (MediaKind::Image, w, h, false, false)
        } else {
            (MediaKind::Video { fps, duration }, w, h, true, false)
        }
    } else {
        let dur_sec = format_duration_sec.unwrap_or(0.0);
        let duration = Time::from_secs_f64(dur_sec);
        (MediaKind::Audio { duration }, 0, 0, false, true)
    };

    // Extract a sample frame to compute PerceptionCard if visual
    let perception = if width > 0 && height > 0 {
        let sample_time = if is_video {
            // Probe near 20% or 0.5s into the clip to avoid initial black intro
            let dur = format_duration_sec.unwrap_or(1.0);
            (dur * 0.2).clamp(0.0, 1.0)
        } else {
            0.0
        };

        match extract_sample_frame_rgba(path, sample_time, width, height) {
            Ok(rgba) => Some(compute_perception_card(
                width,
                height,
                &rgba,
                is_video,
                is_audio_only,
            )),
            Err(e) => {
                // Fallback minimal perception card
                Some(PerceptionCard {
                    role: if is_video { AssetRole::Video } else { AssetRole::Graphic },
                    width,
                    height,
                    palette: Vec::new(),
                    mean_luma: 0.5,
                    entropy: 4.0,
                    alpha_fraction: None,
                    evidence: vec![format!("Sample extraction note: {}", e)],
                })
            }
        }
    } else {
        Some(compute_perception_card(0, 0, &[], is_video, is_audio_only))
    };

    Ok(MediaAsset {
        id,
        name: name.into(),
        path: Some(path.to_string_lossy().into_owned()),
        kind,
        slot: None,
        alias: None,
        perception,
    })
}

/// Helper to decode a single sample frame into raw RGBA bytes via ffmpeg.
fn extract_sample_frame_rgba(
    path: &Path,
    timestamp_sec: f64,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, ProbeError> {
    let expected_bytes = (width * height * 4) as usize;
    let output = Command::new("ffmpeg")
        .arg("-v")
        .arg("error")
        .arg("-ss")
        .arg(format!("{:.3}", timestamp_sec))
        .arg("-i")
        .arg(path)
        .arg("-frames:v")
        .arg("1")
        .arg("-f")
        .arg("rawvideo")
        .arg("-pix_fmt")
        .arg("rgba")
        .arg("-")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| ProbeError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(ProbeError::PerceptionDecodeFailed(err.into_owned()));
    }

    if output.stdout.len() != expected_bytes {
        return Err(ProbeError::PerceptionDecodeFailed(format!(
            "Expected {} bytes for {}x{} RGBA, got {}",
            expected_bytes,
            width,
            height,
            output.stdout.len()
        )));
    }

    Ok(output.stdout)
}
