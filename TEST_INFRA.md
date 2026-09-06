# Test infrastructure

## Reproducible commands

From the repository root, with stable Rust and FFmpeg/ffprobe installed:

```sh
cargo test --workspace --exclude bonaparte-app
cargo check -p bonaparte-app  # native WebView/GTK prerequisites on Linux
rustup target add wasm32-unknown-unknown
cargo check -p bonaparte-engine --target wasm32-unknown-unknown
```

From `ui`:

```sh
npm ci
npm run check
npm run build
npx playwright install chromium
npm run test:e2e
```

On minimal Linux CI, use `npx playwright install --with-deps chromium`. Media export regression tests require FFmpeg/ffprobe; tests that explicitly skip without FFmpeg are not evidence of encoding coverage. The recorded verification run had both installed.

## Browser isolation

`ui/playwright.config.ts` launches `npm run studio` with **Vite 5183 / Rust 4318**, one test worker and no reuse of an existing server. Keep these ports free. The normal user preview is **5173 / 4317** and is not reset by this suite.

The browser tests set initial fixtures through the real API, then drive the visible controls. No requests are intercepted, no document backend is mocked, and no JS render substitute is used. Assertions inspect Rust state, canvas pixels, downloaded project/PNG files and `ffprobe` output. A separate test opens the editable Orbit project and captures all three workspaces.

## Regression coverage

| Tests | Evidence |
|---|---|
| `model/tests/foundation.rs` | Ordered effects/tracks and exact undo; duplicate IDs; failing Batch atomicity; layer retiming; overflow/transaction limit; complete 256-operation inverses; safe ID allocators; parent cycles; legacy defaults; nonfinite/unsorted data |
| `effects/tests/color_grading.rs` | Neutral byte identity; linear exposure; temperature/saturation; alpha; manifest type/range checks; animated parameters; premultiplied blur; a third-party CPU plugin |
| `effects/tests/shader_validation.rs` | Naga WGSL syntax/type validation of every bundled pack, **not GPU rendering** |
| `engine/tests/effect_compositing.rs` | Ordered/bypassed effects; adjustment ordering and premultiplied opacity; effect-aware tile equality; external registry; text rendering/limits; alpha encoding and bilinear sampling |
| `runtime/tests/editor_workflow.rs` | Real example rendering; versioned files; embedded image import/render/undo; malformed data atomicity; transient previews; binary protocol; linear black matte; exact NTSC MP4 frame count; failed export preserves existing file |
| `mcp/tests/foundation.rs` | Catalog discovery; shared effect persistence/mutation; invalid proposals and explicitly missing composition requests |
| `ui/tests/editor.spec.ts` | Composition create/settings/undo; text/shape edits; canvas gestures/lock/duplicate; grade pixels/bypass/order/keys; image/save/open/errors/exports; motion/key dragging/easing/playback; three workspaces |

The pre-existing arithmetic, blend, affine, layer, tile, effect, Op round-trip, media/cache and MCP suites are retained. Old test names that mention a historical count are not a coverage measurement.

## Exclusions and next tests

Native WebView/dialog interactions, Windows/macOS behavior, GPU execution/parity, long-running memory stability, power-loss durability, complex-script typography and sustained interactive performance are **not covered**. The current tests do not substantiate the former “100% feature complete” or “forensic audit clean” claims.

Playwright artifacts are generated under `ui/test-results`; dependency/build/artifact directories are not part of the source deliverable.
