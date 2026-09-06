//! Fast frame decoder using crash-isolated FFmpeg child processes.
//!
//! Features:
//! - Sub-second input seeking (-ss before -i for fast keyframe/packet seek).
//! - Direct memory output: raw RGBA8 stream piped from child stdout into Vec<u8>.
//! - Crash-isolated: any corrupt or malformed codec payload terminates the child process,
//!   leaving the host process completely safe and unaffected.

use bonaparte_model::Time;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("I/O error accessing {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("FFmpeg decode process failed for {path}: {message}")]
    ProcessFailed { path: PathBuf, message: String },
    #[error("Incomplete frame decoded: expected {expected} bytes ({width}x{height} RGBA), got {actual} bytes")]
    IncompleteFrame {
        expected: usize,
        actual: usize,
        width: u32,
        height: u32,
    },
}

/// A single decoded video or image frame in 8-bit RGBA row-major format.
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl DecodedFrame {
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Self {
        Self {
            width,
            height,
            rgba,
        }
    }

    pub fn byte_len(&self) -> usize {
        self.rgba.len()
    }
}

/// Decode a single frame at the specified `time` into raw RGBA8 bytes.
///
/// Output dimensions are specified by `width` and `height`.
pub fn decode_frame(
    path: impl AsRef<Path>,
    time: Time,
    width: u32,
    height: u32,
) -> Result<DecodedFrame, DecodeError> {
    let path = path.as_ref();
    if !path.exists() {
        return Err(DecodeError::Io {
            path: path.to_path_buf(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "File not found"),
        });
    }

    let sec = time.as_secs_f64().max(0.0);
    let expected_bytes = (width * height * 4) as usize;

    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-v").arg("error");

    // Fast seek before input
    if sec > 0.0 {
        cmd.arg("-ss").arg(format!("{:.3}", sec));
    }

    cmd.arg("-i").arg(path).arg("-frames:v").arg("1");

    if width > 0 && height > 0 {
        cmd.arg("-s").arg(format!("{}x{}", width, height));
    }

    cmd.arg("-f")
        .arg("rawvideo")
        .arg("-pix_fmt")
        .arg("rgba")
        .arg("-")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let output = cmd.output().map_err(|e| DecodeError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DecodeError::ProcessFailed {
            path: path.to_path_buf(),
            message: stderr.into_owned(),
        });
    }

    if output.stdout.len() != expected_bytes {
        return Err(DecodeError::IncompleteFrame {
            expected: expected_bytes,
            actual: output.stdout.len(),
            width,
            height,
        });
    }

    Ok(DecodedFrame {
        width,
        height,
        rgba: output.stdout,
    })
}
