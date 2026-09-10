# v0.3 — rendering and preview performance

## Delivered

The preview is no longer limited to the original CPU renderer. `crates/gpu` initializes a real **wgpu** device, records render passes, submits command buffers, and reads RGBA pixels back. It supports headless Vulkan, Metal and DX12 through wgpu. The browser and Tauri use the same native preview service; there is no JavaScript compositor or fabricated GPU badge.

This is a **hybrid preview renderer**, not a fully GPU-native editor. Typography and shape-generator source rasters are produced/cached on the CPU. Geometry evaluation is CPU-side. GPU passes handle transformed source sampling, all eight blend modes, nested compositions, adjustment mixing and GPU-enabled effect stacks. Exports remain on the full-resolution CPU reference path.

## Preview controls

The viewer footer offers:

- **Auto quality:** full resolution when paused; half resolution during playback.
- **Full / Half / Quarter:** choose the actual render grid, not just CSS zoom. Odd dimensions round up, and dimensions never become zero.
- **Auto engine / CPU / GPU:** a preference. The engine badge always reports the **actual** executor.
- **LIVE / CACHED:** whether this particular frame was rendered or served from the frame LRU.

Open the **Rust compositor** badge for the adapter/API, software-device warning, frame/source memory, cache hits, CPU/GPU frame counts, **Clear caches** and **Retry GPU**. Clearing caches is not a document edit and does not touch undo history.

A quarter-resolution preview does not resize the composition, move its layers, or downsample an exported PNG/MP4. The histogram describes the currently displayed preview grid. Export remains 8-bit sRGB at the composition's original dimensions.

## Actual backend selection

| Situation | Behavior |
|---|---|
| CPU requested | Prepared scene executes on CPU |
| Auto, compatible hardware adapter and supported scene | GPU preview |
| Auto, software adapter only | CPU preview; software device is identified |
| GPU explicitly requested with a software adapter | GPU commands execute through the software driver; badge says **GPU · software** |
| Adapter unavailable, GPU disabled/uncompiled, unsupported effect or GPU error | Complete CPU frame, with a fallback reason |

The verification sandbox exposes **llvmpipe (Mesa software Vulkan)**, not a physical GPU. Real command submission/readback and CPU/GPU pixel comparisons were tested there. These results do not prove a speedup on a physical integrated/discrete GPU.

`BONAPARTE_DISABLE_GPU=1` forces runtime CPU fallback. A binary can also be built without the optional GPU dependency:

```sh
cargo build -p bonaparte-runtime --no-default-features
```

The `gpu` feature belongs to **bonaparte-runtime**, not the pure engine. The old empty engine feature was removed; its legacy shader-descriptor module remains for compatibility.

## Effect coverage

The fragment-pass host executes **9 filters**: Color Grade, Gradient Ramp, Color Adjust, Tint, Invert, Vignette, Transform Effect, Chromatic Aberration and Directional Blur. The Circle generator's fragment program is also supported by the contract; normal shape content currently uses cached CPU generator pixels.

**Gaussian Blur, Glow and Drop Shadow remain CPU preview kernels.** If an enabled instance appears anywhere in a visible prepared scene, the entire frame falls back to CPU. There are no mixed intermediate CPU/GPU round-trips masquerading as an accelerated frame. Bypassing effects can make such a scene GPU-eligible again.

GPU effects opt in through `gpu_preview = true` in their manifest. A pack's `fs_main` uses the public group-0 bindings: source texture, sampler, raster resolution, composition time and `Params`. **Naga reflection supplies member offsets and struct padding**; manifest order is not assumed to be a GPU buffer layout. Tests register an effect unknown to the engine with scalar, vec4, vec2 and boolean uniforms.

The GUI still does not install arbitrary user shader files. Hosts can register trusted CPU/WGSL plugins programmatically. Unsupported/invalid programs produce a reason and CPU fallback, not silently incorrect output. Device failures are retained until **Retry GPU** rather than being retried every frame.

Pixel-valued parameters opt in to `scale_with_resolution = true`. The shared registry validates defaults/keyframes, evaluates and bounds animation, then scales those lengths for the reduced raster grid. Normalized values (e.g. vignette radius) are not scaled.

## Prepared scenes and correctness

`engine::preview::prepare_scene` produces the same logical geometry for either executor. It keeps composition coordinates, parenting, source dimensions, effects and timing intact; only the output raster grid changes. This avoids the common error of resizing the saved project to render a smaller preview.

Full-resolution prepared CPU output is checked **byte-for-byte** against the independent original reference renderer on the editable Orbit example and edited typography. Reduced grids use linear sampling for text/image sources to reduce aliasing. GPU fixture comparisons use bounded 8-bit tolerances (1–3 channel levels depending on the fixture), including alpha, not a claim of universal bit identity across drivers.

Coverage includes all blend modes, non-integer transforms, nested compositions, colored typography, half/quarter grids, ordered effects, adjustment opacity, RGBA uploads and row-padded readback. Low-level GPU tests call `GpuRenderer` directly and assert real submission counts; that API cannot silently invoke the CPU renderer.

## Cache and memory policy

- **Frame LRU:** 64 MiB, at most 2048 entries. Keys include epoch, document revision, composition, exact time, resolution, effects bypass and requested backend.
- **Source LRU:** 32 MiB, at most 1024 retained text/generator rasters. Transform changes do not rerasterize unchanged glyphs or generators.
- **GPU target pool:** 128 MiB; textures are reused when not held by the current scene.
- **GPU upload cache:** 64 MiB, at most 1024 uploads. Immutable shared source identity prevents a reused media ID from selecting an old texture.
- **Prepared CPU source limit:** 128 MiB of distinct pinned source rasters per prepared scene.

These are accounting limits for named resources, **not total RSS, total VRAM or an 8 GB guarantee**. In-flight Arcs, project documents, readback buffers, encoders, driver allocations and compiled programs have additional costs.

Every persistent mutation/open/undo/redo invalidates frame entries. In-flight jobs keep immutable snapshots, and an old epoch cannot repopulate a cleared cache. Gesture overrides are validated separately and never enter the persistent frame LRU. Normal playback shares the document snapshot instead of copying embedded base64 images each frame.

The UI coalesces requests, rejects obsolete revision/quality/backend/bypass responses, and avoids letting a late paused-scrub response overwrite the current frame. During playback, completed frames can be displayed while the next requested time advances; playback is not guaranteed to render every frame. The stale-response browser test delays a **real Rust response** rather than mocking pixels.

## Protocol

`POST /api/preview_frame` and Tauri `preview_frame` return:

```
4 bytes  ASCII BPF3
4 bytes  little-endian JSON metadata length
N bytes  UTF-8 JSON metadata
W*H*4    tightly packed straight-alpha RGBA8 sRGB
```

Metadata includes raster/logical dimensions, divisor, time, revision, actual executor, adapter, fallback reason, cache hit and render timing. The old width/height/raw endpoint is retained for existing callers. `preview_status`, `clear_preview_cache` and `retry_gpu` are available through the shared command service.

## Reproduce the performance sample

```sh
cargo run -p bonaparte-runtime --example preview_benchmark -- --include-gpu
```

The command uses its own in-process editable example and never resets a live session. It warms the source/program caches, samples eight consecutive times, then repeats a frame to measure cache delivery. Raw data and environment are in [`performance-v0.3.json`](performance-v0.3.json).

Measured medians in the two-vCPU sandbox, including snapshot/packet work but **excluding HTTP and browser painting**:

| Path | Raster | Median | Repeated cached frame |
|---|---:|---:|---:|
| Original CPU raw path | 960×540 | 102.37 ms | — |
| Prepared CPU, full | 960×540 | 83.02 ms | 0.22 ms |
| Prepared CPU, half | 480×270 | 22.94 ms | 0.10 ms |
| Prepared CPU, quarter | 240×135 | 6.80 ms | 0.06 ms |
| GPU API, **software Vulkan**, full | 960×540 | 62.80 ms | 0.26 ms |
| GPU API, **software Vulkan**, half | 480×270 | 23.41 ms | 0.09 ms |
| GPU API, **software Vulkan**, quarter | 240×135 | 12.79 ms | 0.05 ms |

Half/quarter are faster partly because they render fewer pixels; they are not full-quality speedups. Cache delivery is not rendering throughput. Hardware-GPU measurements and long-running memory/latency benchmarks remain necessary.

## Static-prefix frame caching (0.7)

After scene preparation, CPU compositing walks layers bottom-up. Most motion
graphics frames are mostly static: the pixels below the first animated layer
are identical frame to frame. `engine::prefix::PrefixCache` exploits exactly
that. Every layer gets a tiny token of the *sampled* state the painter
actually reads (layer id, inverse transform, size, bounds, opacity, blend
mode, and content identity — solid colors, shape-style hashes, or
`Arc::ptr_eq` of cached rasters, whose reference the token pins against
eviction). The first frame renders normally and snapshots the frame buffer at
the last-known static boundary; later frames find the first token mismatch —
the boundary — and only if the snapshot covers at least that many layers do
they resume from it. Anything above the boundary (the animated layers, effects,
adjustments, rasters with volatile content) is repainted normally; a volatile
layer can never be part of any prefix. The result is **bit-identical output by
construction** — a resumed frame is a prefix of the fully painted one — which
the test suite verifies against `render_scene_cpu` across mid-sequence edits
and non-adjacent scrub jumps.

Measured on this machine (debug build, 2 vCPUs):

| Workload | Plain CPU | With prefix cache | Speedup |
| --- | --- | --- | --- |
| 121-layer plate wall, 320×180, steady-state frame | 332 ms | 0.28 ms | **~1,180×** |
| 62-layer text poster, 320×180, full composite loop | 8 ms/frame | 0.9 ms/frame | ~9× |
| Live preview, 241-layer wall, 640×360, 10 distinct times (median) | 2,629 ms | 52 ms | **~50×** |
| Full Playwright suite wall time (114 tests) | 20.7 min | 5.2 min | ~4× overall |

The last row is the point: prepare-scene sampling, PNG encoding and transport
are per-frame fixed costs the cache doesn't touch, so end-to-end wins
converge on the composited share of work. Stacks of large full-frame layers
(grading plates, glow passes, vignettes) see the largest returns; the skip
ratio equals the fraction of composited pixels that didn't change.

Caches are keyed by (comp, output size), capped at five live slots, and shared
by the preview runtime and the sequential export path; concurrent renders of
one comp serialize on the slot but stay correct by validation, and a canceled
render can never corrupt the snapshot (the pair of snapshot+boundary is
written only on full pass completion). `BONAPARTE_NO_PREFIX_CACHE=1` restores
the plain path for A/B benchmarking or emergency rollback.

## Still outstanding

Separable GPU blur/glow/shadow; ROI/halo-aware tiling; cancellation of in-flight native/GPU work; playback prefetch/RAM preview; GPU export; device hot-plug stress testing; physical integrated-GPU testing and cross-platform native E2E. Video/audio/proxy integration, masks, expressions, 3D and professional color management remain later editor milestones.
