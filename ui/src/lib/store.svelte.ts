/// The only client-side document mirror. Rust owns validation, rendering and history.
import { command, binary, desktop, invoke } from "./bridge";
import { saveRecovery, readRecovery } from "./persistence";
import { clearGeometryCache } from "./geometry";
import { AudioTransport, type AudioMeter, type AudioChunkMeta } from "./audio/transport";
import { AUDIO_RATE, emptyAudio, type AudioArrangement } from "./audio/model";
import {
  parsePreviewPacket,
  type PreviewMetadata,
  type PreviewStatus,
  type PreviewBackend,
  type PreviewQuality,
} from "./preview";
import {
  evaluate,
  ease,
  lerp,
  ticksPerFrame,
  snapToFrame,
  findKeyframeAtTime,
  DEFAULT_EASING,
  TICKS_PER_SEC,
  type Comp,
  type Layer,
  type LayerKind,
  type Op,
  type Project,
  type Property,
  type PropValue,
  type Snapshot,
  type SnapshotPatch,
  mergeSnapshotPatch,
  type EffectManifest,
  type EffectInstance,
  type EffectValue,
  type ParamDef,
  type Track,
  type Easing,
  type Keyframe,
  type Camera3D,
  DEFAULT_CAMERA,
} from "./model";
import { cameraAt, effectiveDepth } from "./geometry";

/** Live export telemetry mirrored from the Rust runtime's `export_progress`,
 * with client-side rate math (percent, fps, ETA) for the export HUD. */
export interface ExportProgress {
  active: boolean;
  canceled: boolean;
  framesDone: number;
  totalFrames: number;
  /** 0 idle · 1 rendering+encoding · 2 finalizing. */
  stage: number;
  startedMs: number;
  lastFrameMs: number;
  percent: number;
  elapsedSec: number;
  etaSec: number;
  fps: number;
}

class EditorState {
  project = $state.raw(null as Project | null);
  activeComp = $state(null as number | null);
  selected = $state(null as number | null);
  currentTime = $state(144000);
  playing = $state(false);
  renderSeq = $state(0);
  documentEpoch = $state(0);
  revision = $state(0);
  canUndo = $state(false);
  canRedo = $state(false);
  history = $state([] as string[]);
  dirty = $state(false);
  pending = $state(0);
  loading = $state(true);
  initError = $state("");
  renderError = $state("");
  frameMs = $state(0);
  previewQuality = $state("auto" as PreviewQuality);
  previewBackend = $state("auto" as PreviewBackend);
  previewStatus = $state.raw(null as PreviewStatus | null);
  previewMetadata = $state.raw(null as PreviewMetadata | null);
  previewProtocol = $state(0);
  renderedTime = $state(0);
  effects = $state.raw([] as EffectManifest[]);
  ffmpeg = $state(false);
  projectVersion = $state(2);
  audioProtocol = $state(0);
  timelineMode = $state("layers" as "layers" | "audio");
  audioBusSelection = $state<string | null>(null);
  audioProcessingMeters = $state.raw<NonNullable<AudioChunkMeta["processing"]>>([]);
  audioSelection = $state.raw(null as { track: string; clip: string | null } | null);
  audioCursor = $state(57600);
  audioStatus = $state("Ready");
  audioUnderruns = $state(0);
  audioDeviceRate = $state(0);
  audioImporting = $state(false);
  imageImporting = $state(false);
  audioScrub = $state(false);
  audioLoop = $state(true);
  audioSnap = $state("frame" as "sample" | "frame" | "beat");
  audioMeter = $state.raw({ peak: [0, 0], rms: [0, 0] } as AudioMeter);
  audioTrackMeters = $state.raw([] as AudioChunkMeta["tracks"]);
  monitorVolume = $state(1);
  audioStarting = $state(false);
  workspace = $state("Design" as "Design" | "Color" | "Animate" | "Audio");
  sidebar = $state("project" as "project" | "effects" | "motion" | "audio");
  inspector = $state("properties" as "properties" | "effects");
  tool = $state("select" as "select" | "hand" | "rotate" | "orbit");
  showGuides = $state(false);
  bypassEffects = $state(false);
  graphProperty = $state(null as Property | null);
  previewLayer = $state.raw(null as Layer | null);
  liveEdit = $state.raw<null | {
    comp: number;
    layer: number;
    draft: Layer;
    token: number;
    group: string;
  }>(null);

  histogram = $state.raw([[], [], []] as number[][]);
  toast = $state(null as { message: string; error: boolean } | null);
  dialog = $state(
    null as
      | { kind: "composition"; compId: number | null }
      | { kind: "export" | "shortcuts" | "new-project" }
      | { kind: "rename-layer"; compId: number; layerId: number; name: string }
      | null,
  );
  exporting = $state(false);
  exportProgress = $state(null as ExportProgress | null);
  recovery = $state("Ready" as string);
  interaction = $state.raw<null | {
    layer: number;
    comp: number;
    property: Property;
    value: PropValue;
    time: number;
    sequence: number;
    gesture: number;
    phase: "drag" | "commit";
    committedRevision?: number;
  }>(null);
  interactionProtocol = $state(0);
  interactionReady = $state(false);
  deltaProtocol = $state(0);
  timelineGesture = $state(false);
  visibilityPending = $state.raw<Record<string, { value: boolean; token: number }>>({});
  lockPending = $state.raw<Record<string, { value: boolean; token: number }>>({});
  nativeInteractionSequence = $state(0);
  refineInteractionSequence = $state(0);

  audioRevision = $state(0);
}
export const editor = new EditorState();

export const audioTransport = new AudioTransport();
audioTransport.onMeter = (meter, tracks, processing) => {
  editor.audioMeter = meter;
  editor.audioTrackMeters = tracks;
  editor.audioProcessingMeters = processing ?? [];
};
audioTransport.onStatus = (status, underflows) => {
  editor.audioStatus = status;
  editor.audioUnderruns = underflows;
  editor.audioDeviceRate = audioTransport.context?.sampleRate ?? 0;
};
audioTransport.onError = (message) => notify(message, true);
audioTransport.onEnd = () => pause();
let playToken = 0;
let mutationQueue = Promise.resolve();
let recoveryTimer: ReturnType<typeof setTimeout>;
let toastTimer: ReturnType<typeof setTimeout>;
let raf: number | null = null;
let lastTimestamp = 0;
let remainder = 0;

export function activeComp(): Comp | null {
  return editor.project?.comps[String(editor.activeComp)] ?? null;
}
export function selectedLayer(): Layer | null {
  return activeComp()?.layers[String(editor.selected)] ?? null;
}
export function clone<T>(value: T): T {
  try {
    return structuredClone(value);
  } catch {
    return JSON.parse(JSON.stringify(value));
  }
}
export function notify(message: string, error = false) {
  editor.toast = { message, error };
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (editor.toast = null), error ? 9000 : 4000);
}
function queueRecovery() {
  clearTimeout(recoveryTimer);
  editor.recovery = "Saving recovery…";
  recoveryTimer = setTimeout(async () => {
    if (!editor.project) return;
    if (editor.interaction || editor.timelineGesture || editor.liveEdit) {
      queueRecovery();
      return;
    }
    const revision = editor.revision;
    try {
      // Audio blobs are excluded from normal snapshots. Fetch a full portable
      // document only after idle time, never on every waveform/mixer gesture.
      const hasAudioAssets = Object.values(editor.project.media).some((m) => m.audio);
      const data = hasAudioAssets
        ? await command<string>("save_project", { compact: true })
        : editor.project;
      if (revision !== editor.revision) return;
      await saveRecovery(data, editor.projectVersion);
      editor.recovery = "Recovery saved";
    } catch {
      editor.recovery = "Recovery unavailable — save a file";
    }
  }, 1500);
}

export function accept(snapshot: Snapshot | SnapshotPatch, dirty = true) {
  const previousAudioRevision = editor.audioRevision;
  if ("kind" in snapshot && snapshot.kind === "patch") {
    if (!editor.project || snapshot.baseRevision !== editor.revision)
      throw new Error("Project revision mismatch; reload to resynchronize");
    editor.project = mergeSnapshotPatch(editor.project, snapshot);
  } else editor.project = (snapshot as Snapshot).project;
  editor.canUndo = snapshot.canUndo;
  editor.canRedo = snapshot.canRedo;
  editor.history = snapshot.history;
  editor.revision = snapshot.revision;
  editor.audioRevision = snapshot.audioRevision ?? snapshot.revision;
  if (!editor.project.comps[String(editor.activeComp)])
    editor.activeComp = Object.values(editor.project.comps)[0]?.id ?? null;
  if (!activeComp()?.layers[String(editor.selected)]) editor.selected = null;
  if (editor.interaction) {
    const i = editor.interaction,
      l = editor.project.comps[String(i.comp)]?.layers[String(i.layer)];
    if (!l || !l.visible || l.locked || editor.activeComp !== i.comp) {
      editor.interaction = null;
      editor.interactionReady = false;
    }
  }
  const comp = activeComp();
  if (comp)
    editor.currentTime = Math.min(
      editor.currentTime,
      Math.max(0, comp.duration - ticksPerFrame(comp.fps)),
    );
  editor.renderSeq++;
  editor.dirty = dirty;
  if (editor.audioStarting && previousAudioRevision !== editor.audioRevision) pause();
  if (editor.playing && previousAudioRevision !== editor.audioRevision) {
    if (hasAudio()) void restartAudio();
    else {
      audioTransport.stop();
      lastTimestamp = performance.now();
      remainder = 0;
    }
  }
  if (
    editor.audioBusSelection &&
    !activeComp()?.audio?.buses?.some((b) => b.id === editor.audioBusSelection)
  )
    editor.audioBusSelection = null;
  if (editor.audioSelection) {
    const track = activeComp()?.audio?.tracks.find((t) => t.id === editor.audioSelection?.track);
    if (!track) editor.audioSelection = null;
    else if (
      editor.audioSelection.clip &&
      !track.clips.some((c) => c.id === editor.audioSelection?.clip)
    )
      editor.audioSelection = { track: track.id, clip: null };
  }
  queueRecovery();
}
/** Resolves after every queued mutation has been applied to the local store.
 * UI read-modify-write gestures (e.g. nudging an easing handle right after a
 * preset click) await this so they read merged state, not stale copies. */
export function opSettled(): Promise<void> {
  return mutationQueue.then(() => undefined);
}

/** Pull the bridge state and merge it when somebody else moved the project
 * (an AI session, a second tab). Runs through the same mutation queue as
 * edits, so it can never race a local mutation. Skipped while a local edit
 * is in flight, during playback, or while the tab is hidden. */
async function reconcileExternalEdits(): Promise<boolean> {
  return queued(async () => {
    if (!editor.project) return;
    if (editor.pending > 1 || editor.playing || pendingLive !== null) return;
    if (typeof document !== "undefined" && document.hidden) return;
    const snapshot = (await command<Snapshot | SnapshotPatch>("state", {})) as
      | Snapshot
      | SnapshotPatch;
    if (snapshot.revision !== editor.revision) accept(snapshot);
  }).then(() => true).catch(() => false);
}
let reconcileTimer: ReturnType<typeof setTimeout> | null = null;
export function startReconciling() {
  if (reconcileTimer !== null) return;
  const tick = () => {
    void reconcileExternalEdits().finally(() => {
      reconcileTimer = setTimeout(tick, 1200);
    });
  };
  reconcileTimer = setTimeout(tick, 1200);
}
export function stopReconciling() {
  if (reconcileTimer !== null) clearTimeout(reconcileTimer);
  reconcileTimer = null;
}
export async function queued(action: () => Promise<void>): Promise<boolean> {
  editor.pending++;
  let succeeded = false;
  const next = mutationQueue.then(async () => {
    try {
      await action();
      succeeded = true;
    } catch (error) {
      notify(String(error instanceof Error ? error.message : error), true);
    } finally {
      editor.pending--;
    }
  });
  mutationQueue = next;
  await next;
  return succeeded;
}

export async function init() {
  editor.loading = true;
  startReconciling();
  editor.initError = "";
  try {
    try {
      const saved = JSON.parse(localStorage.getItem("bonaparte-preview-v3") ?? "null");
      if (["auto", "1", "2", "4"].includes(saved?.quality)) editor.previewQuality = saved.quality;
      if (["auto", "cpu", "gpu"].includes(saved?.backend)) editor.previewBackend = saved.backend;
    } catch {
      /* Preview preferences are optional, never project data. */
    }
    const [snapshot, catalog] = await Promise.all([
      command<Snapshot>("state"),
      command<{
        effects: EffectManifest[];
        ffmpeg: boolean;
        preview?: PreviewStatus;
        projectVersion?: number;
        deltaProtocol?: number;
        interactionProtocol?: number;
        audio?: { protocol: number };
      }>("catalog"),
    ]);
    editor.effects = catalog.effects;
    editor.ffmpeg = catalog.ffmpeg;
    editor.projectVersion = catalog.projectVersion ?? 2;
    editor.deltaProtocol = catalog.deltaProtocol ?? 0;
    editor.interactionProtocol = catalog.interactionProtocol ?? 0;
    editor.audioProtocol = catalog.audio?.protocol ?? 0;
    editor.previewStatus = catalog.preview ?? null;
    editor.previewProtocol = catalog.preview?.protocol ?? 0;
    let state = snapshot;
    if (snapshot.revision === 0) {
      const recovery = await readRecovery().catch(() => null);
      if (recovery) {
        try {
          state = await command<Snapshot>("open_project", { json: recovery });
          setTimeout(
            () => notify("Restored your unsaved work from the last session."),
            600,
          );
          notify("Your last session was recovered.");
        } catch {
          notify("The saved recovery could not be opened. Your original file is unchanged.", true);
        }
      }
    }
    editor.documentEpoch++;
    accept(state, false);
    editor.selected =
      Object.values(activeComp()?.layers ?? {}).find((l) => l.name === "Orbital form")?.id ??
      activeComp()?.layer_order.at(-1) ??
      null;
    if (hasAudio()) setWorkspace("Audio");
    await document.fonts.load('14px "Bonaparte Sans"');
    clearGeometryCache();
    editor.renderSeq++;
  } catch (error) {
    editor.initError = String(error instanceof Error ? error.message : error);
  } finally {
    editor.loading = false;
  }
}

export async function applyOp(
  op: Op | ((project: Project) => Op | null),
  editGroup?: string,
): Promise<boolean> {
  return queued(async () => {
    if (!editor.project) return;
    const value = typeof op === "function" ? op(editor.project) : op;
    if (value)
      accept(
        await command<Snapshot | SnapshotPatch>("apply", {
          op: value,
          delta: !!editor.deltaProtocol,
          baseRevision: editor.revision,
          editGroup,
        }),
      );
  });
}
export async function undoOp() {
  await flushLiveEdits();
  editor.liveEdit = null;
  pause();
  return queued(async () => {
    const response = await command<Snapshot | SnapshotPatch>("undo", {
      delta: !!editor.deltaProtocol,
      baseRevision: editor.revision,
    });
    accept(response);
    // The bridge names what actually reverted (drained undos stay silent).
    const label = (response as { lastUndone?: string }).lastUndone;
    if (label) notify(`Undid: ${label}`);
  });
}
export async function redoOp() {
  await flushLiveEdits();
  editor.liveEdit = null;
  pause();
  return queued(async () => {
    const response = await command<Snapshot | SnapshotPatch>("redo", {
      delta: !!editor.deltaProtocol,
      baseRevision: editor.revision,
    });
    accept(response);
    const label = (response as { lastRedone?: string }).lastRedone;
    if (label) notify(`Redid: ${label}`);
  });
}
export function selectComp(id: number) {
  void flushLiveEdits();
  editor.liveEdit = null;
  cancelInteraction();
  pause();
  editor.activeComp = id;
  editor.audioSelection = null;
  editor.audioBusSelection = null;
  editor.audioCursor = 0;
  editor.selected = null;
  editor.currentTime = 0;
  editor.previewLayer = null;
  editor.graphProperty = null;
}
export function setWorkspace(workspace: typeof editor.workspace) {
  editor.workspace = workspace;
  if (workspace === "Audio") {
    editor.timelineMode = "audio";
    editor.sidebar = "audio";
    editor.graphProperty = null;
    editor.selected = null;
    return;
  }
  editor.timelineMode = "layers";
  if (workspace === "Color") {
    editor.inspector = "effects";
    const grade = Object.values(activeComp()?.layers ?? {}).find((l) =>
      l.effects.some((e) => e.effect_id === "builtin.color_grade"),
    );
    if (grade) editor.selected = grade.id;
  } else if (workspace === "Animate") {
    editor.sidebar = "motion";
    editor.inspector = "properties";
  } else {
    editor.sidebar = "project";
    editor.inspector = "properties";
  }
}
export function hasAudio(id = editor.activeComp, seen = new Set<number>()): boolean {
  if (!id || seen.has(id) || !editor.audioProtocol) return false;
  seen.add(id);
  const comp = editor.project?.comps[String(id)];
  return (
    !!comp &&
    (!!comp.audio?.tracks.some((t) => t.clips.length) ||
      Object.values(comp.layers).some(
        (l) => l.visible && "PreComp" in l.kind && hasAudio(l.kind.PreComp.comp, seen),
      ))
  );
}
export function arrangement(): AudioArrangement {
  return activeComp()?.audio ?? emptyAudio();
}
function transportState() {
  const comp = activeComp();
  if (!comp) throw new Error("No active composition");
  return {
    compId: comp.id,
    revision: editor.audioRevision,
    durationTicks: comp.duration,
    loop: editor.audioLoop,
  };
}
export function seekAudio(frame: number) {
  const comp = activeComp();
  if (!comp) return;
  editor.audioCursor = Math.max(
    0,
    Math.min(Math.ceil((comp.duration * AUDIO_RATE) / TICKS_PER_SEC) - 1, Math.round(frame)),
  );
  editor.currentTime = Math.max(
    0,
    Math.floor((editor.audioCursor * TICKS_PER_SEC) / AUDIO_RATE / ticksPerFrame(comp.fps)) *
      ticksPerFrame(comp.fps),
  );
  if (editor.playing) void restartAudio(true);
  else if (editor.audioScrub && hasAudio())
    audioTransport.audition(transportState(), (editor.audioCursor * TICKS_PER_SEC) / AUDIO_RATE);
}
export function scrub(time: number) {
  const comp = activeComp();
  if (!comp) return;
  editor.currentTime = Math.max(
    0,
    Math.min(Math.max(0, comp.duration - ticksPerFrame(comp.fps)), snapToFrame(time, comp.fps)),
  );
  editor.audioCursor = Math.round((editor.currentTime * AUDIO_RATE) / TICKS_PER_SEC);
  if (editor.playing && hasAudio()) void restartAudio(true);
  else if (editor.audioScrub && hasAudio())
    audioTransport.audition(transportState(), editor.currentTime);
}
async function restartAudio(seek = false) {
  if (!editor.playing || !hasAudio()) return;
  try {
    if (seek)
      await audioTransport.start(
        transportState(),
        (editor.audioCursor * TICKS_PER_SEC) / AUDIO_RATE,
      );
    else await audioTransport.update(transportState());
  } catch (error) {
    pause();
    notify(String(error), true);
  }
}
export async function play() {
  if (editor.playing || editor.audioStarting || !activeComp()) return;
  const token = ++playToken;
  if (hasAudio()) {
    editor.audioStarting = true;
    try {
      if (
        !(await audioTransport.start(
          transportState(),
          (editor.audioCursor * TICKS_PER_SEC) / AUDIO_RATE,
        )) ||
        token !== playToken
      )
        return;
    } catch (error) {
      notify(String(error), true);
      return;
    } finally {
      editor.audioStarting = false;
    }
  }
  if (token !== playToken) return;
  editor.playing = true;
  lastTimestamp = performance.now();
  remainder = 0;
  const tick = (now: number) => {
    const comp = activeComp();
    if (!editor.playing || !comp) return;
    const tpf = ticksPerFrame(comp.fps);
    if (hasAudio()) {
      const time = audioTransport.positionTicks();
      if (time !== null) {
        editor.audioCursor = Math.round((time * AUDIO_RATE) / TICKS_PER_SEC);
        editor.currentTime = Math.floor(time / tpf) * tpf;
      }
    } else {
      remainder += (Math.min(now - lastTimestamp, 1000) * TICKS_PER_SEC) / 1000;
      const frames = Math.floor(remainder / tpf);
      if (frames) {
        const next = editor.currentTime + frames * tpf;
        if (!editor.audioLoop && next >= comp.duration) {
          pause();
          return;
        }
        editor.currentTime = next % Math.max(tpf, Math.ceil(comp.duration / tpf) * tpf);
        remainder %= tpf;
        editor.audioCursor = Math.round((editor.currentTime * AUDIO_RATE) / TICKS_PER_SEC);
      }
    }
    lastTimestamp = now;
    raf = requestAnimationFrame(tick);
  };
  raf = requestAnimationFrame(tick);
}
export function pause() {
  playToken++;
  editor.playing = false;
  editor.audioStarting = false;
  audioTransport.stop();
  if (raf !== null) cancelAnimationFrame(raf);
  raf = null;
}

export function newLayer(kind: LayerKind, name: string, duration: number): Layer {
  return {
    id: 0,
    name,
    kind,
    start: 0,
    duration,
    parent: null,
    blend_mode: "Normal",
    visible: true,
    locked: false,
    transform: {
      position: [0, 0],
      scale: [100, 100],
      rotation: 0,
      opacity: 1,
      anchor_point: [0, 0],
    },
    tracks: {},
    effects: [],
  };
}
export async function addLayer(type: "rectangle" | "circle" | "text" | "solid" | "adjustment") {
  const comp = activeComp();
  if (!comp) return;
  let kind: LayerKind;
  const mint: [number, number, number, number] = [0.51, 0.78, 0.43, 1];
  if (type === "text")
    kind = {
      Text: {
        text: "Your next idea.",
        size: 64,
        style: { color: [1, 1, 1, 1], bold: true, tracking: 0 },
      },
    };
  else if (type === "solid") kind = { Solid: { color: [0.04, 0.06, 0.05, 1] } };
  else if (type === "adjustment") kind = { Adjustment: {} };
  else
    kind = {
      Shape: {
        color: mint,
        generator: type === "circle" ? "builtin.circle" : null,
        style: { size: [300, 300], corner_radius: 0, stroke_width: 0, stroke_color: [1, 1, 1, 1] },
      },
    };
  const name = {
    rectangle: "Rectangle",
    circle: "Ellipse",
    text: "Text",
    solid: "Solid",
    adjustment: "Adjustment layer",
  }[type];
  if (
    await applyOp({ type: "addLayer", comp: comp.id, layer: newLayer(kind, name, comp.duration) })
  ) {
    editor.selected = (editor.project?.next_layer ?? 1) - 1;
    editor.inspector = "properties";
  }
}
let toggleToken = 0;
export function layerVisibility(compId: number, layer: Layer): boolean {
  return editor.visibilityPending[`${compId}:${layer.id}`]?.value ?? layer.visible;
}
export function layerLocked(compId: number, layer: Layer): boolean {
  return editor.lockPending[`${compId}:${layer.id}`]?.value ?? layer.locked;
}
export function cancelInteraction() {
  editor.interaction = null;
  editor.interactionReady = false;
  editor.nativeInteractionSequence = 0;
  editor.refineInteractionSequence = 0;
  editor.renderSeq++;
}
export async function toggleLayerVisibility(compId: number, layerId: number) {
  await flushLiveEdits();
  const l = editor.project?.comps[String(compId)]?.layers[String(layerId)];
  if (!l) return;
  const key = `${compId}:${layerId}`,
    token = ++toggleToken,
    value = !layerVisibility(compId, l);
  editor.visibilityPending = { ...editor.visibilityPending, [key]: { value, token } };
  cancelInteraction();
  try {
    await applyOp({ type: "setLayerVisible", comp: compId, layer: layerId, visible: value });
  } finally {
    if (editor.visibilityPending[key]?.token === token) {
      const pending = { ...editor.visibilityPending };
      delete pending[key];
      editor.visibilityPending = pending;
    }
  }
}
export async function toggleLayerLock(compId: number, layerId: number) {
  await flushLiveEdits();
  const l = editor.project?.comps[String(compId)]?.layers[String(layerId)];
  if (!l) return;
  const key = `${compId}:${layerId}`,
    token = ++toggleToken,
    locked = !layerLocked(compId, l);
  editor.lockPending = { ...editor.lockPending, [key]: { value: locked, token } };
  cancelInteraction();
  try {
    await applyOp({ type: "setLayerLocked", comp: compId, layer: layerId, locked });
  } finally {
    if (editor.lockPending[key]?.token === token) {
      const pending = { ...editor.lockPending };
      delete pending[key];
      editor.lockPending = pending;
    }
  }
}

type PendingLive = {
  comp: number;
  layer: number;
  time: number;
  group: string;
  token: number;
  kind?: LayerKind;
  properties: Partial<Record<Property, PropValue>>;
  parameters: Map<string, { effect: string; param: string; value: EffectValue }>;
};
let pendingLive: PendingLive | null = null;
let liveTimer: ReturnType<typeof setTimeout>;
let liveToken = 0;
let lastLiveCommit: Promise<boolean> = Promise.resolve(true);
export function editingLayer(): Layer | null {
  const layer = selectedLayer(),
    edit = editor.liveEdit;
  if (!layer || !edit || edit.comp !== editor.activeComp || edit.layer !== layer.id) return layer;
  return {
    ...edit.draft,
    visible: layer.visible,
    locked: layer.locked,
    parent: layer.parent,
    start: layer.start,
    duration: layer.duration,
  };
}
function liveSession(group: string): { pending: PendingLive; draft: Layer } | null {
  const layer = editingLayer(),
    comp = activeComp();
  if (!layer || !comp || layer.locked) return null;
  if (editor.playing) pause();
  if (
    pendingLive &&
    (pendingLive.group !== group || pendingLive.comp !== comp.id || pendingLive.layer !== layer.id)
  )
    void flushLiveEdits();
  if (!pendingLive) {
    pendingLive = {
      comp: comp.id,
      layer: layer.id,
      time: editor.currentTime,
      group,
      token: 0,
      properties: {},
      parameters: new Map(),
    };
  }
  if (editor.liveEdit?.group !== group) {
    cancelInteraction();
    editor.renderSeq++;
  }
  return { pending: pendingLive, draft: clone(layer) };
}
function publishLive(pending: PendingLive, draft: Layer) {
  editor.dirty = true;
  pending.token = ++liveToken;
  editor.liveEdit = {
    comp: pending.comp,
    layer: pending.layer,
    draft,
    token: pending.token,
    group: pending.group,
  };
  clearTimeout(liveTimer);
  liveTimer = setTimeout(() => void flushLiveEdits(), 140);
}
export function liveContent(group: string, update: (kind: LayerKind) => void) {
  const state = liveSession(group);
  if (!state) return;
  update(state.draft.kind);
  state.pending.kind = clone(state.draft.kind);
  publishLive(state.pending, state.draft);
}
export function liveProperty(group: string, property: Property, value: PropValue) {
  const state = liveSession(group);
  if (!state) return;
  state.pending.properties[property] = value;
  delete state.draft.tracks[property];
  if (property === "Position" && "Vec2" in value) state.draft.transform.position = value.Vec2;
  if (property === "Scale" && "Vec2" in value) state.draft.transform.scale = value.Vec2;
  if (property === "AnchorPoint" && "Vec2" in value)
    state.draft.transform.anchor_point = value.Vec2;
  if (property === "Rotation" && "Scalar" in value) state.draft.transform.rotation = value.Scalar;
  if (property === "Opacity" && "Scalar" in value) state.draft.transform.opacity = value.Scalar;
  publishLive(state.pending, state.draft);
}
export function liveEffect(group: string, effectId: string, param: ParamDef, value: EffectValue) {
  const state = liveSession(group);
  if (!state) return;
  const effect = state.draft.effects.find((e) => e.id === effectId);
  if (!effect) return;
  effect.params[param.id] = value;
  delete effect.tracks[param.id];
  state.pending.parameters.set(`${effectId}:${param.id}`, {
    effect: effectId,
    param: param.id,
    value,
  });
  publishLive(state.pending, state.draft);
}
export function flushLiveEdits(): Promise<boolean> {
  clearTimeout(liveTimer);
  const pending = pendingLive;
  pendingLive = null;
  if (!pending) return lastLiveCommit;
  lastLiveCommit = applyOp((project) => {
    const layer = project.comps[String(pending.comp)]?.layers[String(pending.layer)];
    if (!layer || layer.locked) return null;
    const ops: Op[] = [];
    if (pending.kind)
      ops.push({
        type: "setLayerContent",
        comp: pending.comp,
        layer: pending.layer,
        kind: pending.kind,
      });
    for (const [key, value] of Object.entries(pending.properties)) {
      const property = key as Property;
      if (layer.tracks[property]?.keys.length)
        ops.push({
          type: "addKeyframe",
          comp: pending.comp,
          layer: pending.layer,
          property,
          key: {
            time: pending.time,
            value,
            easing:
              findKeyframeAtTime(
                layer.tracks[property],
                pending.time,
                project.comps[String(pending.comp)].fps,
              )?.easing ?? DEFAULT_EASING,
          },
        });
      else
        ops.push({ type: "setValue", comp: pending.comp, layer: pending.layer, property, value });
    }
    if (pending.parameters.size) {
      const effects = clone(layer.effects);
      for (const item of pending.parameters.values()) {
        const effect = effects.find((e) => e.id === item.effect);
        if (!effect) continue;
        if (
          effect.tracks[item.param]?.keys.length &&
          ("Float" in item.value || "Point" in item.value)
        ) {
          const keys = effect.tracks[item.param].keys,
            old = keys.find((k) => k.time === pending.time);
          effect.tracks[item.param].keys = [
            ...keys.filter((k) => k.time !== pending.time),
            {
              time: pending.time,
              value:
                "Float" in item.value ? { Scalar: item.value.Float } : { Vec2: item.value.Point },
              easing: old?.easing ?? DEFAULT_EASING,
            },
          ].sort((a, b) => a.time - b.time);
        } else effect.params[item.param] = item.value;
      }
      ops.push({ type: "setLayerEffects", comp: pending.comp, layer: pending.layer, effects });
    }
    return ops.length ? { type: "batch", label: `Edited ${layer.name}`, ops } : null;
  }, pending.group).then((ok) => {
    if (editor.liveEdit?.token === pending.token) {
      editor.liveEdit = null;
      editor.renderSeq++;
    }
    return ok;
  });
  return lastLiveCommit;
}

export async function duplicateSelected() {
  const layer = selectedLayer(),
    comp = activeComp();
  if (!layer || !comp || layer.locked) return;
  const copy = clone(layer);
  copy.name += " copy";
  if (await applyOp({ type: "addLayer", comp: comp.id, layer: copy }))
    editor.selected = (editor.project?.next_layer ?? 1) - 1;
}
export async function deleteSelected() {
  const layer = selectedLayer(),
    comp = activeComp();
  if (!layer || !comp || layer.locked) return;
  await applyOp({ type: "removeLayer", comp: comp.id, layer: layer.id });
}
export async function setProperty(layerId: number, property: Property, value: PropValue) {
  const comp = activeComp();
  if (!comp) return;
  const time = snapToFrame(editor.currentTime, comp.fps);
  return applyOp((project) => {
    const layer = project.comps[String(comp.id)]?.layers[String(layerId)];
    if (!layer || layer.locked) return null;
    if (layer.tracks[property]?.keys.length)
      return {
        type: "addKeyframe",
        comp: comp.id,
        layer: layerId,
        property,
        key: {
          time,
          value,
          easing:
            findKeyframeAtTime(layer.tracks[property], time, comp.fps)?.easing ?? DEFAULT_EASING,
        },
      };
    return { type: "setValue", comp: comp.id, layer: layerId, property, value };
  });
}
export async function keyframeAtPlayhead(layerId: number, property: Property) {
  const comp = activeComp(),
    layer = comp?.layers[String(layerId)];
  if (!comp || !layer || layer.locked) return;
  await applyOp({
    type: "addKeyframe",
    comp: comp.id,
    layer: layerId,
    property,
    key: {
      time: snapToFrame(editor.currentTime, comp.fps),
      value: evaluate(layer, property, editor.currentTime),
      easing: DEFAULT_EASING,
    },
  });
}
export async function toggleKeyframeAtPlayhead(layerId: number, property: Property) {
  const comp = activeComp(),
    layer = comp?.layers[String(layerId)];
  if (!comp || !layer || layer.locked) return;
  const existing = findKeyframeAtTime(layer.tracks[property], editor.currentTime, comp.fps);
  if (existing)
    await applyOp({
      type: "removeKeyframe",
      comp: comp.id,
      layer: layerId,
      property,
      time: existing.time,
    });
  else await keyframeAtPlayhead(layerId, property);
}

export function defaultParam(param: ParamDef): EffectValue {
  const k = param.kind;
  if ("Slider" in k) return { Float: k.Slider.default };
  if ("Color" in k) return { Color: [...k.Color.default] };
  if ("Point" in k) return { Point: [...k.Point.default] };
  if ("Checkbox" in k) return { Bool: k.Checkbox.default };
  return { Index: k.Dropdown.default };
}
export function evaluateTrack(track: Track | undefined, time: number): PropValue | null {
  const keys = track?.keys;
  if (!keys?.length) return null;
  if (time <= keys[0].time) return keys[0].value;
  const last = keys.at(-1)!;
  if (time >= last.time) return last.value;
  const index = keys.findIndex((k) => k.time > time);
  const a = keys[index - 1],
    b = keys[index];
  return lerp(a.value, b.value, ease(a.easing, (time - a.time) / (b.time - a.time)));
}
export function effectValue(effect: EffectInstance, param: ParamDef): EffectValue {
  const animated = evaluateTrack(effect.tracks[param.id], editor.currentTime);
  if (animated && "Scalar" in animated && "Slider" in param.kind)
    return {
      Float: Math.max(param.kind.Slider.min, Math.min(param.kind.Slider.max, animated.Scalar)),
    };
  if (animated && "Vec2" in animated) return { Point: animated.Vec2 };
  return effect.params[param.id] ?? defaultParam(param);
}
export async function updateEffects(update: (effects: EffectInstance[], layer: Layer) => void) {
  const comp = activeComp(),
    layerId = editor.selected;
  if (!comp || layerId === null) return false;
  return applyOp((project) => {
    const layer = project.comps[String(comp.id)]?.layers[String(layerId)];
    if (!layer || layer.locked) return null;
    const effects = clone(layer.effects);
    update(effects, layer);
    return { type: "setLayerEffects", comp: comp.id, layer: layerId, effects };
  });
}
export async function addEffect(id: string) {
  const manifest = editor.effects.find((e) => e.id === id);
  if (!manifest) return;
  if (!selectedLayer()) await addLayer("adjustment");
  if (selectedLayer()?.locked) {
    notify("Unlock this layer before adding an effect.", true);
    return;
  }
  await updateEffects((effects) =>
    effects.push({ id: crypto.randomUUID(), effect_id: id, enabled: true, params: {}, tracks: {} }),
  );
  editor.inspector = "effects";
  notify(`${manifest.name} added to the effect stack.`);
}
export function previewEffect(id: string, param: ParamDef, value: EffectValue) {
  const layer = selectedLayer();
  if (!layer || layer.locked) return;
  const preview = clone(layer),
    effect = preview.effects.find((e) => e.id === id);
  if (!effect) return;
  effect.params[param.id] = value;
  delete effect.tracks[param.id];
  editor.previewLayer = preview;
}
export async function setEffectParam(id: string, param: ParamDef, value: EffectValue) {
  const time = editor.currentTime;
  await updateEffects((effects) => {
    const effect = effects.find((e) => e.id === id);
    if (!effect) return;
    if (effect.tracks[param.id]?.keys.length && ("Float" in value || "Point" in value)) {
      const keys = effect.tracks[param.id].keys;
      const existing = keys.find((k) => k.time === time);
      const key = {
        time,
        value: "Float" in value ? { Scalar: value.Float } : { Vec2: value.Point },
        easing: existing?.easing ?? DEFAULT_EASING,
      };
      effect.tracks[param.id].keys = [...keys.filter((k) => k.time !== time), key].sort(
        (a, b) => a.time - b.time,
      );
    } else effect.params[param.id] = value;
  });
  editor.previewLayer = null;
}
export async function toggleEffectKey(id: string, param: ParamDef) {
  const time = editor.currentTime;
  await updateEffects((effects) => {
    const effect = effects.find((e) => e.id === id);
    if (!effect) return;
    const value = effectValue(effect, param);
    if (!("Float" in value) && !("Point" in value)) return;
    const track = effect.tracks[param.id] ?? { keys: [] };
    if (track.keys.some((k) => k.time === time))
      track.keys = track.keys.filter((k) => k.time !== time);
    else
      track.keys = [
        ...track.keys,
        {
          time,
          value: "Float" in value ? { Scalar: value.Float } : { Vec2: value.Point },
          easing: DEFAULT_EASING,
        },
      ].sort((a, b) => a.time - b.time);
    if (track.keys.length) effect.tracks[param.id] = track;
    else delete effect.tracks[param.id];
  });
}

export function previewDivisor(): number {
  if (editor.previewProtocol < 3) return 1;
  if (editor.previewQuality !== "auto") return Number(editor.previewQuality);
  if (editor.interaction?.phase === "drag")
    return editor.interaction.property !== "Position" ||
      editor.refineInteractionSequence === editor.interaction.sequence
      ? 1
      : 2;
  if (editor.timelineGesture) return 2;
  return editor.playing ? 2 : 1;
}

export function setPreviewPreference(quality: PreviewQuality, backend: PreviewBackend) {
  editor.previewQuality = quality;
  editor.previewBackend = backend;
  editor.renderSeq++;
  try {
    localStorage.setItem("bonaparte-preview-v3", JSON.stringify({ quality, backend }));
  } catch {
    /* optional */
  }
}
export async function refreshPreviewStatus() {
  if (editor.previewProtocol < 3) return;
  try {
    editor.previewStatus = await command<PreviewStatus>("preview_status");
  } catch (error) {
    notify(String(error), true);
  }
}
export async function clearPreviewCache(retryGpu = false) {
  if (editor.previewProtocol < 3) return;
  try {
    editor.previewStatus = await command<PreviewStatus>(
      retryGpu ? "retry_gpu" : "clear_preview_cache",
    );
    editor.renderSeq++;
    notify(
      retryGpu
        ? "GPU device reset. The next frame will choose its backend again."
        : "Preview caches cleared. Your project and undo history are unchanged.",
    );
  } catch (error) {
    notify(String(error), true);
  }
}

let rendering = false,
  pendingRender = false;
let lastCanvas: HTMLCanvasElement | null = null;
export async function renderTo(canvas: HTMLCanvasElement) {
  lastCanvas = canvas;
  if (rendering) {
    pendingRender = true;
    return;
  }
  const comp = activeComp();
  if (!comp) return;
  if (
    editor.interaction?.phase === "drag" &&
    editor.nativeInteractionSequence === editor.interaction.sequence
  )
    return;
  if (
    editor.interaction?.phase === "drag" &&
    editor.interactionReady &&
    editor.refineInteractionSequence !== editor.interaction.sequence &&
    editor.previewQuality !== "1"
  )
    return;
  if (editor.interaction?.phase === "commit" && editor.interaction.committedRevision === undefined)
    return;
  rendering = true;
  const started = performance.now(),
    seq = editor.renderSeq,
    time = editor.currentTime;
  const divisor = previewDivisor(),
    backend = editor.previewBackend,
    bypass = editor.bypassEffects;
  const live = editor.liveEdit;
  const override =
    live && live.comp === comp.id
      ? { ...live.draft, visible: comp.layers[String(live.layer)]?.visible ?? live.draft.visible }
      : editor.previewLayer;
  const interaction = editor.interaction;
  const transform =
    interaction?.phase === "drag" && editor.interactionProtocol
      ? {
          compId: interaction.comp,
          layerId: interaction.layer,
          property: interaction.property,
          value: interaction.value,
        }
      : undefined;
  const interactionSequence = interaction?.sequence;
  const stale = () =>
    editor.activeComp !== comp.id ||
    seq !== editor.renderSeq ||
    !canvas.isConnected ||
    divisor !== previewDivisor() ||
    backend !== editor.previewBackend ||
    bypass !== editor.bypassEffects ||
    (!editor.playing && time !== editor.currentTime) ||
    !!override !== !!(editor.liveEdit?.comp === editor.activeComp || editor.previewLayer) ||
    (live?.group !== editor.liveEdit?.group && (!!live || !!editor.liveEdit)) ||
    (interactionSequence !== editor.interaction?.sequence &&
      (interaction?.phase === "drag" || editor.interaction?.phase === "drag") &&
      !(
        interaction?.phase === "drag" &&
        editor.interaction?.phase === "drag" &&
        interaction.gesture === editor.interaction.gesture &&
        interaction.layer === editor.interaction.layer
      ));
  try {
    const args = {
      compId: comp.id,
      time,
      bypassEffects: bypass,
      layerOverride: override,
      divisor,
      backend,
      transformOverride: transform,
    };
    const data = await binary(
      editor.previewProtocol >= 3 ? "preview_frame" : "render_frame_raw",
      args,
    );
    if (stale()) return;
    let width: number, height: number, bytes: Uint8ClampedArray<ArrayBuffer>;
    if (editor.previewProtocol >= 3) {
      const parsed = parsePreviewPacket(data);
      if (parsed.metadata.revision !== editor.revision) return;
      ({ width, height } = parsed.metadata);
      bytes = parsed.bytes;
      editor.previewMetadata = parsed.metadata;
    } else {
      const view = new DataView(data);
      width = view.getUint32(0, true);
      height = view.getUint32(4, true);
      if (data.byteLength !== 8 + width * height * 4)
        throw new Error("Invalid frame received from renderer");
      bytes = new Uint8ClampedArray(data, 8);
    }
    if (canvas.width !== width) canvas.width = width;
    if (canvas.height !== height) canvas.height = height;
    canvas.getContext("2d")?.putImageData(new ImageData(bytes, width, height), 0, 0);
    editor.frameMs = performance.now() - started;
    editor.renderedTime = time;
    canvas.dataset.previewTime = String(time);
    canvas.dataset.previewRevision = String(editor.revision);
    canvas.dataset.renderer = editor.previewMetadata?.backend ?? "cpu";
    canvas.dataset.divisor = String(divisor);
    canvas.dataset.cacheHit = String(editor.previewMetadata?.cacheHit ?? false);
    editor.renderError = "";
    if (editor.interaction?.phase === "drag" && editor.interaction.sequence === interactionSequence)
      editor.nativeInteractionSequence = interactionSequence;
    if (
      editor.interaction?.phase === "commit" &&
      editor.interaction.committedRevision !== undefined &&
      editor.interaction.committedRevision <= editor.revision
    ) {
      editor.interaction = null;
      editor.interactionReady = false;
    }
    const hist = [Array(64).fill(0), Array(64).fill(0), Array(64).fill(0)] as number[][];
    for (let i = 0; i < bytes.length; i += 32) {
      if (!bytes[i + 3]) continue;
      for (let c = 0; c < 3; c++) hist[c][bytes[i + c] >> 2]++;
    }
    editor.histogram = hist;
  } catch (error) {
    if (!stale()) {
      editor.renderError = String(error instanceof Error ? error.message : error);
      if (editor.interaction?.phase === "commit") {
        editor.interaction = null;
        editor.interactionReady = false;
      }
    }
  } finally {
    rendering = false;
    if (pendingRender && lastCanvas) {
      pendingRender = false;
      requestAnimationFrame(() => {
        if (lastCanvas?.isConnected) void renderTo(lastCanvas);
      });
    }
  }
}

/** One-click snapshot of the current frame as a PNG download. */
export async function snapshotFrame() {
  const comp = activeComp();
  if (!comp) return;
  await flushLiveEdits();
  await mutationQueue;
  pause();
  try {
    const data = await binary("export_png", {
      compId: comp.id,
      time: editor.currentTime,
      bypassEffects: false,
    });
    const stamp = new Date().toISOString().slice(0, 19).replace(/[:T]/g, "-");
    download(new Blob([data], { type: "image/png" }), `${filename()}-${stamp}.png`);
    notify("Frame snapshot saved.");
  } catch (error) {
    notify(error instanceof Error ? error.message : String(error), true);
  }
}

export function download(data: Blob, name: string) {
  const url = URL.createObjectURL(data),
    a = document.createElement("a");
  a.href = url;
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 10000);
}
function filename() {
  return (
    (editor.project?.name ?? "Untitled").replace(/[^\p{L}\p{N}\-_ ]/gu, "").trim() || "Untitled"
  );
}
export async function saveProject() {
  await flushLiveEdits();
  await mutationQueue;
  if (!editor.project) return;
  try {
    if (desktop) {
      const { save } = await import("@tauri-apps/plugin-dialog");
      const path = await save({
        defaultPath: `${filename()}.bonaparte`,
        filters: [{ name: "Bonaparte project", extensions: ["bonaparte"] }],
      });
      if (!path) return;
      await invoke("save_project_file", { path });
    } else {
      const text = await command<string>("save_project");
      download(new Blob([text], { type: "application/json" }), `${filename()}.bonaparte`);
    }
    editor.dirty = false;
    notify("Project saved with layers, animation, effects, and embedded images.");
  } catch (error) {
    notify(String(error), true);
  }
}
export async function openProject() {
  pause();
  if (
    editor.dirty &&
    !confirm(
      "Open another project? Save a project file first if you want to keep your current changes.",
    )
  )
    return;
  if (desktop) {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const path = await open({
        multiple: false,
        filters: [{ name: "Bonaparte project", extensions: ["bonaparte", "json"] }],
      });
      if (typeof path === "string")
        await queued(async () => {
          editor.documentEpoch++;
          accept(await invoke<Snapshot>("open_project_file", { path }), false);
          editor.currentTime = 0;
        });
    } catch (error) {
      notify(String(error), true);
    }
    return;
  }
  const input = document.createElement("input");
  input.type = "file";
  input.accept = ".bonaparte,.json";
  input.onchange = () => {
    const file = input.files?.[0];
    if (file) void openProjectFile(file);
  };
  input.click();
}
export async function openProjectFile(file: File) {
  await flushLiveEdits();
  editor.liveEdit = null;
  cancelInteraction();
  if (file.size > 64 * 1024 * 1024) {
    notify("Project exceeds the 64 MB limit.", true);
    return;
  }
  await queued(async () => {
    editor.documentEpoch++;
    accept(await command<Snapshot>("open_project", { json: await file.text() }), false);
    editor.currentTime = 0;
    editor.selected = null;
  });
}
export async function newProject(example = false) {
  await flushLiveEdits();
  editor.liveEdit = null;
  cancelInteraction();
  pause();
  await queued(async () => {
    editor.documentEpoch++;
    accept(await command<Snapshot>(example ? "load_example" : "new_project"), false);
    editor.activeComp = Object.values(editor.project!.comps)[0]?.id ?? null;
    editor.selected = null;
    editor.currentTime = 0;
  });
  editor.dialog = null;
}
export async function importImage(file?: File) {
  const comp = activeComp();
  if (!comp) return;
  if (!file) {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = "image/png,image/jpeg,image/webp";
    input.onchange = () => {
      const selected = input.files?.[0];
      if (selected) void importImage(selected);
    };
    input.click();
    return;
  }
  try {
    if (file.size > 20 * 1024 * 1024) throw new Error("Images must be smaller than 20 MB.");
    editor.imageImporting = true;
    notify("Importing image… decoding away from the canvas interaction thread.");
    let decoded: { width: number; height: number; ratio: number; rgbaBase64: string };
    if (typeof Worker !== "undefined" && typeof OffscreenCanvas !== "undefined") {
      decoded = await new Promise((resolve, reject) => {
        const worker = new Worker(new URL("./workers/image-import.worker.ts", import.meta.url), {
          type: "module",
        });
        const timeout = setTimeout(() => {
          worker.terminate();
          reject(new Error("Image decoding timed out"));
        }, 30000);
        worker.onmessage = ({ data }) => {
          clearTimeout(timeout);
          worker.terminate();
          data.error ? reject(new Error(data.error)) : resolve(data);
        };
        worker.onerror = (e) => {
          clearTimeout(timeout);
          worker.terminate();
          reject(new Error(e.message));
        };
        worker.postMessage({ file });
      });
    } else {
      const image = await createImageBitmap(file);
      const ratio = Math.min(1, 2048 / Math.max(image.width, image.height));
      const canvas = document.createElement("canvas");
      canvas.width = Math.max(1, Math.round(image.width * ratio));
      canvas.height = Math.max(1, Math.round(image.height * ratio));
      const context = canvas.getContext("2d")!;
      context.drawImage(image, 0, 0, canvas.width, canvas.height);
      image.close();
      const pixels = context.getImageData(0, 0, canvas.width, canvas.height).data;
      let bytes = "";
      for (let i = 0; i < pixels.length; i += 8192)
        bytes += String.fromCharCode(...pixels.subarray(i, i + 8192));
      decoded = { width: canvas.width, height: canvas.height, ratio, rgbaBase64: btoa(bytes) };
    }
    await queued(async () =>
      accept(
        await command<Snapshot>("import_image", {
          name: file.name,
          width: decoded.width,
          height: decoded.height,
          rgbaBase64: decoded.rgbaBase64,
          compId: comp.id,
        }),
      ),
    );
    editor.selected = (editor.project?.next_layer ?? 1) - 1;
    notify(
      decoded.ratio < 1
        ? "Image imported at a 2048 px working resolution and embedded in the project."
        : "Image imported and embedded in the project.",
    );
  } catch (error) {
    notify(String(error), true);
  } finally {
    editor.imageImporting = false;
  }
}
/** Vector import: SVG geometry becomes editable shape layers (one per
 * element), scaled to fit the comp without upscaling. */
export async function importSvg(file?: File) {
  const comp = activeComp();
  if (!comp) return;
  if (!file) {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".svg,image/svg+xml";
    input.onchange = () => {
      const selected = input.files?.[0];
      if (selected) void importSvg(selected);
    };
    input.click();
    return;
  }
  try {
    if (file.size > 8 * 1024 * 1024) throw new Error("SVG files must be smaller than 8 MB.");
    editor.imageImporting = true;
    notify("Importing SVG… every element lands as an editable vector shape.");
    const svg = await file.text();
    await queued(async () =>
      accept(
        await command<Snapshot>("import_svg", {
          name: file.name,
          svg,
          compId: comp.id,
        }),
      ),
    );
    editor.selected = (editor.project?.next_layer ?? 1) - 1;
    notify("SVG imported as editable vector shapes. Recolor, keyframe, and extrude away.");
  } catch (error) {
    notify(String(error), true);
  } finally {
    editor.imageImporting = false;
  }
}
/** 3D import: OBJ groups become depth-positioned wireframe layers so meshes
 * render instantly in the 3D diorama and every group animates on its own. */
export async function importObj(file?: File) {
  const comp = activeComp();
  if (!comp) return;
  if (!file) {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".obj,model/obj";
    input.onchange = () => {
      const selected = input.files?.[0];
      if (selected) void importObj(selected);
    };
    input.click();
    return;
  }
  try {
    if (file.size > 32 * 1024 * 1024) throw new Error("OBJ files must be smaller than 32 MB.");
    editor.imageImporting = true;
    notify("Importing 3D model… groups become depth-sorted wireframes you can animate.");
    const obj = await file.text();
    await queued(async () =>
      accept(
        await command<Snapshot>("import_obj", {
          name: file.name,
          obj,
          compId: comp.id,
        }),
      ),
    );
    editor.selected = (editor.project?.next_layer ?? 1) - 1;
    notify("3D model imported. Nudge Z on any group for instant depth parallax.");
  } catch (error) {
    notify(String(error), true);
  } finally {
    editor.imageImporting = false;
  }
}
/** Raster→vector: traces an embedded image layer into editable vector shape
 * layers (posterized marching squares) and hides the original pixels. */
export async function vectorizeImage(compId: number, layerId: number) {
  try {
    await queued(async () =>
      accept(
        await command<Snapshot>("vectorize_image", {
          compId,
          layerId,
          maxColors: 8,
        }),
      ),
    );
    notify("Vectorized. Every color region is now an editable vector shape.");
  } catch (error) {
    notify(String(error), true);
  }
}
/** One front door for drops and browse: routes a file to the right importer. */
export async function importAnyFile(file?: File) {
  if (!file) {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = "image/png,image/jpeg,image/webp,image/svg+xml,.svg,.obj,audio/*";
    input.multiple = true;
    input.onchange = () => {
      for (const selected of Array.from(input.files ?? [])) void importAnyFile(selected);
    };
    input.click();
    return;
  }
  if (/\.svg$/i.test(file.name) || file.type === "image/svg+xml") await importSvg(file);
  else if (/\.obj$/i.test(file.name) || file.type === "model/obj") await importObj(file);
  else if (file.type.startsWith("audio/") || /\.(wav|mp3|flac|ogg|oga|aif|aiff|m4a|aac)$/i.test(file.name)) {
    const { importAudio } = await import("./audio/actions");
    await importAudio(file);
  } else if (file.type.startsWith("image/")) await importImage(file);
  else notify("Import an image, SVG, OBJ model, audio file, or .bonaparte project.", true);
}
/** Keyframe clipboard: copies a property's whole key list so animations can
 * be reused across layers and properties (type-compatible only). */
export interface CopiedKeys {
  property: Property;
  kind: "Scalar" | "Vec2";
  keys: { time: number; value: PropValue; easing: Easing }[];
}
export const keyframeClipboard: { current: CopiedKeys | null } = $state({ current: null });

/** Right-click context menu. One instance lives in App; surfaces call
 * openContextMenu with items and the menu renders where the pointer landed. */
export interface ContextMenuItem {
  label?: string;
  icon?: string;
  hint?: string;
  danger?: boolean;
  disabled?: boolean;
  separator?: boolean;
  run?: () => void;
}
export const contextMenu: { current: { x: number; y: number; items: ContextMenuItem[] } | null } =
  $state({ current: null });
export function openContextMenu(event: MouseEvent, items: ContextMenuItem[]) {
  if (!items.length) return;
  event.preventDefault();
  event.stopPropagation();
  contextMenu.current = { x: event.clientX, y: event.clientY, items };
}
export function closeContextMenu() {
  contextMenu.current = null;
}

/** Actions for right-clicking a layer (canvas or timeline). */
export function layerContextItems(layerId: number): ContextMenuItem[] {
  const comp = activeComp();
  const layer = comp?.layers[String(layerId)];
  if (!comp || !layer) return [];
  const index = comp.layer_order.indexOf(layerId);
  return [
    {
      label: "Duplicate",
      icon: "copy",
      disabled: layer.locked,
      run: () => {
        editor.selected = layerId;
        void duplicateSelected();
      },
    },
    {
      label: "Rename…",
      icon: "type",
      disabled: layer.locked,
      run: () => {
        editor.dialog = {
          kind: "rename-layer",
          compId: comp.id,
          layerId,
          name: layer.name,
        };
      },
    },
    {
      label: layer.locked ? "Unlock" : "Lock",
      icon: layer.locked ? "unlock" : "lock",
      run: () => void applyOp({ type: "setLayerLocked", comp: comp.id, layer: layerId, locked: !layer.locked }),
    },
    {
      label: layer.visible ? "Hide" : "Show",
      icon: layer.visible ? "eye-off" : "eye",
      run: () => void toggleLayerVisibility(comp.id, layerId),
    },
    { separator: true },
    ...("Footage" in layer.kind
      ? [
          {
            label: "Vectorize to shapes",
            icon: "sparkles",
            run: () => void vectorizeImage(comp.id, layerId),
          },
        ]
      : []),
    { separator: true },
    {
      label: "Bring forward",
      icon: "up",
      disabled: index < 0 || index >= comp.layer_order.length - 1,
      run: () => {
        editor.selected = layerId;
        void applyOp({ type: "reorderLayer", comp: comp.id, layer: layerId, newIndex: index + 1 });
      },
    },
    {
      label: "Send backward",
      icon: "down",
      disabled: index <= 0,
      run: () => {
        editor.selected = layerId;
        void applyOp({ type: "reorderLayer", comp: comp.id, layer: layerId, newIndex: index - 1 });
      },
    },
    { separator: true },
    {
      label: "Delete",
      icon: "trash",
      danger: true,
      disabled: layer.locked,
      run: () => {
        editor.selected = layerId;
        void deleteSelected();
      },
    },
  ];
}

/** The comp's effective camera (turntable applied) — for HUD readouts. */
export function compCameraAt(time = editor.currentTime): Camera3D {
  const comp = activeComp();
  if (!comp) return { ...DEFAULT_CAMERA };
  return cameraAt(comp, time);
}

/** Live camera drag state (viewport orbit); committed as setCamera ops. */
export async function moveCamera(
  position: [number, number],
  z: number,
  fov: number,
  group?: string,
  focus?: number,
  dof?: number,
) {
  const comp = activeComp();
  if (!comp) return;
  await applyOp(
    {
      type: "setCamera",
      comp: comp.id,
      position,
      z,
      fov,
      focus: focus ?? comp.camera?.focus ?? 0,
      dof: dof ?? comp.camera?.dof ?? 0,
    },
    group,
  );
}

/** Point the focal plane at the selected layer so it renders sharp. */
export async function focusCameraOnSelection() {
  const comp = activeComp(),
    layer = selectedLayer();
  if (!comp || !layer) return;
  const cam = cameraAt(comp, editor.currentTime);
  if (cam.fov <= 0) {
    // No camera yet: enable one, focused on the layer's plane.
    const depth = effectiveDepth(comp, layer, editor.currentTime);
    await applyOp({
      type: "setCamera",
      comp: comp.id,
      position: [0, 0],
      z: 0,
      fov: 500,
      focus: Math.round(depth * 100) / 100,
      dof: 0.7,
    });
    notify(`3D camera on, focused on ${layer.name}.`);
    return;
  }
  // Parent-chain compounded depth, not raw Z, so children focus correctly.
  const depth = effectiveDepth(comp, layer, editor.currentTime);
  await applyOp({
    type: "setCamera",
    comp: comp.id,
    position: [...cam.position] as [number, number],
    z: cam.z,
    fov: cam.fov,
    focus: Math.round(depth * 100) / 100,
    dof: Math.max(comp.camera?.dof ?? 0, 0.7),
  });
  notify(`Cinematic focus locked on ${layer.name}.`);
}

export async function setTurntableEnabled(enabled: boolean) {
  const comp = activeComp();
  if (!comp) return;
  await applyOp({
    type: "setTurntable",
    comp: comp.id,
    enabled,
    period: comp.turntable?.period ?? 12,
  });
}

export function valueKind(value: PropValue): "Scalar" | "Vec2" {
  return "Scalar" in value ? "Scalar" : "Vec2";
}

/** Copy a single keyframe (timeline diamond context menu) to the clipboard. */
export function copyKeyframe(row: { prop?: Property }, key: Keyframe) {
  keyframeClipboard.current = {
    property: row.prop ?? "Position",
    kind: valueKind(key.value),
    keys: [{ time: key.time, value: key.value, easing: key.easing }],
  };
}

/** Delete a timeline keyframe, whether it animates a transform property or an
 * effect parameter (effect keys live inside the effect's own tracks). */
export async function deleteKeyframe(
  layerId: number,
  row: { prop?: Property; effectId?: string; paramId?: string },
  keyTime: number,
) {
  const comp = activeComp();
  const layer = comp?.layers[String(layerId)];
  if (!comp || !layer || layer.locked) return;
  if (row.prop) {
    await applyOp({
      type: "removeKeyframe",
      comp: comp.id,
      layer: layerId,
      property: row.prop,
      time: keyTime,
    });
    return;
  }
  await applyOp((project) => {
    const target = project.comps[String(comp.id)]?.layers[String(layerId)];
    if (!target) return null;
    const effects = clone(target.effects);
    const track = effects.find((e) => e.id === row.effectId)?.tracks[row.paramId!];
    if (!track) return null;
    track.keys = track.keys.filter((k) => k.time !== keyTime);
    return { type: "setLayerEffects", comp: comp.id, layer: layerId, effects };
  });
}

export async function exportFile(
  format: "png" | "mp4" | "wav",
  options: { bitDepth?: number; outputSpace?: string } = {},
) {
  await flushLiveEdits();
  await mutationQueue;
  pause();
  const comp = activeComp();
  if (!comp) return;
  editor.exporting = true;
  editor.exportProgress = null;
  const args: Record<string, unknown> = { compId: comp.id, time: editor.currentTime, bypassEffects: false };
  if (format === "png") {
    if (options.bitDepth) args.bitDepth = options.bitDepth;
    if (options.outputSpace) args.outputSpace = options.outputSpace;
  }
  // MP4 exports stream real telemetry from the Rust runtime: poll it every
  // 200 ms and smooth the rate so the ETA does not twitch.
  let poller: ReturnType<typeof setInterval> | null = null;
  let sawCanceled = false;
  const watchProgress = () => {
    let etaSmoothed = 0;
    let lastDone = -1;
    const tick = async () => {
      try {
        const raw = await command<{
          active: boolean;
          canceled: boolean;
          framesDone: number;
          totalFrames: number;
          stage: number;
          startedMs: number;
          lastFrameMs: number;
        }>("export_progress");
        if (raw.canceled) sawCanceled = true;
        // Idle runtime (or telemetry left over from a PREVIOUS export — the
        // slot is process-global): ignore until this export's frames flow.
        if (raw.framesDone < lastDone || (lastDone === -1 && raw.framesDone === 0 && !raw.active)) {
          etaSmoothed = 0;
          if (!raw.active) return;
        }
        lastDone = raw.framesDone;
        const total = Math.max(1, raw.totalFrames);
        const done = Math.min(raw.framesDone, total);
        const elapsedMs = raw.startedMs > 0 ? Math.max(0, Date.now() - raw.startedMs) : 0;
        // The parallel pipeline needs a moment before the encoder stream is
        // steady — early samples produce absurd ETAs, so wait for real flow.
        const rate =
          elapsedMs > 1500 && done >= 4 ? done / (elapsedMs / 1000) : 0;
        const eta = rate > 0 ? (total - done) / rate : 0;
        etaSmoothed = etaSmoothed === 0 ? eta : etaSmoothed * 0.7 + eta * 0.3;
        editor.exportProgress = {
          ...raw,
          framesDone: done,
          totalFrames: total,
          percent: (done / total) * 100,
          elapsedSec: elapsedMs / 1000,
          etaSec: etaSmoothed,
          fps: rate,
        };
      } catch {
        /* the export request owns the bridge; keep the last sample */
      }
    };
    void tick();
    poller = setInterval(() => void tick(), 200);
  };
  try {
    if (desktop) {
      const { save } = await import("@tauri-apps/plugin-dialog");
      const path = await save({
        defaultPath: `${filename()}.${format}`,
        filters: [{ name: format.toUpperCase(), extensions: [format] }],
      });
      if (!path) return;
      if (format === "mp4") watchProgress();
      await invoke(
        format === "mp4"
          ? "export_video_file"
          : format === "wav"
            ? "export_wav_file"
            : "export_png_file",
        { args, path },
      );
    } else {
      if (format === "mp4") watchProgress();
      const data = await binary(
        format === "mp4" ? "export_video" : format === "wav" ? "export_wav" : "export_png",
        args,
      );
      download(
        new Blob([data], {
          type: format === "mp4" ? "video/mp4" : format === "wav" ? "audio/wav" : "image/png",
        }),
        `${filename()}.${format}`,
      );
    }
    notify(
      format === "wav"
        ? "Float WAV exported from the Rust audio mixer."
        : `${format.toUpperCase()} exported from the Rust runtime.`,
    );
    editor.dialog = null;
  } catch (error) {
    const message = String(error);
    if (sawCanceled || message.includes("cancel")) notify("Export canceled.");
    else notify(message, true);
  } finally {
    if (poller) clearInterval(poller);
    editor.exporting = false;
    editor.exportProgress = null;
  }
}
/** Ask the Rust runtime to stop the running export at the next frame
 * boundary; the temp file is cleaned up and nothing partial is saved. */
export async function cancelExport() {
  if (!editor.exporting) return;
  try {
    await command("export_cancel", {});
    notify("Canceling export…");
  } catch (error) {
    notify(String(error), true);
  }
}
