//! Pure, deterministic float audio mixer. No devices, files, threads or FFmpeg.
//! Every output sample has an absolute time; chunk size and seeking do not reset
//! envelope/resampler state. Playback and export call this exact implementation.
pub mod beats;
mod dsp;
pub mod fx;
mod graph;
use bonaparte_model::*;
pub use dsp::eq_response;
use serde::Serialize;
use std::{collections::BTreeMap, sync::Arc};
pub trait AudioSource: Send + Sync {
    fn frames(&self) -> u64;
    fn channels(&self) -> u16;
    fn sample(&self, frame: i64, channel: usize) -> f32;
}
pub trait AudioSources {
    fn source(&self, id: MediaId) -> Option<Arc<dyn AudioSource>>;
}
#[derive(Clone)]
pub struct PcmSource {
    pub channels: u16,
    pub samples: Vec<f32>,
}
impl AudioSource for PcmSource {
    fn frames(&self) -> u64 {
        (self.samples.len() / self.channels as usize) as u64
    }
    fn channels(&self) -> u16 {
        self.channels
    }
    fn sample(&self, frame: i64, channel: usize) -> f32 {
        if frame < 0 || channel >= self.channels as usize {
            return 0.0;
        }
        self.samples
            .get(frame as usize * self.channels as usize + channel)
            .copied()
            .unwrap_or(0.0)
    }
}
#[derive(Clone)]
struct Voice {
    clip: AudioClip,
    source: Arc<dyn AudioSource>,
    origin: f64,
    begin: f64,
    end: f64,
    gain: f64,
    track_pan: [f64; 2],
}
#[derive(Clone)]
struct Bus {
    comp: CompId,
    id: String,
    voices: Vec<Voice>,
}
#[derive(Clone)]
pub struct MixPlan {
    buses: Vec<Bus>,
    pub duration_frames: f64,
    master: f64,
    graph: Option<Arc<graph::ProcessedGraph>>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Meter {
    pub peak: [f32; 2],
    pub rms: [f32; 2],
    pub clipped_samples: u64,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackMeter {
    pub comp_id: CompId,
    pub track_id: String,
    pub meter: Meter,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessorMeter {
    pub comp_id: CompId,
    pub id: String,
    pub kind: String,
    pub reduction_db: f32,
    pub meter: Meter,
}
pub struct MixedBlock {
    pub samples: Vec<f32>,
    pub meter: Meter,
    pub tracks: Vec<TrackMeter>,
    pub processing: Vec<ProcessorMeter>,
    pub compensated_latency: usize,
}
pub fn db_gain(db: f64) -> f64 {
    if db <= -96.0 {
        0.0
    } else {
        10.0f64.powf(db / 20.0)
    }
}
pub fn balance(pan: f64) -> [f64; 2] {
    let p = pan.clamp(-1.0, 1.0);
    if p < 0.0 {
        [1.0, (-p * std::f64::consts::FRAC_PI_2).cos()]
    } else {
        [(p * std::f64::consts::FRAC_PI_2).cos(), 1.0]
    }
}
pub fn automation(points: &[AudioPoint], frame: f64, default: f64) -> f64 {
    if points.is_empty() {
        return default;
    }
    if frame <= points[0].frame as f64 {
        return points[0].value;
    }
    let i = points.partition_point(|p| (p.frame as f64) <= frame);
    if i >= points.len() {
        return points[points.len() - 1].value;
    }
    let a = &points[i - 1];
    let b = &points[i];
    let t = (frame - a.frame as f64) / (b.frame - a.frame) as f64;
    a.value + (b.value - a.value) * t
}
fn fade(f: &AudioFade, frame: f64, incoming: bool) -> f64 {
    let x = ((frame - f.start as f64) / (f.end - f.start) as f64).clamp(0.0, 1.0);
    let x = if incoming { x } else { 1.0 - x };
    match f.shape {
        FadeShape::Linear => x,
        FadeShape::EqualPower => (x * std::f64::consts::FRAC_PI_2).sin(),
    }
}
/// 32-tap windowed-sinc resampling. Integer unity-rate reads are exact. Lower
/// cutoffs suppress aliasing for varispeed/downsampling; no pitch-preserving DSP is implied.
pub fn resample(source: &dyn AudioSource, position: f64, step: f64, channel: usize) -> f64 {
    if step.abs() <= 1.000000001 && (position - position.round()).abs() < 1e-9 {
        return source.sample(position.round() as i64, channel) as f64;
    }
    let cutoff = (1.0 / step.abs().max(1.0)).min(1.0) * 0.97;
    let center = position.floor() as i64;
    let mut sum = 0.0;
    let mut weight_sum = 0.0;
    for i in -15..=16 {
        let frame = center + i;
        let distance = position - frame as f64;
        let x = distance * cutoff;
        let sinc = if x.abs() < 1e-12 {
            1.0
        } else {
            (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x)
        };
        let window = 0.5 + 0.5 * (std::f64::consts::PI * distance / 16.0).cos();
        let weight = sinc * window * cutoff;
        sum += source.sample(frame, channel) as f64 * weight;
        weight_sum += weight;
    }
    if weight_sum.abs() > 1e-12 {
        sum / weight_sum
    } else {
        0.0
    }
}
impl MixPlan {
    pub fn new(
        project: &Project,
        comp: CompId,
        sources: &dyn AudioSources,
    ) -> Result<Self, String> {
        project.validate()?;
        let root = project.comp(comp).ok_or("Audio composition not found")?;
        if project.comps.values().any(|c| c.audio.has_processing()) {
            return Ok(Self {
                buses: vec![],
                duration_frames: root.duration.0 as f64 * 0.4,
                master: 1.0,
                graph: Some(Arc::new(graph::ProcessedGraph::new(
                    project, comp, sources,
                )?)),
            });
        }
        let duration = root.duration.0 as f64 * AUDIO_RATE as f64 / 120000.0;
        let master = if root.audio.muted {
            0.0
        } else {
            db_gain(root.audio.gain_db)
        };
        let mut buses: BTreeMap<(CompId, String), Vec<Voice>> = BTreeMap::new();
        fn collect(
            p: &Project,
            id: CompId,
            offset: f64,
            begin: f64,
            end: f64,
            gain: f64,
            depth: usize,
            sources: &dyn AudioSources,
            buses: &mut BTreeMap<(CompId, String), Vec<Voice>>,
            expanded: &mut usize,
        ) -> Result<(), String> {
            *expanded += 1;
            if *expanded > 4096 {
                return Err("Expanded audio composition graph exceeds 4096 instances".into());
            }
            if depth >= 32 {
                return Err("Audio nesting exceeds 32 compositions".into());
            }
            let c = p.comp(id).ok_or("Nested audio composition not found")?;
            if c.audio.muted {
                return Ok(());
            }
            let end = end.min(offset + c.duration.0 as f64 * AUDIO_RATE as f64 / 120000.0);
            let solo = c.audio.tracks.iter().any(|t| t.solo && !t.muted);
            for track in &c.audio.tracks {
                if track.muted || (solo && !track.solo) {
                    continue;
                }
                for clip in &track.clips {
                    if clip.muted {
                        continue;
                    }
                    let origin = offset + clip.start_frame as f64;
                    let vbegin = begin.max(origin);
                    let vend = end.min(origin + clip.duration_frames as f64);
                    if vend <= vbegin {
                        continue;
                    }
                    let source = sources
                        .source(clip.media)
                        .ok_or_else(|| format!("Audio source {} is unavailable", clip.media))?;
                    if source.frames() == 0 || !(1..=2).contains(&source.channels()) {
                        return Err("Invalid PCM source".into());
                    }
                    if (buses.len() >= 512 && !buses.contains_key(&(id, track.id.clone())))
                        || buses.values().map(Vec::len).sum::<usize>() >= 16384
                    {
                        return Err(
                            "Expanded audio arrangement exceeds 512 buses or 16384 voices".into(),
                        );
                    }
                    buses
                        .entry((id, track.id.clone()))
                        .or_default()
                        .push(Voice {
                            clip: clip.clone(),
                            source,
                            origin,
                            begin: vbegin,
                            end: vend,
                            gain: gain * db_gain(track.gain_db),
                            track_pan: balance(track.pan),
                        });
                }
            }
            if !solo {
                for layer in c.layers.values() {
                    if !layer.visible {
                        continue;
                    }
                    if let LayerKind::PreComp { comp: child } = layer.kind {
                        let child_offset =
                            offset + layer.start.0 as f64 * AUDIO_RATE as f64 / 120000.0;
                        let child_begin = begin.max(child_offset);
                        let child_end = end.min(
                            child_offset + layer.duration.0 as f64 * AUDIO_RATE as f64 / 120000.0,
                        );
                        if child_end > child_begin {
                            let g = p
                                .comp(child)
                                .map(|c| db_gain(c.audio.gain_db))
                                .unwrap_or(1.0);
                            collect(
                                p,
                                child,
                                child_offset,
                                child_begin,
                                child_end,
                                gain * g,
                                depth + 1,
                                sources,
                                buses,
                                expanded,
                            )?;
                        }
                    }
                }
            }
            Ok(())
        }
        if master != 0.0 && project.has_audio(comp) {
            collect(
                project, comp, 0.0, 0.0, duration, 1.0, 0, sources, &mut buses, &mut 0,
            )?;
        }
        Ok(Self {
            buses: buses
                .into_iter()
                .map(|((comp, id), voices)| Bus { comp, id, voices })
                .collect(),
            duration_frames: duration,
            master,
            graph: None,
        })
    }
    pub fn render(
        &self,
        start_frame: i64,
        frame_count: usize,
        output_rate: u32,
    ) -> Result<MixedBlock, String> {
        if !(8000..=192000).contains(&output_rate)
            || frame_count > output_rate as usize * 2
            || start_frame < 0
        {
            return Err(
                "Audio chunks must be nonnegative, at most two seconds, at 8–192 kHz".into(),
            );
        }
        if start_frame as f64 * AUDIO_RATE as f64 / output_rate as f64 >= self.duration_frames {
            return Ok(MixedBlock {
                samples: vec![0.0; frame_count * 2],
                meter: Meter {
                    peak: [0.0; 2],
                    rms: [0.0; 2],
                    clipped_samples: 0,
                },
                tracks: vec![],
                processing: vec![],
                compensated_latency: 0,
            });
        }
        if let Some(graph) = &self.graph {
            return graph.render(start_frame, frame_count, output_rate);
        }
        let step = AUDIO_RATE as f64 / output_rate as f64;
        let from = start_frame as f64 * step;
        let until = (start_frame as f64 + frame_count as f64) * step;
        let mut mix = vec![0.0f64; frame_count * 2];
        let mut track_samples = vec![0.0f64; frame_count * 2];
        let mut tracks = vec![];
        for bus in &self.buses {
            let voices: Vec<_> = bus
                .voices
                .iter()
                .filter(|v| v.begin < until && v.end > from)
                .collect();
            if voices.is_empty() {
                continue;
            }
            if voices.len() > 256 {
                return Err("More than 256 overlapping audio voices in one track window".into());
            }
            track_samples.fill(0.0);
            for voice in voices {
                let first = (((voice.begin / step) - start_frame as f64).ceil().max(0.0) as usize)
                    .min(frame_count);
                let last = (((voice.end / step) - start_frame as f64).ceil().max(0.0) as usize)
                    .min(frame_count);
                let fixed_gain = if voice.clip.gain_points.is_empty() {
                    Some(db_gain(voice.clip.gain_db))
                } else {
                    None
                };
                for i in first..last {
                    let timeline = (start_frame as f64 + i as f64) * step;
                    let local = timeline - voice.origin;
                    if timeline < voice.begin || timeline >= voice.end {
                        continue;
                    }
                    let source_pos = voice.clip.source_offset + local * voice.clip.rate;
                    let mut gain = voice.gain
                        * fixed_gain.unwrap_or_else(|| {
                            db_gain(
                                automation(&voice.clip.gain_points, local, voice.clip.gain_db)
                                    .clamp(-96.0, 24.0),
                            )
                        });
                    if let Some(f) = &voice.clip.fade_in {
                        gain *= fade(f, local, true);
                    }
                    if let Some(f) = &voice.clip.fade_out {
                        gain *= fade(f, local, false);
                    }
                    let pan =
                        automation(&voice.clip.pan_points, local, voice.clip.pan).clamp(-1.0, 1.0);
                    let stereo = voice.source.channels() == 2;
                    let p = if stereo {
                        balance(pan)
                    } else {
                        let angle = (pan + 1.0) * std::f64::consts::FRAC_PI_4;
                        [angle.cos(), angle.sin()]
                    };
                    let left =
                        resample(voice.source.as_ref(), source_pos, step * voice.clip.rate, 0);
                    let right = if stereo {
                        resample(voice.source.as_ref(), source_pos, step * voice.clip.rate, 1)
                    } else {
                        left
                    };
                    track_samples[i * 2] += left * gain * p[0] * voice.track_pan[0];
                    track_samples[i * 2 + 1] += right * gain * p[1] * voice.track_pan[1];
                }
            }
            if track_samples
                .iter()
                .any(|v| !v.is_finite() || v.abs() > f32::MAX as f64)
            {
                return Err(
                    "Audio gain exceeds floating-point headroom; reduce nested track gains".into(),
                );
            }
            tracks.push(TrackMeter {
                comp_id: bus.comp,
                track_id: bus.id.clone(),
                meter: measure(&track_samples),
            });
            for (dst, src) in mix.iter_mut().zip(&track_samples) {
                *dst += src;
            }
        }
        for sample in &mut mix {
            *sample *= self.master;
        }
        if mix
            .iter()
            .any(|v| !v.is_finite() || v.abs() > f32::MAX as f64)
        {
            return Err("Audio mix exceeds floating-point headroom; reduce master gain".into());
        }
        let meter = measure(&mix);
        let samples = mix.into_iter().map(|v| v as f32).collect();
        Ok(MixedBlock {
            samples,
            meter,
            tracks,
            processing: vec![],
            compensated_latency: 0,
        })
    }
}
fn measure(samples: &[f64]) -> Meter {
    let mut peak = [0.0f64; 2];
    let mut sum = [0.0; 2];
    let mut clipped = 0;
    for (i, v) in samples.iter().enumerate() {
        let c = i % 2;
        peak[c] = peak[c].max(v.abs());
        sum[c] += v * v;
        if v.abs() > 1.0 {
            clipped += 1;
        }
    }
    let count = (samples.len() / 2).max(1) as f64;
    Meter {
        peak: [peak[0] as f32, peak[1] as f32],
        rms: [
            (sum[0] / count).sqrt() as f32,
            (sum[1] / count).sqrt() as f32,
        ],
        clipped_samples: clipped,
    }
}
