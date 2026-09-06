# Verification — v0.5.1 editing stability

Verified locally on **2026-09-06**. This patch addresses reproduced editing failures, not an assertion that every UI bug is fixed. Previous aggregate test results missed several of these interaction sequences.

## Current results

| Check | Result |
|---|---|
| Locked Rust workspace tests (native app harness excluded) | **213 passed**, no warnings |
| New history-group regressions | **3 passed**: original inverse/latest redo, separate targets, invalid edit safety |
| New raster-quality regressions | **3 passed**: enlarged glyph/generator grids, nested render density, preview/output consistency |
| Native Linux Tauri compile | Passed |
| Pure engine/audio WASM check | Passed |
| Svelte / TypeScript | **0 errors, 0 warnings** |
| Production frontend | Built successfully |
| Audio worklet tests | **6 passed** |
| Real-browser workflow suite | **36 passed**, including **14 editing-stability workflows** |
| Live smoke | Full-quality native frame loaded on port 5173; rotation-tool control accessible; no reported page/console errors |

The full Rust/browser results include the earlier audio, effects, persistence, history, GPU-API and export regressions. GPU execution was on software Vulkan/llvmpipe; physical GPU/native-device validation is not claimed.

## Behavior specifically covered

- Text, position, color, alpha, effect values and shape dimensions change while the input remains focused.
- Native color-picker `input` events work without waiting for its final `change` event.
- Incomplete numeric values are not treated as zero edits.
- Delayed acknowledgements do not overwrite newer typed text.
- Focused edits persist incrementally and still undo/redo as one compatible edit group.
- Rapid visibility toggles do not reuse the same stale boolean.
- Visibility changes during transform refinement do not leave an old proxy covering the canvas.
- Rotation-handle placement follows the rotated edge; anchor pivots and nonuniform parent transforms are tested.
- Scaling uses the anchor and respects an explicit Full-quality selection.
- Enlarged text, generators and nested compositions get output-aware rasterization rather than fixed-size bitmap enlargement.
- Timeline view resets on composition changes, 75-second navigation works, duration can grow, and blank projects begin with 30 seconds.

These tests check rendered pixels and the real Rust document, not only field labels or mock state. The delayed-response tests delay genuine runtime responses.

## Scope / remaining uncertainty

The initial restoration attempt lacked Chromium system libraries and did not exercise the app; those dependencies were installed before behavior was tested. A later reproduction run exposed the focus, visibility, rotation and stale-proxy failures. One preliminary timeline test used an invalid external-state/disabled-Undo setup and was corrected to test real composition switching.

No claim is made that “hundreds of bugs” were exhaustively found or fixed. Additional input methods, platforms, complex hierarchies and large project combinations still need real-use coverage. The current bug ledger is [STABILITY-0.5.1.md](docs/STABILITY-0.5.1.md).

The 24-hour composition validation ceiling remains; “no five-second limit” does not mean literally infinite export duration. Imported raster images retain their native detail limits. Adaptive source rasterization is bounded to 8× density and the documented memory/pixel budgets. Auto movement proxies may still approximate expensive composites until native refinement; scaling/rotation use fresh frames, and explicit Full quality does not use a low-resolution transform proxy.

The workspace has one current preview: **Bonaparte Studio · Stability fixes**, port **5173**, using Rust service **4317**. The saved Orbit project was restored from its retained project copy. Old in-memory undo history is not reconstructed after an environment restart.

Source edits remain local and uncommitted. No GitHub push, remote CI success or cross-platform native GUI E2E validation is claimed. Evidence is under `docs/verification/v0.5.1`.
