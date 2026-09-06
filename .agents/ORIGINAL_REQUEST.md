# Original User Request

## 2026-08-30T09:19:57Z

<USER_REQUEST>
Complete the full Ring 1 Vertical Slice of Bonaparte: a GPU-first, AI-native motion-graphics desktop editor with After Effects' mental model, designed for 8GB RAM integrated-GPU PCs, featuring Tauri v2 + Svelte 5 + a pure Rust engine + headless MCP server.

Working directory: C:\Users\khati\.zcode\workspace\default\bonaparte
Integrity mode: development

Requested team structure & roles:
- Full multi-agent team with active adversarial peer review and real-time bug catching.
- Dedicated UI Bug Hunter & Real-Time Challenger: Audits every UI component, gesture, dragging interaction, keyframe curve editing, property binding, resize behavior, and playhead synchronization to catch any UI bug, race condition, or render artifact in real time.
- Dedicated UI Improver & Polish Specialist: Ensures pixel-perfect layout, smooth 60fps scrubbing, tactile direct-DOM drag interactions, and clean Dark Mode Tailwind CSS design.

## Requirements

### R1. Robust Domain Model & Invertible Operation Engine (bonaparte-model)
- Fix and complete all integer tick (Time(i64)) and rational FrameRate arithmetic, display traits, and serialization.
- Implement the full Op mutation enum covering composition management, layer hierarchy, property transforms, Bézier keyframe tracks, and media registration.
- Ensure every Op variant implements exact invertibility in History to guarantee flawless undo/redo and atomic diff previews for AI/MCP automation.

### R2. Pure Tile-Based Render Engine & Graph Compositor (bonaparte-engine)
- Implement a pure, synchronous 256×256 tiled rendering graph with topological dependency sorting, tile culling, and cycle detection.
- Complete the CPU software reference renderer to support all layer types (Solid, Shape, Text, Footage, PreComp) and alpha blend modes.
- Implement the GPU compositor backend using wgpu/WGSL shaders, ensuring bonaparte-engine compiles cleanly to wasm32-unknown-unknown without OS or thread dependencies.

### R3. Microkernel Plugin Host & First-Party Effect Packs (bonaparte-effects)
- Complete the microkernel plugin loader and manifest parser (manifest.toml + WGSL).
- Implement ~10 core first-party GPU effect packs (Glow, Blur, Drop Shadow, Color Adjust, Transform, Vignette, etc.) adhering strictly to the public plugin contract with zero private engine hooks.
- Provide automated manifest validation and pixel-deterministic golden-frame tests.

### R4. Crash-Isolated Media I/O & Streaming Export (bonaparte-media)
- Implement the native-only FFmpeg child-process pipeline for decoding media assets into raw frames and generating half-res proxy clips.
- Implement disk-backed playback caching with flat RAM consumption.
- Implement streaming frame-by-frame MP4 video export directly from the compositor into FFmpeg stdin.

### R5. Zero-Bug Interactive Svelte 5 UI Shell & Headless MCP Server (ui & bonaparte-mcp)
- Build the full Svelte 5 desktop interface in Tauri: interactive timeline with scrubber, in-place Bézier curve graph editor, draggable canvas handles, properties panel, and animation preset browser.
- Ensure absolutely zero UI bugs: smooth drag ghosts, frame synchronization, non-blocking scrubbing, accurate keyframe diamond toggles, and flawless keyboard shortcuts (Ctrl+Z / Ctrl+Shift+Z / Space to play).
- Implement the bonaparte-mcp server exposing the exact same Op vocabulary for headless AI generation, project inspection, and batch rendering.

## Acceptance Criteria

### Automated Verification
- [ ] cargo test --workspace passes 100% across all crates with zero warnings or errors.
- [ ] Round-trip undo/redo tests pass for every Op mutation variant.
- [ ] Pixel-deterministic golden-frame tests pass for all built-in effects and reference rendering passes.
- [ ] Engine purity verified: cargo check --target wasm32-unknown-unknown -p bonaparte-engine succeeds with zero native/tokio/filesystem dependencies.
- [ ] Plugin manifests validate successfully with complete parameter docstrings and declarations.

### Manual & UI Quality Verification
- [ ] Tauri desktop app builds, launches, and runs without console errors or UI glitches.
- [ ] Viewport renders interactive composition preview with real-time playhead scrubbing and keyframe manipulation.
- [ ] Timeline dragging, keyframe addition, in-place curve easing adjustments, and drag handles operate smoothly without visual lag or state desynchronization.
- [ ] Headless MCP server starts and executes project modifications via Op commands.
- [ ] Export pipeline produces a valid MP4 video from a composition without memory spikes.

</USER_REQUEST>

## 2026-08-30T09:38:13Z

User Directive: The project codebase is compact and well-defined. Do not spawn multiple redundant explorer subagents. Wrap up the survey immediately and proceed directly to focused implementation, bug-fixing, and test verification across the crates and UI.

## 2026-08-30T09:46:29Z

User Directive regarding Team Collaboration & Specialization:

The user requests an organized multi-agent team structure with specialized agents collaborating actively through the Project Orchestrator:

1. Specialized Domain Focus:
   - UI Shell Specialist (Svelte 5 components, layout, dark theme, panel system)
   - Engine Specialist (bonaparte-engine, render graph, tile scheduler, CPU reference renderer, GPU wgpu/WGSL, wasm32 purity)
   - GUI Interaction & Timeline Tweaker (canvas handles, playhead scrubbing, in-place Bézier curve editor, drag gestures)
   - AI & MCP Specialist (bonaparte-mcp server, tool schemas, Op diff generation, headless pipeline)
   - Core Model & Media/Effects Lead (bonaparte-model, bonaparte-effects WGSL packs, bonaparte-media FFmpeg streaming export)
   - Adversarial Code Auditor & Flaw Finder (reviews files, checks cross-crate contracts, runs tests, catches flaws, suggests cleaner/faster approaches)

2. Inter-Agent Communication & Synchronization:
   - When an agent introduces a new API, plugin contract, or feature, coordinate it through the Orchestrator so dependent agents stay synchronized.
   - When an agent has questions regarding an API or implementation detail, consult the corresponding domain specialist.

3. Team Flexibility & Running Agents:
   - The team count can be adjusted (5, 7, 8, or as the Orchestrator sees fit).
   - DO NOT kill any currently active/running agents; integrate and coordinate them into this collaborative workflow.

## 2026-08-30T09:56:48Z

User Directive: Strict File & Folder Write Boundaries to Prevent Overwrite Collisions:

Enforce strict write boundaries across all active agents to prevent race conditions or file overwriting:

1. Strict Write Permissions (Single-Writer per Path):
   - worker_model: Write ONLY to crates/model/
   - worker_engine: Write ONLY to crates/engine/
   - worker_effects_media: Write ONLY to crates/effects/ and crates/media/
   - worker_mcp: Write ONLY to crates/mcp/
   - worker_ui_shell: Write to ui/src/ shell layout & top-level structure (App.svelte, TopBar.svelte, styling)
   - worker_ui_timeline_tweaker: Write to interaction components (Timeline.svelte, Viewport.svelte, Properties.svelte, store.svelte.ts)
   - eviewer_auditor: Read-only across all crates; write only to dedicated test harness / verification reports.

2. Read & Coordination Rules:
   - All agents are permitted to READ anywhere across the repository for context.
   - NO agent may write to or modify files outside their designated boundary.
   - If an agent requires a change, new type, or API adjustment in another crate/module, they must request it through the Project Orchestrator or coordinate directly with the owning specialist.
