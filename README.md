# Bonaparte ??

> **A GPU-first, AI-native motion graphics desktop editor with After Effects' mental model, engineered for 8GB RAM integrated-GPU PCs.**

Built with **Tauri v2**, **Svelte 5**, a **pure Rust microkernel engine**, and a **headless MCP server**.

---

## Architecture Overview

`
                          +------------------------+
                          ¦   Svelte 5 Desktop UI  ¦
                          ¦      (Tauri v2)        ¦
                          +------------------------+
                                      ¦ Fast Binary IPC
                                      ?
+-------------------------+   +------------------------+   +-------------------------+
¦  bonaparte-mcp (stdio)  ¦--?¦ bonaparte-model (core) ¦?--¦  AI Agent / Automation  ¦
¦ Headless JSON-RPC tools ¦   ¦ Exact Invertible Ops   ¦   ¦ (Claude / Cursor / MCP) ¦
+-------------------------+   +------------------------+   +-------------------------+
                                          ¦
                  +-----------------------+-----------------------+
                  ?                       ?                       ?
      +-----------------------+ +------------------+ +------------------------+
      ¦   bonaparte-engine    ¦ ¦bonaparte-effects ¦ ¦    bonaparte-media     ¦
      ¦ 256x256 Tile Graph    ¦ ¦Microkernel Plugin¦ ¦ Child-Process FFmpeg   ¦
      ¦ Pure wasm32 Compositor¦ ¦ WGSL + Manifests ¦ ¦ Flat-RAM Disk Cache    ¦
      +-----------------------+ +------------------+ +------------------------+
`

### Core Crates & Packages

- **crates/model (onaparte-model)**:
  - Integer-tick Time(i64) arithmetic (120,000 ticks/sec, exact SMPTE timecode).
  - Rational FrameRate with NTSC support (23.976, 29.97, 24, 25, 30, 60 fps).
  - Invertible Op mutation engine with complete undo/redo history and atomic diff previews.
  - Cubic Bézier keyframe tracks, layer parenting hierarchies, and blend modes.

- **crates/engine (onaparte-engine)**:
  - Pure, synchronous 256×256 tiled rendering graph with topological dependency sorting.
  - Cycle detection and hierarchical affine coordinate transform propagation.
  - Software reference renderer supporting all layer types: Solid, Shape (Rectangle, Circle), Text, Footage, PreComp.
  - Compiles cleanly to wasm32-unknown-unknown without OS or thread dependencies.

- **crates/effects (onaparte-effects)**:
  - Microkernel plugin host: zero private engine hooks; all filters and generators adhere strictly to public plugin contracts (manifest.toml + WGSL + CPU evaluator).
  - 11 built-in first-party plugin packs:
    - uiltin.glow, uiltin.blur, uiltin.drop_shadow, uiltin.color_adjust
    - uiltin.transform, uiltin.vignette, uiltin.chromatic_aberration, uiltin.invert
    - uiltin.tint, uiltin.directional_blur
    - uiltin.circle (vector signed distance function generator)

- **crates/media (onaparte-media)**:
  - Native-only crash-isolated FFmpeg child-process pipeline.
  - Frame decoding, half-resolution proxy generation, and perception card probing.
  - Flat-RAM playback cache guarantee (disk-backed with LRU memory bounds).
  - Streaming frame-by-frame MP4 video export directly from compositor into FFmpeg stdin.

- **crates/mcp (onaparte-mcp)**:
  - Headless Model Context Protocol server over stdio JSON-RPC.
  - Full Op vocabulary exposed for autonomous AI generation, project inspection, dry-run diff proposals, and batch rendering.

- **ui (onaparte-ui)**:
  - Svelte 5 runes (\, \) responsive desktop shell.
  - Interactive timeline with high-precision playhead scrubbing and frame stepping.
  - In-place cubic Bézier curve graph editor.
  - Canvas viewport with draggable layer transform handles.
  - Layer properties panel with keyframe toggles and generator plugin inspectors.
  - Dynamic layer creation menu (Circle, Rectangle, Text, Solid).

---

## Getting Started

### Prerequisites
- **Rust 1.80+** (ustup default stable)
- **Node.js 20+** & 
pm
- **Tauri v2 Prerequisites** (C++ build tools / WebView2 on Windows)
- **FFmpeg** (optional for media import/export tests)

### Build & Test Suite

`ash
# Run all workspace tests (100% green)
cargo test --workspace

# Verify pure wasm32 engine compilation
cargo check --target wasm32-unknown-unknown -p bonaparte-engine

# Build UI
cd ui
npm install
npm run build
`

### Launch Desktop Editor

`ash
# Run in development mode
npm run tauri dev
`

### Run Headless MCP Server

`ash
cargo run -p bonaparte-mcp
`

---

## License

MIT or Apache 2.0.
