# Handoff Report: Effects & Media Explorer (R3 & R4)

**From**: Effects & Media Explorer (`survey_effects_media`)  
**To**: Orchestrator (`parent`, id: `bedaa0af-da34-4abe-9c13-3820626660a7`)  
**Date**: 2026-08-30  
**Type**: Hard Handoff (Investigation & Survey Complete)  

---

## 1. Observation

1. **Workspace Compilation State**:
   - Running `cargo check --workspace` fails with exit code 1:
     - `crates/model/src/document.rs:328`: `error[E0369]: cannot add time::Time to time::Time`
     - `crates/model/src/ops.rs:110, 114`: `error[E0599]: &time::Time doesn't satisfy time::Time: std::fmt::Display`
     - `crates/model/src/ops.rs:497`: `error[E0382]: use of moved value: op`
   - `bonaparte-effects` and `bonaparte-media` depend on `bonaparte-model` and cannot build until model compiles.

2. **R3 (`bonaparte-effects`) Codebase Structure**:
   - `crates/effects/Cargo.toml`: contains only `bonaparte-model = { path = "../model" }`.
   - `crates/effects/src/lib.rs` (44 lines total):
     - Line 19–20: `pub const GLOW_MANIFEST: &str = include_str!("../packs/glow/manifest.toml"); pub const GLOW_SHADER: &str = include_str!("../packs/glow/glow.wgsl");`
     - Line 24–28: `pub fn builtin_manifests() -> Vec<EffectManifest> { vec![EffectManifest::parse(GLOW_MANIFEST)...] }`
     - Line 37–42: 1 unit test `every_builtin_is_a_valid_public_api_plugin` checking `m.validate()` and non-empty `doc`.
   - `crates/effects/packs/`: Only `packs/glow/` exists (`manifest.toml`, `glow.wgsl`, `README.md`).
   - 9 core effect packs required by R3 are completely missing: `blur`, `drop_shadow`, `color_adjust`, `transform`, `vignette`, `chromatic_aberration`, `invert`, `tint`, `directional_blur`.
   - No dynamic plugin loader / directory scanner or registry lookup struct (`EffectRegistry`) exists.
   - No golden-frame pixel tests or CPU reference effect evaluators exist.

3. **R4 (`bonaparte-media`) Codebase Structure**:
   - `crates/media/Cargo.toml`: contains `bonaparte-model`, `thiserror`.
   - `crates/media/src/lib.rs` (22 lines total):
     - Line 17–21: `pub mod placeholder { pub const NOT_YET_IMPLEMENTED: &str = "media pipeline lands with the vertical slice"; }`
   - 0 modules implemented: no probe, decode, proxy, cache, or export modules.
   - 0 tests exist.

4. **External Tool Availability**:
   - `ffmpeg -version` executed successfully: `ffmpeg version 8.1.1-full_build-www.gyan.dev` with support for `libx264`, `libx265`, `rawvideo`, hardware codecs (`nvenc`, `amf`, `qsv`, `dxva2`).
   - `ffprobe -version` executed successfully: `ffprobe version 8.1.1-full_build-www.gyan.dev`.

5. **Engine & App Seam**:
   - `crates/engine/src/reference.rs:26-30`: Defines `MediaFrames` trait (`fn frame_rgba(&self, media: MediaId, time: Time) -> Option<FrameView<'_>>`).
   - `crates/engine/src/graph.rs:23-28`: `NodeKind::Effect { effect: String, params: Vec<(String, EffectParamValue)> }` expects effect plugin identifiers.
   - `ui/src-tauri/src/main.rs:15`: imports `render_comp`, but currently has type mismatches on `Time` and missing `MediaFrames` argument.

---

## 2. Logic Chain

1. **Microkernel Compliance (R3)**:
   - Observation 2 shows that only `builtin.glow` exists, and its shader is a 4-sample stub.
   - To satisfy R3 ("~10 core first-party GPU effect packs adhering strictly to the public plugin contract with zero private engine hooks"), 9 additional first-party packs must be authored in `crates/effects/packs/` along with complete TOML manifests, WGSL shaders, and parameter documentation.
   - An `EffectRegistry` must be created to resolve manifests and shader sources both at compile-time and dynamically from filesystem paths.
   - To enable pixel-deterministic testing without GPU hardware dependencies in CI, a CPU software reference evaluator (`cpu_reference.rs`) is required.

2. **Crash-Isolated Media I/O & Export (R4)**:
   - Observation 3 shows `bonaparte-media` is currently an empty placeholder.
   - Observation 4 confirms `ffmpeg` 8.1.1 and `ffprobe` 8.1.1 are installed on the host system.
   - To satisfy R4 without linking C libraries (LGPL safety & crash isolation), `bonaparte-media` must spawn `ffmpeg` and `ffprobe` as child subprocesses using `std::process::Command`.
   - The media cache must implement `bonaparte_engine::reference::MediaFrames` (Observation 5) with a two-tier design: an in-memory LRU cache capped at 64MB and a disk frame store. This guarantees flat RAM consumption during scrubbing.
   - The export pipeline must stream rendered composition frames from `bonaparte_engine::render_comp` directly into FFmpeg stdin. This guarantees $O(1)$ RAM usage regardless of export duration.

3. **Prerequisite Unblocking**:
   - Observation 1 demonstrates that `bonaparte-model` fails to compile due to missing arithmetic/traits on `Time` and an `Op` ownership issue. Fixing `bonaparte-model` is the strict first dependency before implementing R3 or R4.

---

## 3. Caveats

1. **Hardware Acceleration Support**:
   - The FFmpeg installation supports NVENC and AMF hardware encoders, but fallback to CPU software encoder `libx264` (`-c:v libx264 -preset medium -crf 18`) should remain default to guarantee portability on all target systems.
2. **GPU Shaders Execution in CI**:
   - While WGSL shaders will run via `wgpu` when the GPU engine feature is enabled, CI environments often lack native Vulkan/DirectX 12 adapters; therefore, CPU reference evaluators and WGSL syntax validators (`naga`) are critical for CI golden-frame passes.
3. **Model Effect Wiring**:
   - Attaching effects to layers on the timeline requires adding an `effects: Vec<EffectInstance>` field to `Layer` and corresponding `Op` variants (`AddEffect`, `RemoveEffect`, `SetEffectParam`). This is a shared model-engine-ui coordination item.

---

## 4. Conclusion

1. **Scope & Status**:
   - R3 is currently a 44-line skeleton with 1 stub pack. Needs 9 additional effect packs, `EffectRegistry`, WGSL shaders, and golden-frame tests.
   - R4 is currently empty. Needs complete implementation of probing (`probe.rs`), decoding (`decode.rs`), proxying (`proxy.rs`), disk-backed caching (`cache.rs`), and streaming MP4 export (`export.rs`).
2. **Actionable Roadmap**:
   - **Step 1**: Unblock `bonaparte-model` (`Time` arithmetic + `Display`, `Op::commit` fix).
   - **Step 2**: Implement R3 (`crates/effects`: 10 effect packs, registry, CPU reference evaluator, manifest validation tests, golden-frame tests).
   - **Step 3**: Implement R4 (`crates/media`: FFprobe parser, FFmpeg decoder, proxy generator, two-tier disk cache implementing `MediaFrames`, streaming MP4 exporter, unit tests).
   - **Step 4**: Integrate media cache and export command into `bonaparte-app` and verify workspace test pass.

---

## 5. Verification Method

1. **Model Compilation Check**:
   ```bash
   cargo check -p bonaparte-model
   ```
   *Pass criteria*: Compiles with zero errors and zero warnings.

2. **Effects Crate & Golden Frame Tests**:
   ```bash
   cargo test -p bonaparte-effects
   ```
   *Pass criteria*: All 10 effect manifests validate, all parameter docstrings are present, and CPU golden-frame tests pass.

3. **Media Pipeline Tests**:
   ```bash
   cargo test -p bonaparte-media
   ```
   *Pass criteria*: FFmpeg/FFprobe availability check passes, synthetic media probing extracts valid duration/FPS/resolution, frame decoder returns RGBA8 buffers, disk cache enforces RAM ceiling, and MP4 export generates a valid playable video file.

4. **Full Workspace Verification**:
   ```bash
   cargo test --workspace
   ```
   *Pass criteria*: 100% tests pass across all workspace crates with zero warnings or errors.
