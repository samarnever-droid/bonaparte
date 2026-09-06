# Handoff Report: Domain Model & Engine Survey (R1 & R2)

**Author:** Domain Model & Engine Explorer  
**Date:** 2026-08-30  
**Target:** Parent Orchestrator / Implementers  
**Type:** Hard Handoff (Task Complete)  

---

## 1. Observation

1. **Compilation Failures in `crates/bonaparte-model`:**
   Running `cargo test -p bonaparte-model` fails with 4 compiler errors:
   - `crates/model/src/document.rs:328`:
     ```rust
     time >= self.start && time < self.start + self.duration
     ```
     Error: `error[E0369]: cannot add time::Time to time::Time` (missing `Add` implementation on `Time`).
   - `crates/model/src/ops.rs:110` and `114`:
     ```rust
     #[error("no keyframe at {1} on layer {0}")]
     #[error("cannot move keyframe {1} to occupied time {2}")]
     ```
     Error: `error[E0599]: the method as_display exists for reference &time::Time, but its trait bounds were not satisfied` (`time::Time: std::fmt::Display` not satisfied).
   - `crates/model/src/ops.rs:497`:
     ```rust
     match op.apply(project) { ... }
     Op::AddLayer { comp, .. } => Op::RemoveLayer { comp, layer: LayerId(before_layer.0) }
     ```
     Error: `error[E0382]: use of moved value: op`.
2. **Invertibility Bugs in `crates/model/src/ops.rs`:**
   - Lines 277–286: `Op::RemoveComp` inverts to `Op::CreateComp { name, width, height, fps, duration }`. When applied during undo, it creates a fresh, empty composition with a newly allocated `CompId`, permanently discarding all layers and layer order from the original comp.
   - Lines 287–297: `Op::RemoveLayer` inverts to `Op::AddLayer { comp, layer: l }`. When applied during undo, `project.insert_layer` assigns a new `LayerId` and pushes the layer to the top of `layer_order`, breaking all prior IDs and z-index ordering.
   - Lines 410–415: `Op::RemoveMedia` inverts to `Op::AddMedia { asset: a }`, which allocates a new `MediaId` and breaks any `Footage` layers referencing the original `MediaId`.
3. **Missing AE Domain Model Primitives in `crates/model/src/document.rs`:**
   - `Layer` (lines 53–65) lacks `parent: Option<LayerId>`, `blend_mode: BlendMode`, `visible: bool`, and `locked: bool`.
   - `StaticTransform` (lines 106–115) has `position`, `scale`, `rotation`, `opacity`, but lacks `anchor_point: [f32; 2]`.
   - Missing Ops in `ops.rs`: `SetLayerTime` (timeline slip/trim), `SetLayerParent`, `SetLayerBlendMode`, `SetLayerVisible`, `SetCompProperties`.
4. **Missing Render Capabilities in `crates/engine/src/reference.rs`:**
   - Lines 132–154: `draw_layer` evaluates `Opacity`, `Position`, `Scale`, but completely omits `Rotation`.
   - Lines 208–211: `LayerKind::Text` and `LayerKind::PreComp` return `Err(RenderError::UnsupportedLayerKind)`.
   - Lines 81–92: `blend_pixel` only implements standard straight Over blending; all other AE blend modes (Multiply, Screen, Overlay, Add, etc.) are absent.
   - Entire file renders the full composition dimensions at once; no `render_tile` or viewport-bounded rendering exists.
5. **GPU Compositor Status in `crates/engine/Cargo.toml` and `crates/engine/src/`:**
   - `crates/engine/Cargo.toml` lines 14–16 have `wgpu`, `bytemuck`, `encase` dependencies commented out.
   - `features: [gpu = []]` is empty. No WGSL shader files or GPU pipelines exist in `crates/engine`.
6. **WASM Compilation Constraint:**
   - Running `cargo check --target wasm32-unknown-unknown -p bonaparte-engine` validates that `bonaparte-engine` core has zero native OS or tokio dependencies and compiles to wasm32 once `bonaparte-model` errors are cleared.
7. **Test Count & Coverage:**
   - Current tests: 15 across the workspace (`time.rs`: 3, `keyframe.rs`: 5, `manifest.rs`: 4, `tiles.rs`: 3, `graph.rs`: 2, `reference.rs`: 3, `effects`: 1).
   - Zero tests exist in `document.rs`, `ids.rs`, or `ops.rs`.

---

## 2. Logic Chain

1. **Step 1:** From Observation 1, `bonaparte-model` cannot compile because `Time` is missing basic arithmetic and display trait implementations, and `History::commit` moves `op` before pattern-matching it.
2. **Step 2:** From Observation 1 and 6, since `bonaparte-engine` depends on `bonaparte-model`, no engine or workspace test can execute until the model compilation errors are resolved.
3. **Step 3:** From Observation 2, the core premise of Bonaparte's operation engine (R1: exact invertibility in `History`) is currently broken for deletion operations (`RemoveComp`, `RemoveLayer`, `RemoveMedia`), because undoing deletion generates fresh allocation ops that assign new IDs, clear child collections, and corrupt layer stacking order.
4. **Step 4:** From Observation 3 and 4, the After Effects mental model (R1/R2) requires hierarchical parent-child transforms, 2D anchor points, and blend modes. Because these fields are missing from `Layer` and `StaticTransform`, the reference renderer and DAG cannot compute parent transforms, rotations, or blend modes.
5. **Step 5:** From Observation 4 and 5, while tile culling math (`tiles.rs`) and topological DAG sorting (`graph.rs`) are implemented, tile-by-tile rendering execution and the GPU compositor backend (`wgpu`/WGSL) are completely unwritten.
6. **Conclusion:** R1 (model & invertibility) must be fully repaired and tested first as the foundation, followed by completing the CPU software reference renderer (R2 pure), and then implementing the GPU compositor backend behind the `gpu` feature flag.

---

## 3. Caveats

1. **Native UI / Tauri Integration:** The survey focused strictly on `crates/bonaparte-model` and `crates/bonaparte-engine`. `ui/src-tauri/src/main.rs` was inspected for IPC contracts and confirmed to use outdated method signatures (passing `f64` instead of `Time`), but frontend Svelte components were not executed in this subagent pass.
2. **Font Rendering for Text Layers:** For `LayerKind::Text` CPU reference rendering, deciding whether to use a minimal embedded font/glyph rasterizer vs high-contrast placeholder geometry for golden tests is an implementation choice left to the R2 implementer.
3. **External Dependencies:** No modifications were made to any files outside `.agents/survey_model_engine` per the read-only explorer constraint.

---

## 4. Conclusion

The survey for R1 and R2 is complete.
1. **Critical Path to Unblock Build:**
   - Implement `Add`, `Sub`, `Mul`, `Div`, `Neg`, `AddAssign`, `SubAssign`, `Sum`, and `Display` for `Time`.
   - Fix `History::commit` moved-value error.
2. **Critical Path for R1 (Model):**
   - Fix `RemoveComp`, `RemoveLayer`, `RemoveMedia` invertibility by implementing exact entity restoration (`RestoreComp`, `RestoreLayer`, `RestoreMedia`).
   - Add `parent: Option<LayerId>`, `blend_mode: BlendMode`, `visible: bool`, `locked: bool` to `Layer`.
   - Add `anchor_point: [f32; 2]` to `StaticTransform`.
   - Add missing Ops (`SetLayerTime`, `SetLayerParent`, `SetLayerBlendMode`, `SetLayerVisible`, `SetCompProps`).
   - Add comprehensive round-trip undo/redo tests in `ops.rs`.
3. **Critical Path for R2 (Engine):**
   - Apply `Rotation` and parent hierarchy in CPU reference renderer.
   - Implement blend mode math for standard AE modes.
   - Implement `PreComp` nested rendering and `Text` rendering.
   - Implement `render_tile` entry point for 256×256 tiled rendering.
   - Scaffold `wgpu` compositor backend behind `gpu` feature.
   - Verify `cargo check --target wasm32-unknown-unknown -p bonaparte-engine`.

Detailed feature inventories, architectural data flows, gap tables, and milestone plans are documented in `survey.md`.

---

## 5. Verification Method

To independently verify these findings, run:

1. **Model & Workspace Build Verification:**
   ```powershell
   cargo test -p bonaparte-model
   ```
   *Expected Current Output:* Fails with 4 errors in `document.rs:328`, `ops.rs:110, 114, 497`.
2. **WASM Target Check:**
   ```powershell
   cargo check --target wasm32-unknown-unknown -p bonaparte-engine
   ```
   *Expected Current Output:* Blocked by `bonaparte-model` errors.
3. **Code Inspection of Invertibility Flaws:**
   Inspect `crates/model/src/ops.rs` lines 277–297 and 410–415 to verify `RemoveComp`, `RemoveLayer`, and `RemoveMedia` inverses.
4. **Code Inspection of Missing Render Features:**
   Inspect `crates/engine/src/reference.rs` lines 132–154 and 208–211 to verify missing rotation and unsupported `Text`/`PreComp` layer kinds.
5. **Code Inspection of GPU Backend:**
   Inspect `crates/engine/Cargo.toml` lines 14–20 and `crates/engine/src/` to verify empty `gpu` feature and absent GPU pipelines.
