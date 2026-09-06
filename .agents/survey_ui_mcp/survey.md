# Comprehensive Survey Report: Requirement R5 — Interactive Svelte 5 UI Shell & Headless MCP Server

**Date**: 2026-08-30  
**Scope**: `ui` (Svelte 5 desktop interface in Tauri v2), `ui/src-tauri` (IPC bridge & application shell), and `crates/mcp` (headless MCP server).

---

## 1. Executive Summary

Requirement **R5** requires a zero-bug interactive Svelte 5 desktop interface in Tauri and a headless MCP server exposing the exact same `Op` vocabulary for AI automation, project inspection, and batch rendering.

### Current Overall Status: **Partial / Early Skeleton**
- **Svelte 5 UI**: A foundational shell exists with `TopBar`, `Viewport`, `Properties`, and `Timeline` components using Svelte 5 runes (`$state`, `$derived`, `$effect`). However, critical motion-graphics features are missing (in-place Bézier curve graph editor, draggable canvas transform handles, animation preset browser), several interaction bugs exist (canvas drag overrides static transform while ignoring active tracks, keyframe diamond toggles do not remove keyframes, non-reactive Svelte 5 variables), and keyboard shortcuts like `Space` (play/pause) and `K` (keyframe) are completely unbound.
- **Tauri IPC Bridge**: Basic commands (`project_state`, `apply_op`, `undo`, `redo`, `render_frame`, `history_list`) are defined, but `src-tauri/src/main.rs` is currently broken due to signature mismatches with `bonaparte-engine::reference::render_comp` (`Time` struct vs `f64`, missing `MediaFrames`). In addition, frame transfers serialize raw RGBA byte vectors into JSON number arrays, causing severe latency overhead during scrubbing.
- **Headless MCP Server (`crates/mcp`)**: Empty stub containing only a placeholder module. No tool catalog, no JSON-RPC / stdio transport, and no headless batch render execution.
- **Testing**: Zero frontend tests and zero MCP server tests exist in the repository.

---

## 2. Feature Inventory for R5

| Feature / Capability | Category | Status | Exact File Path(s) | Description / Notes |
| :--- | :--- | :--- | :--- | :--- |
| **Svelte 5 Editor Store** | UI State | Partial | `ui/src/lib/store.svelte.ts` | Uses `$state` for `project`, `currentTime`, `playing`, `selected`, `drag`, `renderSeq`. Lacks time tick conversion and high-precision animation loop. |
| **Top Navigation Bar** | UI Component | Implemented | `ui/src/lib/components/TopBar.svelte` | Logo, Play/Pause button, timecode display, Undo/Redo buttons. Needs frame-accurate FPS calculation and active button disable states. |
| **Interactive Viewport** | UI Component | Partial | `ui/src/lib/components/Viewport.svelte` | Renders frame into `<canvas>`, draws dashed drag preview rectangle. Lacks transform handles, motion path tangents, high-DPI scaling. |
| **Direct Canvas Dragging** | UI Gesture | Partial / Buggy | `ui/src/lib/components/Viewport.svelte:29-70` | Drag gesture hit tests shapes and dispatches `setValue(Position)`. Fails to update animated keyframe tracks when present. |
| **Timeline Scrubber & Ruler** | UI Component | Partial | `ui/src/lib/components/Timeline.svelte:60-78` | Pointer-capture scrubber on ruler. Missing frame-level zoom, sub-second snapping, and ARIA roles. |
| **Timeline Layer Rows & Keys** | UI Component | Partial | `ui/src/lib/components/Timeline.svelte:80-108` | Displays layer duration bars and keyframe diamonds. Lacks keyframe dragging (`MoveKeyframe`), selection, and deletion (`RemoveKeyframe`). |
| **In-Place Bézier Curve Editor** | UI Feature | **Missing** | `ui/src/lib/components/CurveEditor.svelte` *(TBD)* | Velocity/value graph curves, tangent handle manipulation, easing preset buttons (`Linear`, `Bezier`). Required by R5. |
| **Properties Panel** | UI Component | Partial | `ui/src/lib/components/Properties.svelte` | Inputs for Position, Scale, Rotation, Opacity. Commits static `setValue`; does not edit keyframes when property is animated. |
| **Keyframe Diamond Toggles** | UI Interaction | Partial / Buggy | `ui/src/lib/components/Properties.svelte:51-60` | Only executes `AddKeyframe`; never removes keyframe when playhead is over an existing key. Indicator reflects track existence, not key at playhead. |
| **Animation Preset Browser** | UI Component | **Missing** | `ui/src/lib/components/PresetBrowser.svelte` *(TBD)* | Preset catalog (Fade, Pop, Slide, Bounce), curve previews, one-click track insertion. Required by R5. |
| **Global Keyboard Shortcuts** | UI Interaction | Partial | `ui/src/App.svelte:11-18` | `Ctrl+Z` (undo) and `Ctrl+Shift+Z` (redo) work. `Space` (play/pause), `K` (keyframe), `Delete` (remove layer/key), Arrow keys (step frame) are **missing**. |
| **Dark Mode Design System** | UI Styling | Implemented | `ui/src/app.css` | Tailwind v4 tokens: `--bg-base`, `--bg-panel`, `--bg-raised`, `--border`, `--text`, `--text-dim`, `--accent`, `--danger`. |
| **Tauri Core IPC Commands** | Tauri IPC | Broken / Incomplete | `ui/src-tauri/src/main.rs` | `project_state`, `apply_op`, `undo`, `redo`, `render_frame`, `history_list`. Fails to compile against `engine` reference renderer. |
| **Tauri Video Export Command** | Tauri IPC | **Missing** | `ui/src-tauri/src/main.rs` | Headless/streaming MP4 export bridge connecting `bonaparte-media` to UI. |
| **MCP Server Protocol & Stdio** | MCP Server | **Missing** | `crates/mcp/src/lib.rs` | JSON-RPC 2.0 stdio server handling MCP handshake and tool calls. |
| **MCP `op_apply` Tool** | MCP Server | **Missing** | `crates/mcp/src/lib.rs` | Applies any `Op` to the project with full validation and undo history tracking. |
| **MCP `ops_propose` Tool** | MCP Server | **Missing** | `crates/mcp/src/lib.rs` | Generates human-readable diffs and dry-run validation for multi-op AI proposals. |
| **MCP `project_*` Tools** | MCP Server | **Missing** | `crates/mcp/src/lib.rs` | `project_open`, `project_save`, `project_info`, `project_create`. |
| **MCP `history_*` Tools** | MCP Server | **Missing** | `crates/mcp/src/lib.rs` | `history_list`, `history_undo`, `history_redo`. |
| **MCP `comp_render` Tool** | MCP Server | **Missing** | `crates/mcp/src/lib.rs` | Headless rendering of composition frames or MP4 files. |
| **Frontend Unit & Component Tests** | Test Suite | **Missing** | `ui/src/**/*.test.ts` | Vitest / Testing Library suite for stores, geometry, interpolation, and components. |
| **MCP Server Automated Tests** | Test Suite | **Missing** | `crates/mcp/tests/` | Integration tests verifying Op execution, undo roundtrips, tool schema conformity. |

---

## 3. Architectural Analysis

### 3.1 Svelte 5 Stores & Runes (`$state`, `$derived`, `$effect`)
In Svelte 5, reactivity is governed by signals via compiler runes:
- **`editor` store (`ui/src/lib/store.svelte.ts`)**:
  ```ts
  export const editor = $state({
    project: null as Project | null,
    activeComp: null as number | null,
    currentTime: 0,
    playing: false,
    selected: null as number | null,
    drag: null as { layer: number; dx: number; dy: number } | null,
    renderSeq: 0,
  });
  ```
  - **Single Source of Truth**: The client state mirrors the Rust `Project` document. The UI enforces the microkernel contract (RULES §5.3) by routing mutations through `applyOp(op: Op)` which round-trips through Tauri IPC.
  - **Derived Values**: Functions like `activeComp()` and `selectedLayer()` read `editor.project` and `editor.selected`. In `Properties.svelte`, `$derived(selectedLayer())` accurately updates when `editor.selected` or `editor.project` changes.
  - **Reactivity Violations Found**: In `Viewport.svelte:25`, `let dragging = false;` is declared as a plain mutable variable rather than `$state(false)`. Vite emits a `non_reactive_update` warning because mutations to `dragging` inside pointer event listeners do not trigger re-renders of the cursor style `{dragging ? 'cursor-grabbing' : 'cursor-grab'}`.

### 3.2 Time Units and Representation Mismatch
- **Rust Domain Model (`bonaparte-model`)**:
  - `Time(i64)` represents exact integer ticks at `TICKS_PER_SEC = 120_000`.
  - `Comp.duration` is `Time(i64)`.
  - `Keyframe.time` is `Time(i64)`.
  - `FrameRate` is a rational struct `{ num: u32, den: u32 }`.
- **TypeScript UI (`ui/src/lib/model.ts` & `store.svelte.ts`)**:
  - `currentTime` is stored as float seconds `number` (e.g., `0.5`).
  - `Keyframe.time` and `Layer.start`/`duration` are defined as `number`.
  - **Desynchronization Hazard**: If `bonaparte-model` serializes `Time(120000)` into JSON, TS receives the raw integer `120000`. Evaluating `pct(120000) = (120000 / 8.0) * 100` yields `1500000%`, blowing keyframe diamonds far offscreen. Time conversion utilities (`ticksToSecs(ticks: number): number` and `secsToTicks(secs: number): number`) must be added to normalize all UI displays to ticks internally.

### 3.3 Tauri IPC Bridge Architecture
The Tauri backend (`ui/src-tauri/src/main.rs`) manages `AppState`:
```rust
pub struct AppState {
    project: Mutex<Project>,
    history: Mutex<History>,
}
```
- Every mutation goes through `apply_op(state, op: Op) -> Result<Project, String>`, which commits to `History` and returns the updated `Project` snapshot.
- **Compilation Breakage**: `render_frame` calls `render_comp(&project, CompId(comp_id), time)` with `time: f64` instead of `Time::from_secs_f64(time)`, and omits the required `frames: &dyn MediaFrames` argument.
- **Serialization Performance Bottleneck**: `FrameDto` serializes `rgba: Vec<u8>` as a standard JSON array of numbers. For a 640×360 canvas (921,600 numbers), JSON string serialization, IPC transfer, and JSON parsing in JS takes 15–40ms per frame, causing frame drops and visual stutter during 60fps playhead scrubbing. Tauri's binary response (`tauri::ipc::Response`) or shared memory / ArrayBuffer transfer must be used for smooth scrubbing.

### 3.4 Headless MCP Server Architecture (`bonaparte-mcp`)
The MCP server is specified as the "headless twin" of the UI shell:
- **Core Principle**: Identical `Op` vocabulary. The MCP server must never implement private editor logic that cannot be expressed as an `Op`.
- **Planned Surface**:
  1. **Project Lifecycle**: `project.create`, `project.open`, `project.save`, `project.info`.
  2. **Atomic Mutations**: `op.apply` (executes single `Op` through `History::commit`).
  3. **AI Speculative Diff**: `ops.propose` (takes a batch of `Op`s, runs a dry-run against the document, and returns human-readable `Op::describe()` summaries and inverted diffs without committing).
  4. **Legible Undo**: `history.list`, `history.undo`, `history.redo`.
  5. **Batch Render Engine**: `comp.render` (renders frames or MP4 video to a target directory via `bonaparte-media` / `bonaparte-engine`).
  6. **Dynamic Tool Generation**: Auto-registers tools for installed effect plugins by introspecting `EffectManifest.params`.

---

## 4. Identified UI Bugs, Interaction Gaps, and Curve Editor State

### 4.1 Viewport & Direct-DOM Drag Bugs
1. **Track Override Bug on Canvas Drag**:
   - **Location**: `ui/src/lib/components/Viewport.svelte:58-66`
   - **Issue**: On pointer release (`up`), the viewport dispatches `setValue` for `Position`. In `bonaparte-model`, if a keyframe track exists on `Property::Position`, `Track::evaluate` takes precedence over `StaticTransform`. As a result, moving an animated object on the canvas reverts to the keyframe track value on the next frame!
   - **Fix**: When dragging a layer that has keyframes on `Position`, pointerup must check if a track exists. If yes, it must call `addKeyframe` at `editor.currentTime` (or `moveKeyframe` if playhead is exactly on a keyframe) rather than `setValue`.
2. **Non-Reactive Variable (`dragging`)**:
   - **Location**: `ui/src/lib/components/Viewport.svelte:25`
   - **Issue**: `let dragging = false;` triggers Svelte 5 warning `non_reactive_update`. The dynamic canvas cursor class `{dragging ? 'cursor-grabbing' : 'cursor-grab'}` does not update properly on pointerdown.
   - **Fix**: Declare with `let dragging = $state(false);`.
3. **Missing Draggable Canvas Handles**:
   - **Location**: `ui/src/lib/components/Viewport.svelte:77-87`
   - **Issue**: Only a static dashed rectangle is drawn for drag preview. There are no interactive corner/edge resize handles (for Scale), rotation handles (for Rotation), anchor point handles, or spatial Bézier motion path handles.
4. **Hit-Testing Restrictions & Matrix Transforms**:
   - **Location**: `ui/src/lib/geometry.ts:30-39`
   - **Issue**: `hitTest` explicitly filters out everything except `Shape` (`if (!layer || !("Shape" in layer.kind)) continue;`). Text, Footage, PreComp, and Solid layers cannot be selected or dragged on canvas. Furthermore, rotated layers are hit-tested against unrotated bounding boxes.

### 4.2 Properties Panel & Keyframe Diamond Bugs
1. **Keyframe Diamond Toggle Cannot Remove Keyframes**:
   - **Location**: `ui/src/lib/components/Properties.svelte:51-60` and `ui/src/lib/store.svelte.ts:90-105`
   - **Issue**: Clicking `◆` in `Properties.svelte` unconditionally calls `keyframeAtPlayhead`, which only issues `{ type: "addKeyframe" }`. If a keyframe already exists at `currentTime`, it does not toggle it off (remove it).
   - **Fix**: Check whether `layer.tracks[property]` contains a keyframe at `currentTime`. If yes, issue `{ type: "removeKeyframe" }`; if no, issue `{ type: "addKeyframe" }`.
2. **Incorrect Diamond Active State**:
   - **Location**: `ui/src/lib/components/Properties.svelte:32-34`
   - **Issue**: `hasTrack(p.key)` returns true if *any* keyframe exists on the track. The diamond is permanently lit blue even when the playhead is between keyframes. In After Effects / motion graphics convention, the diamond is filled/lit only when the playhead is *exactly on a keyframe*, and hollow/dim when at an interpolated position.
3. **Property Input Edits Ignore Active Keyframe Tracks**:
   - **Location**: `ui/src/lib/components/Properties.svelte:23-30`
   - **Issue**: Typing into property number inputs calls `commit()`, which sends `setValue`. If the property is keyframed, the static value is ignored by the evaluation engine, making inputs appear unresponsive.

### 4.3 Timeline & Bézier Curve Graph Editor Gaps
1. **Missing In-Place Bézier Curve Graph Editor**:
   - **Requirement**: R5 mandates an "in-place Bézier curve graph editor".
   - **Gap**: `Timeline.svelte` only has track bars and diamonds. There is no curve view showing:
     - Value and velocity curves plotted across time.
     - Tangent handles with editable slope and influence.
     - Easing preset selectors (`Linear`, `Ease In`, `Ease Out`, `Bezier(p1, p2)`).
     - Dispatching `SetEasing` ops when handles are adjusted.
2. **Keyframe Moving / Dragging Unsupported**:
   - **Location**: `ui/src/lib/components/Timeline.svelte:94-104`
   - **Gap**: Keyframe diamond buttons only have `onclick` (adding a keyframe at playhead). Users cannot drag keyframe diamonds along the time ruler to reposition them (`MoveKeyframe` op).
3. **Timeline Zoom & Pan Unsupported**:
   - **Gap**: Timeline ruler displays fixed 1-second interval labels (`0s`, `1s`, `2s`...). Users working on short clips (e.g. 12-frame transitions) cannot zoom in to see individual frame ticks.
4. **Playback Loop Timing & Frame Lag**:
   - **Location**: `ui/src/lib/store.svelte.ts:63-72`
   - **Issue**: `play()` uses `setInterval` with integer millisecond intervals. `setInterval` drifts under load and does not synchronize with the display refresh rate (`requestAnimationFrame`). When playing at 60fps, `renderTo` triggers rapid async Tauri IPC calls that queue up, causing frame lag and out-of-order rendering.
5. **Missing Keyboard Shortcuts**:
   - **Location**: `ui/src/App.svelte:11-18`
   - `Space`: Not intercepted in `window.addEventListener("keydown")` (pressing Space does nothing unless focused on play button).
   - `K`: Not intercepted (keyframe at playhead).
   - `Delete` / `Backspace`: Not intercepted (delete selected layer or keyframe).
   - `Left` / `Right` Arrow: Not intercepted (step backward / forward 1 frame).

---

## 5. Headless MCP Server Gap Analysis (`crates/mcp`)

### 5.1 Current Implementation State
`crates/mcp/src/lib.rs` currently contains only 22 lines of comments and a dummy placeholder module:
```rust
pub mod placeholder {
    pub const NOT_YET_IMPLEMENTED: &str = "MCP server lands with the vertical slice";
}
```

### 5.2 Required Tool Vocabulary & Implementation Blueprint

```
┌─────────────────────────────────────────────────────────────┐
│                      bonaparte-mcp                          │
│                                                             │
│   Stdio JSON-RPC 2.0 Transport (rmcp / async stdio)         │
│                                                             │
│   ┌─────────────────────────────────────────────────────┐   │
│   │                 MCP Tool Catalog                    │   │
│   │                                                     │   │
│   │  • project.open / save / info / create             │   │
│   │  • op.apply (executes Op via History::commit)       │   │
│   │  • ops.propose (dry-run diff preview)               │   │
│   │  • history.list / undo / redo                       │   │
│   │  • comp.render (batch rendering to file)            │   │
│   │  • plugin.params (introspects EffectManifest)       │   │
│   └─────────────────────────────────────────────────────┘   │
│                              │                              │
│                              ▼                              │
│                 bonaparte-model (Document/Ops)              │
│                 bonaparte-engine (Compositor)               │
│                 bonaparte-media (FFmpeg / Frames)           │
└─────────────────────────────────────────────────────────────┘
```

1. **`project.info` / `project.open` / `project.save` / `project.create`**:
   - Inspect project metadata, compositions, layer hierarchy, active tracks, and registered media assets.
   - Load and save JSON project files.
2. **`op.apply`**:
   - Receives strongly typed `Op` JSON (e.g. `{"type": "addKeyframe", "comp": 1, "layer": 2, "property": "Position", "key": {...}}`).
   - Validates through `Op::apply` and commits to `History`.
   - Returns updated project summary and generated inverse op description.
3. **`ops.propose` (AI-Diff Preview)**:
   - Takes a list of proposed `Op`s from an LLM.
   - Clones current `Project`, applies ops in sequence, detects any `ModelError`, and computes human-readable descriptions (`Op::describe()`) and affected layer summaries without mutating the active session.
4. **`history.list` / `history.undo` / `history.redo`**:
   - Inspects undo stack descriptions and steps backward/forward.
5. **`comp.render`**:
   - Renders a range of frames (`start_time` to `end_time`) or exports an MP4 video using `bonaparte-media` / `bonaparte-engine`.

---

## 6. Test Coverage and Testing Plan

### 6.1 Existing Test Coverage
- **`ui`**: Zero tests. No test runner (`vitest`) installed.
- **`ui/src-tauri`**: Zero tests.
- **`crates/mcp`**: Zero tests.

### 6.2 Required Test Suites for R5

1. **Frontend Unit Tests (`ui/test/`)**:
   - **`model.test.ts`**: Verify `evaluate()`, `ease()`, `lerp()`, and time tick conversions match `bonaparte-model`'s Rust output bit-for-bit.
   - **`store.test.ts`**: Verify `applyOp`, `undoOp`, `redoOp`, `keyframeAtPlayhead` toggle logic, and playhead clamping.
   - **`geometry.test.ts`**: Verify `shapeRect` and `hitTest` with scaling, translation, and bounds checking.
2. **Frontend Component & Interaction Tests (`ui/test/components/`)**:
   - **`Timeline.test.ts`**: Verify scrubber pointerdown/move scrubbing, keyframe diamond rendering, keyframe dragging, and time formatting.
   - **`CurveEditor.test.ts`**: Verify SVG Bézier path generation and tangent handle drag event dispatching.
   - **`Properties.test.ts`**: Verify property input change commits, keyframe toggle states (`isKeyAtPlayhead`), and disabled states.
   - **`Viewport.test.ts`**: Verify pointerdown selection, drag ghost preview, pointerup Op dispatch, and canvas resize scaling.
3. **Tauri IPC Integration Tests (`ui/src-tauri/tests/`)**:
   - Test `project_state`, `apply_op`, `undo`, `redo`, `history_list`, and `render_frame` endpoints.
4. **MCP Server Integration Tests (`crates/mcp/tests/`)**:
   - Test JSON-RPC stdio tool execution for all `Op` variants.
   - Test `ops.propose` diff generation without session state mutation.
   - Test round-trip undo/redo via MCP tool calls.
   - Test headless batch rendering of a composition to PNG / MP4.

---

## 7. Recommended Milestones & Dependency Order

```
┌────────────────────────────────────────────────────────────┐
│ M1: Fix Tauri IPC Bridge & Domain Model Compilation        │
│ • Resolve Time tick / FrameRate serde in model & IPC       │
│ • Fix render_comp call in src-tauri/src/main.rs            │
│ • Upgrade FrameDto to fast binary response                 │
└─────────────────────────────┬──────────────────────────────┘
                              │
                              ▼
┌────────────────────────────────────────────────────────────┐
│ M2: Zero-Bug Core UI Interactions & Keyboard Navigation   │
│ • Fix Viewport reactive dragging state & track override bug│
│ • Add transform handles (corners, scale, rotation)        │
│ • Implement accurate Keyframe diamond toggle (add/remove)  │
│ • Add Space (play/pause), K (keyframe), Delete shortcuts   │
│ • Replace setInterval with requestAnimationFrame in play() │
└─────────────────────────────┬──────────────────────────────┘
                              │
                              ▼
┌────────────────────────────────────────────────────────────┐
│ M3: In-Place Bézier Curve Graph Editor & Timeline Drag     │
│ • Implement CurveEditor.svelte with SVG curve visualization│
│ • Interactive tangent handle manipulation & SetEasing ops  │
│ • Keyframe diamond dragging in Timeline (MoveKeyframe)     │
│ • Timeline zoom / pan and frame-rate snapping              │
└─────────────────────────────┬──────────────────────────────┘
                              │
                              ▼
┌────────────────────────────────────────────────────────────┐
│ M4: Animation Preset Browser & Effects Integration        │
│ • PresetBrowser.svelte with animation curve presets        │
│ • One-click track insertion and multi-key application      │
│ • Dynamic property controls generated from EffectManifest  │
└─────────────────────────────┬──────────────────────────────┘
                              │
                              ▼
┌────────────────────────────────────────────────────────────┐
│ M5: Headless MCP Server (`bonaparte-mcp`)                  │
│ • Stdio JSON-RPC protocol implementation                   │
│ • op.apply, ops.propose, history.*, project.* tools        │
│ • comp.render batch render execution                       │
│ • Comprehensive MCP test suite                             │
└────────────────────────────────────────────────────────────┘
```

---

## 8. Summary Table of Files & Action Items

| File Path | Component / Layer | Primary Deficiencies & Bugs | Action Items |
| :--- | :--- | :--- | :--- |
| `ui/src/lib/model.ts` | TS Data Model | Missing `Op` variants (`CreateComp`, `RemoveComp`, `AddMedia`, `RemoveMedia`); time unit mismatch (seconds vs ticks). | Align TypeScript types with Rust `bonaparte-model`; add tick-to-second conversion helpers. |
| `ui/src/lib/store.svelte.ts` | Svelte 5 Store | `setInterval` playback drift; `keyframeAtPlayhead` only adds; float seconds sent to IPC; unthrottled frame rendering. | Switch to `requestAnimationFrame`; implement keyframe toggle; normalize time to `Time(i64)` ticks. |
| `ui/src/lib/components/Viewport.svelte` | Viewport Canvas | Non-reactive `dragging` warning; canvas drag dispatches `setValue` instead of `addKeyframe` on animated tracks; no transform handles; high-DPI blur. | Fix `$state(dragging)`; add track-aware keyframing on drag release; add canvas handles and devicePixelRatio scaling. |
| `ui/src/lib/components/Properties.svelte` | Properties Panel | Diamond toggle does not remove keys; diamond active state is inaccurate; editing inputs ignores active keyframe tracks. | Connect diamond to `isKeyAtPlayhead`; support toggle on/off; update keyframe value when track is active. |
| `ui/src/lib/components/Timeline.svelte` | Timeline UI | Missing in-place Bézier curve editor; cannot drag keyframes (`MoveKeyframe`); cannot reorder layers; missing ARIA role. | Add curve editor view toggle; implement keyframe dragging; add layer reordering drag; fix a11y. |
| `ui/src/App.svelte` | Main App Shell | Keyboard shortcuts for Space (play/pause), K (keyframe), Delete (remove) are not bound; listener leak on unmount. | Add full keyboard shortcut handler; clean up event listener in `onDestroy`. |
| `ui/src-tauri/src/main.rs` | Tauri IPC Backend | Broken call to `render_comp`; JSON array serialization of RGBA bytes causes severe IPC lag. | Fix `render_comp` arguments; convert frame transfer to binary IPC response; add MP4 export command. |
| `crates/mcp/src/lib.rs` | MCP Server | Empty placeholder stub; no tools, transport, or tests. | Implement stdio JSON-RPC MCP server with `op.apply`, `ops.propose`, `history.*`, `comp.render`. |
