//! Streaming direct compositor-to-FFmpeg MP4 video export.
//!
//! Frames are produced one at a time and written to an FFmpeg child process.
//! This avoids retaining the entire sequence, but does not bound allocations
//! inside the render callback or encoder. FFmpeg runs with the caller's OS
//! privileges; a separate process is not a security sandbox. Output uses libx264,
//! YUV420p and faststart; codec profile selection is left to the encoder.

use bonaparte_model::{FrameRate, Time};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("I/O error during export to `{path}`: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("FFmpeg export process failed: {0}")]
    EncodingFailed(String),
    #[error("Frame render callback failed at frame {frame_idx} (time: {time}): {message}")]
    RenderCallbackFailed {
        frame_idx: usize,
        time: Time,
        message: String,
    },
    #[error("Exported MP4 output file is empty or missing at `{0}`")]
    EmptyOutput(PathBuf),
}

/// Configuration parameters for direct MP4 streaming export.
#[derive(Debug, Clone)]
pub struct ExportConfig {
    pub output_path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub fps: FrameRate,
    pub total_frames: usize,
    pub crf: u32,
    pub preset: String,
}

impl ExportConfig {
    pub fn new(
        output_path: impl AsRef<Path>,
        width: u32,
        height: u32,
        fps: FrameRate,
        total_frames: usize,
    ) -> Self {
        Self {
            output_path: output_path.as_ref().to_path_buf(),
            width,
            height,
            fps,
            total_frames,
            crf: 18, // High visual quality default
            preset: "medium".to_string(),
        }
    }

    pub fn with_crf(mut self, crf: u32) -> Self {
        self.crf = crf;
        self
    }

    pub fn with_preset(mut self, preset: impl Into<String>) -> Self {
        self.preset = preset.into();
        self
    }
}

/// Statistics reported upon completion of an export run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportStats {
    pub frames_exported: usize,
    pub bytes_streamed: u64,
    pub output_file_size: u64,
    pub output_path: PathBuf,
}

/// Stream rendered frames one by one into FFmpeg stdin to produce an MP4 video file.
///
/// render_frame is invoked synchronously for each frame index 0..config.total_frames.
/// Only a single frame buffer exists in memory at any point in time.
pub fn export_mp4_stream<F>(
    config: &ExportConfig,
    mut render_frame: F,
) -> Result<ExportStats, ExportError>
where
    F: FnMut(usize, Time) -> Result<Vec<u8>, String>,
{
    if let Some(parent) = config.output_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ExportError::Io {
            path: parent.to_path_buf(),
            source: e,
        })?;
    }

    if config.width == 0
        || config.height == 0
        || config.width % 2 != 0
        || config.height % 2 != 0
        || config.width > 8192
        || config.height > 8192
        || config.fps.num == 0
        || config.fps.den == 0
        || config.total_frames == 0
    {
        return Err(ExportError::EncodingFailed(
            "Invalid dimensions, duration or frame rate".into(),
        ));
    }
    let expected_frame_bytes = (config.width * config.height * 4) as usize;

    let mut child = Command::new("ffmpeg")
        .arg("-y")
        .arg("-v")
        .arg("error")
        .arg("-f")
        .arg("rawvideo")
        .arg("-pix_fmt")
        .arg("rgba")
        .arg("-s")
        .arg(format!("{}x{}", config.width, config.height))
        .arg("-r")
        .arg(format!("{}/{}", config.fps.num, config.fps.den))
        .arg("-i")
        .arg("-")
        .arg("-c:v")
        .arg("libx264")
        .arg("-pix_fmt")
        .arg("yuv420p")
        .arg("-preset")
        .arg(&config.preset)
        .arg("-crf")
        .arg(format!("{}", config.crf))
        .arg("-movflags")
        .arg("+faststart")
        .arg(&config.output_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| ExportError::Io {
            path: config.output_path.clone(),
            source: e,
        })?;

    let mut child_stdin = child.stdin.take().expect("stdin must be piped");
    // Reap FFmpeg on every early return (bad frames, render failures, broken pipes).
    let mut process = ExportChild(Some(child));
    let child = process.0.as_mut().expect("running process");
    // Drain stderr concurrently; never let a full stderr pipe block an encoder.
    let stderr = child.stderr.take();
    let stderr_reader = std::thread::spawn(move || {
        use std::io::Read;
        let mut saved = Vec::new();
        if let Some(mut stream) = stderr {
            let mut buf = [0u8; 4096];
            while let Ok(n) = stream.read(&mut buf) {
                if n == 0 {
                    break;
                }
                let keep = n.min(65536usize.saturating_sub(saved.len()));
                saved.extend_from_slice(&buf[..keep]);
            }
        }
        saved
    });
    let mut bytes_streamed = 0u64;

    for frame_idx in 0..config.total_frames {
        let time = config.fps.from_frame(frame_idx as i64);
        let frame_rgba =
            render_frame(frame_idx, time).map_err(|msg| ExportError::RenderCallbackFailed {
                frame_idx,
                time,
                message: msg,
            })?;

        if frame_rgba.len() != expected_frame_bytes {
            return Err(ExportError::EncodingFailed(format!(
                "Render callback produced invalid buffer length at frame {}: expected {} bytes, got {}",
                frame_idx,
                expected_frame_bytes,
                frame_rgba.len()
            )));
        }

        child_stdin
            .write_all(&frame_rgba)
            .map_err(|e| ExportError::Io {
                path: config.output_path.clone(),
                source: e,
            })?;

        bytes_streamed += frame_rgba.len() as u64;
    }

    drop(child_stdin);

    let output = process
        .0
        .take()
        .expect("running process")
        .wait_with_output()
        .map_err(|e| ExportError::Io {
            path: config.output_path.clone(),
            source: e,
        })?;

    let stderr_bytes = stderr_reader.join().unwrap_or_default();
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&stderr_bytes);
        return Err(ExportError::EncodingFailed(stderr.into_owned()));
    }

    let meta = std::fs::metadata(&config.output_path).map_err(|e| ExportError::Io {
        path: config.output_path.clone(),
        source: e,
    })?;

    if meta.len() == 0 {
        return Err(ExportError::EmptyOutput(config.output_path.clone()));
    }

    Ok(ExportStats {
        frames_exported: config.total_frames,
        bytes_streamed,
        output_file_size: meta.len(),
        output_path: config.output_path.clone(),
    })
}

struct ExportChild(Option<std::process::Child>);
impl Drop for ExportChild {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
