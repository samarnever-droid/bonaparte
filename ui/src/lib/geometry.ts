/// Affine geometry matching the Rust compositor, including parenting and anchors.
import { evaluate, type Comp, type Layer, type Project } from "./model";
export type Matrix = [number, number, number, number, number, number];
export const identity: Matrix = [1, 0, 0, 1, 0, 0];
export function multiply(a: Matrix, b: Matrix): Matrix {
  return [
    a[0] * b[0] + a[2] * b[1],
    a[1] * b[0] + a[3] * b[1],
    a[0] * b[2] + a[2] * b[3],
    a[1] * b[2] + a[3] * b[3],
    a[0] * b[4] + a[2] * b[5] + a[4],
    a[1] * b[4] + a[3] * b[5] + a[5],
  ];
}
export function point(m: Matrix, x: number, y: number): [number, number] {
  return [m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]];
}
export function inverse(m: Matrix): Matrix | null {
  const d = m[0] * m[3] - m[1] * m[2];
  if (Math.abs(d) < 0.000001) return null;
  return [
    m[3] / d,
    -m[1] / d,
    -m[2] / d,
    m[0] / d,
    (m[2] * m[5] - m[3] * m[4]) / d,
    (m[1] * m[4] - m[0] * m[5]) / d,
  ];
}
export function localMatrix(layer: Layer, time: number): Matrix {
  const p = evaluate(layer, "Position", time),
    s = evaluate(layer, "Scale", time),
    r = evaluate(layer, "Rotation", time),
    a = evaluate(layer, "AnchorPoint", time);
  const pos = "Vec2" in p ? p.Vec2 : [0, 0],
    sc = "Vec2" in s ? s.Vec2 : [100, 100],
    an = "Vec2" in a ? a.Vec2 : [0, 0],
    rad = (("Scalar" in r ? r.Scalar : 0) * Math.PI) / 180;
  const c = Math.cos(rad),
    sn = Math.sin(rad),
    sx = sc[0] / 100,
    sy = sc[1] / 100;
  const m: Matrix = [sx * c, sx * sn, -sy * sn, sy * c, 0, 0];
  m[4] = pos[0] - an[0] * m[0] - an[1] * m[2];
  m[5] = pos[1] - an[0] * m[1] - an[1] * m[3];
  return m;
}
export function worldMatrix(
  comp: Comp,
  layer: Layer,
  time: number,
  visited = new Set<number>(),
): Matrix {
  if (visited.has(layer.id)) return identity;
  visited.add(layer.id);
  const parent = layer.parent === null ? null : comp.layers[String(layer.parent)];
  return multiply(
    parent ? worldMatrix(comp, parent, time, visited) : identity,
    localMatrix(layer, time),
  );
}
let context: CanvasRenderingContext2D | null;
export function naturalSize(comp: Comp, layer: Layer, project: Project): [number, number] {
  const k = layer.kind;
  if ("Shape" in k) return k.Shape.style.size ?? [comp.width, comp.height];
  if ("PreComp" in k) {
    const c = project.comps[String(k.PreComp.comp)];
    return [c?.width ?? comp.width, c?.height ?? comp.height];
  }
  if ("Footage" in k) {
    const a = project.media[String(k.Footage.media)];
    return [a?.embedded?.width ?? comp.width, a?.embedded?.height ?? comp.height];
  }
  if ("Text" in k) {
    context ??= document.createElement("canvas").getContext("2d");
    if (!context) return [k.Text.text.length * k.Text.size * 0.6, k.Text.size * 1.2];
    context.font = `${k.Text.style.bold ? "bold" : "normal"} ${k.Text.size}px "Bonaparte Sans"`;
    const lines = k.Text.text.split("\n");
    const widths = lines.map(
      (text) =>
        context!.measureText(text).width +
        Math.max(0, [...text].length - 1) * k.Text.style.tracking +
        2,
    );
    const metrics = context.measureText("Mg");
    const height =
      metrics.fontBoundingBoxAscent + metrics.fontBoundingBoxDescent || k.Text.size * 1.2;
    return [
      Math.max(1, Math.ceil(Math.max(...widths))),
      Math.max(1, Math.ceil(height * lines.length)),
    ];
  }
  return [comp.width, comp.height];
}
export function layerGeometry(comp: Comp, layer: Layer, project: Project, time: number) {
  const [width, height] = naturalSize(comp, layer, project),
    world = worldMatrix(comp, layer, time);
  const matrix: Matrix = [...world];
  matrix[4] += comp.width / 2;
  matrix[5] += comp.height / 2;
  const corners = [
    point(matrix, -width / 2, -height / 2),
    point(matrix, width / 2, -height / 2),
    point(matrix, width / 2, height / 2),
    point(matrix, -width / 2, height / 2),
  ];
  return { width, height, matrix, inverse: inverse(matrix), corners, center: point(matrix, 0, 0) };
}
export function hitTest(
  comp: Comp,
  project: Project,
  x: number,
  y: number,
  time: number,
): Layer | null {
  for (const id of [...comp.layer_order].reverse()) {
    const layer = comp.layers[String(id)];
    if (
      !layer ||
      layer.locked ||
      !layer.visible ||
      time < layer.start ||
      time >= layer.start + layer.duration ||
      "Adjustment" in layer.kind
    )
      continue;
    const g = layerGeometry(comp, layer, project, time);
    if (!g.inverse) continue;
    const p = point(g.inverse, x, y);
    if (Math.abs(p[0]) > g.width / 2 || Math.abs(p[1]) > g.height / 2) continue;
    if (
      "Shape" in layer.kind &&
      layer.kind.Shape.generator === "builtin.circle" &&
      Math.hypot(p[0] / (g.width / 2), p[1] / (g.height / 2)) > 1
    )
      continue;
    return layer;
  }
  return null;
}
export function layerIcon(layer: Layer): string {
  const k = layer.kind;
  return "Text" in k
    ? "type"
    : "PreComp" in k
      ? "film"
      : "Footage" in k
        ? "image"
        : "Adjustment" in k
          ? "adjust"
          : "Shape" in k && k.Shape.generator
            ? "circle"
            : "square";
}
export function layerColor(layer: Layer): string {
  const k = layer.kind;
  return "Text" in k
    ? "#93b2d1"
    : "PreComp" in k
      ? "#ae98cf"
      : "Footage" in k
        ? "#89b9b2"
        : "Adjustment" in k
          ? "#cfb584"
          : "Shape" in k
            ? "#a7c693"
            : "#828b7c";
}
export function layerType(layer: Layer): string {
  const k = layer.kind;
  return "Text" in k
    ? "Text layer"
    : "PreComp" in k
      ? "Composition"
      : "Footage" in k
        ? "Image"
        : "Adjustment" in k
          ? "Adjustment layer"
          : "Shape" in k
            ? "Shape layer"
            : "Solid color";
}
