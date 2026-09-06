# Handoff Report: Survey of Requirement R5 (UI Shell & Headless MCP Server)

**Agent Role**: UI Shell & MCP Explorer  
**Working Directory**: `C:\Users\khati\.zcode\workspace\default\bonaparte\.agents\survey_ui_mcp`  
**Report Artifact**: `C:\Users\khati\.zcode\workspace\default\bonaparte\.agents\survey_ui_mcp\survey.md`

---

## 1. Observation

1. **Compilation & Build Checks**:
   - Running `cargo check --workspace` fails in `bonaparte-model` and `bonaparte-app`:
     ```
     error[E0369]: cannot add `time::Time` to `time::Time`
        --> crates\model\src\document.rs:328:49
     error[E0599]: the method `as_display` exists for reference `&time::Time`, but its trait bounds were not satisfied
        --> crates\model\src\ops.rs:110:13
     error[E0382]: use of moved value: `op`
        --> crates\model\src\ops.rs:497:28
     ```
   - In `ui/src-tauri/src/main.rs:79-87`, `render_frame` calls:
     ```rust
     let frame = render_comp(&project, CompId(comp_id), time).map_err(|e| e.to_string())?;
     ```
     Whereas `crates/engine/src/reference.rs:105-110` defines:
     ```rust
     pub fn render_comp(
         project: &Project,
         comp_id: CompId,
         time: Time,
         frames: &dyn MediaFrames,
     ) -> Result<Frame, RenderError>
     ```
     This results in type mismatch (`f64` vs `Time`) and missing argument (`frames`).
   - Running `npm run build` in `ui/` succeeds but outputs compiler warnings:
     ```
     [vite-plugin-svelte] src/lib/components/Viewport.svelte:25:6 `dragging` is updated, but is not declared with `$state(...)`. Changing its value will not correctly trigger updates
     [vite-plugin-svelte] src/lib/components/Timeline.svelte:63:6 `<div>` with a pointerdown, pointermove or pointerup handler must have an ARIA role
     ```

2. **UI Component & State Inspection**:
   - **Canvas Drag vs Keyframe Track Conflict** (`ui/src/lib/components/Viewport.svelte:58-66`):
     ```ts
     await applyOp({
       type: "setValue",
       comp: comp.id,
       layer: editor.selected,
       property: "Position",
       value: { Vec2: [basePos[0] + (p.x - origin.x), basePos[1] + (p.y - origin.y)] },
     });
     ```
     If a layer has keyframes on Position, `bonaparte-model::Layer::evaluate` prioritizes `tracks` over `transform`, so any canvas movement applied via `setValue` is immediately overridden by the animated track upon evaluation.
   - **Keyframe Diamond Toggle Flaw** (`ui/src/lib/components/Properties.svelte:51-60` and `ui/src/lib/store.svelte.ts:90-105`):
     `keyframeAtPlayhead` only calls `applyOp({ type: "addKeyframe", ... })`. It never removes keyframes when the playhead is over an existing keyframe. Furthermore, `hasTrack(p.key)` lights up the diamond whenever *any* keyframe exists on the track, rather than when the playhead is on a keyframe.
   - **Missing Curve Graph Editor & Animation Preset Browser**:
     `Timeline.svelte` contains only basic row tracks and diamond icons; no in-place curve editor component exists. No animation preset browser exists in the project.
   - **Missing Shortcuts** (`ui/src/App.svelte:11-18`):
     `window.addEventListener("keydown")` only checks `e.ctrlKey && e.key.toLowerCase() === "z"`. `Space` (play/pause), `K` (keyframe), `Delete` (remove layer/keyframe), and arrow keys are not bound.
   - **Playback Timing** (`ui/src/lib/store.svelte.ts:63-72`):
     `play()` uses `setInterval` with integer step milliseconds, causing timing drift and uncoordinated async IPC calls to `renderTo`.

3. **Headless MCP Server Inspection**:
   - `crates/mcp/Cargo.toml` contains only dependencies on `bonaparte-model`, `bonaparte-engine`, and `thiserror`.
   - `crates/mcp/src/lib.rs` is an empty stub:
     ```rust
     pub mod placeholder {
         pub const NOT_YET_IMPLEMENTED: &str = "MCP server lands with the vertical slice";
     }
     ```
     No MCP JSON-RPC protocol, tool registry, `op.apply`, `ops.propose`, `history.*`, or `comp.render` tools are implemented.

4. **Testing Infrastructure**:
   - `ui`: No test configuration or test files exist in `ui/`.
   - `crates/mcp`: No integration tests exist in `crates/mcp/`.

---

## 2. Logic Chain

1. **Premise**: Requirement R5 demands a zero-bug interactive Svelte 5 UI shell and headless MCP server exposing the same `Op` vocabulary.
2. **Observed Breakages**:
   - `bonaparte-app` fails to build because `render_frame` calls `render_comp` with obsolete types (Observation 1).
   - Moving an animated object in the canvas triggers a static `setValue` that is ignored by `evaluate()`, creating a severe UX bug where objects snap back (Observation 2).
   - The keyframe diamond toggle in the properties panel cannot delete keyframes and gives false active status (Observation 2).
   - Essential features explicitly called out in R5 (in-place Bézier curve editor, animation preset browser, Space to play/pause, MCP server tools) are completely missing (Observations 2 & 3).
3. **Deduction**: The codebase currently has a skeletal shell, but requires substantial fixes to IPC signatures, store reactivity, gesture-to-op routing, and a complete implementation of the Bézier curve editor and `bonaparte-mcp` crate.

---

## 3. Caveats

- **WASM Renderer Target**: The current shell relies on Tauri IPC frame rendering via CPU reference renderer. The long-term plan notes a WASM in-webview renderer. For Ring 1, the IPC reference renderer is the active path, making binary buffer optimization crucial.
- **Model Dependencies**: Fixes to `ui/src-tauri` depend on `bonaparte-model` arithmetic trait implementations (`Add`, `Display` on `Time`).

---

## 4. Conclusion

Requirement R5 is in a **Partial / Early Skeleton** state:
- **UI Shell**: Needs reactive variable fixes, canvas drag-to-keyframe routing, diamond toggle fix, spacebar playback shortcut, in-place Bézier curve graph editor component, and animation preset browser.
- **Tauri IPC**: Needs `render_comp` call fix, fast binary frame transport, and video export IPC command.
- **MCP Server**: Needs full stdio JSON-RPC server implementation exposing `op.apply`, `ops.propose`, `history.*`, `project.*`, and `comp.render`.
- **Testing**: Needs Vitest setup for UI and Rust integration tests for `bonaparte-mcp`.

---

## 5. Verification Method

1. **Verify Svelte Build & Warnings**:
   ```bash
   cd ui && npm run build
   ```
   *Expected outcome when fixed*: Zero Svelte compiler warnings (`non_reactive_update`, `a11y_no_static_element_interactions`).
2. **Verify Tauri Backend Compilation**:
   ```bash
   cargo check -p bonaparte-app
   ```
   *Expected outcome when fixed*: Clean compilation with 0 errors and 0 warnings.
3. **Verify MCP Server Implementation**:
   ```bash
   cargo test -p bonaparte-mcp
   ```
   *Expected outcome when fixed*: MCP tool execution, proposal diffing, and undo tests pass 100%.
4. **Inspect Generated Survey Report**:
   Inspect `C:\Users\khati\.zcode\workspace\default\bonaparte\.agents\survey_ui_mcp\survey.md`.
