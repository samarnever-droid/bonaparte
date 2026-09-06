import { binary } from "./bridge";
import type { Matrix } from "./geometry";
import type { BlendMode, Layer, Property, PropValue } from "./model";
export interface InteractionPlanes {
  key: string;
  compId: number;
  layerId: number;
  revision: number;
  time: number;
  width: number;
  height: number;
  logicalWidth: number;
  logicalHeight: number;
  matrix: Matrix;
  divisor: number;
  approximate: boolean;
  blendMode: BlendMode;
  images: ImageData[];
  bytes: number;
}
const cache = new Map<string, InteractionPlanes>();
let retained = 0;
let busy = false;
export const planeKey = (
  comp: number,
  layer: number,
  revision: number,
  time: number,
  bypass: boolean,
) => `${comp}:${layer}:${revision}:${time}:${Number(bypass)}`;
export function getPlanes(
  comp: number,
  layer: number,
  revision: number,
  time: number,
  bypass: boolean,
) {
  return cache.get(planeKey(comp, layer, revision, time, bypass)) ?? null;
}
export async function preparePlanes(
  compId: number,
  layerId: number,
  revision: number,
  time: number,
  bypassEffects: boolean,
): Promise<InteractionPlanes | null> {
  const key = planeKey(compId, layerId, revision, time, bypassEffects);
  const old = cache.get(key);
  if (old) {
    cache.delete(key);
    cache.set(key, old);
    return old;
  }
  if (busy) return null;
  busy = true;
  try {
    const data = await binary("interaction_planes", { compId, layerId, time, bypassEffects });
    const view = new DataView(data);
    if (data.byteLength < 8 || view.getUint32(0, true) !== 0x31504942)
      throw new Error("Invalid interaction packet");
    const n = view.getUint32(4, true);
    if (n > 65536 || 8 + n > data.byteLength) throw new Error("Invalid interaction metadata");
    const metadata = JSON.parse(new TextDecoder().decode(new Uint8Array(data, 8, n)));
    const bytes = metadata.width * metadata.height * 4;
    if (
      metadata.revision !== revision ||
      metadata.layerId !== layerId ||
      metadata.compId !== compId ||
      metadata.time !== time ||
      !Number.isInteger(bytes) ||
      bytes <= 0 ||
      bytes * 3 > 24 * 1024 * 1024 ||
      data.byteLength !== 8 + n + bytes * 3
    )
      return null;
    const images = [0, 1, 2].map(
      (i) =>
        new ImageData(
          new Uint8ClampedArray(data, 8 + n + i * bytes, bytes),
          metadata.width,
          metadata.height,
        ),
    );
    const result: InteractionPlanes = { ...metadata, key, images, bytes: bytes * 3 };
    while (retained + result.bytes > 32 * 1024 * 1024 || cache.size >= 4) {
      const oldest = cache.keys().next().value;
      if (!oldest) break;
      retained -= cache.get(oldest)!.bytes;
      cache.delete(oldest);
    }
    cache.set(key, result);
    retained += result.bytes;
    return result;
  } catch {
    return null;
  } finally {
    busy = false;
  }
}
export function transformedLayer(layer: Layer, property: Property, value: PropValue): Layer {
  const transform = { ...layer.transform };
  if (property === "Position" && "Vec2" in value) transform.position = value.Vec2;
  if (property === "Scale" && "Vec2" in value) transform.scale = value.Vec2;
  if (property === "AnchorPoint" && "Vec2" in value) transform.anchor_point = value.Vec2;
  if (property === "Rotation" && "Scalar" in value) transform.rotation = value.Scalar;
  if (property === "Opacity" && "Scalar" in value) transform.opacity = value.Scalar;
  return { ...layer, transform, tracks: { ...layer.tracks, [property]: undefined } };
}
export const cssBlend = (mode: BlendMode) =>
  ({
    Normal: "normal",
    Multiply: "multiply",
    Screen: "screen",
    Overlay: "overlay",
    Add: "plus-lighter",
    Darken: "darken",
    Lighten: "lighten",
    Difference: "difference",
  })[mode];
