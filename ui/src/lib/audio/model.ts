export const AUDIO_RATE = 48000;
export interface AudioPoint {
  frame: number;
  value: number;
}
export interface AudioFade {
  start: number;
  end: number;
  shape: "linear" | "equal_power";
}
export interface AudioClip {
  id: string;
  name: string;
  media: number;
  start_frame: number;
  duration_frames: number;
  source_offset: number;
  rate: number;
  gain_db: number;
  pan: number;
  muted: boolean;
  fade_in: AudioFade | null;
  fade_out: AudioFade | null;
  gain_points: AudioPoint[];
  pan_points: AudioPoint[];
}
export interface AudioTrack {
  id: string;
  name: string;
  color: string;
  gain_db: number;
  pan: number;
  muted: boolean;
  solo: boolean;
  locked: boolean;
  clips: AudioClip[];
  processing?: AudioProcessing;
  output?: string | null;
  sends?: AudioSend[];
}
export interface AudioMarker {
  id: string;
  frame: number;
  name: string;
}
export interface AudioArrangement {
  tracks: AudioTrack[];
  markers: AudioMarker[];
  gain_db: number;
  muted: boolean;
  bpm: number;
  beat_offset: number;
  buses?: AudioBus[];
  processing?: AudioProcessing;
  limiter?: AudioLimiter;
}
export interface AudioAsset {
  data_base64: string;
  /** Astra extent: chunk hashes when the source streams from the store
   * instead of riding inside the project file (sources > 16 MiB). */
  astra_chunks?: string[] | null;
  sha256: string;
  frames: number;
  channels: number;
  sample_rate: number;
  original_sample_rate: number;
  original_channels: number;
  codec: string;
  peak: number;
}
export const emptyAudio = (): AudioArrangement => ({
  tracks: [],
  markers: [],
  gain_db: 0,
  muted: false,
  bpm: 120,
  beat_offset: 0,
});
export const newTrack = (name = "Audio track"): AudioTrack => ({
  id: crypto.randomUUID(),
  name,
  color: "#81B8C6",
  gain_db: 0,
  pan: 0,
  muted: false,
  solo: false,
  locked: false,
  clips: [],
});
export const newClip = (media: number, name: string, frames: number, start = 0): AudioClip => ({
  id: crypto.randomUUID(),
  name,
  media,
  start_frame: start,
  duration_frames: frames,
  source_offset: 0,
  rate: 1,
  gain_db: 0,
  pan: 0,
  muted: false,
  fade_in: null,
  fade_out: null,
  gain_points: [],
  pan_points: [],
});
export const sampleToSeconds = (frame: number) => frame / AUDIO_RATE;
export const dbLabel = (value: number) =>
  value <= -96 ? "−∞ dB" : `${value > 0 ? "+" : ""}${value.toFixed(1)} dB`;
export const peakDb = (peak: number) => 20 * Math.log10(Math.max(peak, 0.000001));
export function trimLeft(clip: AudioClip, delta: number): AudioClip {
  const c = JSON.parse(JSON.stringify(clip)) as AudioClip;
  c.start_frame += delta;
  c.duration_frames -= delta;
  c.source_offset += delta * c.rate;
  for (const f of [c.fade_in, c.fade_out])
    if (f) {
      f.start -= delta;
      f.end -= delta;
    }
  for (const p of [...c.gain_points, ...c.pan_points]) p.frame -= delta;
  return c;
}
export function envelope(points: AudioPoint[], frame: number, base: number) {
  if (!points.length) return base;
  if (frame <= points[0].frame) return points[0].value;
  for (let i = 1; i < points.length; i++)
    if (frame < points[i].frame) {
      const a = points[i - 1],
        b = points[i],
        t = (frame - a.frame) / (b.frame - a.frame);
      return a.value + (b.value - a.value) * t;
    }
  return points.at(-1)!.value;
}

export interface AudioEq {
  enabled: boolean;
  highpass_hz: number;
  low_hz: number;
  low_db: number;
  mid_hz: number;
  mid_db: number;
  mid_q: number;
  high_hz: number;
  high_db: number;
}
export interface AudioCompressor {
  enabled: boolean;
  threshold_db: number;
  ratio: number;
  knee_db: number;
  attack_ms: number;
  release_ms: number;
  makeup_db: number;
}
export interface AudioProcessing {
  eq: AudioEq;
  compressor: AudioCompressor;
}
export interface AudioLimiter {
  enabled: boolean;
  ceiling_db: number;
  release_ms: number;
}
export interface AudioSend {
  bus: string;
  gain_db: number;
  enabled: boolean;
}
export interface AudioBus {
  id: string;
  name: string;
  gain_db: number;
  pan: number;
  muted: boolean;
  output: string | null;
  sends: AudioSend[];
  processing: AudioProcessing;
}
export const defaultProcessing = (): AudioProcessing => ({
  eq: {
    enabled: false,
    highpass_hz: 0,
    low_hz: 120,
    low_db: 0,
    mid_hz: 1500,
    mid_db: 0,
    mid_q: 1,
    high_hz: 8000,
    high_db: 0,
  },
  compressor: {
    enabled: false,
    threshold_db: -18,
    ratio: 3,
    knee_db: 6,
    attack_ms: 10,
    release_ms: 120,
    makeup_db: 0,
  },
});
export const defaultLimiter = (): AudioLimiter => ({
  enabled: false,
  ceiling_db: -1,
  release_ms: 80,
});
export const newBus = (name = "Mix bus"): AudioBus => ({
  id: crypto.randomUUID(),
  name,
  gain_db: 0,
  pan: 0,
  muted: false,
  output: null,
  sends: [],
  processing: defaultProcessing(),
});
