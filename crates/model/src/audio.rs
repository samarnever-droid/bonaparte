//! Sample-addressed, non-destructive audio arrangement. Audio edits use 48 kHz
//! sample frames, independently of the 120 kHz video tick grid (2.5 ticks/sample).
use crate::{CompId, MediaId, Project, Time};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
pub const AUDIO_RATE: u32 = 48_000;
pub const MAX_AUDIO_FRAMES: u64 = AUDIO_RATE as u64 * 3600;
pub const MAX_AUDIO_TIMELINE: i64 = AUDIO_RATE as i64 * 86_400;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EmbeddedAudio {
    /// Original encoded file, not the decoded working PCM. Arc keeps edit snapshots cheap.
    /// Empty when the source lives in the Astra store instead (see `astra_chunks`).
    pub data_base64: Arc<str>,
    /// Astra extent list: content-addressed chunk hashes of the original
    /// encoded file, in order. `Some` only when the source exceeded the
    /// inline-embed limit and streams from the Astra store instead —
    /// projects stay light no matter how big the audio is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub astra_chunks: Option<Arc<[String]>>,
    /// Detected beat grid (Beat Cut): beat times in milliseconds,
    /// ascending. Absent until the user runs beat detection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub beat_grid: Option<Arc<[f64]>>,
    pub sha256: String,
    pub frames: u64,
    pub channels: u16,
    pub sample_rate: u32,
    pub original_sample_rate: u32,
    pub original_channels: u16,
    pub codec: String,
    pub peak: f32,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudioPoint {
    pub frame: i64,
    pub value: f64,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum FadeShape {
    Linear,
    #[default]
    EqualPower,
}
/// Fade coordinates are clip-local and may extend beyond a split/trimmed clip.
/// This preserves the exact original envelope when a clip is split.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudioFade {
    pub start: i64,
    pub end: i64,
    pub shape: FadeShape,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudioClip {
    pub id: String,
    pub name: String,
    pub media: MediaId,
    pub start_frame: i64,
    pub duration_frames: u64,
    /// First source sample, fractional for sample-exact varispeed splits.
    pub source_offset: f64,
    /// Signed playback rate. Negative values reverse; this is not pitch-preserving stretch.
    pub rate: f64,
    pub gain_db: f64,
    pub pan: f64,
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub fade_in: Option<AudioFade>,
    #[serde(default)]
    pub fade_out: Option<AudioFade>,
    #[serde(default)]
    pub gain_points: Vec<AudioPoint>,
    #[serde(default)]
    pub pan_points: Vec<AudioPoint>,
}
impl AudioClip {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        media: MediaId,
        frames: u64,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            media,
            start_frame: 0,
            duration_frames: frames,
            source_offset: 0.0,
            rate: 1.0,
            gain_db: 0.0,
            pan: 0.0,
            muted: false,
            fade_in: None,
            fade_out: None,
            gain_points: vec![],
            pan_points: vec![],
        }
    }
    pub fn end_frame(&self) -> i64 {
        self.start_frame + self.duration_frames as i64
    }
    /// Keep retained audio/envelopes stationary while changing the left boundary.
    pub fn trim_left(&self, delta: i64) -> Result<Self, String> {
        if delta.unsigned_abs() > MAX_AUDIO_TIMELINE as u64 {
            return Err("Audio trim exceeds the timeline limit".into());
        }
        if delta >= self.duration_frames as i64 {
            return Err("Trim must leave at least one sample".into());
        }
        let mut out = self.clone();
        out.start_frame = out
            .start_frame
            .checked_add(delta)
            .ok_or("Audio time overflow")?;
        out.duration_frames = u64::try_from(self.duration_frames as i64 - delta)
            .map_err(|_| "Invalid audio duration")?;
        out.source_offset += delta as f64 * out.rate;
        out.shift_envelope(-delta)?;
        Ok(out)
    }
    pub fn split(&self, at: i64, new_id: String) -> Result<(Self, Self), String> {
        let local = at
            .checked_sub(self.start_frame)
            .ok_or("Audio split time overflow")?;
        if local <= 0 || local >= self.duration_frames as i64 {
            return Err("Split point must be inside the clip".into());
        }
        let mut a = self.clone();
        a.duration_frames = local as u64;
        let mut b = self.trim_left(local)?;
        b.id = new_id;
        Ok((a, b))
    }
    fn shift_envelope(&mut self, delta: i64) -> Result<(), String> {
        for fade in [&mut self.fade_in, &mut self.fade_out]
            .into_iter()
            .flatten()
        {
            fade.start = fade.start.checked_add(delta).ok_or("Fade time overflow")?;
            fade.end = fade.end.checked_add(delta).ok_or("Fade time overflow")?;
        }
        for point in self
            .gain_points
            .iter_mut()
            .chain(self.pan_points.iter_mut())
        {
            point.frame = point
                .frame
                .checked_add(delta)
                .ok_or("Automation time overflow")?;
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudioTrack {
    pub id: String,
    pub name: String,
    pub color: String,
    pub gain_db: f64,
    pub pan: f64,
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub solo: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub clips: Vec<AudioClip>,
    #[serde(default, skip_serializing_if = "crate::AudioProcessing::is_default")]
    pub processing: crate::AudioProcessing,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sends: Vec<crate::AudioSend>,
}
impl AudioTrack {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            color: "#81B8C6".into(),
            gain_db: 0.0,
            pan: 0.0,
            muted: false,
            solo: false,
            locked: false,
            clips: vec![],
            processing: Default::default(),
            output: None,
            sends: vec![],
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudioMarker {
    pub id: String,
    pub frame: i64,
    pub name: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudioArrangement {
    #[serde(default)]
    pub tracks: Vec<AudioTrack>,
    #[serde(default)]
    pub markers: Vec<AudioMarker>,
    #[serde(default)]
    pub gain_db: f64,
    #[serde(default)]
    pub muted: bool,
    #[serde(default = "default_bpm")]
    pub bpm: f64,
    #[serde(default)]
    pub beat_offset: i64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub buses: Vec<crate::AudioBus>,
    #[serde(default, skip_serializing_if = "crate::AudioProcessing::is_default")]
    pub processing: crate::AudioProcessing,
    #[serde(default, skip_serializing_if = "crate::AudioLimiter::is_default")]
    pub limiter: crate::AudioLimiter,
}
fn default_bpm() -> f64 {
    120.0
}
impl Default for AudioArrangement {
    fn default() -> Self {
        Self {
            tracks: vec![],
            markers: vec![],
            gain_db: 0.0,
            muted: false,
            bpm: 120.0,
            beat_offset: 0,
            buses: vec![],
            processing: Default::default(),
            limiter: Default::default(),
        }
    }
}
impl AudioArrangement {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
}
fn id_valid(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
fn within(value: f64, low: f64, high: f64) -> bool {
    value.is_finite() && (low..=high).contains(&value)
}
fn points_valid(points: &[AudioPoint], min: f64, max: f64) -> bool {
    points.len() <= 10000
        && points.windows(2).all(|w| w[0].frame < w[1].frame)
        && points.iter().all(|p| {
            p.frame.unsigned_abs() <= MAX_AUDIO_TIMELINE as u64 && within(p.value, min, max)
        })
}
impl Project {
    pub fn validate_audio(&self) -> Result<(), String> {
        for asset in self.media.values() {
            if let Some(a) = &asset.audio {
                if !matches!(asset.kind, crate::MediaKind::Audio { .. }) || asset.embedded.is_some()
                {
                    return Err("Audio payload needs an audio asset, not an image".into());
                }
                if a.frames == 0
                    || a.frames > MAX_AUDIO_FRAMES
                    || a.sample_rate != AUDIO_RATE
                    || !(1..=2).contains(&a.channels)
                    || !(1..=2).contains(&a.original_channels)
                    || !(8000..=384000).contains(&a.original_sample_rate)
                    || !a.peak.is_finite()
                    || a.peak < 0.0
                    || a.peak > 64.0
                    || a.codec.len() > 80
                    || a.sha256.len() != 64
                    || !a.sha256.bytes().all(|b| b.is_ascii_hexdigit())
                    || a.data_base64.is_empty()
                {
                    return Err("Invalid embedded audio metadata".into());
                }
            }
        }
        for comp in self.comps.values() {
            let a = &comp.audio;
            a.validate_processing()?;
            if a.tracks.len() > 64
                || a.markers.len() > 2048
                || !within(a.gain_db, -96.0, 24.0)
                || !within(a.bpm, 20.0, 400.0)
                || a.beat_offset.unsigned_abs() > MAX_AUDIO_TIMELINE as u64
            {
                return Err("Invalid audio arrangement or track limit (64)".into());
            }
            let mut tracks = std::collections::BTreeSet::new();
            let mut clips = std::collections::BTreeSet::new();
            let mut markers = std::collections::BTreeSet::new();
            for marker in &a.markers {
                if !id_valid(&marker.id)
                    || !markers.insert(&marker.id)
                    || marker.frame < 0
                    || marker.frame > MAX_AUDIO_TIMELINE
                    || marker.name.len() > 256
                {
                    return Err("Invalid or duplicate audio marker".into());
                }
            }
            for track in &a.tracks {
                if !id_valid(&track.id)
                    || !tracks.insert(&track.id)
                    || track.name.len() > 256
                    || track.color.len() != 7
                    || !track.color.starts_with('#')
                    || !track.color[1..].bytes().all(|b| b.is_ascii_hexdigit())
                    || !within(track.gain_db, -96.0, 24.0)
                    || !within(track.pan, -1.0, 1.0)
                    || track.clips.len() > 2048
                {
                    return Err("Invalid audio track".into());
                }
                for clip in &track.clips {
                    if !id_valid(&clip.id)
                        || !clips.insert(&clip.id)
                        || clips.len() > 8192
                        || clip.name.len() > 256
                        || clip.start_frame.unsigned_abs() > MAX_AUDIO_TIMELINE as u64
                        || clip.duration_frames == 0
                        || clip.duration_frames > MAX_AUDIO_TIMELINE as u64
                        || !clip.source_offset.is_finite()
                        || !within(clip.rate.abs(), 0.25, 4.0)
                        || !within(clip.gain_db, -96.0, 24.0)
                        || !within(clip.pan, -1.0, 1.0)
                        || !points_valid(&clip.gain_points, -96.0, 24.0)
                        || !points_valid(&clip.pan_points, -1.0, 1.0)
                    {
                        return Err("Invalid audio clip, rate, gain or automation".into());
                    }
                    let source = self
                        .media
                        .get(&clip.media)
                        .and_then(|m| m.audio.as_ref())
                        .ok_or("Audio clip source is missing or not decoded audio")?;
                    let end = clip.source_offset + (clip.duration_frames - 1) as f64 * clip.rate;
                    if clip.source_offset.min(end) < -0.000001
                        || clip.source_offset.max(end)
                            > source.frames.saturating_sub(1) as f64 + 0.000001
                    {
                        return Err("Audio clip extends beyond available source samples".into());
                    }
                    for fade in [&clip.fade_in, &clip.fade_out].into_iter().flatten() {
                        if fade.start >= fade.end
                            || fade.start.unsigned_abs() > MAX_AUDIO_TIMELINE as u64
                            || fade.end.unsigned_abs() > MAX_AUDIO_TIMELINE as u64
                        {
                            return Err("Invalid audio fade region".into());
                        }
                    }
                }
            }
        }
        Ok(())
    }
    /// Audio from nested compositions follows precomp in/out time. Visual opacity
    /// does not secretly change sound level. Visibility does gate a nested comp.
    pub fn has_audio(&self, id: CompId) -> bool {
        fn visit(p: &Project, id: CompId, seen: &mut std::collections::BTreeSet<CompId>) -> bool {
            if !seen.insert(id) {
                return false;
            }
            let Some(c) = p.comp(id) else {
                return false;
            };
            c.audio.tracks.iter().any(|t|!t.clips.is_empty()) || c.layers.values().any(|l| matches!(l.kind,crate::LayerKind::PreComp{comp} if l.visible && visit(p,comp,seen)))
        }
        visit(self, id, &mut std::collections::BTreeSet::new())
    }
}
pub fn audio_frames(time: Time) -> i64 {
    ((time.0 as i128 * AUDIO_RATE as i128).div_euclid(120000)) as i64
}
pub fn audio_duration(time: Time) -> u64 {
    ((time.0.max(0) as u128 * AUDIO_RATE as u128).div_ceil(120000)) as u64
}
