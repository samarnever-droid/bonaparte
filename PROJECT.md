# Bonaparte — foundation milestone

## Status

**v0.2: broader editor foundation.** The goal of this milestone is an end-to-end editable composition workflow, not feature parity with After Effects. The previous “100% implemented / forensic clean / GPU-first complete” status was not an accurate description of the application.

The delivered Svelte workspace uses the real Rust renderer in both the Tauri native shell and the development browser bridge. Persistent effects, content editing, composition settings, save/open, still-image import, animation controls and export are integrated. The Orbit example is an ordinary editable document.

## Delivered architecture

- **model:** typed document/effect/style/image data; integer 120,000-tick timebase; rational frame rates; validated transform/effect tracks; atomic bounded Op transactions and undo/redo. Moving a layer retimes transform and effect keys together. Restore operations cannot overwrite live IDs.
- **effects:** manifest-owned types/defaults/ranges/groups and public CPU callbacks; 13 first-party packs, including Color Grade and Gradient Ramp. Color Grade is neutral-by-default and alpha-preserving. Blur is separable and premultiplied, and glow/shadow include alpha-aware halos.
- **engine:** deterministic CPU compositing with styled content, bundled antialiased typography, hierarchical transforms, precomps, ordered effects and backdrop adjustments. Precomp/generated raster sampling is premultiplied linear-light bilinear. Effect-bearing tile paths render full frames then crop.
- **runtime:** shared `EditorSession`, catalog, document validation, versioned persistence, embedded-image decoding, preview overrides, immutable render snapshots, PNG and atomic tick-exact MP4 range export. The user-facing video export uses the full active composition.
- **preview:** development-only HTTP transport, four request workers, rendering outside the state mutex, bounded requests, no arbitrary-path read/write routes. This is not a hosted multi-user backend.
- **native:** Tauri commands around the runtime, file dialogs, asynchronous frame/export work, atomic project/PNG/video destinations.
- **MCP:** catalog, common validated Ops, proposals, history, project serialization/loading, embedded-image rendering and common MP4 exports. MCP has a separate session from the GUI; it does not live-edit an already running desktop session.
- **UI:** Design/Color/Animate views, project/effect/motion library, property/effect inspector, interactive canvas, timeline and normalized easing editor, dialogs, keyboard controls, real RGB histogram and IndexedDB document recovery.

## Acceptance evidence

See [TEST_READY.md](TEST_READY.md) for exact commands/results. Tests include actual rendered pixels and file contents, real FFmpeg exports, an extension plugin with no private engine hooks, failure atomicity, and browser interactions. A successful native compile is **not** claimed as native GUI end-to-end coverage.

## What is not complete

| Capability | Actual state |
|---|---|
| GPU renderer | WGSL and descriptor scaffolding only; the `gpu` feature does not activate rendering |
| CPU/GPU parity | No GPU pixel comparisons; shader validation checks syntax/types only |
| Bounded tile memory | Unfiltered tile utilities exist; filters fall back to full-frame rendering |
| Playback/proxy caching | Lower-level media/cache code exists, but is not an integrated video preview pipeline |
| Media editing | Still images work; video/audio import, decoding/retiming in the editor, waveforms and mixing are not integrated |
| Professional animation | Basic keys/segment easing and recipes; no expressions, spatial paths, roving keys, multiselect key workflows or motion blur |
| Shapes/compositing | Styled primitives and adjustment layers; no pen paths, masks, mattes, booleans, tracking or 3D |
| Typography | Bundled font, regular/bold, size, color, tracking and newlines; no general font browser, shaping, bidi or per-character animators |
| Color finishing | 8-bit sRGB controls and RGB histogram; no HDR, ACES/OCIO, LUTs, wheels, RGB curves or calibrated scopes |
| Export/packaging | PNG and silent H.264, synchronous job lifecycle; no render queue, cancellation, signed installers or updates |

## Next milestones, in order

### 1. Rendering and performance

Implement a real wgpu executor and validate every filter against CPU pixels, including alpha and transfer functions. Add effect ROI/halo propagation, buffer pooling, bounded cache accounting, preview resolutions and cancellation. Replace full-frame-per-tile fallback. Establish repeatable latency/memory benchmarks on integrated GPUs before advertising a real-time or 8 GB target.

**Exit criteria:** correct CPU/GPU comparison fixtures, no filter seams, a measured memory budget, and responsive preview on documented hardware.

### 2. Media and compositing

Wire FFmpeg probing/decoding/proxies into the shared runtime. Introduce source in/out and time remap semantics, asset relinking, audio timeline/mixing, masks/track mattes and editable vector paths. Keep editing/export on one evaluator.

**Exit criteria:** save/open and deterministic export of video+audio+masked compositions, including missing-source recovery.

### 3. Animation and typography

Add timeline multiselect and key selection/deletion/duplication, value/speed graphs, spatial Bézier paths, motion blur, robust font selection and shaping, and text animators. Design expressions with explicit time/resource limits rather than arbitrary host execution.

**Exit criteria:** reusable, retimable, parameterized motion compositions with round-trip and undo regressions.

### 4. Professional finishing and delivery

Introduce a documented higher-precision color pipeline, color spaces/OCIO, curves/wheels/LUTs and calibrated scopes. Add export queue, cancellation, recovery, wider codecs and cross-platform desktop E2E/installer testing.

**Exit criteria:** color-managed reference files, resilient batch export and signed releases tested on supported platforms.

These are substantial engineering milestones. There is no responsible single-step claim or completion percentage for “After Effects parity.”
