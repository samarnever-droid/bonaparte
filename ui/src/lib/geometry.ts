/// Comp-space geometry for hit testing, handles, and motion paths.
import type { Comp, Layer, PropValue } from "./model";
import { evaluate } from "./model";

export interface LayerGeometry {
  x: number;
  y: number;
  w: number;
  h: number;
  cx: number;
  cy: number;
  rotation: number;
  scale: [number, number];
  position: [number, number];
  anchorPoint: [number, number];
  corners: [number, number][]; // [NW, NE, SE, SW] in comp coordinates
  center: [number, number];
}

export interface HandlePoint {
  id: "nw" | "n" | "ne" | "e" | "se" | "s" | "sw" | "w" | "rot" | "anchor";
  x: number;
  y: number;
  cursor: string;
}

export interface MotionPathPoint {
  x: number;
  y: number;
  time: number;
  isKeyframe: boolean;
  keyIndex?: number;
}

export interface MotionPathTangent {
  id: string; // e.g. "tangent-p1-0", "tangent-p2-0"
  keyIndex: number;
  segmentIndex: number;
  type: "p1" | "p2";
  x: number;
  y: number;
  startX: number;
  startY: number;
  keyTime: number;
}

export interface MotionPathData {
  path: MotionPathPoint[];
  keyframes: MotionPathPoint[];
  tangents: MotionPathTangent[];
}

function getVec2(v: PropValue, fallback: [number, number] = [0, 0]): [number, number] {
  return "Vec2" in v ? v.Vec2 : fallback;
}

function getScalar(v: PropValue, fallback = 0): number {
  return "Scalar" in v ? v.Scalar : fallback;
}

/** Rotate a point around a pivot point by angle in degrees. */
export function rotatePoint(
  px: number,
  py: number,
  cx: number,
  cy: number,
  angleDeg: number,
): [number, number] {
  if (angleDeg === 0) return [px, py];
  const rad = (angleDeg * Math.PI) / 180;
  const cos = Math.cos(rad);
  const sin = Math.sin(rad);
  const dx = px - cx;
  const dy = py - cy;
  return [cx + dx * cos - dy * sin, cy + dx * sin + dy * cos];
}

export interface LayerTransformOverride {
  position?: [number, number];
  scale?: [number, number];
  rotation?: number;
  anchorPoint?: [number, number];
}

/** Computes the dimensions and comp-space bounds for any layer kind. */
export function layerRect(
  comp: Comp,
  layer: Layer,
  time: number,
  override?: LayerTransformOverride,
): LayerGeometry {
  const posVal = evaluate(layer, "Position", time);
  const scaleVal = evaluate(layer, "Scale", time);
  const rotVal = evaluate(layer, "Rotation", time);
  const anchorVal = evaluate(layer, "AnchorPoint" as any, time);

  const [px, py] = override?.position ?? getVec2(posVal, [0, 0]);
  const [sx, sy] = override?.scale ?? getVec2(scaleVal, [100, 100]);
  const rotation = override?.rotation ?? getScalar(rotVal, 0);
  const [ax, ay] = override?.anchorPoint ?? getVec2(anchorVal, [0, 0]);

  let w = (comp.width * Math.abs(sx)) / 100;
  let h = (comp.height * Math.abs(sy)) / 100;

  if ("Solid" in layer.kind) {
    w = (comp.width * Math.abs(sx)) / 100;
    h = (comp.height * Math.abs(sy)) / 100;
  } else if ("Text" in layer.kind) {
    const text = layer.kind.Text.text || "Text";
    const fontSize = layer.kind.Text.size || 36;
    const baseW = Math.max(30, text.length * fontSize * 0.55);
    const baseH = Math.max(20, fontSize * 1.2);
    w = (baseW * Math.abs(sx)) / 100;
    h = (baseH * Math.abs(sy)) / 100;
  } else if ("Footage" in layer.kind) {
    w = (comp.width * Math.abs(sx)) / 100;
    h = (comp.height * Math.abs(sy)) / 100;
  } else if ("PreComp" in layer.kind) {
    w = (comp.width * Math.abs(sx)) / 100;
    h = (comp.height * Math.abs(sy)) / 100;
  } else if ("Shape" in layer.kind) {
    w = (comp.width * Math.abs(sx)) / 100;
    h = (comp.height * Math.abs(sy)) / 100;
  }

  // Visual center in comp coordinates (top-left is (0,0), center is (comp.width/2 + px, comp.height/2 + py))
  const cx = comp.width / 2 + px;
  const cy = comp.height / 2 + py;

  const halfW = w / 2;
  const halfH = h / 2;

  // Unrotated corners relative to center
  const unrotatedCorners: [number, number][] = [
    [cx - halfW, cy - halfH], // NW
    [cx + halfW, cy - halfH], // NE
    [cx + halfW, cy + halfH], // SE
    [cx - halfW, cy + halfH], // SW
  ];

  // Rotate corners around pivot (cx + ax, cy + ay)
  const pivotX = cx + ax;
  const pivotY = cy + ay;

  const corners = unrotatedCorners.map(([x, y]) =>
    rotatePoint(x, y, pivotX, pivotY, rotation),
  ) as [number, number][];

  return {
    x: cx - halfW,
    y: cy - halfH,
    w,
    h,
    cx,
    cy,
    rotation,
    scale: [sx, sy],
    position: [px, py],
    anchorPoint: [ax, ay],
    corners,
    center: [cx, cy],
  };
}

/** Check if point is inside polygon using ray casting. */
export function pointInPolygon(pt: [number, number], poly: [number, number][]): boolean {
  let inside = false;
  const [x, y] = pt;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const xi = poly[i][0];
    const yi = poly[i][1];
    const xj = poly[j][0];
    const yj = poly[j][1];

    const intersect = yi > y !== yj > y && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi;
    if (intersect) inside = !inside;
  }
  return inside;
}

/** A shape's drawn rectangle in comp pixel coordinates (backward compatibility). */
export function shapeRect(
  comp: Comp,
  layer: Layer,
  time: number,
): { x: number; y: number; w: number; h: number } {
  const geom = layerRect(comp, layer, time);
  return { x: geom.x, y: geom.y, w: geom.w, h: geom.h };
}

/** Comprehensive hit testing across all layer kinds (Shape, Text, Footage, PreComp, Solid). */
export function hitTest(comp: Comp, x: number, y: number, time: number): Layer | null {
  // Topmost layer wins — iterate backwards through layer_order
  for (let i = comp.layer_order.length - 1; i >= 0; i--) {
    const layerId = comp.layer_order[i];
    const layer = comp.layers[String(layerId)];
    if (!layer) continue;

    // Check layer visibility and duration bounds
    if (layer.visible === false || layer.locked) continue;
    if (time < layer.start || time >= layer.start + layer.duration) continue;

    const geom = layerRect(comp, layer, time);
    if ("Shape" in layer.kind && layer.kind.Shape.geometry === "circle") {
      const pivotX = geom.cx + geom.anchorPoint[0];
      const pivotY = geom.cy + geom.anchorPoint[1];
      const [unrotX, unrotY] = rotatePoint(x, y, pivotX, pivotY, -geom.rotation);
      const hw = geom.w / 2;
      const hh = geom.h / 2;
      if (hw > 0 && hh > 0) {
        const dx = (unrotX - geom.cx) / hw;
        const dy = (unrotY - geom.cy) / hh;
        if (dx * dx + dy * dy <= 1.0) {
          return layer;
        }
      }
    } else if (pointInPolygon([x, y], geom.corners)) {
      return layer;
    }
  }
  return null;
}

/** Get interactive drag handles for the given layer geometry. */
export function getHandles(geom: LayerGeometry): HandlePoint[] {
  const { corners, cx, cy, rotation, anchorPoint } = geom;
  const [nw, ne, se, sw] = corners;

  // Midpoint helpers
  const mid = (p1: [number, number], p2: [number, number]): [number, number] => [
    (p1[0] + p2[0]) / 2,
    (p1[1] + p2[1]) / 2,
  ];

  const n = mid(nw, ne);
  const e = mid(ne, se);
  const s = mid(se, sw);
  const w = mid(sw, nw);

  // Rotation handle extends 28px above top-center in the local rotated up-vector
  const rad = ((rotation - 90) * Math.PI) / 180;
  const rotDist = 28;
  const rot: [number, number] = [n[0] + Math.cos(rad) * rotDist, n[1] + Math.sin(rad) * rotDist];

  // Anchor point pivot position
  const pivot = rotatePoint(
    cx + anchorPoint[0],
    cy + anchorPoint[1],
    cx + anchorPoint[0],
    cy + anchorPoint[1],
    rotation,
  );

  return [
    { id: "nw", x: nw[0], y: nw[1], cursor: "nwse-resize" },
    { id: "n", x: n[0], y: n[1], cursor: "ns-resize" },
    { id: "ne", x: ne[0], y: ne[1], cursor: "nesw-resize" },
    { id: "e", x: e[0], y: e[1], cursor: "ew-resize" },
    { id: "se", x: se[0], y: se[1], cursor: "nwse-resize" },
    { id: "s", x: s[0], y: s[1], cursor: "ns-resize" },
    { id: "sw", x: sw[0], y: sw[1], cursor: "nesw-resize" },
    { id: "w", x: w[0], y: w[1], cursor: "ew-resize" },
    { id: "rot", x: rot[0], y: rot[1], cursor: "grab" },
    { id: "anchor", x: pivot[0], y: pivot[1], cursor: "crosshair" },
  ];
}

/** Computes the motion path trajectory points, keyframe coordinates, and curve tangents in comp space. */
export function getMotionPath(
  comp: Comp,
  layer: Layer,
  samples = 50,
): MotionPathData {
  const posTrack = layer.tracks.Position;
  if (!posTrack || posTrack.keys.length < 2) {
    return { path: [], keyframes: [], tangents: [] };
  }

  const keys = posTrack.keys;
  const tMin = keys[0].time;
  const tMax = keys[keys.length - 1].time;
  if (tMax <= tMin) {
    return { path: [], keyframes: [], tangents: [] };
  }

  const path: MotionPathPoint[] = [];
  const dt = (tMax - tMin) / Math.max(1, samples);

  for (let i = 0; i <= samples; i++) {
    const t = Math.round(tMin + i * dt);
    const pos = evaluate(layer, "Position", t);
    const [px, py] = getVec2(pos);
    path.push({
      x: comp.width / 2 + px,
      y: comp.height / 2 + py,
      time: t,
      isKeyframe: false,
    });
  }

  const keyframes: MotionPathPoint[] = keys.map((k, idx) => {
    const pos = evaluate(layer, "Position", k.time);
    const [px, py] = getVec2(pos);
    return {
      x: comp.width / 2 + px,
      y: comp.height / 2 + py,
      time: k.time,
      isKeyframe: true,
      keyIndex: idx,
    };
  });

  // Calculate motion path curve tangents for each segment between keyframes
  const tangents: MotionPathTangent[] = [];
  for (let i = 0; i < keys.length - 1; i++) {
    const k0 = keyframes[i];
    const k1 = keyframes[i + 1];
    const easing = keys[i].easing;

    let p1: [number, number] = [0.42, 0.0];
    let p2: [number, number] = [0.58, 1.0];
    if (easing === "Linear") {
      p1 = [0.25, 0.25];
      p2 = [0.75, 0.75];
    } else if (typeof easing === "object" && "Bezier" in easing) {
      p1 = easing.Bezier.p1;
      p2 = easing.Bezier.p2;
    }

    const dx = k1.x - k0.x;
    const dy = k1.y - k0.y;

    // Tangent handle 1 (P1 - outgoing from k0)
    tangents.push({
      id: `tangent-p1-${i}`,
      keyIndex: i,
      segmentIndex: i,
      type: "p1",
      x: k0.x + p1[0] * dx,
      y: k0.y + p1[1] * dy,
      startX: k0.x,
      startY: k0.y,
      keyTime: keys[i].time,
    });

    // Tangent handle 2 (P2 - incoming to k1)
    tangents.push({
      id: `tangent-p2-${i}`,
      keyIndex: i,
      segmentIndex: i,
      type: "p2",
      x: k0.x + p2[0] * dx,
      y: k0.y + p2[1] * dy,
      startX: k1.x,
      startY: k1.y,
      keyTime: keys[i].time,
    });
  }

  return { path, keyframes, tangents };
}
