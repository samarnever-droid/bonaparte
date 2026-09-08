import type { AudioArrangement, AudioAsset } from "./audio/model";
/// Wire types matching bonaparte-model. Struct fields are snake_case; Op fields camelCase.
export const TICKS_PER_SEC = 120_000;
export type Color = [number, number, number, number];
export type Property = "Position" | "Scale" | "Rotation" | "Opacity" | "AnchorPoint" | "Z";
/** The comp's 3D perspective camera; fov <= 0 disables projection (pure 2D). */
export interface Camera3D {
  position: [number, number];
  z: number;
  fov: number;
  /** Camera-space depth that renders perfectly sharp when DoF is on. */
  focus?: number;
  /** Depth-of-field strength 0..1 (0 = off). */
  dof?: number;
}
/** Turntable auto-orbit of the camera around the comp center. */
export interface Turntable {
  enabled: boolean;
  period: number;
}
export const DEFAULT_CAMERA: Camera3D = { position: [0, 0], z: 0, fov: 0, focus: 0, dof: 0 };
export const DEFAULT_TURNTABLE: Turntable = { enabled: false, period: 12 };
export type PropValue = { Scalar: number } | { Vec2: [number, number] };
export type Easing = "Linear" | "Hold" | { Bezier: { p1: [number, number]; p2: [number, number] } };
export interface Keyframe {
  time: number;
  value: PropValue;
  easing: Easing;
}
export interface Track {
  keys: Keyframe[];
}
export interface StaticTransform {
  position: [number, number];
  scale: [number, number];
  rotation: number;
  opacity: number;
  anchor_point: [number, number];
  /** Depth in px; positive pushes the layer away from the camera. */
  z?: number;
}
export type BlendMode =
  "Normal" | "Multiply" | "Screen" | "Overlay" | "Add" | "Darken" | "Lighten" | "Difference";
export const BLEND_MODES: BlendMode[] = [
  "Normal",
  "Multiply",
  "Screen",
  "Overlay",
  "Add",
  "Darken",
  "Lighten",
  "Difference",
];
export interface ShapeStyle {
  size: [number, number] | null;
  corner_radius: number;
  stroke_width: number;
  stroke_color: Color;
}
export interface TextStyle {
  color: Color;
  bold: boolean;
  tracking: number;
}
export type LayerKind =
  | { Solid: { color: Color } }
  | {
      Shape: {
        color: Color;
        generator: string | null;
        style: ShapeStyle;
        points?: [number, number][][];
      };
    }
  | { Text: { text: string; size: number; style: TextStyle } }
  | { Footage: { media: number } }
  | { PreComp: { comp: number } }
  | { Adjustment: Record<string, never> };
export type EffectValue =
  | { Float: number }
  | { Color: Color }
  | { Point: [number, number] }
  | { Bool: boolean }
  | { Index: number };
export type ParamKind =
  | { Slider: { min: number; max: number; default: number } }
  | { Color: { default: Color } }
  | { Point: { default: [number, number] } }
  | { Checkbox: { default: boolean } }
  | { Dropdown: { options: string[]; default: number } };
export interface ParamDef {
  id: string;
  name: string;
  doc: string;
  kind: ParamKind;
  group: string;
  step: number | null;
  unit: string;
  scale_with_resolution?: boolean;
}
export interface EffectManifest {
  api_version: string;
  id: string;
  name: string;
  category: string;
  cost: "light" | "medium" | "heavy";
  shader: string;
  inputs: string[];
  params: ParamDef[];
}
export interface EffectInstance {
  id: string;
  effect_id: string;
  enabled: boolean;
  params: Record<string, EffectValue>;
  tracks: Record<string, Track>;
}
export interface Layer {
  id: number;
  name: string;
  kind: LayerKind;
  start: number;
  duration: number;
  transform: StaticTransform;
  tracks: Partial<Record<Property, Track>>;
  parent: number | null;
  blend_mode: BlendMode;
  visible: boolean;
  locked: boolean;
  effects: EffectInstance[];
}
export interface FrameRate {
  num: number;
  den: number;
}
export const FPS_24 = { num: 24, den: 1 };
export const FPS_25 = { num: 25, den: 1 };
export const FPS_30 = { num: 30, den: 1 };
export const FPS_60 = { num: 60, den: 1 };
export const NTSC_FILM = { num: 24000, den: 1001 };
export const NTSC_30 = { num: 30000, den: 1001 };
export interface Comp {
  id: number;
  name: string;
  width: number;
  height: number;
  fps: FrameRate;
  duration: number;
  background: Color;
  layer_order: number[];
  layers: Record<string, Layer>;
  audio?: AudioArrangement;
  camera?: Camera3D;
  turntable?: Turntable;
}
export interface MediaAsset {
  id: number;
  name: string;
  path: string | null;
  kind: "Image" | { Video: { fps: FrameRate; duration: number } } | { Audio: { duration: number } };
  embedded?: { width: number; height: number; rgba_base64: string };
  audio?: AudioAsset;
  video?: {
    width: number;
    height: number;
    fps: FrameRate;
    duration: number;
    times_millis: number[];
    frames_base64: string[];
  };
  slot: unknown;
  alias: string | null;
  perception: unknown;
}
export interface Project {
  name: string;
  comps: Record<string, Comp>;
  media: Record<string, MediaAsset>;
  next_comp: number;
  next_layer: number;
  next_media: number;
}
export interface Snapshot {
  project: Project;
  canUndo: boolean;
  canRedo: boolean;
  history: string[];
  revision: number;
  audioRevision?: number;
}
export type Op =
  | { type: "setCompAudio"; comp: number; audio: AudioArrangement }
  | {
      type: "setCamera";
      comp: number;
      position: [number, number];
      z: number;
      fov: number;
      focus?: number;
      dof?: number;
    }
  | { type: "setTurntable"; comp: number; enabled: boolean; period: number }
  | { type: "batch"; label: string; ops: Op[] }
  | { type: "renameProject"; name: string }
  | { type: "shiftLayer"; comp: number; layer: number; delta: number }
  | {
      type: "createComp";
      name: string;
      width: number;
      height: number;
      fps: FrameRate;
      duration: number;
    }
  | { type: "restoreComp"; comp: Comp }
  | { type: "removeComp"; comp: number }
  | {
      type: "setCompProps";
      comp: number;
      name: string;
      width: number;
      height: number;
      fps: FrameRate;
      duration: number;
      background: Color;
    }
  | { type: "addLayer"; comp: number; layer: Layer }
  | { type: "restoreLayer"; comp: number; layer: Layer; index: number }
  | { type: "removeLayer"; comp: number; layer: number }
  | { type: "renameLayer"; comp: number; layer: number; name: string }
  | { type: "setLayerContent"; comp: number; layer: number; kind: LayerKind }
  | { type: "setLayerEffects"; comp: number; layer: number; effects: EffectInstance[] }
  | { type: "setLayerTime"; comp: number; layer: number; start: number; duration: number }
  | { type: "setLayerParent"; comp: number; layer: number; parent: number | null }
  | { type: "setLayerBlendMode"; comp: number; layer: number; blendMode: BlendMode }
  | { type: "setLayerVisible"; comp: number; layer: number; visible: boolean }
  | { type: "setLayerLocked"; comp: number; layer: number; locked: boolean }
  | { type: "setValue"; comp: number; layer: number; property: Property; value: PropValue }
  | { type: "addKeyframe"; comp: number; layer: number; property: Property; key: Keyframe }
  | { type: "removeKeyframe"; comp: number; layer: number; property: Property; time: number }
  | {
      type: "moveKeyframe";
      comp: number;
      layer: number;
      property: Property;
      from: number;
      to: number;
    }
  | {
      type: "setEasing";
      comp: number;
      layer: number;
      property: Property;
      time: number;
      easing: Easing;
    }
  | { type: "reorderLayer"; comp: number; layer: number; newIndex: number }
  | { type: "addMedia"; asset: MediaAsset }
  | { type: "restoreMedia"; asset: MediaAsset }
  | { type: "removeMedia"; media: number };

export const DEFAULT_EASING: Easing = {
  Bezier: { p1: [0.42, 0.0], p2: [0.58, 1.0] },
};

/** Normalize FrameRate or numeric fps to rational FrameRate struct. */
export function normalizeFps(fps: FrameRate | number | undefined | null): FrameRate {
  if (!fps) return FPS_30;
  if (typeof fps === "number") {
    if (Math.abs(fps - 23.976) < 0.02) return NTSC_FILM;
    if (Math.abs(fps - 29.97) < 0.02) return NTSC_30;
    if (Math.abs(fps - 59.94) < 0.02) return { num: 60000, den: 1001 };
    return { num: Math.round(fps), den: 1 };
  }
  return { num: fps.num || 30, den: fps.den || 1 };
}

/** Exact ticks in one frame — integer by construction of 120k ticks/sec. */
export function ticksPerFrame(fps: FrameRate | number | undefined | null): number {
  const r = normalizeFps(fps);
  if (r.num === 0) return 4000;
  return Math.floor((TICKS_PER_SEC * r.den) / r.num);
}

export function fpsAsNumber(fps: FrameRate | number | undefined | null): number {
  const r = normalizeFps(fps);
  if (r.den === 0) return 30;
  return r.num / r.den;
}

export function formatFps(fps: FrameRate | number | undefined | null): string {
  const r = normalizeFps(fps);
  if (r.den === 1) return `${r.num} fps`;
  const val = r.num / r.den;
  const str = val.toFixed(3).replace(/\.?0+$/, "");
  return `${str} fps`;
}

/** Convert integer ticks Time(i64) to frame number at given frame rate. */
export function timeToFrame(time: number, fps: FrameRate | number | undefined | null): number {
  const tpf = ticksPerFrame(fps);
  if (tpf === 0) return 0;
  return Math.floor(time / tpf);
}

/** Convert frame number to integer ticks Time(i64) at given frame rate. */
export function frameToTime(frame: number, fps: FrameRate | number | undefined | null): number {
  return frame * ticksPerFrame(fps);
}

/** Convert integer ticks Time(i64) to fractional seconds. */
export function timeToSecs(time: number): number {
  return time / TICKS_PER_SEC;
}

/** Convert fractional seconds to integer ticks Time(i64). */
export function secsToTime(secs: number): number {
  return Math.round(secs * TICKS_PER_SEC);
}

/** Snap integer ticks to the nearest frame boundary of fps. */
export function snapToFrame(time: number, fps: FrameRate | number | undefined | null): number {
  const tpf = ticksPerFrame(fps);
  if (tpf === 0) return time;
  return Math.round(time / tpf) * tpf;
}

/** Check if a keyframe exists within half-a-frame tolerance of time. */
export function findKeyframeAtTime(
  track: Track | undefined,
  time: number,
  fps: FrameRate | number | undefined | null,
): Keyframe | null {
  if (!track || !track.keys.length) return null;
  const tpf = ticksPerFrame(fps);
  const halfFrame = tpf > 0 ? tpf / 2 : 1;
  const snapped = snapToFrame(time, fps);
  return track.keys.find((k) => k.time === snapped || Math.abs(k.time - time) < halfFrame) ?? null;
}

/** Convert integer ticks to SMPTE timecode string (HH:MM:SS:FF). */
export function timeToTimecode(time: number, fps: FrameRate | number | undefined | null): string {
  const r = normalizeFps(fps);
  const frameNum = timeToFrame(time, r);
  const nomFps = Math.max(1, Math.round(r.num / r.den));

  const isNeg = frameNum < 0;
  const absFrames = Math.abs(frameNum);

  const ff = absFrames % nomFps;
  const totalSec = Math.floor(absFrames / nomFps);
  const ss = totalSec % 60;
  const totalMin = Math.floor(totalSec / 60);
  const mm = totalMin % 60;
  const hh = Math.floor(totalMin / 60);

  const pad = (n: number) => String(n).padStart(2, "0");
  const prefix = isNeg ? "-" : "";
  return `${prefix}${pad(hh)}:${pad(mm)}:${pad(ss)}:${pad(ff)}`;
}

/** Parse an SMPTE timecode string (`HH:MM:SS:FF` or `HH:MM:SS;FF`) into integer ticks Time(i64). */
export function timecodeToTime(
  tc: string,
  fps: FrameRate | number | undefined | null,
): number | null {
  const trimmed = tc.trim();
  const isNeg = trimmed.startsWith("-");
  const s = isNeg ? trimmed.slice(1) : trimmed;
  const parts = s.split(/[:;.]/);
  if (parts.length !== 4) return null;
  const [hh, mm, ss, ff] = parts.map(Number);
  if (isNaN(hh) || isNaN(mm) || isNaN(ss) || isNaN(ff)) return null;
  if (mm >= 60 || ss >= 60) return null;

  const r = normalizeFps(fps);
  const nomFps = Math.max(1, Math.round(r.num / r.den));
  if (ff >= nomFps) return null;

  const totalFrames = ((hh * 60 + mm) * 60 + ss) * nomFps + ff;
  const signedFrames = isNeg ? -totalFrames : totalFrames;
  return frameToTime(signedFrames, r);
}

/** Linear-bezier solve identical to `keyframe.rs` (bisection, 32 iters). */
export function ease(easing: Easing, u: number): number {
  const uc = Math.min(1, Math.max(0, u));
  if (easing === "Linear") return uc;
  // Stepped/Hold: the outgoing value persists until the next key's instant.
  if (easing === "Hold") return uc >= 1 ? 1 : 0;
  const { p1, p2 } = easing.Bezier;
  let lo = 0;
  let hi = 1;
  for (let i = 0; i < 32; i++) {
    const mid = (lo + hi) / 2;
    const x = bez(mid, p1[0], p2[0]);
    if (x < uc) lo = mid;
    else hi = mid;
  }
  const t = (lo + hi) / 2;
  return bez(t, p1[1], p2[1]);
}

function bez(t: number, p1: number, p2: number): number {
  const it = 1 - t;
  return 3 * it * it * t * p1 + 3 * it * t * t * p2 + t * t * t;
}

/** Mirrors `Track::evaluate` — track wins over static, keys hold outside. */
export function evaluate(layer: Layer, property: Property, time: number): PropValue {
  const track = layer.tracks[property];
  if (track && track.keys.length > 0) {
    const keys = track.keys;
    if (time <= keys[0].time) return keys[0].value;
    const last = keys[keys.length - 1];
    if (time >= last.time) return last.value;
    let lo = 0,
      hi = keys.length - 1;
    while (lo + 1 < hi) {
      const mid = (lo + hi) >>> 1;
      if (keys[mid].time <= time) lo = mid;
      else hi = mid;
    }
    const i = lo;
    const a = keys[i];
    const b = keys[i + 1];
    const u = (time - a.time) / Math.max(b.time - a.time, 1);
    const s = ease(a.easing, u);
    return lerp(a.value, b.value, s);
  }
  return staticValue(layer.transform, property);
}

export function lerp(a: PropValue, b: PropValue, t: number): PropValue {
  if ("Scalar" in a && "Scalar" in b) return { Scalar: a.Scalar + (b.Scalar - a.Scalar) * t };
  if ("Vec2" in a && "Vec2" in b)
    return {
      Vec2: [a.Vec2[0] + (b.Vec2[0] - a.Vec2[0]) * t, a.Vec2[1] + (b.Vec2[1] - a.Vec2[1]) * t],
    };
  return a;
}

export function staticValue(t: StaticTransform, property: Property): PropValue {
  switch (property) {
    case "Position":
      return { Vec2: [...(t.position ?? [0, 0])] };
    case "Scale":
      return { Vec2: [...(t.scale ?? [100, 100])] };
    case "Rotation":
      return { Scalar: t.rotation ?? 0 };
    case "Opacity":
      return { Scalar: t.opacity ?? 1 };
    case "AnchorPoint":
      return { Vec2: [...(t.anchor_point ?? [0, 0])] };
    case "Z":
      return { Scalar: t.z ?? 0 };
  }
}

export interface SnapshotPatch {
  kind: "patch";
  baseRevision: number;
  revision: number;
  audioRevision: number;
  canUndo: boolean;
  canRedo: boolean;
  history: string[];
  name: string;
  nextComp: number;
  nextLayer: number;
  nextMedia: number;
  comps: Record<
    string,
    {
      full?: Comp;
      props?: Partial<Comp>;
      layers?: Record<string, Layer>;
      removedLayers?: number[];
      layerOrder?: number[];
      audio?: AudioArrangement;
    }
  >;
  removedComps: number[];
  media: Record<string, MediaAsset>;
  removedMedia: number[];
}
export function mergeSnapshotPatch(project: Project, patch: SnapshotPatch): Project {
  const comps = { ...project.comps };
  for (const id of patch.removedComps) delete comps[String(id)];
  for (const [id, delta] of Object.entries(patch.comps)) {
    if (delta.full) {
      comps[id] = delta.full;
      continue;
    }
    const old = comps[id];
    if (!old) throw new Error("A changed composition is missing; reload the project state");
    let layers = old.layers;
    if (delta.layers || delta.removedLayers?.length) {
      layers = { ...layers, ...delta.layers };
      for (const id of delta.removedLayers ?? []) delete layers[String(id)];
    }
    comps[id] = {
      ...old,
      ...delta.props,
      layers,
      ...(delta.layerOrder ? { layer_order: delta.layerOrder } : {}),
      ...(delta.audio ? { audio: delta.audio } : {}),
    };
  }
  let media = project.media;
  if (Object.keys(patch.media).length || patch.removedMedia.length) {
    media = { ...media, ...patch.media };
    for (const id of patch.removedMedia) delete media[String(id)];
  }
  return {
    ...project,
    name: patch.name,
    comps,
    media,
    next_comp: patch.nextComp,
    next_layer: patch.nextLayer,
    next_media: patch.nextMedia,
  };
}
