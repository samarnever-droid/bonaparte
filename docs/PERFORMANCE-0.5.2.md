# Performance and GPU blur patch (post-0.5.1)

This patch speeds up the CPU render paths and finishes the largest GPU preview
gap, without changing any saved output pixel. It is measurement-first: every
number below is reproducible with the commands shown.

## What changed

### 1. Row-parallel CPU rendering (`bonaparte-engine`, `bonaparte-effects`)

The compositors spent most of a frame inside per-pixel loops that are
independent per output pixel. Those loops are now executed in parallel with
rayon on native targets and stay sequential on wasm
(`[target.'cfg(not(target_family = "wasm"))'.dependencies]`):

- `reference.rs` — the transformed-layer bounding-box loop and the
  effect/adjustment composite loop.
- `preview.rs` — `render_scene_cpu` layer loop and effect composite loop (the
  prepared preview path the editor actually uses).
- `cpu_reference.rs` / `grading.rs` — every first-party CPU kernel: separable
  blur (both passes), glow composite, drop shadow, directional blur,
  transform, chromatic aberration, vignette, invert, tint, color adjust,
  color grade, gradient. The color-grade "previous pixel" memo is kept
  per-row; it only caches results of a pure function, so output is unchanged.

**Byte-exactness policy:** each parallel row performs the identical per-pixel
arithmetic as the sequential loop (`slice_pixel`/`slice_set_pixel`/
`slice_blend_pixel` are the shared helpers behind the `Frame` accessors).
Full-resolution prepared CPU output remains byte-for-byte identical to the
independent legacy reference renderer; the existing parity fixtures assert
this and still pass. No conversion math (powf-based sRGB/linear) was changed,
because quantization-order differences could shift bytes.

### 2. Cached text rasterization (`bonaparte-engine::typography`)

`rasterize_text` was re-running fontdue glyph rasterization for every text
layer on every rendered frame — the legacy reference path had no cache (the
prepared preview's `SourceCache` is a different layer). A bounded global cache
keyed by `(text, size bits, bold, tracking bits)` now returns
`Arc<TextBitmap>`; 512 entries max, cleared wholesale when full. Coverage
output is unchanged.

### 3. Separable two-pass GPU Gaussian Blur (`bonaparte-gpu`, `builtin.blur`)

Gaussian Blur previously forced a whole-frame CPU fallback. The pack's WGSL is
now a **separable two-pass kernel** that matches the CPU reference algorithm
(per-axis normalized gaussian, premultiplied accumulation, unpremultiplied
output, out-of-bounds samples skipped rather than clamped when repeat edge is
off):

- The host detects the contract generically: any program whose reflected
  `Params` declares `direction_x`/`direction_y` runs as two passes —
  `(1, 0)` into an intermediate target, then `(0, 1)` into the final one.
  No pack ID is hard-coded; naga reflection drives it like the rest of the
  uniform contract.
- `manifest.toml` sets `gpu_preview = true`; scenes containing only blur (and
  the other nine GPU filters) no longer fall back to CPU.
- New fixture `separable_gpu_blur_matches_cpu_reference` compares real GPU
  submission/readback against the CPU path (static radius, both repeat-edge
  modes, and an animated radius track) at the standard 2-level tolerance.
- The "unsupported kernels are explicit" test now uses Glow, which remains a
  CPU fallback kernel; the runtime workflow test and the browser performance
  e2e were updated the same way.

Glow and Drop Shadow still execute on the CPU preview path. They compose a
blur with additional passes and need the same multi-pass treatment plus their
own fixtures; this patch deliberately does not claim them.

## Measurements

Two-vCPU sandbox, llvmpipe (Mesa software Vulkan) for GPU tests, release
build, `examples/orbit.bonaparte.json` through the in-process session:

```sh
cargo run --release -p bonaparte-runtime --example preview_benchmark
```

| Path | Divisor | Before (ms) | After (ms) | Speedup |
|---|---:|---:|---:|---:|
| Legacy CPU raw | 1 | 55.53 | 34.56 | 1.61× |
| Prepared preview packet | 1 | 54.74 | 34.62 | 1.58× |
| Prepared preview packet | 2 | 14.89 | 9.66 | 1.54× |
| Prepared preview packet | 4 | 3.95 | 3.32 | 1.19× |

These are medians of 8 successive frames after warm-up, including
snapshot/packet work, excluding network and browser painting. They are **not**
hardware-GPU or real-time claims; the sandbox has no physical GPU. The row
parallelism scales with available cores, so desktop hardware should see
larger gains than this two-vCPU environment — and still needs to be measured
there before claiming any number.

## Verification

```sh
cargo fmt --all --check
cargo test --workspace --exclude bonaparte-app
BONAPARTE_REQUIRE_GPU_TESTS=1 cargo test -p bonaparte-gpu
cargo check -p bonaparte-runtime --no-default-features
cargo check -p bonaparte-engine -p bonaparte-effects -p bonaparte-audio --target wasm32-unknown-unknown
```

All pass in the development sandbox except `bonaparte-media`'s
`test_streaming_export_mp4_video`, which requires an FFmpeg binary on PATH and
fails identically on the unpatched baseline here.

## Not done here (next candidates, in value order)

- DONE since this document was written: embedded media payloads are `Arc`
  shared (edit latency no longer scales with media size), render jobs are
  cooperatively cancellable, and glow/drop shadow run as native multi-pass
  GPU programs with CPU-parity fixtures.
- Phase 2 structural sharing: effect stacks, tracks, and compositions still
  clone on commit/undo; `Arc` them the way media payloads now are.
- Playback prefetch (RAM preview) on top of the cancellation plumbing.
- Bounded-ROI effect processing into the tile path (filters still render
  full frames and crop).
- The separable blur pack could adopt the same `Rgba16Float` chain the
  multi-pass packs use; its 8-bit intermediate currently passes parity only
  within tolerance.
