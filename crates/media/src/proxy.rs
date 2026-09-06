//! Half-resolution proxy video generation for responsive viewport playback.
//!
//! Purpose:
//! - Background transcoding of high-bitrate or high-resolution (4K/8K) clips.
//! - Downsamples spatially by half (scale=trunc(iw/4)*2:trunc(ih/4)*2).
//! - Fast encode settings (-preset ultrafast -crf 28) to minimize CPU/disk pressure.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, thiserror::Error)]
pub enum ProxyError {
    #[error("I/O error for proxy {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("FFmpeg proxy transcode failed for {path}: {message}")]
    TranscodeFailed { path: PathBuf, message: String },
    #[error("Proxy output file was not created or empty at {0}")]
    EmptyOutput(PathBuf),
}

/// Information about a generated proxy asset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyInfo {
    pub input_path: PathBuf,
    pub proxy_path: PathBuf,
    pub original_width: u32,
    pub original_height: u32,
    pub proxy_width: u32,
    pub proxy_height: u32,
}

/// Transcode a video into a half-resolution proxy clip.
pub fn generate_proxy(
    input_path: impl AsRef<Path>,
    output_path: impl AsRef<Path>,
    original_width: u32,
    original_height: u32,
) -> Result<ProxyInfo, ProxyError> {
    let input = input_path.as_ref();
    let output = output_path.as_ref();

    if !input.exists() {
        return Err(ProxyError::Io {
            path: input.to_path_buf(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "Input video not found"),
        });
    }

    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ProxyError::Io {
            path: parent.to_path_buf(),
            source: e,
        })?;
    }

    // Half-resolution calculation ensuring even pixel dimensions for H.264
    let proxy_width = ((original_width / 4) * 2).max(2);
    let proxy_height = ((original_height / 4) * 2).max(2);

    let child = Command::new("ffmpeg")
        .arg("-y")
        .arg("-v")
        .arg("error")
        .arg("-i")
        .arg(input)
        .arg("-vf")
        .arg("scale=trunc(iw/4)*2:trunc(ih/4)*2")
        .arg("-c:v")
        .arg("libx264")
        .arg("-preset")
        .arg("ultrafast")
        .arg("-crf")
        .arg("28")
        .arg("-c:a")
        .arg("copy")
        .arg(output)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| ProxyError::Io {
            path: input.to_path_buf(),
            source: e,
        })?;

    if !child.status.success() {
        let stderr = String::from_utf8_lossy(&child.stderr);
        return Err(ProxyError::TranscodeFailed {
            path: input.to_path_buf(),
            message: stderr.into_owned(),
        });
    }

    let meta = std::fs::metadata(output).map_err(|e| ProxyError::Io {
        path: output.to_path_buf(),
        source: e,
    })?;

    if meta.len() == 0 {
        return Err(ProxyError::EmptyOutput(output.to_path_buf()));
    }

    Ok(ProxyInfo {
        input_path: input.to_path_buf(),
        proxy_path: output.to_path_buf(),
        original_width,
        original_height,
        proxy_width,
        proxy_height,
    })
}
