/// The only client-side document mirror. Rust owns validation, rendering and history.
import { command, binary, desktop, invoke } from "./bridge";
import { saveRecovery, readRecovery } from "./persistence";
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
  type EffectManifest,
  type EffectInstance,
  type EffectValue,
  type ParamDef,
  type Track,
} from "./model";

export const editor = $state({
  project: null as Project | null,
  activeComp: null as number | null,
  selected: null as number | null,
  currentTime: 144000,
  playing: false,
  renderSeq: 0,
  revision: 0,
  canUndo: false,
  canRedo: false,
  history: [] as string[],
  dirty: false,
  pending: 0,
  loading: true,
  initError: "",
  renderError: "",
  frameMs: 0,
  renderedTime: 0,
  effects: [] as EffectManifest[],
  ffmpeg: false,
  workspace: "Design" as "Design" | "Color" | "Animate",
  sidebar: "project" as "project" | "effects" | "motion",
  inspector: "properties" as "properties" | "effects",
  tool: "select" as "select" | "hand",
  showGuides: false,
  bypassEffects: false,
  graphProperty: null as Property | null,
  previewLayer: null as Layer | null,
  histogram: [[], [], []] as number[][],
  toast: null as { message: string; error: boolean } | null,
  dialog: null as
    | { kind: "composition"; compId: number | null }
    | { kind: "export" | "shortcuts" | "new-project" }
    | null,
  exporting: false,
  recovery: "Ready" as string,
});

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
  return JSON.parse(JSON.stringify(value));
}
export function notify(message: string, error = false) {
  editor.toast = { message, error };
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (editor.toast = null), error ? 9000 : 4000);
}
function queueRecovery() {
  clearTimeout(recoveryTimer);
  editor.recovery = "Saving recovery…";
  recoveryTimer = setTimeout(() => {
    if (!editor.project) return;
    void saveRecovery(clone(editor.project))
      .then(() => (editor.recovery = "Recovery saved"))
      .catch(() => (editor.recovery = "Recovery unavailable — save a file"));
  }, 400);
}
function accept(snapshot: Snapshot, dirty = true) {
  editor.project = snapshot.project;
  editor.canUndo = snapshot.canUndo;
  editor.canRedo = snapshot.canRedo;
  editor.history = snapshot.history;
  editor.revision = snapshot.revision;
  if (!editor.project.comps[String(editor.activeComp)])
    editor.activeComp = Object.values(editor.project.comps)[0]?.id ?? null;
  if (!activeComp()?.layers[String(editor.selected)]) editor.selected = null;
  const comp = activeComp();
  if (comp)
    editor.currentTime = Math.min(
      editor.currentTime,
      Math.max(0, comp.duration - ticksPerFrame(comp.fps)),
    );
  editor.renderSeq++;
  editor.dirty = dirty;
  queueRecovery();
}
async function queued(action: () => Promise<void>): Promise<boolean> {
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
  editor.initError = "";
  try {
    const [snapshot, catalog] = await Promise.all([
      command<Snapshot>("state"),
      command<{ effects: EffectManifest[]; ffmpeg: boolean }>("catalog"),
    ]);
    editor.effects = catalog.effects;
    editor.ffmpeg = catalog.ffmpeg;
    let state = snapshot;
    if (snapshot.revision === 0) {
      const recovery = await readRecovery().catch(() => null);
      if (recovery) {
        try {
          state = await command<Snapshot>("open_project", { json: recovery });
          notify("Your last session was recovered.");
        } catch {
          notify("The saved recovery could not be opened. Your original file is unchanged.", true);
        }
      }
    }
    accept(state, false);
    editor.selected =
      Object.values(activeComp()?.layers ?? {}).find((l) => l.name === "Orbital form")?.id ??
      activeComp()?.layer_order.at(-1) ??
      null;
    await document.fonts.load('14px "Bonaparte Sans"');
  } catch (error) {
    editor.initError = String(error instanceof Error ? error.message : error);
  } finally {
    editor.loading = false;
  }
}

export async function applyOp(op: Op | ((project: Project) => Op | null)): Promise<boolean> {
  return queued(async () => {
    if (!editor.project) return;
    const value = typeof op === "function" ? op(editor.project) : op;
    if (value) accept(await command<Snapshot>("apply", { op: value }));
  });
}
export async function undoOp() {
  pause();
  return queued(async () => accept(await command<Snapshot>("undo")));
}
export async function redoOp() {
  pause();
  return queued(async () => accept(await command<Snapshot>("redo")));
}
export function selectComp(id: number) {
  pause();
  editor.activeComp = id;
  editor.selected = null;
  editor.currentTime = 0;
  editor.previewLayer = null;
  editor.graphProperty = null;
}
export function setWorkspace(workspace: typeof editor.workspace) {
  editor.workspace = workspace;
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
export function scrub(time: number) {
  const comp = activeComp();
  if (!comp) return;
  editor.currentTime = Math.max(
    0,
    Math.min(Math.max(0, comp.duration - ticksPerFrame(comp.fps)), snapToFrame(time, comp.fps)),
  );
}
export function play() {
  if (editor.playing || !activeComp()) return;
  editor.playing = true;
  lastTimestamp = performance.now();
  remainder = 0;
  const tick = (now: number) => {
    const comp = activeComp();
    if (!editor.playing || !comp) return;
    remainder += (Math.min(now - lastTimestamp, 1000) * TICKS_PER_SEC) / 1000;
    const tpf = ticksPerFrame(comp.fps);
    const frames = Math.floor(remainder / tpf);
    if (frames) {
      editor.currentTime =
        (editor.currentTime + frames * tpf) % Math.max(tpf, Math.ceil(comp.duration / tpf) * tpf);
      remainder %= tpf;
    }
    lastTimestamp = now;
    raf = requestAnimationFrame(tick);
  };
  raf = requestAnimationFrame(tick);
}
export function pause() {
  editor.playing = false;
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
  rendering = true;
  const started = performance.now(),
    seq = editor.renderSeq,
    time = editor.currentTime;
  try {
    const data = await binary("render_frame_raw", {
      compId: comp.id,
      time,
      bypassEffects: editor.bypassEffects,
      layerOverride: editor.previewLayer ? clone(editor.previewLayer) : null,
    });
    if (editor.activeComp !== comp.id || seq !== editor.renderSeq || !canvas.isConnected) return;
    const view = new DataView(data);
    const width = view.getUint32(0, true),
      height = view.getUint32(4, true);
    if (data.byteLength !== 8 + width * height * 4)
      throw new Error("Invalid frame received from renderer");
    const bytes = new Uint8ClampedArray(data, 8);
    if (canvas.width !== width) canvas.width = width;
    if (canvas.height !== height) canvas.height = height;
    canvas.getContext("2d")?.putImageData(new ImageData(bytes, width, height), 0, 0);
    editor.frameMs = performance.now() - started;
    editor.renderedTime = time;
    editor.renderError = "";
    const hist = [Array(64).fill(0), Array(64).fill(0), Array(64).fill(0)] as number[][];
    for (let i = 0; i < bytes.length; i += 32) {
      if (!bytes[i + 3]) continue;
      for (let c = 0; c < 3; c++) hist[c][bytes[i + c] >> 2]++;
    }
    editor.histogram = hist;
  } catch (error) {
    editor.renderError = String(error instanceof Error ? error.message : error);
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
  if (file.size > 64 * 1024 * 1024) {
    notify("Project exceeds the 64 MB limit.", true);
    return;
  }
  await queued(async () => {
    accept(await command<Snapshot>("open_project", { json: await file.text() }), false);
    editor.currentTime = 0;
    editor.selected = null;
  });
}
export async function newProject(example = false) {
  pause();
  await queued(async () => {
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
    await queued(async () =>
      accept(
        await command<Snapshot>("import_image", {
          name: file.name,
          width: canvas.width,
          height: canvas.height,
          rgbaBase64: btoa(bytes),
          compId: comp.id,
        }),
      ),
    );
    editor.selected = (editor.project?.next_layer ?? 1) - 1;
    notify(
      ratio < 1
        ? "Image imported at a 2048 px working resolution and embedded in the project."
        : "Image imported and embedded in the project.",
    );
  } catch (error) {
    notify(String(error), true);
  }
}
export async function exportFile(format: "png" | "mp4") {
  await mutationQueue;
  pause();
  const comp = activeComp();
  if (!comp) return;
  editor.exporting = true;
  const args = { compId: comp.id, time: editor.currentTime, bypassEffects: false };
  try {
    if (desktop) {
      const { save } = await import("@tauri-apps/plugin-dialog");
      const path = await save({
        defaultPath: `${filename()}.${format}`,
        filters: [{ name: format.toUpperCase(), extensions: [format] }],
      });
      if (!path) return;
      await invoke(format === "mp4" ? "export_video_file" : "export_png_file", { args, path });
    } else {
      const data = await binary(format === "mp4" ? "export_video" : "export_png", args);
      download(
        new Blob([data], { type: format === "mp4" ? "video/mp4" : "image/png" }),
        `${filename()}.${format}`,
      );
    }
    notify(`${format.toUpperCase()} exported from the Rust compositor.`);
    editor.dialog = null;
  } catch (error) {
    notify(String(error), true);
  } finally {
    editor.exporting = false;
  }
}
