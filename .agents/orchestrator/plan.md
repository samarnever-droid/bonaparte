# Execution Plan: Bonaparte Ring 1 Vertical Slice

## Overview
Bonaparte is a GPU-first, AI-native motion-graphics desktop editor (Tauri v2 + Svelte 5 + pure Rust engine + headless MCP server).
This plan guides the execution across all 5 requirement domains (R1 to R5) and acceptance criteria.

## Phases

### Phase 0: Survey & Scope Mapping
- Dispatch 3 parallel Explorers / Spec Miners:
  - Explorer 1 (Domain Model & Engine): Inspect crates `bonaparte-model` and `bonaparte-engine`, data structures, tick arithmetic, Op mutations, undo/redo history, tile render graph, software renderer, and wgpu/WGSL wasm32 purity.
  - Explorer 2 (Effects & Media): Inspect `bonaparte-effects` (microkernel plugin host, manifest parser, WGSL effects, golden-frame tests) and `bonaparte-media` (FFmpeg decoder/proxy process, disk cache, frame-by-frame streaming MP4 export).
  - Explorer 3 (UI Shell & MCP): Inspect `ui` (Svelte 5 desktop app in Tauri, timeline, curve graph editor, canvas drag handles, properties, playhead) and `bonaparte-mcp` (headless MCP server with Op vocabulary).
- Synthesize findings into `PROJECT.md` (Architecture, Feature Inventory, Milestones, Interface Contracts, Code Layout) and `TEST_INFRA.md`.

### Phase 1: Dual Track Initiation
- Track A: E2E Testing Track (opaque-box, 4-tier test suite per TEST_INFRA.md)
- Track B: Implementation Track (Milestone execution)

### Phase 2: Milestone Iteration Loops (M1 -> M5)
- M1: bonaparte-model (Invertible Op Engine, History, Time arithmetic, serialization)
- M2: bonaparte-engine (Tile-based render graph, software reference renderer, wgpu/WGSL compositor, wasm32-unknown-unknown compilation check)
- M3: bonaparte-effects (Microkernel plugin host, manifest validator, 10 core GPU effect packs, golden-frame tests)
- M4: bonaparte-media (FFmpeg child-process decoding, proxy generation, flat RAM disk caching, streaming MP4 export)
- M5: ui & bonaparte-mcp (Svelte 5 desktop shell, timeline/scrubber, Bézier curve editor, canvas handles, MCP server)

### Phase 3: Final Verification & Adversarial Coverage Hardening (M6)
- Phase 1: Pass 100% E2E tests (Tiers 1-4)
- Phase 2: Challenger-driven adversarial coverage hardening (Tier 5)
- Forensic Integrity Audit
- Victory Report to parent sentinel
