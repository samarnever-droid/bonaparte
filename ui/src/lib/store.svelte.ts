/// The editor store — the single client-side mirror of the Rust document.
/// The UI never mutates it directly: every change is an `Op` round-trip
/// through Tauri (RULES §5.3), which is what makes undo and the future
/// AI-diff free.
import { invoke } from "@tauri-apps/api/core";
import {
  evaluate,
  ticksPerFrame,
  snapToFrame,
  findKeyframeAtTime,
  DEFAULT_EASING,
  TICKS_PER_SEC,
  type Comp,
  type Layer,
  type Op,
  type Project,
  type Property,
} from "./model";
import { layerRect } from "./geometry";

export const editor = $state({
  project: null as Project | null,
  activeComp: null as number | null,
  currentTime: 0, // integer ticks Time(i64) at 120,000 ticks/sec
  playing: false,
  selected: null as number | null,
  /// Live drag preview (comp-space position offset); committed as one Op on
  /// pointerup — the gesture contract, not history spam.
  drag: null as { layer: number; dx: number; dy: number } | null,
  /// Bumped on every applied op so render effects re-fire deterministically.
  renderSeq: 0,
});

let rafId: number | null = null;
let lastPlayTimestamp: number | null = null;
let accumulatedTicks = 0;

// Reusable offscreen canvas for crisp high-DPI blitting
let offscreenCanvas: HTMLCanvasElement | null = null;
let offscreenCtx: CanvasRenderingContext2D | null = null;

export function activeComp(): Comp | null {
  if (!editor.project || editor.activeComp === null) return null;
  return editor.project.comps[String(editor.activeComp)] ?? null;
}

export function selectedLayer(): Layer | null {
  const comp = activeComp();
  if (!comp || editor.selected === null) return null;
  return comp.layers[String(editor.selected)] ?? null;
}

export async function init() {
  editor.project = await invoke<Project>("project_state");
  const firstComp = Object.values(editor.project.comps)[0];
  editor.activeComp = firstComp?.id ?? null;
  editor.selected = firstComp?.layer_order.at(-1) ?? null;
}

/** The only mutation path (mirrors the Rust command). */
export async function applyOp(op: Op) {
  editor.project = await invoke<Project>("apply_op", { op });
  editor.renderSeq++;
}

export async function undoOp() {
  const p = await invoke<Project | null>("undo");
  if (p) {
    editor.project = p;
    editor.renderSeq++;
  }
}

export async function redoOp() {
  const p = await invoke<Project | null>("redo");
  if (p) {
    editor.project = p;
    editor.renderSeq++;
  }
}

/** Playback loop using requestAnimationFrame with frame-accurate advancement. */
export function play() {
  const comp = activeComp();
  if (!comp || editor.playing) return;
  editor.playing = true;
  lastPlayTimestamp = typeof performance !== "undefined" ? performance.now() : Date.now();
  accumulatedTicks = 0;

  const loop = (timestamp: number) => {
    if (!editor.playing) return;
    const currentComp = activeComp();
    if (!currentComp || currentComp.duration <= 0) {
      pause();
      return;
    }

    if (lastPlayTimestamp !== null) {
      const elapsedMs = timestamp - lastPlayTimestamp;
      // 120,000 ticks/sec = 120 ticks/ms
      const deltaTicks = (elapsedMs * TICKS_PER_SEC) / 1000;
      accumulatedTicks += deltaTicks;

      const tpf = ticksPerFrame(currentComp.fps);
      if (tpf > 0 && accumulatedTicks >= tpf) {
        const framesToAdvance = Math.floor(accumulatedTicks / tpf);
        accumulatedTicks %= tpf;
        const advancedTicks = framesToAdvance * tpf;
        let nextTime = editor.currentTime + advancedTicks;
        if (nextTime >= currentComp.duration) {
          nextTime = nextTime % currentComp.duration;
        }
        editor.currentTime = nextTime;
      }
    }
    lastPlayTimestamp = timestamp;
    rafId = requestAnimationFrame(loop);
  };

  rafId = requestAnimationFrame(loop);
}

export function pause() {
  editor.playing = false;
  if (rafId !== null) {
    cancelAnimationFrame(rafId);
    rafId = null;
  }
  lastPlayTimestamp = null;
  accumulatedTicks = 0;
  // Snap playhead to nearest frame boundary on pause
  const comp = activeComp();
  if (comp) {
    editor.currentTime = snapToFrame(editor.currentTime, comp.fps);
  }
}

/** Scrub the playhead (integer ticks), clamped to comp duration. */
export function scrub(time: number) {
  const comp = activeComp();
  if (!comp) return;
  editor.currentTime = Math.min(comp.duration, Math.max(0, Math.round(time)));
}

/** Add a keyframe at the playhead with the layer's current evaluated value. */
export async function keyframeAtPlayhead(layerId: number, property: Property) {
  const comp = activeComp();
  const layer = comp?.layers[String(layerId)];
  if (!comp || !layer) return;
  const time = snapToFrame(editor.currentTime, comp.fps);
  await applyOp({
    type: "addKeyframe",
    comp: comp.id,
    layer: layerId,
    property,
    key: {
      time,
      value: evaluate(layer, property, time),
      easing: DEFAULT_EASING,
    },
  });
}

/** Toggle keyframe at the playhead: remove if existing within frame tolerance, else add. */
export async function toggleKeyframeAtPlayhead(layerId: number, property: Property) {
  const comp = activeComp();
  const layer = comp?.layers[String(layerId)];
  if (!comp || !layer) return;
  const track = layer.tracks[property];
  const existing = findKeyframeAtTime(track, editor.currentTime, comp.fps);
  if (existing) {
    await applyOp({
      type: "removeKeyframe",
      comp: comp.id,
      layer: layerId,
      property,
      time: existing.time,
    });
  } else {
    const time = snapToFrame(editor.currentTime, comp.fps);
    await applyOp({
      type: "addKeyframe",
      comp: comp.id,
      layer: layerId,
      property,
      key: {
        time,
        value: evaluate(layer, property, time),
        easing: DEFAULT_EASING,
      },
    });
  }
}

let renderInFlight = false;
let pendingRender = false;
let lastCanvas: HTMLCanvasElement | null = null;

/// Render the current frame into a canvas via the Rust reference renderer with binary IPC and throttling.
export async function renderTo(canvas: HTMLCanvasElement) {
  lastCanvas = canvas;
  if (renderInFlight) {
    pendingRender = true;
    return;
  }
  renderInFlight = true;

  try {
    const comp = activeComp();
    if (!comp) return;

    // Fast binary IPC endpoint (8-byte header: width, height + raw RGBA bytes)
    // Avoids serializing/deserializing 1,000,000 JSON array numbers per frame.
    const res = await invoke<ArrayBuffer | Uint8Array | { width: number; height: number; rgba: number[] }>(
      "render_frame_raw",
      { compId: comp.id, time: editor.currentTime },
    );

    let width: number;
    let height: number;
    let rgbaArray: Uint8ClampedArray;

    if (res instanceof ArrayBuffer) {
      const view = new DataView(res);
      width = view.getUint32(0, true);
      height = view.getUint32(4, true);
      rgbaArray = new Uint8ClampedArray(res, 8, width * height * 4);
    } else if (res instanceof Uint8Array) {
      const view = new DataView(res.buffer, res.byteOffset, res.byteLength);
      width = view.getUint32(0, true);
      height = view.getUint32(4, true);
      rgbaArray = new Uint8ClampedArray(res.buffer, res.byteOffset + 8, width * height * 4);
    } else {
      width = res.width;
      height = res.height;
      rgbaArray = new Uint8ClampedArray(res.rgba);
    }

    if (canvas.width !== width || canvas.height !== height) {
      canvas.width = width;
      canvas.height = height;
    }

    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const imgData = new ImageData(rgbaArray, width, height);
    ctx.putImageData(imgData, 0, 0);
  } catch (err) {
    console.error("renderTo error:", err);
  } finally {
    renderInFlight = false;
    if (pendingRender && lastCanvas) {
      pendingRender = false;
      requestAnimationFrame(() => {
        if (lastCanvas) void renderTo(lastCanvas);
      });
    }
  }
}
