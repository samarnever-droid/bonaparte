# Foundation verification — v0.2

Verified locally on **2026-09-06**, based on upstream commit `b1278c8`. This supersedes the prototype's earlier completion/audit claims. Command output is included under [`docs/verification`](docs/verification).

## Results

| Check | Result |
|---|---|
| `cargo test --locked --workspace --exclude bonaparte-app` | **157 passed, 0 failed** |
| `cargo check --locked -p bonaparte-app` | Passed with the native Linux Tauri prerequisites installed |
| `cargo check --locked -p bonaparte-engine --target wasm32-unknown-unknown` | Passed |
| `cargo fmt --all --check` | Passed |
| `npm run check` | **0 errors, 0 warnings** from Svelte check |
| `npm run build` | Passed, production Vite bundle generated |
| `npm run test:e2e` | **7 passed**, Chromium, real Rust service, isolated ports 4318/5183 |
| Live preview smoke test | Real 960×540 canvas; no page/console errors; source served on port 5173 |
| Bundled effect WGSL | All 13 packs parsed/type-validated with Naga; **not GPU-executed** |
| MP4 verification | FFmpeg/ffprobe ran: native runtime NTSC test confirms 3 frames at 24000/1001; browser download test confirms 3 frames at 160×90 |

The Rust total includes **39 new regression tests** across model, effects, engine, runtime and MCP, plus the retained suites. Browser tests cover composition create/settings/undo, editable text and shapes, canvas transforms, locking/duplication, grade pixels/bypass/ordering/animation, imported images and project round trips, invalid files, PNG/MP4 downloads, motion recipes, key dragging, easing, playback, layer trimming/retiming and all three workspaces.

Regression coverage specifically includes failed operations preserving state/history, malformed base64 inside generic Batches, restoration ID collisions, unsafe ID allocator rejection, **all 256 operations surviving undo despite the 200-transaction history cap**, alpha-aware compositing, oversized text errors, exact video timing and preservation of existing files on failed exports.

## Environment

- Linux x86_64, two-vCPU sandbox (Intel Xeon 2.60 GHz reported by `lscpu`).
- Rust **1.98.1**, Node **20.20.2**.
- FFmpeg/ffprobe **7.1.5**, with libx264 available.
- Engine/effects/font rasterizer optimized in the development Cargo profile.

## Performance observation, not a product guarantee

Eight warm sequential raw-frame requests for the editable Orbit project at **960×540** had a median of **103.23 ms** in this sandbox. One warm-up was discarded; requests used successive 4000-tick steps. Timing includes local HTTP transfer of RGBA bytes, not browser painting. Raw samples and method are recorded in [`docs/performance-sample.json`](docs/performance-sample.json).

This is well below 30-fps playback speed and does **not** prove a frame-rate or memory target on user hardware. Preview coalesces/skips frames under load; video export renders every scheduled frame. GPU execution, caching and ROI work are still needed.

## What these checks do not establish

- Native GUI/file-dialog E2E behavior, Windows/macOS behavior, signed installers or auto-updates.
- GPU rendering, CPU/GPU pixel parity, sustained memory bounds, or an 8 GB performance guarantee.
- Video/audio timeline editing, masks, complex typography, calibrated color management or AE parity.
- Power-loss-safe directory syncing, export cancellation, or long-running stress reliability.

A GitHub Actions workflow is included in `.github/workflows/foundation.yml` to run the core/browser/native-compile checks. It was added locally; no successful remote CI run or upstream push is claimed. See [TEST_INFRA.md](TEST_INFRA.md) for reproducible commands and test files.
