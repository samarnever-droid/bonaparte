/**
 * The auto-allocation engine: reads an asset's real specs (dimensions,
 * duration, bytes, device memory) and decides — with no user interview —
 * how the editor should spend memory and sampling budget on it. Every
 * import asks here first; the returned plan is applied automatically and
 * reported to the user in one line.
 */

export type AssetSpecs = {
  kind: "video" | "image" | "audio";
  bytes: number;
  /** media dimensions (0 for audio) */
  width?: number;
  height?: number;
  /** seconds */
  duration?: number;
};

export type AllocPlan = {
  /** video: how many keyframes to sample */
  sampleCount: number;
  /** video: width to downscale samples to */
  sampleWidth: number;
  /** audio: chunk length the transport should pre-buffer, seconds */
  chunkSeconds: number;
  /** rough project-bytes this asset will occupy once embedded */
  budgetBytes: number;
  /** one-line human summary of the decision */
  note: string;
};

const MB = 1024 * 1024;

function deviceMemoryGB(): number {
  try {
    return (navigator as Navigator & { deviceMemory?: number }).deviceMemory ?? 4;
  } catch {
    return 4;
  }
}

function videoPlan(specs: AssetSpecs): AllocPlan {
  const duration = Math.max(0.1, specs.duration ?? 5);
  const width = Math.max(1, specs.width ?? 640);
  const mem = deviceMemoryGB();
  // Frame budget: short clips get dense samples, long ones sparse. Never
  // more than 32 samples — the embedded track must stay portable.
  let sampleCount = Math.round(Math.min(32, Math.max(8, duration * 3)));
  if (duration > 120) sampleCount = 12;
  else if (duration > 45) sampleCount = 16;
  // Sample width: big/long sources downscale harder so the project stays
  // within its portable budget.
  let sampleWidth = Math.min(width, duration > 90 ? 320 : 480);
  if (mem <= 2) sampleWidth = Math.min(sampleWidth, 320);
  const aspect = (specs.height ?? 360) / width;
  const bytesPerFrame = sampleWidth * Math.round(sampleWidth * aspect) * 4;
  // Poster shares the sample width; +33% for base64.
  const budgetBytes = Math.round((bytesPerFrame * (sampleCount + 1) * 4) / 3);
  const dur =
    duration >= 60
      ? `${Math.round(duration / 60)} min`
      : `${duration.toFixed(duration < 10 ? 1 : 0)}s`;
  return {
    sampleCount,
    sampleWidth,
    chunkSeconds: 0,
    budgetBytes,
    note: `${dur} video → ${sampleCount} keyframes @${sampleWidth}px (≈${Math.max(1, Math.round(budgetBytes / MB))} MB in project)`,
  };
}

function audioPlan(specs: AssetSpecs): AllocPlan {
  const duration = Math.max(0.1, specs.duration ?? 30);
  // Chunking is automatic: short files load whole, long files stream in
  // chunks sized so a chunk is ~1–2 MB of float PCM per channel pair.
  const chunkSeconds = duration <= 45 ? 0 : duration <= 300 ? 10 : 20;
  const budgetBytes = Math.round(duration * 48_000 * 4 * 2);
  const dur =
    duration >= 60
      ? `${Math.floor(duration / 60)}:${String(Math.round(duration % 60)).padStart(2, "0")} min`
      : `${duration.toFixed(1)}s`;
  return {
    sampleCount: 0,
    sampleWidth: 0,
    chunkSeconds,
    budgetBytes,
    note:
      duration <= 45
        ? `${dur} audio → loads whole (no chunking needed)`
        : `${dur} audio → streams in ${chunkSeconds}s chunks automatically`,
  };
}

function imagePlan(specs: AssetSpecs): AllocPlan {
  const width = Math.max(1, specs.width ?? 1024);
  const downscale = width > 4096 ? " · preview downscaled to 4K" : "";
  const budgetBytes = Math.round((specs.bytes * 4) / 3 + 64_000);
  return {
    sampleCount: 0,
    sampleWidth: 0,
    chunkSeconds: 0,
    budgetBytes,
    note: `${width}×${specs.height ?? 0} image${downscale}`,
  };
}

/** Decide how to allocate for an asset. Pure, instant, no prompts. */
export function planAllocation(specs: AssetSpecs): AllocPlan {
  try {
    return specs.kind === "video"
      ? videoPlan(specs)
      : specs.kind === "audio"
        ? audioPlan(specs)
        : imagePlan(specs);
  } catch {
    return {
      sampleCount: 12,
      sampleWidth: 480,
      chunkSeconds: 10,
      budgetBytes: 4 * MB,
      note: "defaults",
    };
  }
}

/** True when the plan exceeds what a portable project should carry. */
export function overBudget(plan: AllocPlan): boolean {
  return plan.budgetBytes > 48 * MB;
}
