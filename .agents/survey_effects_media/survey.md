# Survey Report: Requirements R3 (bonaparte-effects) & R4 (bonaparte-media)
**Bonaparte Ring 1 Vertical Slice Exploration**
**Date**: 2026-08-30
**Explorer**: Effects & Media Explorer (`survey_effects_media`)

---

## Executive Summary

Requirements R3 (Microkernel Plugin Host & First-Party Effect Packs) and R4 (Crash-Isolated Media I/O & Streaming Export) form the visual extensibility and media I/O foundation of Bonaparte. Currently, **R3 is in a minimal skeleton state** (only 1 basic effect pack stub `builtin.glow`, with missing loader/registry infrastructure and 9 missing core effect packs), while **R4 is completely unwritten** (placeholder crate with 0 implemented functions). Furthermore, workspace compilation is currently blocked by trait/arithmetic errors in `bonaparte-model`. This report provides a complete feature inventory, architectural blueprint, gap analysis, test plan, and execution sequence for implementing both requirements.

---

## 1. Feature Inventory for R3 & R4

| Feature ID | Requirement | Feature Name | Status | Exact File Path(s) | Implementation Notes & Gaps |
|---|---|---|---|---|---|
| **F-R3.1** | R3 | Plugin Manifest Schema & Parser | **Partial** | `crates/model/src/manifest.rs` | `EffectManifest::parse`, `EffectManifest::validate`, `ParamDef`, `ParamKind`, `GpuCost` exist. Needs default-value range checks and path verification. |
| **F-R3.2** | R3 | Plugin Loader & Registry | **Missing** | `crates/effects/src/lib.rs` | Currently only exposes `builtin_manifests()` returning a hardcoded `vec![]`. Missing `EffectRegistry`, dynamic directory scanner (`load_from_dir`), and shader code resolver. |
| **F-R3.3** | R3 | WGSL Shader Contract Definition | **Partial** | `crates/effects/packs/glow/glow.wgsl` | 5-binding contract established (`u_texture`, `u_sampler`, `u_resolution`, `u_time`, `params`). Alignment/uniform packing must be strictly validated. |
| **F-R3.4** | R3 | First-Party Pack: Glow (`builtin.glow`) | **Partial** | `crates/effects/packs/glow/` (`manifest.toml`, `glow.wgsl`, `README.md`) | Manifest and README exist; WGSL shader is a 4-sample stub that needs a complete Gaussian/bloom multi-sample implementation. |
| **F-R3.5** | R3 | First-Party Pack: Blur (`builtin.blur`) | **Missing** | `crates/effects/packs/blur/` | Separable/Gaussian blur pack missing (Radius, Direction, Repeat Edge Pixels). |
| **F-R3.6** | R3 | First-Party Pack: Drop Shadow (`builtin.drop_shadow`) | **Missing** | `crates/effects/packs/drop_shadow/` | Drop Shadow pack missing (Angle, Distance, Blur, Color, Opacity). |
| **F-R3.7** | R3 | First-Party Pack: Color Adjust (`builtin.color_adjust`) | **Missing** | `crates/effects/packs/color_adjust/` | Color Adjust pack missing (Brightness, Contrast, Saturation, Hue Shift, Exposure). |
| **F-R3.8** | R3 | First-Party Pack: Transform / Distort (`builtin.transform`) | **Missing** | `crates/effects/packs/transform/` | Transform pack missing (Offset, Scale, Rotation, Skew, Anchor Point). |
| **F-R3.9** | R3 | First-Party Pack: Vignette (`builtin.vignette`) | **Missing** | `crates/effects/packs/vignette/` | Vignette pack missing (Radius, Softness, Intensity, Center, Color). |
| **F-R3.10** | R3 | First-Party Pack: Chromatic Aberration (`builtin.chromatic_aberration`) | **Missing** | `crates/effects/packs/chromatic_aberration/` | Chromatic Aberration pack missing (Offset, Angle, Mode). |
| **F-R3.11** | R3 | First-Party Pack: Invert (`builtin.invert`) | **Missing** | `crates/effects/packs/invert/` | Invert pack missing (Invert RGB, Invert Alpha, Amount). |
| **F-R3.12** | R3 | First-Party Pack: Tint (`builtin.tint`) | **Missing** | `crates/effects/packs/tint/` | Tint / Color Map pack missing (Map Black To, Map White To, Amount). |
| **F-R3.13** | R3 | First-Party Pack: Directional Blur (`builtin.directional_blur`) | **Missing** | `crates/effects/packs/directional_blur/` | Directional / Motion Blur pack missing (Length, Angle). |
| **F-R3.14** | R3 | Pixel-Deterministic Golden-Frame Tests | **Missing** | `crates/effects/tests/` / `crates/effects/src/lib.rs` | No pixel comparison tests or CPU effect reference implementations exist. Only manifest validation is tested. |
| **F-R4.1** | R4 | Crash-Isolated FFmpeg Process Orchestration | **Missing** | `crates/media/src/` | Placeholder only. Needs binary discovery, prober, command builder, and child process lifecycle management. |
| **F-R4.2** | R4 | Media Asset Probing & Perception Cards | **Missing** | `crates/media/src/probe.rs` | FFprobe JSON extraction for fps, duration, resolution, audio/video streams, and perception heuristics (dominant palette, mean luma, entropy). |
| **F-R4.3** | R4 | Video & Image Frame Decoding Pipeline | **Missing** | `crates/media/src/decode.rs` | Frame decoding via `ffmpeg -ss ... -vframes 1 -f rawvideo -pix_fmt rgba -` into raw RGBA bytes. |
| **F-R4.4** | R4 | Half-Resolution Background Proxy Generation | **Missing** | `crates/media/src/proxy.rs` | Background transcode of heavy video clips (`scale=iw/2:ih/2`) with progress tracking and caching. |
| **F-R4.5** | R4 | Two-Tier Disk-Backed Playback Cache (Flat RAM) | **Missing** | `crates/media/src/cache.rs` | Implements `bonaparte_engine::reference::MediaFrames`. LRU RAM buffer (capped at 64MB) + disk frame store for flat RAM guarantee. |
| **F-R4.6** | R4 | Streaming MP4 Video Export | **Missing** | `crates/media/src/export.rs` | Direct frame-by-frame streaming from `bonaparte_engine::render_comp` into FFmpeg child process stdin (`O(1)` memory overhead). |
| **F-R4.7** | R4 | Automated Test Suite for Media Operations | **Missing** | `crates/media/tests/` | Zero tests exist in `bonaparte-media`. Needs probe, decode, cache, and export integration tests. |

---

## 2. Detailed Architectural Analysis: R3 (bonaparte-effects)

### 2.1 Microkernel Plugin Host Architecture
Bonaparte adheres to a strict **microkernel architecture** (RULES §4.1):
1. **Dogfooding Guarantee**: First-party effect packs are ordinary plugins shipping in the box. If an effect cannot be expressed through the public `manifest.toml` + WGSL contract, the plugin API is broken and must be fixed—zero private engine hooks or shortcuts allowed.
2. **Pack Directory Structure**: Every effect pack is an isolated directory containing:
   - `manifest.toml`: Metadata, declared GPU cost (`light`, `medium`, `heavy`), shader entrypoint, input ports, and typed parameter definitions.
   - `<effect>.wgsl`: Fragment shader source adhering to the uniform binding layout.
   - `README.md`: User-facing documentation table of parameters, defaults, and usage examples.
3. **Compile-Time & Dynamic Registry**:
   - First-party packs are embedded via `include_str!` so the editor and tests run without disk dependencies.
   - A runtime `EffectRegistry` allows scanning custom user/third-party plugin folders dynamically.

```
crates/effects/
├── Cargo.toml
├── src/
│   ├── lib.rs            # EffectRegistry, builtin pack exports, validation
│   ├── registry.rs       # Plugin loader, pack discovery, shader resolution
│   └── cpu_reference.rs  # Deterministic CPU reference evaluation for golden tests
└── packs/
    ├── glow/                 (builtin.glow)
    ├── blur/                 (builtin.blur)
    ├── drop_shadow/          (builtin.drop_shadow)
    ├── color_adjust/         (builtin.color_adjust)
    ├── transform/            (builtin.transform)
    ├── vignette/             (builtin.vignette)
    ├── chromatic_aberration/ (builtin.chromatic_aberration)
    ├── invert/               (builtin.invert)
    ├── tint/                 (builtin.tint)
    └── directional_blur/     (builtin.directional_blur)
```

### 2.2 Public WGSL Shader Contract
The public shader contract provides a standard binding table for all single-input and multi-input effects:
- `@group(0) @binding(0) var u_texture: texture_2d<f32>;` (Input texture from upstream render graph node)
- `@group(0) @binding(1) var u_sampler: sampler;` (Linear/clamp sampler)
- `@group(0) @binding(2) var<uniform> u_resolution: vec2<f32>;` (Canvas dimensions in pixels)
- `@group(0) @binding(3) var<uniform> u_time: f32;` (Composition timestamp in seconds)
- `@group(0) @binding(4) var<uniform> params: Params;` (Uniform struct matching `manifest.toml` parameters)

**WGSL std140 Alignment Rule**: Parameter uniform buffer structs must respect 16-byte alignment for `vec4<f32>` and 8-byte alignment for `vec2<f32>`.

### 2.3 Catalog of 10 Core First-Party Effect Packs

1. **`builtin.glow` (Glow)**:
   - **Cost**: `medium`
   - **Params**:
     - `radius` (`Slider`, min: 0.0, max: 100.0, default: 12.0, doc: "Blur radius of the glow halo, in pixels.")
     - `intensity` (`Slider`, min: 0.0, max: 4.0, default: 1.0, doc: "How bright the halo is relative to the source.")
     - `tint` (`Color`, default: [1.0, 1.0, 1.0, 1.0], doc: "Color multiplier applied to the halo.")
   - **WGSL**: Multi-sample neighborhood halo accumulator blended with source RGB.

2. **`builtin.blur` (Gaussian Blur)**:
   - **Cost**: `medium`
   - **Params**:
     - `radius` (`Slider`, min: 0.0, max: 100.0, default: 10.0, doc: "Gaussian blur radius in pixels.")
     - `direction` (`Dropdown`, options: ["Both", "Horizontal", "Vertical"], default: 0, doc: "Directional blur filtering axis.")
     - `repeat_edge_pixels` (`Checkbox`, default: true, doc: "Clamp texture sampling at layer boundaries to prevent edge darkening.")
   - **WGSL**: Weighted Gaussian kernel sampling with standard deviation $\sigma = \text{radius} / 2.0$.

3. **`builtin.drop_shadow` (Drop Shadow)**:
   - **Cost**: `medium`
   - **Params**:
     - `angle` (`Slider`, min: 0.0, max: 360.0, default: 135.0, doc: "Light source angle in degrees.")
     - `distance` (`Slider`, min: 0.0, max: 100.0, default: 10.0, doc: "Shadow displacement distance in pixels.")
     - `blur` (`Slider`, min: 0.0, max: 100.0, default: 8.0, doc: "Shadow feathering softness radius in pixels.")
     - `color` (`Color`, default: [0.0, 0.0, 0.0, 1.0], doc: "Shadow tint color.")
     - `opacity` (`Slider`, min: 0.0, max: 1.0, default: 0.75, doc: "Shadow alpha multiplier.")
   - **WGSL**: Displaced alpha-channel blur composited underneath source image.

4. **`builtin.color_adjust` (Color Adjust)**:
   - **Cost**: `light`
   - **Params**:
     - `brightness` (`Slider`, min: -1.0, max: 1.0, default: 0.0, doc: "Additive luminance shift.")
     - `contrast` (`Slider`, min: 0.0, max: 3.0, default: 1.0, doc: "Contrast multiplier around middle gray (0.5).")
     - `saturation` (`Slider`, min: 0.0, max: 3.0, default: 1.0, doc: "Color saturation multiplier relative to Rec.709 luma.")
     - `hue_shift` (`Slider`, min: -180.0, max: 180.0, default: 0.0, doc: "Hue angle rotation in degrees.")
     - `exposure` (`Slider`, min: -3.0, max: 3.0, default: 0.0, doc: "Photographic exposure factor ($2^{\text{exposure}}$).")
   - **WGSL**: Fast single-pass color grading matrix.

5. **`builtin.transform` (Transform / Distort)**:
   - **Cost**: `light`
   - **Params**:
     - `offset` (`Point`, default: [0.0, 0.0], doc: "Pixel translation offset relative to layer center.")
     - `scale` (`Point`, default: [100.0, 100.0], doc: "Percentage scale along horizontal and vertical axes.")
     - `rotation` (`Slider`, min: -360.0, max: 360.0, default: 0.0, doc: "Clockwise rotation angle in degrees.")
     - `anchor` (`Point`, default: [0.0, 0.0], doc: "Pivot point for rotation and scaling.")
   - **WGSL**: Inverse affine transformation matrix mapping output UVs to source texture coordinates.

6. **`builtin.vignette` (Vignette)**:
   - **Cost**: `light`
   - **Params**:
     - `radius` (`Slider`, min: 0.0, max: 1.0, default: 0.6, doc: "Normalized inner radius where falloff begins.")
     - `softness` (`Slider`, min: 0.0, max: 1.0, default: 0.45, doc: "Feathering smoothness width.")
     - `intensity` (`Slider`, min: 0.0, max: 2.0, default: 0.8, doc: "Vignette darkening amount.")
     - `center` (`Point`, default: [0.5, 0.5], doc: "Center focal point in normalized canvas UV coordinates.")
     - `color` (`Color`, default: [0.0, 0.0, 0.0, 1.0], doc: "Vignette border color.")
   - **WGSL**: Smoothstep radial falloff mask blended over source RGB.

7. **`builtin.chromatic_aberration` (Chromatic Aberration)**:
   - **Cost**: `light`
   - **Params**:
     - `offset` (`Slider`, min: 0.0, max: 50.0, default: 5.0, doc: "Channel separation distance in pixels.")
     - `angle` (`Slider`, min: 0.0, max: 360.0, default: 0.0, doc: "Separation direction angle in degrees.")
     - `mode` (`Dropdown`, options: ["Red-Cyan", "Blue-Yellow"], default: 0, doc: "Color channel split mode.")
   - **WGSL**: Independent directional UV displacement sampling for red and blue channels.

8. **`builtin.invert` (Invert)**:
   - **Cost**: `light`
   - **Params**:
     - `invert_rgb` (`Checkbox`, default: true, doc: "Invert red, green, and blue color channels ($1 - C$).")
     - `invert_alpha` (`Checkbox`, default: false, doc: "Invert transparency alpha channel ($1 - A$).")
     - `amount` (`Slider`, min: 0.0, max: 1.0, default: 1.0, doc: "Blend ratio between original and inverted result.")
   - **WGSL**: Channelwise inversion with linear interpolation.

9. **`builtin.tint` (Tint / Color Map)**:
   - **Cost**: `light`
   - **Params**:
     - `map_black_to` (`Color`, default: [0.0, 0.0, 0.0, 1.0], doc: "Target color mapped to dark luminance values.")
     - `map_white_to` (`Color`, default: [1.0, 1.0, 1.0, 1.0], doc: "Target color mapped to bright luminance values.")
     - `amount` (`Slider`, min: 0.0, max: 1.0, default: 1.0, doc: "Interpolation intensity between original and tinted image.")
   - **WGSL**: Luminance-driven linear color ramp interpolation.

10. **`builtin.directional_blur` (Directional Blur)**:
    - **Cost**: `medium`
    - **Params**:
      - `length` (`Slider`, min: 0.0, max: 100.0, default: 15.0, doc: "Motion streak length in pixels.")
      - `angle` (`Slider`, min: 0.0, max: 360.0, default: 0.0, doc: "Motion streak direction in degrees.")
    - **WGSL**: Linear multi-tap accumulator oriented along the given angle vector.

### 2.4 Pixel-Deterministic Golden-Frame Testing Strategy
To guarantee reproducible, pixel-perfect rendering across platforms without requiring dedicated GPU test runners in CI:
1. **Manifest Validation Test**: Validates every TOML manifest, verifies parameter types, ensures docstrings are non-empty, and checks slider bounds.
2. **WGSL Syntax Verification**: Ensures every `.wgsl` shader file parses without syntax errors.
3. **Deterministic CPU Reference Evaluator**: Implements mathematical software equivalents of all 10 effects in `cpu_reference.rs`.
4. **Golden Frame Fixtures**: Tests apply each effect with fixed parameters to synthetic test images (e.g. concentric circles, color ramps, checkerboards) and assert exact/tolerance-bounded pixel equality.

---

## 3. Detailed Architectural Analysis: R4 (bonaparte-media)

### 3.1 Crash-Isolated Architecture & Engine Seam
`bonaparte-media` is a **native-only** crate designed around three core architectural tenets:
1. **Crash Isolation via Child Processes**: Uses `std::process::Command` to spawn `ffmpeg` and `ffprobe` as isolated child processes. Corrupted media, malformed codecs, or memory leaks inside decoders terminate only the child process, never crashing the Bonaparte host process.
2. **LGPL Safety**: No C libraries from FFmpeg (`libavcodec`, `libavformat`) are linked into Bonaparte binaries.
3. **Engine Purity**: The render engine (`bonaparte-engine`) never performs disk I/O or spawns processes. `bonaparte-media` implements the `bonaparte_engine::reference::MediaFrames` trait:

```rust
pub trait MediaFrames {
    fn frame_rgba(&self, media: MediaId, time: Time) -> Option<FrameView<'_>>;
}
```

```
crates/media/
├── Cargo.toml
├── src/
│   ├── lib.rs        # Public API, MediaService, MediaFrames implementation
│   ├── probe.rs      # FFprobe execution, metadata parsing, perception cards
│   ├── decode.rs     # Frame extraction via FFmpeg rawvideo stdout pipes
│   ├── proxy.rs      # Background half-res proxy generation
│   ├── cache.rs      # Two-tier LRU RAM (capped) + disk-backed frame store
│   └── export.rs     # Streaming frame-by-frame MP4 video export into FFmpeg stdin
└── tests/
    └── media_pipeline_tests.rs
```

### 3.2 Media Asset Probing & Perception Cards (`probe.rs`)
- Executes `ffprobe -v quiet -print_format json -show_format -show_streams <path>`.
- Extracts:
  - Container duration $\to$ `Time(duration_secs * 120_000)`
  - Frame rate rational $\to$ `FrameRate { num, den }`
  - Dimensions $\to$ `width: u32, height: u32`
  - Codec information $\to$ `video_codec`, `audio_codec`, `pix_fmt`
- Computes `PerceptionCard` heuristics:
  - **Dominant Palette**: 5 dominant RGB colors via histogram bucketing.
  - **Mean Luminance**: Average perceptual brightness in $0.0..1.0$.
  - **Entropy**: Shannon entropy in $0.0..8.0$ bits (photographic complexity).
  - **Alpha Fraction**: Percentage of transparent pixels for PNG/WebP graphics.
  - **Role Classification**: `AssetRole::Video`, `AssetRole::Photo`, `AssetRole::Graphic`, `AssetRole::LogoCandidate`.

### 3.3 Frame Decoding Pipeline (`decode.rs`)
- **Single-Frame Random Access**:
  ```bash
  ffmpeg -ss <timestamp> -i <file_path> -vframes 1 -f rawvideo -pix_fmt rgba -
  ```
  Fast keyframe seek placement (`-ss` before `-i`) ensures sub-50ms seek latency. Reads exactly `width * height * 4` bytes from `stdout`.
- **Image Decoding**: Directly decodes still image assets (PNG, JPEG, WebP, SVG rasterization) into RGBA8 buffers.

### 3.4 Half-Resolution Background Proxy Generation (`proxy.rs`)
- To ensure fluid 60fps scrubbing on 8GB integrated-GPU laptops, video assets above 1080p automatically trigger background proxy creation:
  ```bash
  ffmpeg -y -i <input_path> -vf scale=iw/2:ih/2 -c:v libx264 -preset veryfast -crf 22 <proxy_path>
  ```
- The proxy file is stored in `%LOCALAPPDATA%/bonaparte/cache/proxies/` or system temp dir.
- The playback cache seamlessly swaps between proxy and full-resolution frames based on playback vs export mode.

### 3.5 Disk-Backed Playback Caching with Flat RAM Consumption (`cache.rs`)
To satisfy the strict 8GB RAM requirement:
1. **Tier 1 (RAM LRU Cache)**:
   - In-memory `LruCache<(MediaId, i64), Frame>` with a hard cap (e.g. 64 MB / 16 frames at 1080p).
   - Once the memory threshold is reached, least-recently-used frames are evicted from RAM.
2. **Tier 2 (Disk Cache Store)**:
   - Evicted frames or decoded frame ranges are written to disk as uncompressed raw binary files (`<cache_dir>/<media_id>_<tick>.raw`).
   - On cache miss in Tier 1, the frame is read back from Tier 2 disk cache via fast block I/O.
3. **Flat RAM Guarantee**: RAM consumption remains completely flat regardless of how long the user scrubs the timeline.

### 3.6 Streaming MP4 Video Export Pipeline (`export.rs`)
Export operates on a strict **single-frame-in-flight streaming model**:
1. Spawns an FFmpeg encoding process with stdin pipe:
   ```bash
   ffmpeg -y -f rawvideo -pix_fmt rgba -s {width}x{height} -r {fps.num}/{fps.den} \
          -i - -c:v libx264 -pix_fmt yuv420p -preset medium -crf 18 {output_path}
   ```
2. Iterates sequentially from frame index `0` to `total_frames - 1`:
   - Calculates integer tick: `time = Time(frame_idx * fps.ticks_per_frame())`.
   - Evaluates composition: `let frame = render_comp(project, comp_id, time, media_frames)?;`.
   - Streams raw bytes into stdin: `child_stdin.write_all(&frame.rgba)?;`.
   - Flushes stdin and invokes progress callback `on_progress(frame_idx + 1, total_frames)`.
3. Closes stdin (EOF) and waits for FFmpeg process completion.
4. **Memory Footprint**: Memory usage is $O(1)$ relative to video duration—a 2-hour 4K export uses the exact same memory footprint as a 1-second 720p export.

---

## 4. Identified Bugs, Compilation Blockers, and Gaps

### 4.1 Upstream Compilation Errors in `bonaparte-model`
`bonaparte-effects` and `bonaparte-media` cannot build until these four compilation errors in `crates/model` are resolved:
1. **Missing `Add` Implementation on `Time`**:
   - `crates/model/src/document.rs:328`: `time < self.start + self.duration` fails with `cannot add Time to Time`.
   - **Fix**: Implement `std::ops::Add<Time>`, `std::ops::Sub<Time>`, `std::ops::Mul<i64>`, `std::ops::Div<i64>` for `Time`.
2. **Missing `Display` Implementation on `Time`**:
   - `crates/model/src/ops.rs:110, 114`: `#[error("no keyframe at {1} on layer {0}")]` requires `Time: std::fmt::Display`.
   - **Fix**: Implement `std::fmt::Display` for `Time` (e.g. `write!(f, "{}t ({:.3}s)", self.0, self.as_secs_f64())`).
3. **Use of Moved Value `op` in `History::commit`**:
   - `crates/model/src/ops.rs:483, 497`: `op.apply(project)` consumes `op`, then `match op` accesses moved fields.
   - **Fix**: Clone `op` or match before consuming.
4. **Unused Import Warning**:
   - `crates/model/src/ops.rs:16`: `use crate::document::{Comp, ...}` warning.

### 4.2 Gaps in `bonaparte-effects`
1. **Missing Registry & Dynamic Loader**: No mechanism to query effect definitions by ID (`registry.get("builtin.glow")`), list effects for the UI, or scan external plugin directories.
2. **Missing 9 Core Effect Packs**: Only `glow` exists; Blur, Drop Shadow, Color Adjust, Transform, Vignette, Chromatic Aberration, Invert, Tint, and Directional Blur are absent.
3. **Glow Shader is Incomplete**: `glow.wgsl` is a 4-sample stub rather than a true bloom/glow shader.
4. **Missing Golden Frame Tests**: No pixel-deterministic test suite to verify effect outputs against reference fixtures.
5. **No Layer Effect Attachment in Model**: `Layer` in `document.rs` lacks an `effects: Vec<EffectInstance>` field, and `Op` lacks `AddEffect`, `RemoveEffect`, `SetEffectParam` variants.

### 4.3 Gaps in `bonaparte-media`
1. **100% Unimplemented Crate**: Currently contains only a placeholder module.
2. **Missing `MediaFrames` Integration**: The engine's `MediaFrames` trait has no native implementation backed by the decode cache.
3. **Missing CLI Subprocess Helpers**: No safe wrapper for spawning, timeout handling, and parsing `ffmpeg` / `ffprobe` outputs.
4. **Missing Export Pipeline**: No streaming MP4 export implementation.
5. **Missing Test Suite**: Zero tests for probing, decoding, caching, or exporting.

---

## 5. Test Coverage Analysis & Required Test Matrix

### 5.1 Current Test Coverage
- `bonaparte-effects`: 1 test (`every_builtin_is_a_valid_public_api_plugin` in `src/lib.rs`).
- `bonaparte-media`: 0 tests.

### 5.2 Required Test Matrix for R3 (bonaparte-effects)

| Test ID | Test Category | Target Component | Test Description | Expected Result |
|---|---|---|---|---|
| **T-R3.1** | Validation | Manifest Parser | Parse and validate all 10 built-in TOML manifests | All manifests pass `validate()` without error |
| **T-R3.2** | Documentation | Manifest Parameters | Assert every parameter across all 10 packs has a non-empty `doc` string | 100% parameter doc coverage |
| **T-R3.3** | Contract | Parameter Ranges | Verify slider defaults fall strictly within $[min, max]$ | Zero range violations |
| **T-R3.4** | Shaders | WGSL Parsing | Verify all 10 WGSL shader files parse cleanly | All shaders parse without syntax errors |
| **T-R3.5** | Golden Frame | Glow Effect | Apply `builtin.glow` to a high-contrast step function image | Matches CPU golden frame pixels |
| **T-R3.6** | Golden Frame | Blur Effect | Apply `builtin.blur` to a single-pixel impulse source | Verified Gaussian bell curve distribution |
| **T-R3.7** | Golden Frame | Color Adjust | Apply brightness (+0.2), contrast (1.5), saturation (0.0) | Verified luminance and grayscale conversion |
| **T-R3.8** | Golden Frame | Transform | Apply 90-degree rotation and $2\times$ scaling | Output pixels match analytical affine transform |
| **T-R3.9** | Golden Frame | Vignette | Apply vignette to pure white canvas | Output has radial darkening falloff to edges |
| **T-R3.10** | Golden Frame | Invert | Apply invert to RGBA test pattern | Inverted colors match $1.0 - C$ exactly |

### 5.3 Required Test Matrix for R4 (bonaparte-media)

| Test ID | Test Category | Target Component | Test Description | Expected Result |
|---|---|---|---|---|
| **T-R4.1** | Environment | Prober | Check `is_ffmpeg_available()` and `is_ffprobe_available()` | Returns true when binaries are present |
| **T-R4.2** | Probing | Metadata Parser | Probe synthetic test pattern video and image | Extracts correct dimensions, duration, and FPS |
| **T-R4.3** | Perception | Perception Card | Generate perception card for a known solid color image | Dominant palette and mean luma match expected values |
| **T-R4.4** | Decoding | Frame Decoder | Decode frame at timestamp $t=0.5\text{s}$ from test video | Returns RGBA8 frame of exact resolution |
| **T-R4.5** | Cache | LRU Eviction | Fill cache past RAM limit (e.g. 5 frames on a 3-frame limit) | Oldest frame evicted from RAM, retrievable from disk |
| **T-R4.6** | Cache | Flat RAM Guarantee | Scrub 100 sequential frames through cache | Process memory remains bounded under cap |
| **T-R4.7** | Export | Streaming MP4 Export | Export a 30-frame composition to temporary MP4 | Produces valid MP4 file verifiable with FFprobe |
| **T-R4.8** | Engine Seam | `MediaFrames` Trait | Pass disk cache into `bonaparte_engine::render_comp` | Renders footage layer using decoded pixels |

---

## 6. Recommended Milestones & Dependency Ordering

To achieve complete implementation with zero regressions and clean modular boundaries:

```
┌────────────────────────────────────────────────────────────────┐
│ Milestone 0: Upstream Model Fixes (Foundation)                 │
│ Fix Time arithmetic, Display trait, and Op::commit in model    │
└───────────────────────────────┬────────────────────────────────┘
                                │
        ┌───────────────────────┴───────────────────────┐
        ▼                                               ▼
┌────────────────────────────────┐   ┌───────────────────────────────────┐
│ Milestone 1: Effects Core (R3) │   │ Milestone 2: Media Pipeline (R4)  │
│ - Implement 10 first-party     │   │ - FFprobe metadata & perception   │
│   effect packs (manifest+WGSL) │   │ - Frame decode & proxy transcode  │
│ - EffectRegistry & loader      │   │ - Two-tier disk-backed cache      │
│ - CPU reference & golden tests │   │ - Streaming MP4 export pipeline   │
└───────────────────────┬────────┘   └───────────────────┬───────────────┘
                        │                                │
                        └───────────────┬────────────────┘
                                        ▼
┌────────────────────────────────────────────────────────────────┐
│ Milestone 3: Engine & UI Integration (R3/R4 Seam)              │
│ - Connect Layer effects to engine RenderGraph                  │
│ - Wire MediaFrames cache to AppState & Tauri IPC               │
│ - Run full workspace test suite (cargo test --workspace)       │
└────────────────────────────────────────────────────────────────┘
```

### Detailed Milestone Tasks

#### Milestone 0: Upstream Model Fixes (Foundation)
1. Add `std::ops::Add`, `Sub`, `Mul`, `Div` and `std::fmt::Display` to `Time` in `crates/model/src/time.rs`.
2. Fix `Op::commit` move error in `crates/model/src/ops.rs`.
3. Verify `cargo check -p bonaparte-model` passes cleanly.

#### Milestone 1: Microkernel Plugin Host & 10 First-Party Effect Packs (R3)
1. Create directory structure for all 10 packs in `crates/effects/packs/`:
   - `glow`, `blur`, `drop_shadow`, `color_adjust`, `transform`, `vignette`, `chromatic_aberration`, `invert`, `tint`, `directional_blur`.
2. Write `manifest.toml`, `<effect>.wgsl`, and `README.md` for each pack with complete docstrings.
3. Build `EffectRegistry` in `crates/effects/src/lib.rs` with `builtin_manifests()`, `get(id)`, `list()`, and `load_from_dir(path)`.
4. Implement `cpu_reference.rs` with mathematical evaluators for all 10 effects.
5. Implement unit tests and pixel-deterministic golden-frame tests in `crates/effects/tests/`.

#### Milestone 2: Crash-Isolated Media I/O & Streaming Export (R4)
1. Add `thiserror`, `serde`, `serde_json`, `tempfile` dependencies to `crates/media/Cargo.toml`.
2. Implement `probe.rs`: FFprobe execution, JSON deserializer, stream analysis, and `PerceptionCard` generator.
3. Implement `decode.rs`: Child-process FFmpeg frame decoder and image loader.
4. Implement `proxy.rs`: Background half-res transcode pipeline.
5. Implement `cache.rs`: Two-tier memory LRU + disk file store implementing `MediaFrames`.
6. Implement `export.rs`: Direct compositor-to-FFmpeg streaming MP4 video export.
7. Implement integration tests in `crates/media/tests/` with synthetic test patterns.

#### Milestone 3: Engine & App Integration
1. Wire `MediaCache` into `ui/src-tauri/src/main.rs` to serve footage frames to `render_comp`.
2. Wire export IPC command in Tauri app.
3. Run `cargo test --workspace` and verify 100% pass rate across all crates.
