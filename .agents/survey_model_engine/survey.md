# Comprehensive Survey Report: Domain Model (R1) & Render Engine (R2)

**Target Crate 1 (R1):** `crates/bonaparte-model`  
**Target Crate 2 (R2):** `crates/bonaparte-engine`  
**Date:** 2026-08-30  
**Author:** Domain Model & Engine Explorer  
**Status:** In-Depth Survey Complete  

---

## 1. Executive Summary

A comprehensive investigation of `crates/bonaparte-model` and `crates/bonaparte-engine` was performed.

### Key Takeaways:
1. **Compilation State:** `bonaparte-model` currently fails compilation with **4 errors** across `document.rs` and `ops.rs` due to:
   - Missing `std::ops::Add` on `time::Time` (`document.rs:328`).
   - Missing `std::fmt::Display` on `time::Time` (`ops.rs:110, 114`).
   - Use of moved value `op` in `History::commit` (`ops.rs:483, 497`).
   - Because `bonaparte-engine`, `bonaparte-effects`, and `bonaparte-app` depend on `bonaparte-model`, the entire workspace fails to build and test.
2. **R1 (Domain Model & Invertibility) Status:**
   - Integer tick `Time(i64)` (120k ticks/sec) and rational `FrameRate` primitives exist, but lack essential arithmetic traits (`Add`, `Sub`, `Mul`, `Div`, `Neg`, `AddAssign`, `SubAssign`), `Display` formatting, frame number conversion (`to_frame`, `from_frame`), and SMPTE timecode formatting.
   - Core `Op` enum has 13 variants implemented. However, **critical invertibility bugs exist in `History::invert` / `History::commit`**:
     - `RemoveComp` inverse generates `CreateComp`, which wipes all layers and issues a new `CompId` on undo.
     - `RemoveLayer` inverse generates `AddLayer`, which allocates a new `LayerId` and places the layer at the top of the stack instead of preserving its original `LayerId` and stack position.
     - `RemoveMedia` inverse generates `AddMedia`, which allocates a new `MediaId` and invalidates any `Footage` layers referencing the old asset.
   - **Missing Domain Model Features:**
     - Layer hierarchy / parenting (`parent: Option<LayerId>` on `Layer`, transform inheritance, cycle guard).
     - Layer blend modes (`blend_mode: BlendMode`).
     - Layer flags (`visible: bool`, `locked: bool`).
     - Anchor point (`anchor_point: [f32; 2]` in `StaticTransform`).
     - Missing `Op` variants: `SetLayerTime` (timeline move/trim), `SetLayerParent`, `SetLayerBlendMode`, `SetLayerVisible`, `SetLayerLocked`, `SetCompProperties`, and effect parameter mutations.
   - **Zero tests exist in `ops.rs` and `document.rs`**.
3. **R2 (Tile Engine & Compositor) Status:**
   - `tiles.rs`: 256×256 `TileGrid` viewport tile calculation is implemented and verified with 3 passing tests. Gaps: per-layer tile bounding-box intersection and tile-level render execution function.
   - `graph.rs`: Pure dependency-sorted `RenderGraph` using Kahn's algorithm with deterministic tie-breaking and cycle detection is implemented with 2 passing tests. Gap: automated `build_render_graph(project, comp, time)` builder from document state.
   - `reference.rs`: CPU software reference renderer supports `Solid`, `Shape` (rect), and `Footage` (via pure `MediaFrames` injection trait) with linear/sRGB conversion and Over alpha blending.
     - **Gaps/Bugs:** Ignores `Rotation` in layer transforms; fails on `Text` and `PreComp` layers (`UnsupportedLayerKind`); lacks all non-Over blend modes (Multiply, Screen, Overlay, Add, etc.); renders full comp only, lacking tile-by-tile render entry point.
   - **GPU Backend:** `wgpu`, `bytemuck`, `encase` are commented out in `Cargo.toml`. `gpu = []` feature is empty; no WGSL compositor shaders or GPU tile renderer pipeline are currently implemented.
   - **WASM Purity:** `bonaparte-engine` core has **zero** OS, thread, or filesystem dependencies. `cargo check --target wasm32-unknown-unknown -p bonaparte-engine` will pass cleanly once `bonaparte-model` errors are fixed.

---

## 2. Requirement R1: Robust Domain Model & Invertible Operation Engine

### 2.1 Feature Inventory (R1)

| Feature | Submodule | Status | File Path | Description & Notes |
| :--- | :--- | :--- | :--- | :--- |
| **Integer Tick Time (`Time(i64)`)** | `time.rs` | **Partial / Broken** | `crates/model/src/time.rs` | 120,000 ticks/sec. Missing `Add`, `Sub`, `Mul`, `Div`, `Neg`, `AddAssign`, `SubAssign`, `Display`. |
| **Rational FrameRate** | `time.rs` | **Partial** | `crates/model/src/time.rs` | `FPS_24`, `25`, `30`, `60`, `NTSC_FILM`. `ticks_per_frame()`, `as_f64()`. Missing `Display`, `from_frame`, `to_frame`. |
| **Strongly Typed IDs** | `ids.rs` | **Implemented** | `crates/model/src/ids.rs` | `CompId`, `LayerId`, `MediaId`. Monotonic IDs with transparent serde and `Display`. |
| **Bézier Keyframe Tracks** | `keyframe.rs` | **Implemented** | `crates/model/src/keyframe.rs` | `PropValue` (Scalar, Vec2), `Easing::Bezier` (32-iter bisection solver), `Track::set_key`, `evaluate`. |
| **Project & Comp Model** | `document.rs` | **Partial** | `crates/model/src/document.rs` | `Project`, `Comp`, `Layer`, `StaticTransform`, `LayerKind`, `MediaAsset`. |
| **Layer Hierarchy (Parenting)** | `document.rs` | **Missing** | `crates/model/src/document.rs` | `Layer` lacks `parent: Option<LayerId>` and transform inheritance calculation. |
| **Layer Blend Modes** | `document.rs` | **Missing** | `crates/model/src/document.rs` | `Layer` lacks `blend_mode: BlendMode` (Normal, Multiply, Screen, Add, Overlay, etc.). |
| **Layer Flags & Anchor Point** | `document.rs` | **Missing** | `crates/model/src/document.rs` | Missing `visible: bool`, `locked: bool`, `anchor_point: [f32; 2]`. |
| **Asset Perception & Slots** | `document.rs` | **Implemented** | `crates/model/src/document.rs` | `SlotDef`, `PerceptionCard`, `AssetRole`, `MediaKind`. |
| **Plugin Manifest Schema** | `manifest.rs` | **Implemented** | `crates/model/src/manifest.rs` | `EffectManifest`, `ParamDef`, `ParamKind`, `GpuCost`, validation with `SUPPORTED_API_MAJOR = 1`. |
| **Op Mutation Enum** | `ops.rs` | **Partial** | `crates/model/src/ops.rs` | 13 variants implemented. Missing `SetLayerTime`, `SetLayerParent`, `SetLayerBlendMode`, `SetCompProps`. |
| **History & Undo/Redo Engine** | `ops.rs` | **Broken** | `crates/model/src/ops.rs` | Stored inverse architecture. Broken moved-value bug; flawed inverse logic on `RemoveComp`, `RemoveLayer`, `RemoveMedia`. |
| **Serialization / Deserialization** | Multiple | **Implemented** | All model modules | Full Serde JSON compatibility. |

---

### 2.2 Architectural Analysis & Data Flows (R1)

```
                       ┌────────────────────────────┐
                       │        Project Root        │
                       │ ├─ next_comp, next_layer...│
                       │ ├─ media: Map<MediaId, ...>│
                       │ └─ comps: Map<CompId, ...> │
                       └─────────────┬──────────────┘
                                     │
                    ┌────────────────┴────────────────┐
                    │       Comp (Composition)        │
                    │ ├─ width, height, fps, duration │
                    │ ├─ layer_order: Vec<LayerId>    │
                    │ └─ layers: Map<LayerId, Layer>  │
                    └────────────────┬────────────────┘
                                     │
           ┌─────────────────────────┴─────────────────────────┐
           │                      Layer                        │
           │ ├─ id: LayerId, name: String                      │
           │ ├─ start: Time, duration: Time                    │
           │ ├─ kind: Solid | Shape | Text | Footage | PreComp │
           │ ├─ transform: StaticTransform                     │
           │ └─ tracks: Map<Property, Track>                   │
           └───────────────────────────────────────────────────┘
```

#### Mutation Flow:
1. Caller creates an `Op` (e.g. `Op::SetValue { ... }` or `Op::AddKeyframe { ... }`).
2. Passed to `History::commit(&mut self, project: &mut Project, op: Op)`:
   - For non-allocating ops: `inverse = op.invert(project)?; op.apply(project)?; undo_stack.push(inverse)`.
   - For allocating ops (`CreateComp`, `AddLayer`, `AddMedia`): snapshots ID allocators, applies `op`, patches inverse with the allocated ID, pushes inverse to `undo_stack`.
3. Inverse Application (`History::undo`):
   - Pops inverse from `undo_stack`.
   - `redo = inverse.invert(project)?`.
   - `inverse.apply(project)?`.
   - `redo_stack.push(redo)`.

---

### 2.3 Identified Bugs & Deficiencies (R1)

#### Bug 1: Missing Trait Implementations on `Time` (`time.rs`)
- **Observation:** `crates/model/src/document.rs:328` (`self.start + self.duration`) and `crates/model/src/ops.rs:110, 114` fail to compile.
- **Cause:** `Time` does not implement `std::ops::Add`, `Sub`, `Mul`, `Div`, `Neg`, `AddAssign`, `SubAssign`, or `std::fmt::Display`.
- **Fix Required:**
  ```rust
  impl std::ops::Add for Time {
      type Output = Self;
      fn add(self, rhs: Self) -> Self { Self(self.0 + rhs.0) }
  }
  impl std::ops::Sub for Time {
      type Output = Self;
      fn sub(self, rhs: Self) -> Self { Self(self.0 - rhs.0) }
  }
  // Implement Mul<i64>, Div<i64>, Neg, AddAssign, SubAssign, Sum
  impl std::fmt::Display for Time {
      fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
          write!(f, "{}t ({:.3}s)", self.0, self.as_secs_f64())
      }
  }
  ```

#### Bug 2: Move Error in `History::commit` (`ops.rs:483, 497`)
- **Observation:** `op.apply(project)` consumes `op` by value, then `match op` matches `op.clone()` or fields on the moved value.
- **Cause:** `Op` does not implement `Copy`.
- **Fix Required:** Clone or match before consuming, or pass `&op` to compute inverse type.

#### Bug 3: Data Loss & Inconsistent IDs in `RemoveComp`, `RemoveLayer`, `RemoveMedia` Inverses
- **Observation:**
  - In `Op::RemoveComp { comp }`: `invert` returns `Op::CreateComp { name, width, height, fps, duration }`. When undone, this creates an **empty** comp with a **new** ID, permanently discarding all layers and layer ordering!
  - In `Op::RemoveLayer { comp, layer }`: `invert` returns `Op::AddLayer { comp, layer: l }`. When undone, `insert_layer` assigns a **new** `LayerId` and places it at the **top** of `layer_order`, breaking any downstream references to the old `LayerId` and corrupting z-index ordering!
  - In `Op::RemoveMedia { media }`: `invert` returns `Op::AddMedia { asset: a }`, allocating a new `MediaId` and breaking `Footage` layers referencing the old asset.
- **Fix Required:**
  - Introduce explicit restore ops or exact restore methods:
    - `Op::RestoreComp { comp: Comp }` (restores comp with exact ID, layers, and order).
    - `Op::RestoreLayer { comp: CompId, layer: Layer, index: usize }` (restores layer with its exact `LayerId` at its exact position in `layer_order`).
    - `Op::RestoreMedia { asset: MediaAsset }` (restores asset with its exact `MediaId`).

#### Gap 4: Missing Op Variants
The current 13 `Op` variants cannot perform basic timeline interactions:
- `SetLayerTime { comp: CompId, layer: LayerId, start: Time, duration: Time }`: Moving/trimming layers on the timeline.
- `SetCompProps { comp: CompId, name: String, width: u32, height: u32, fps: FrameRate, duration: Time, background: [f32; 4] }`.
- `SetLayerParent { comp: CompId, layer: LayerId, parent: Option<LayerId> }`.
- `SetLayerBlendMode { comp: CompId, layer: LayerId, blend_mode: BlendMode }`.
- `SetLayerVisible { comp: CompId, layer: LayerId, visible: bool }`.
- `SetLayerLocked { comp: CompId, layer: LayerId, locked: bool }`.

---

## 3. Requirement R2: Pure Tile-Based Render Engine & Graph Compositor

### 3.1 Feature Inventory (R2)

| Feature | Submodule | Status | File Path | Description & Notes |
| :--- | :--- | :--- | :--- | :--- |
| **256×256 Tile Grid Scheduler** | `tiles.rs` | **Implemented** | `crates/engine/src/tiles.rs` | `Tile`, `TileGrid`, `TILE_SIZE = 256`, `tiles_for_viewport`. |
| **Tile Bounding Box Intersection** | `tiles.rs` | **Missing** | `crates/engine/src/tiles.rs` | Tile pixel bounds and layer-tile intersection culling. |
| **Topological Render Graph** | `graph.rs` | **Implemented** | `crates/engine/src/graph.rs` | `RenderGraph`, `NodeId`, `NodeKind`, `topological_order` (Kahn's algorithm + tie-breaking). |
| **Graph Cycle Detection** | `graph.rs` | **Implemented** | `crates/engine/src/graph.rs` | Returns `GraphError::Cycle` on recursive loops. |
| **Comp-to-Graph Builder** | `graph.rs` | **Missing** | `crates/engine/src/graph.rs` | Automatic DAG generation from `Comp` layers, transforms, and effects at `Time`. |
| **Pure Media Injection Seam** | `reference.rs` | **Implemented** | `crates/engine/src/reference.rs` | `MediaFrames` trait, `FrameView`, `NoMedia` (zero filesystem I/O in engine). |
| **Solid & Shape Layer Rasterizer** | `reference.rs` | **Partial** | `crates/engine/src/reference.rs` | Renders Solid and Shape rect. Missing Rotation transform and vector shape geometry. |
| **Footage Layer Rasterizer** | `reference.rs` | **Implemented** | `crates/engine/src/reference.rs` | Nearest-neighbor sampled frame drawing into destination rect. |
| **Text Layer Rasterizer** | `reference.rs` | **Missing** | `crates/engine/src/reference.rs` | Currently returns `RenderError::UnsupportedLayerKind`. |
| **PreComp Nested Layer Rasterizer** | `reference.rs` | **Missing** | `crates/engine/src/reference.rs` | Currently returns `RenderError::UnsupportedLayerKind`. |
| **Alpha Blend Modes** | `reference.rs` | **Partial** | `crates/engine/src/reference.rs` | Only standard straight "Over" blend implemented. Missing Multiply, Screen, Overlay, Add, etc. |
| **Tile-Based Render Entrypoint** | `reference.rs` | **Missing** | `crates/engine/src/reference.rs` | `render_comp` renders whole frame; lacks `render_tile` / `render_viewport`. |
| **GPU Compositor Backend (wgpu)** | `Cargo.toml` / `gpu` | **Missing** | `crates/engine/src/gpu/` | `wgpu` deps commented out; no WGSL shaders or GPU pipeline implemented. |
| **WASM Purity Compliance** | Engine Crate | **Compliant** | All engine source files | Pure synchronous code, zero `std::fs`, `tokio`, or threads. |

---

### 3.2 Architectural Analysis & Render Pipeline (R2)

```
                       ┌───────────────────────────────┐
                       │  Project Document & Comp (R1) │
                       └───────────────┬───────────────┘
                                       │
                       ┌───────────────┴───────────────┐
                       │ 256×256 Tile Grid Scheduler   │
                       │ - Divides comp into tiles     │
                       │ - Culls non-viewport tiles    │
                       └───────────────┬───────────────┘
                                       │
                       ┌───────────────┴───────────────┐
                       │   Render Graph Construction   │
                       │ - Layer sources (Solid/Shape/ │
                       │   Footage/Text/PreComp)       │
                       │ - Effect nodes (manifest IDs) │
                       │ - Topological sort (Kahn)     │
                       └───────────────┬───────────────┘
                                       │
                ┌──────────────────────┴──────────────────────┐
                ▼                                             ▼
  ┌───────────────────────────┐                 ┌───────────────────────────┐
  │   CPU Reference Renderer  │                 │    GPU Compositor (wgpu)  │
  │ - Frame / Tile buffer     │                 │ - 256×256 Tile Textures   │
  │ - Linear / sRGB gamma 2.2 │                 │ - WGSL Compositing Shader │
  │ - Alpha blend modes       │                 │ - Plugin Effect Shaders   │
  │ - Injected MediaFrames    │                 │ - Zero OS/Thread deps     │
  └───────────────────────────┘                 └───────────────────────────┘
```

---

### 3.3 Identified Bugs & Deficiencies (R2)

1. **Rotation Ignored in CPU Reference Renderer (`reference.rs:132-154`)**:
   `layer.evaluate(Property::Rotation, time)` is never called or applied. Layers with rotation keyframes render unrotated in CPU software reference mode.
2. **Text and PreComp Layers Cause Render Failure (`reference.rs:208-211`)**:
   Encountering a `Text` or `PreComp` layer immediately returns `Err(RenderError::UnsupportedLayerKind)`.
   - `Text` should draw text glyphs or fallback high-contrast vector rasterization.
   - `PreComp` should recursively evaluate the referenced `Comp` at `time - layer.start` with a recursion/cycle guard.
3. **Blend Modes Incomplete (`reference.rs:81-92`)**:
   `blend_pixel` only computes `src * src_a + dst * (1 - src_a)`. Standard 2D blend modes (Multiply, Screen, Overlay, Add/Linear Dodge, Darken, Lighten, Difference) must be supported.
4. **Tile-Level Evaluation Missing**:
   `render_comp` renders `0..frame.height` and `0..frame.width` in one allocation. To satisfy O(viewport) memory constraints on 8GB machines, the engine needs `render_tile(project, comp_id, time, tile: Tile, frames)` or `render_viewport_tiles`.
5. **GPU Backend Not Scaffolded**:
   `Cargo.toml` has `wgpu` commented out, and `src/` contains zero GPU execution code.

---

## 4. Test Coverage Analysis

### Current Test Suite (15 Tests Total across Workspace)
- `crates/model/src/time.rs` (3 tests):
  - `common_rates_have_integer_ticks_per_frame`
  - `ntsc_film_frame_is_exactly_5005_ticks`
  - `roundtrip_secs`
- `crates/model/src/keyframe.rs` (5 tests):
  - `linear_interpolation`
  - `keys_stay_sorted_after_out_of_order_inserts`
  - `default_easing_is_symmetric_ease_in_out`
  - `bezier_easing_matches_known_css_curve`
  - `vec2_lerp`
- `crates/model/src/manifest.rs` (4 tests):
  - `parses_valid_manifest`
  - `refuses_wrong_api_major`
  - `refuses_duplicate_param_ids`
  - `refuses_inverted_slider_range`
- `crates/model/src/document.rs`: **0 tests**
- `crates/model/src/ids.rs`: **0 tests**
- `crates/model/src/ops.rs`: **0 tests**
- `crates/engine/src/tiles.rs` (3 tests):
  - `full_viewport_on_1080p_costs_32_tiles_not_a_4k_buffer`
  - `zoomed_viewport_culls_to_intersecting_tiles_only`
  - `viewports_outside_bounds_clamp_without_panicking`
- `crates/engine/src/graph.rs` (2 tests):
  - `chain_orders_inputs_before_consumers`
  - `cycle_is_detected_not_hung`
- `crates/engine/src/reference.rs` (3 tests):
  - `renders_background_and_layering_is_bottom_to_top`
  - `opacity_track_wins_over_static`
  - `footage_layer_draws_injected_pixels`
- `crates/effects/src/lib.rs` (1 test):
  - `every_builtin_is_a_valid_public_api_plugin`

### Required Tests to Add:
1. **Model Arithmetic & Timecode Tests:**
   - Time addition, subtraction, negation, scaling, division, and assignment operators.
   - Timecode formatting (`HH:MM:SS:FF`) at 24fps, 23.976fps, 30fps, 60fps.
   - Frame rate conversions and display traits.
2. **Round-Trip Undo/Redo Invariant Tests for EVERY Op Variant:**
   - Property test: For initial project state $P_0$ and op $O$, executing $P_1 = O.\text{apply}(P_0)$, then $O^{-1}.\text{apply}(P_1)$ must result in $P_2 \equiv P_0$.
   - Test undo/redo sequence for `CreateComp`, `RemoveComp`, `AddLayer`, `RemoveLayer`, `RenameLayer`, `SetValue`, `AddKeyframe`, `RemoveKeyframe`, `MoveKeyframe`, `SetEasing`, `ReorderLayer`, `AddMedia`, `RemoveMedia`, `SetLayerTime`.
3. **Layer Hierarchy & Parent Transform Tests:**
   - Child layer inheriting parent translation, rotation, and scale.
   - Deep hierarchy evaluation ($A \to B \to C$).
   - Reparenting and unparenting undo/redo.
   - Parenting cycle prevention.
4. **Reference Renderer Layer & Blend Mode Tests:**
   - Rotation transform accuracy.
   - PreComp rendering with time-offset.
   - Blend mode truth tables (Multiply, Screen, Add, Overlay vs reference math).
   - Tile rendering vs full-frame rendering equivalence.
5. **WASM Purity Automated Test:**
   - `cargo check --target wasm32-unknown-unknown -p bonaparte-engine`.

---

## 5. Recommended Implementation Milestones & Dependency Order

```
┌────────────────────────────────────────────────────────┐
│ Milestone 1: Fix Model Core (R1 Foundation)            │
│ 1. Implement Time arithmetic, Display, and timecode    │
│ 2. Fix History::commit and invertibility bugs          │
│ 3. Add Layer hierarchy, blend modes, anchor point      │
│ 4. Add missing Ops and comprehensive round-trip tests  │
└───────────────────────────┬────────────────────────────┘
                            │
┌───────────────────────────┴────────────────────────────┐
│ Milestone 2: Complete CPU Reference Renderer (R2 Pure) │
│ 1. Add Rotation & Anchor Point transform math          │
│ 2. Implement BlendMode math (Multiply, Screen, Add...) │
│ 3. Implement PreComp nested rendering + Text fallback  │
│ 4. Add 256×256 tile render execution & layer culling   │
│ 5. Verify cargo check wasm32-unknown-unknown           │
└───────────────────────────┬────────────────────────────┘
                            │
┌───────────────────────────┴────────────────────────────┐
│ Milestone 3: GPU Compositor Backend (R2 GPU)           │
│ 1. Enable wgpu / bytemuck behind `gpu` feature         │
│ 2. Implement WGSL composite & blend shaders            │
│ 3. Implement Tile GPU cache and readback pipeline      │
│ 4. Add golden-frame CPU vs GPU pixel parity tests      │
└────────────────────────────────────────────────────────┘
```
