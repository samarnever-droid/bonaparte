# Bonaparte — editor, preview performance and Audio Studio

## Status

**v0.5.1: editing stability**, following v0.5 interaction/channel processing and v0.4 Audio Studio and the v0.3 rendering/performance work and v0.2 broader editor foundation. The current goal is a useful, faster editing workflow—not feature parity with After Effects. The previous “100% implemented / forensic clean / GPU-first complete” status was not an accurate description of the application.

The delivered Svelte workspace uses the real Rust renderer in both the Tauri native shell and the development browser bridge. Persistent effects, content editing, composition settings, save/open, still-image import, animation controls and export are integrated. The Orbit example is an ordinary editable document. The v0.3 preview adds an executable wgpu path, CPU fallback, resolution-aware prepared scenes, bounded caches and truthful diagnostics. See [rendering architecture and measurements](docs/RENDERING.md).

The new audio arrangement is sample-addressed and fully undoable. Native decoding, disk-backed PCM, waveforms, multitrack mixing, streaming AudioWorklet playback, WAV and AAC movie exports are integrated. The v0.5 update adds real channel-strip processing, bus routing, stateful DSP checkpoints and low-latency interaction proxies/deltas. This is not “best audio ever” or a complete DAW. [Audio details](docs/AUDIO.md).

## Current stabilization work

The user-reported rotation, scaling quality, focus-dependent text/color changes, timeline range and visibility failures were reproduced in specific scenarios. Fixes and remaining limits are recorded in [STABILITY-0.5.1.md](docs/STABILITY-0.5.1.md). This is not a claim to have resolved every unreported UI defect. No new audio/DAW milestone was implemented during this patch.

## Delivered architecture

- **model:** typed document/effect/style/image data; integer 120,000-tick timebase; rational frame rates; validated transform/effect tracks; atomic bounded Op transactions and undo/redo. Moving a layer retimes transform and effect keys together. Restore operations cannot overwrite live IDs.
- **effects:** manifest-owned types/defaults/ranges/groups and public CPU callbacks; 13 first-party packs, including Color Grade and Gradient Ramp. Color Grade is neutral-by-default and alpha-preserving. Blur is separable and premultiplied, and glow/shadow include alpha-aware halos.
- **engine:** deterministic CPU compositing with styled content, bundled antialiased typography, hierarchical transforms, precomps, ordered effects and backdrop adjustments. Precomp/generated raster sampling is premultiplied linear-light bilinear. Effect-bearing tile paths render full frames then crop.
- **GPU preview:** real wgpu device/queue, linear-light transforms/blends, nested compositions, adjustment mixing, reflected fragment-effect uniforms, row-padded readback and bounded texture/upload reuse. Nine filters plus the Circle program are enabled; unsupported kernels fall back to a whole CPU frame.
- **runtime:** shared `EditorSession`, catalog, document validation, versioned persistence, embedded-image decoding, preview overrides, immutable render snapshots, PNG and atomic tick-exact MP4 range export. The user-facing video export uses the full active composition.
- **preview:** development-only HTTP transport, four request workers, rendering outside the state mutex, bounded requests, no arbitrary-path read/write routes. This is not a hosted multi-user backend.
- **audio:** pure deterministic float mixer, clip/source timing, gain/pan, envelopes, mute/solo, nested composition timing and windowed-sinc resampling. One evaluation path feeds playback and exports.
- **audio I/O / UI:** original encoded sources in version-3 projects, native decode/peaks, memory-mapped PCM, bounded worklet queue and audio-clocked visual playhead; audio workspace/inspector, clip editing and source library.
- **native:** Tauri commands around the runtime, file dialogs, asynchronous frame/export work, atomic project/PNG/video destinations.
- **MCP:** catalog, common validated Ops, proposals, history, project serialization/loading, embedded-image rendering and common MP4 exports. MCP has a separate session from the GUI; it does not live-edit an already running desktop session.
- **prepared preview:** immutable scene snapshots, full/half/quarter grids, 64 MiB frame LRU, 32 MiB source LRU, epoch-safe invalidation and transient-gesture exclusion. Original CPU rendering remains the export/reference path.
- **UI:** Auto/CPU/GPU selection, Auto/full/half/quarter quality, cache diagnostics/clear/retry, plus Design/Color/Animate views, project/effect/motion library, property/effect inspector, interactive canvas, timeline and normalized easing editor, dialogs, keyboard controls, real RGB histogram and IndexedDB document recovery.

## Acceptance evidence

See [TEST_READY.md](TEST_READY.md) for exact commands/results. Tests include actual rendered pixels and file contents, real FFmpeg exports, an extension plugin with no private engine hooks, failure atomicity, and browser interactions. A successful native compile is **not** claimed as native GUI end-to-end coverage.

## What is not complete

| Capability | Actual state |
|---|---|
| GPU renderer | Executable hybrid preview path; Gaussian Blur/Glow/Drop Shadow remain CPU fallback. GPU export and fully GPU-native rasterization remain future work |
| CPU/GPU parity | Executed fixture comparisons on software Vulkan with 1–3 byte tolerances; physical-GPU and cross-platform validation still needed |
| Bounded tile memory | Unfiltered tile utilities exist; filters fall back to full-frame rendering |
| Playback/proxy caching | Rendered-frame/source/upload caching works; video proxies, prefetch and an integrated video playback pipeline do not |
| Media editing | Images and sample-addressed mono/stereo audio work; integrated video editing/proxies and large linked-media sessions remain future work |
| Professional animation | Basic keys/segment easing and recipes; no expressions, spatial paths, roving keys, multiselect key workflows or motion blur |
| Shapes/compositing | Styled primitives and adjustment layers; no pen paths, masks, mattes, booleans, tracking or 3D |
| Typography | Bundled font, regular/bold, size, color, tracking and newlines; no general font browser, shaping, bidi or per-character animators |
| Color finishing | 8-bit sRGB controls and RGB histogram; no HDR, ACES/OCIO, LUTs, wheels, RGB curves or calibrated scopes |
| Audio workstation depth | EQ/compression, compensated limiting and buses/sends work; no recording, external plugins, pitch-preserving stretch, surround, spectral repair or loudness workflow yet |
| Export/packaging | PNG, float WAV and H.264 with AAC; still no job queue/cancellation, signed installers or updates |

## Next milestones, in order

### 1. Rendering and performance

Delivered in v0.3: executable wgpu preview, CPU/GPU fixture comparisons, reduced-resolution sampling, source/frame caching, texture reuse and diagnostics. Still needed: separable GPU blur/glow/shadow, ROI/halo propagation, cancellation, prefetch and GPU export. Replace full-frame-per-tile fallback and benchmark physical integrated GPUs before advertising a real-time or 8 GB target.

**Exit criteria:** correct CPU/GPU comparison fixtures, no filter seams, a measured memory budget, and responsive preview on documented hardware.

### 2. Media and compositing

Audio import, timeline/source timing, waveforms, mixing and audio export landed in v0.4. Next: deepen audio DSP and long-session handling, then wire video/proxies/relinking, masks/track mattes and editable vector paths. Keep playback/export on shared evaluation rules.

**Exit criteria:** save/open and deterministic export of video+audio+masked compositions, including missing-source recovery.

### 3. Animation and typography

Add timeline multiselect and key selection/deletion/duplication, value/speed graphs, spatial Bézier paths, motion blur, robust font selection and shaping, and text animators. Design expressions with explicit time/resource limits rather than arbitrary host execution.

**Exit criteria:** reusable, retimable, parameterized motion compositions with round-trip and undo regressions.

### 4. Professional finishing and delivery

Introduce a documented higher-precision color pipeline, color spaces/OCIO, curves/wheels/LUTs and calibrated scopes. Add export queue, cancellation, recovery, wider codecs and cross-platform desktop E2E/installer testing.

**Exit criteria:** color-managed reference files, resilient batch export and signed releases tested on supported platforms.

These are substantial engineering milestones. There is no responsible single-step claim or completion percentage for “After Effects parity.”
