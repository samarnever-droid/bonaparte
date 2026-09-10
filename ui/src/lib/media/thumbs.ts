/**
 * Little visual previews for media assets — no new fetches, no server work.
 * Embedded images and video posters already ride inside the project as raw
 * RGBA; we decode once, cover-fit to a 40×24 PNG and memoize by content, so
 * the asset shelf shows what a file actually is at a glance.
 */
import type { MediaAsset } from "../model";

const cache = new Map<string, string>();
const inflight = new Map<string, Promise<string | null>>();

function keyFor(asset: MediaAsset): string | null {
  if (asset.embedded)
    return `i${asset.id}:${asset.embedded.width}x${asset.embedded.height}:${asset.embedded.rgba_base64.length}:${asset.embedded.rgba_base64.slice(0, 24)}`;
  if (asset.video?.frames_base64.length)
    return `v${asset.id}:${asset.video.width}x${asset.video.height}:${asset.video.frames_base64.length}:${asset.video.frames_base64[0].slice(0, 24)}`;
  return null;
}

function decodeRgba(b64: string, w: number, h: number): ImageData | null {
  try {
    const bin = atob(b64);
    const bytes = Uint8Array.from(bin, (c) => c.charCodeAt(0));
    if (bytes.length < w * h * 4) return null;
    return new ImageData(new Uint8ClampedArray(bytes.buffer, bytes.byteOffset, w * h * 4), w, h);
  } catch {
    return null;
  }
}

function fit(data: ImageData, w: number, h: number, tw: number, th: number): string {
  const src = document.createElement("canvas");
  src.width = w;
  src.height = h;
  src.getContext("2d")!.putImageData(data, 0, 0);
  const out = document.createElement("canvas");
  out.width = tw;
  out.height = th;
  const ctx = out.getContext("2d")!;
  const scale = Math.max(tw / w, th / h); // cover-crop, centered
  const dw = Math.round(w * scale);
  const dh = Math.round(h * scale);
  ctx.imageSmoothingQuality = "high";
  ctx.drawImage(src, Math.round((tw - dw) / 2), Math.round((th - dh) / 2), dw, dh);
  return out.toDataURL("image/png");
}

/** The memoized thumbnail data URL, built on demand. Null for assets whose
 *  pixels are not in the document (path-only footage, audio). */
export async function ensureThumb(asset: MediaAsset): Promise<string | null> {
  const key = keyFor(asset);
  if (!key) return null;
  const hit = cache.get(key);
  if (hit) return hit;
  const busy = inflight.get(key);
  if (busy) return busy;
  const job = (async () => {
    try {
      if (asset.embedded) {
        const { width, height, rgba_base64 } = asset.embedded;
        const data = decodeRgba(rgba_base64, width, height);
        if (!data) return null;
        const url = fit(data, width, height, 40, 24);
        cache.set(key, url);
        return url;
      }
      const v = asset.video;
      if (v?.frames_base64.length) {
        const data = decodeRgba(v.frames_base64[0], v.width, v.height);
        if (!data) return null;
        const url = fit(data, v.width, v.height, 40, 24);
        cache.set(key, url);
        return url;
      }
      return null;
    } finally {
      inflight.delete(key);
    }
  })();
  inflight.set(key, job);
  return job;
}

/** Full-resolution re-encode of an embedded image (or a video's poster
 *  frame) as a PNG blob — used by "Send to vault" so the shelf holds a
 *  real file even when the import never had one on disk. */
export async function assetPngBlob(asset: MediaAsset): Promise<Blob | null> {
  try {
    const src = asset.embedded
      ? decodeRgba(asset.embedded.rgba_base64, asset.embedded.width, asset.embedded.height)
      : asset.video?.frames_base64.length
        ? decodeRgba(asset.video.frames_base64[0], asset.video.width, asset.video.height)
        : null;
    if (!src) return null;
    const canvas = document.createElement("canvas");
    canvas.width = src.width;
    canvas.height = src.height;
    canvas.getContext("2d")!.putImageData(src, 0, 0);
    return await new Promise<Blob | null>((resolve) =>
      canvas.toBlob((b) => resolve(b), "image/png"),
    );
  } catch {
    return null;
  }
}
