export type PreviewBackend = "auto" | "cpu" | "gpu";
export type PreviewQuality = "auto" | "1" | "2" | "4";
export interface PreviewMetadata {
  width: number;
  height: number;
  logicalWidth: number;
  logicalHeight: number;
  divisor: number;
  time: number;
  revision: number;
  backend: "cpu" | "gpu" | "gpu-software";
  adapter: string | null;
  fallbackReason: string | null;
  cacheHit: boolean;
  renderMs: number;
  originalRenderMs: number;
}
export interface PreviewStatus {
  protocol: number;
  frames: { bytes: number; budgetBytes: number; entries: number; hits: number; misses: number };
  sources: { bytes: number; budgetBytes: number; entries: number; hits: number; misses: number };
  gpu: {
    compiled: boolean;
    available: boolean;
    software: boolean;
    name: string | null;
    api: string | null;
    deviceType: string | null;
    reason: string | null;
    initialized: boolean;
    pooledBytes: number;
    uploadBytes: number;
    submittedFrames: number;
  };
  renders: number;
  cpuFrames: number;
  gpuFrames: number;
  fallbacks: number;
}
export function parsePreviewPacket(data: ArrayBuffer): {
  metadata: PreviewMetadata;
  bytes: Uint8ClampedArray<ArrayBuffer>;
} {
  if (data.byteLength < 8) throw new Error("Incomplete preview packet");
  const view = new DataView(data);
  if (view.getUint32(0, true) !== 0x33465042) throw new Error("Unsupported preview protocol");
  const length = view.getUint32(4, true);
  if (length > 65536 || length > data.byteLength - 8)
    throw new Error("Invalid preview metadata length");
  const metadata: PreviewMetadata = JSON.parse(
    new TextDecoder().decode(new Uint8Array(data, 8, length)),
  );
  const { width, height } = metadata;
  if (
    !Number.isInteger(width) ||
    !Number.isInteger(height) ||
    width < 1 ||
    height < 1 ||
    width > 8192 ||
    height > 8192 ||
    width * height > 16777216 ||
    data.byteLength !== 8 + length + width * height * 4 ||
    !["cpu", "gpu", "gpu-software"].includes(metadata.backend)
  ) {
    throw new Error("Invalid preview dimensions, backend or pixel payload");
  }
  return { metadata, bytes: new Uint8ClampedArray(data, 8 + length) };
}
export function backendLabel(backend: PreviewMetadata["backend"] | undefined): string {
  return backend === "gpu" ? "GPU" : backend === "gpu-software" ? "GPU · software" : "CPU";
}
export const memoryLabel = (bytes: number) => `${(bytes / 1048576).toFixed(1)} MiB`;
