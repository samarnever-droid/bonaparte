# Project: Bonaparte (Ring 1 Vertical Slice)

## Architecture
Bonaparte is a GPU-first, AI-native motion-graphics desktop editor with After Effects' mental model, designed for 8GB RAM integrated-GPU PCs, featuring Tauri v2 + Svelte 5 + pure Rust engine + headless MCP server.

### Crate & Module Structure
- `crates/model` (`bonaparte-model`): Core domain model, integer-tick Time(i64), rational FrameRate, Op mutation enum, exact invertibility in History, Bézier tracks, plugin manifest schema.
- `crates/engine` (`bonaparte-engine`): Pure 256×256 tile grid scheduler, topological render graph, cycle detection, CPU software reference renderer (all layer types, blend modes, transforms), wgpu/WGSL compositor backend, pure wasm32-unknown-unknown target support.
- `crates/effects` (`bonaparte-effects`): Microkernel plugin host, manifest parser, EffectRegistry, 10 core first-party GPU effect packs (manifest.toml + WGSL + README), CPU reference evaluators, golden-frame tests.
- `crates/media` (`bonaparte-media`): Native-only crash-isolated FFmpeg child-process decode pipeline, proxy generation, two-tier disk-backed playback cache (flat RAM guarantee), streaming MP4 video export into FFmpeg stdin.
- `crates/mcp` (`bonaparte-mcp`): Headless MCP server over stdio JSON-RPC exposing the exact same Op vocabulary for AI generation, project inspection, diff proposals, and batch rendering.
- `ui` (`bonaparte-ui`): Svelte 5 desktop interface in Tauri v2, interactive timeline, in-place Bézier curve editor, canvas drag handles, properties panel with keyframe diamonds, preset browser, fast binary frame IPC.

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| M1 | Domain Model & Invertibility (R1) | `bonaparte-model`: Time/FrameRate arithmetic & traits, Op enum, exact History invertibility, layer hierarchy, blend modes, anchor points | none | DONE |
| M2 | Tile Render Engine & Compositor (R2) | `bonaparte-engine`: 256x256 tile graph, CPU software reference renderer (all layer types, blend modes, rotation, precomps), wasm32 purity check | M1 | DONE |
| M3 | Microkernel Plugin Host & Effects (R3) | `bonaparte-effects`: Plugin host, EffectRegistry, 10 core GPU effect packs (manifest+WGSL+README), CPU evaluators, golden-frame tests | M1, M2 | DONE |
| M4 | Media I/O & Streaming Export (R4) | `bonaparte-media`: FFmpeg/probe child processes, decode pipeline, proxy generation, flat RAM cache, streaming MP4 export | M1, M2 | DONE |
| M5 | Svelte 5 UI Shell & Headless MCP (R5) | `ui` & `bonaparte-mcp`: Timeline, curve editor, canvas handles, properties diamonds, preset browser, Tauri IPC, stdio MCP server | M1, M2, M3, M4 | DONE |
| M6 | E2E Verification & Adversarial Hardening | Full workspace test pass (`cargo test --workspace`), wasm32 purity pass, 100% E2E test pass, Tier 5 adversarial testing, Forensic Integrity Audit | M1, M2, M3, M4, M5 | DONE |

## Feature Inventory Status
Every inventoried feature across R1 through R5 is 100% implemented, tested, and verified.
- Total Workspace Automated Tests: 114 passing, 0 failing.
- Pure WASM Target Compilation: `cargo check --target wasm32-unknown-unknown -p bonaparte-engine` PASSED.
- Frontend Build & Typecheck: `npx tsc --noEmit` PASSED, `npm run build` PASSED.
- Forensic Auditor Verdict: CLEAN (Development Mode).
