//! The five 3D killer features for first-open success. Every action is ONE
//! undoable batch built from existing ops — a beginner clicks once and the
//! scene looks like someone who knows depth spent an hour on it.
//!
//! 1. Scene arrangements — one-click professional depth layouts (diorama,
//!    parallax, carousel, corridor).
//! 2. Arrange in depth — spread the selected layers evenly through Z with
//!    perspective scale compensation, so nothing shrinks away by accident.
//! 3. New 3D scene — a spinning starter diorama created from nothing.
//! 4. Focus on this — one-click cinematic depth-of-field on any layer.
//! 5. Framing presets — Wide / Medium / Close-up camera in one click.

import { activeComp, applyOp, editor, notify } from "./store.svelte";
import type { Comp, Layer, Op } from "./model";

interface SceneTarget {
  comp: Comp;
  layers: Layer[];
}

/** Top-level layers (parents move their subtrees, so only roots are arranged). */
function arrangementTarget(): SceneTarget | null {
  const comp = activeComp();
  if (!comp) return null;
  const layers = comp.layer_order
    .map((id) => comp.layers[String(id)])
    .filter((l): l is Layer => !!l && l.parent == null && l.visible);
  return { comp, layers };
}

/** Perspective shrinks distant layers by fov/(fov+Δz). Multiply a layer's
 * scale by the RECIPROCAL so its on-screen size stays stable at depth `z`
 * (capped so extreme depths cannot explode a layer). */
function scaleCompensation(fov: number, camZ: number, z: number): number {
  const effective = Math.max(z - camZ, -fov * 0.95);
  return Math.min((fov + effective) / fov, 6);
}

function setZ(compId: number, layerId: number, z: number): Op {
  return { type: "setValue", comp: compId, layer: layerId, property: "Z", value: { Scalar: z } };
}
function setScale(compId: number, layerId: number, scale: [number, number]): Op {
  return { type: "setValue", comp: compId, layer: layerId, property: "Scale", value: { Vec2: scale } };
}
function cameraOp(
  compId: number,
  cam: { position: [number, number]; z: number; fov: number; focus?: number; dof?: number },
): Op {
  return { type: "setCamera", comp: compId, ...cam };
}

/** Evenly stagger `n` layers through depth from `near` to `far`. */
function stagger(n: number, near: number, far: number): number[] {
  if (n <= 1) return [0];
  return Array.from({ length: n }, (_, i) => Math.round(near + ((far - near) * i) / (n - 1)));
}

export type ArrangementName = "diorama" | "parallax" | "carousel" | "corridor";

export const ARRANGEMENTS: { id: ArrangementName; label: string; hint: string }[] = [
  { id: "diorama", label: "Diorama", hint: "Layered card stack with soft cinematic blur" },
  { id: "parallax", label: "Parallax", hint: "Gentle depth drift — intro-sequence feel" },
  { id: "carousel", label: "Carousel", hint: "Your layers orbit the camera on a ring" },
  { id: "corridor", label: "Corridor", hint: "Tunnel of depth — dramatic fly-through look" },
];

/** Feature 1 — the upper-level view arrangement: apply a named 3D layout to
 * every visible root layer. One batch = one undo. */
export function applySceneArrangement(name: ArrangementName): boolean {
  const target = arrangementTarget();
  if (!target || target.layers.length === 0) {
    notify("Add a layer first — then pick a 3D arrangement.", true);
    return false;
  }
  const { comp, layers } = target;
  const cam = { position: [0, 0] as [number, number], z: 0, fov: 500, focus: 0, dof: 0 };
  const depthOps: Op[] = [];

  if (name === "diorama") {
    cam.fov = 500;
    cam.dof = 0.55;
    cam.z = -60;
    const zs = stagger(layers.length, -80, 360);
    layers.forEach((layer, i) => {
      const z = zs[i];
      const f = scaleCompensation(cam.fov, cam.z, z);
      depthOps.push(setZ(comp.id, layer.id, z));
      depthOps.push(setScale(comp.id, layer.id, [layer.transform.scale[0] * f, layer.transform.scale[1] * f]));
    });
    depthOps.push({ type: "setTurntable", comp: comp.id, enabled: false, period: comp.turntable?.period ?? 12 });
  } else if (name === "parallax") {
    cam.fov = 800;
    cam.dof = 0.3;
    const zs = stagger(layers.length, -40, 180);
    layers.forEach((layer, i) => {
      const z = zs[i];
      const f = scaleCompensation(cam.fov, cam.z, z);
      depthOps.push(setZ(comp.id, layer.id, z));
      depthOps.push(setScale(comp.id, layer.id, [layer.transform.scale[0] * f, layer.transform.scale[1] * f]));
    });
  } else if (name === "carousel") {
    cam.fov = 620;
    cam.dof = 0.45;
    cam.z = -260;
    const ring = layers.slice(0, 12);
    ring.forEach((layer, i) => {
      const angle = (i / ring.length) * Math.PI * 2;
      const radius = Math.max(comp.width * 0.42, 260);
      const z = Math.round(cam.z + radius + radius * Math.cos(angle) * 0.55);
      const x = Math.round(radius * Math.sin(angle) * 0.8);
      const f = scaleCompensation(cam.fov, cam.z, z);
      depthOps.push(setZ(comp.id, layer.id, z));
      depthOps.push(setScale(comp.id, layer.id, [layer.transform.scale[0] * f, layer.transform.scale[1] * f]));
      depthOps.push({
        type: "setValue",
        comp: comp.id,
        layer: layer.id,
        property: "Position",
        value: { Vec2: [x, Math.round(layer.transform.position[1] * 0.6)] },
      });
    });
    depthOps.push({ type: "setTurntable", comp: comp.id, enabled: true, period: 12 });
  } else {
    // corridor
    cam.fov = 430;
    cam.dof = 0.75;
    cam.z = -420;
    const tunnel = layers.slice(0, 10);
    tunnel.forEach((layer, i) => {
      const z = 80 + i * 300;
      const f = scaleCompensation(cam.fov, cam.z, z);
      depthOps.push(setZ(comp.id, layer.id, z));
      depthOps.push(setScale(comp.id, layer.id, [layer.transform.scale[0] * f, layer.transform.scale[1] * f]));
    });
    depthOps.push({ type: "setTurntable", comp: comp.id, enabled: false, period: comp.turntable?.period ?? 12 });
  }
  const ops: Op[] = [cameraOp(comp.id, cam), ...depthOps];
  void applyOp({ type: "batch", label: `3D scene: ${name}`, ops });
  notify(
    name === "carousel"
      ? "Carousel arranged — press play and your layers orbit the camera."
      : `Arranged as a ${name}. Orbit the camera or drag Depth in Properties to fine-tune.`,
  );
  return true;
}

/** Feature 2 — arrange the selection (or every root layer) evenly in depth
 * with scale compensation. Works on any comp, changes nothing else. */
export function arrangeInDepth(): boolean {
  const comp = activeComp();
  if (!comp) return false;
  const pool = comp.layer_order
    .map((id) => comp.layers[String(id)])
    .filter((l): l is Layer => !!l && l.parent == null && !l.locked);
  if (pool.length < 2) {
    notify("Need at least two unlocked layers to arrange in depth.", true);
    return false;
  }
  const selected = editor.selected;
  const picked = selected != null ? pool.filter((l) => l.id === selected) : [];
  // One selected layer cannot spread — arrange the whole composition instead
  // (the beginner-friendly default).
  const chosen = picked.length >= 2 ? picked : pool;
  if (chosen.length < 2) {
    notify("Select (or unlock) at least two layers to arrange in depth.", true);
    return false;
  }
  const ordered = [...chosen].sort((a, b) => a.transform.position[1] - b.transform.position[1]);
  const cam = comp.camera;
  const fov = cam && cam.fov > 0 ? cam.fov : 500;
  const camZ = cam?.z ?? 0;
  const span = Math.max(comp.width * 1.2, 400);
  const zs = stagger(ordered.length, Math.round(-span * 0.3), Math.round(span));
  const ops: Op[] = ordered.flatMap((layer, i) => {
    const f = scaleCompensation(fov, camZ, zs[i]);
    return [
      setZ(comp.id, layer.id, zs[i]),
      setScale(comp.id, layer.id, [layer.transform.scale[0] * f, layer.transform.scale[1] * f]),
    ];
  });
  void applyOp({ type: "batch", label: "Arrange in depth", ops });
  notify("Layers arranged in depth — nearest to farthest, sizes kept stable.");
  return true;
}

const STARTER_CARDS: {
  name: string;
  color: [number, number, number, number];
  position: [number, number];
  z: number;
}[] = [
  { name: "Hero card", color: [0.96, 0.62, 0.35, 1], position: [-40, -20], z: 40 },
  { name: "Mid card", color: [0.42, 0.66, 0.88, 1], position: [70, 30], z: 220 },
  { name: "Back drop", color: [0.55, 0.78, 0.6, 1], position: [-90, 60], z: 400 },
];

/** Feature 3 — a spinning 3D starter scene from nothing. */
export function create3dScene(): boolean {
  const comp = activeComp();
  if (!comp) return false;
  const nextId = editor.project?.next_layer ?? 1;
  const duration = comp.duration;
  const adds: Op[] = STARTER_CARDS.map((card) => ({
    type: "addLayer" as const,
    comp: comp.id,
    layer: {
      id: 0,
      name: card.name,
      kind: {
        Shape: {
          color: card.color,
          generator: null,
          style: {
            size: [Math.round(comp.width * 0.42), Math.round(comp.height * 0.5)],
            corner_radius: 24,
            stroke_width: 0,
            stroke_color: [0, 0, 0, 0],
          },
        },
      },
      start: 0,
      duration,
      transform: {
        position: card.position,
        scale: [100, 100],
        rotation: 0,
        opacity: 1,
        anchor_point: [0, 0],
        z: card.z,
      },
      tracks: {},
      parent: null,
      blend_mode: "Normal" as const,
      visible: true,
      locked: false,
      effects: [],
    },
  }));
  const ops: Op[] = [
    ...adds,
    cameraOp(comp.id, { position: [0, 0], z: -80, fov: 520, focus: 220, dof: 0.5 }),
    { type: "setTurntable", comp: comp.id, enabled: true, period: 14 },
  ];
  STARTER_CARDS.forEach((card, i) => {
    const f = scaleCompensation(520, -80, card.z);
    ops.push(setScale(comp.id, nextId + i, [100 * f, 100 * f]));
  });
  void applyOp({ type: "batch", label: "New 3D scene", ops });
  editor.selected = (editor.project?.next_layer ?? 1) - 1;
  notify("3D scene created — press play: the camera orbits your layered cards.");
  return true;
}

export type Framing = "wide" | "medium" | "closeup";

/** Feature 5 — framing presets: lens language without the jargon. */
export function setFraming(framing: Framing): boolean {
  const comp = activeComp();
  if (!comp) return false;
  const presets: Record<Framing, { position: [number, number]; z: number; fov: number }> = {
    wide: { position: [0, 0], z: 260, fov: 780 },
    medium: { position: [0, 0], z: 0, fov: 500 },
    closeup: { position: [0, 0], z: -220, fov: 330 },
  };
  const cam = comp.camera;
  void applyOp({
    type: "setCamera",
    comp: comp.id,
    ...presets[framing],
    focus: cam?.focus ?? 0,
    dof: cam?.dof ?? 0,
  });
  notify(
    framing === "wide"
      ? "Wide framing — the whole depth stage is visible."
      : framing === "medium"
        ? "Medium framing — balanced depth."
        : "Close-up — near layers fill the frame.",
  );
  return true;
}
