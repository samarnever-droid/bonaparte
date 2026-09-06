## 2026-09-04T08:43:51Z
You are the Engine Specialist for Bonaparte.
Your working directory is: C:\Users\khati\.zcode\workspace\default\bonaparte\.agents\worker_engine
You MUST read:
- C:\Users\khati\.zcode\workspace\default\bonaparte\ORIGINAL_REQUEST.md
- C:\Users\khati\.zcode\workspace\default\bonaparte\PROJECT.md
- C:\Users\khati\.zcode\workspace\default\bonaparte\.agents\worker_model\handoff.md

MANDATORY INTEGRITY WARNING:
DO NOT CHEAT. All implementations must be genuine. DO NOT hardcode test results, create dummy/facade implementations, or circumvent the intended task.

Your exclusive write scope: `crates/engine/`.

Tasks:
1. Purity & WASM check:
   - Ensure `bonaparte-engine` core has zero OS, thread, or filesystem dependencies.
   - Verify `cargo check --target wasm32-unknown-unknown -p bonaparte-engine` succeeds cleanly.
2. 256×256 Tile Grid Scheduler (`tiles.rs`):
   - Tile grid calculation, viewport bounds culling, layer bounding-box intersection.
   - Implement `render_tile` / `render_viewport_tiles` for O(viewport) RAM execution.
3. RenderGraph (`graph.rs`):
   - Topological dependency sorting (Kahn's algorithm) with deterministic tie-breaking and cycle detection.
   - Automated DAG builder from `Comp` layers, transforms, blend modes, and effects at `Time`.
4. CPU Software Reference Renderer (`reference.rs`):
   - 2D affine transform matrix evaluation (translation, scaling, rotation in degrees, anchor point).
   - Support all layer types: `Solid`, `Shape` (rect & path), `Footage` (via `MediaFrames`), `Text` (clean glyph rasterization), and `PreComp` (nested comp rendering with cycle/depth recursion guard).
   - Full 2D alpha blend modes: `Normal` (Over), `Multiply`, `Screen`, `Overlay`, `Add` (Linear Dodge), `Darken`, `Lighten`, `Difference`.
5. GPU backend (`wgpu`/WGSL) scaffolding behind `gpu` feature.
6. Tests:
   - Add comprehensive tests in `crates/engine/tests/` and unit tests.
   - Run `cargo test -p bonaparte-engine` and `cargo check --target wasm32-unknown-unknown -p bonaparte-engine`.
   - Write handoff to `C:\Users\khati\.zcode\workspace\default\bonaparte\.agents\worker_engine\handoff.md`.
Send message when done.
