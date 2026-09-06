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

The browser tests set initial fixtures through the real API, then drive the visible controls. No document backend or pixels are mocked, and no JS render substitute is used. One additional race test delays a genuine Rust response to verify that stale preview results cannot overwrite a newer selection. Assertions inspect Rust state, canvas pixels, downloaded project/PNG files and `ffprobe` output. A separate test opens the editable Orbit project and captures all three workspaces.

## Regression coverage

| Tests | Evidence |
|---|---|
| `model/tests/foundation.rs` | Ordered effects/tracks and exact undo; duplicate IDs; failing Batch atomicity; layer retiming; overflow/transaction limit; complete 256-operation inverses; safe ID allocators; parent cycles; legacy defaults; nonfinite/unsorted data |
| `effects/tests/color_grading.rs` | Neutral byte identity; linear exposure; temperature/saturation; alpha; manifest type/range checks; animated parameters; premultiplied blur; a third-party CPU plugin |
| `effects/tests/shader_validation.rs` | Naga WGSL syntax/type validation of every bundled pack, **not GPU rendering** |
| `engine/tests/effect_compositing.rs` | Ordered/bypassed effects; adjustment ordering and premultiplied opacity; effect-aware tile equality; external registry; text rendering/limits; alpha encoding and bilinear sampling |
| `runtime/tests/editor_workflow.rs` | Real example rendering; versioned files; embedded image import/render/undo; malformed data atomicity; transient previews; binary protocol; linear black matte; exact NTSC MP4 frame count; failed export preserves existing file |
| `mcp/tests/foundation.rs` | Catalog discovery; shared effect persistence/mutation; invalid proposals and explicitly missing composition requests |
| `engine/tests/preview_scene.rs` | Full-grid byte equality against the independent reference; unchanged coordinates at reduced grids; pixel-unit scaling; source cache bounds |
| `gpu/tests/parity.rs` | Real graphics commands/readback, all blends, ten opted-in shader programs, nested/colored text/resolutions, upload reuse and reflected third-party uniforms |
| `runtime/tests/preview_workflow.rs` | Frame cache identity/invalidation/epochs, transient overrides, protocol metadata, full-size exports and explicit CPU/GPU fallback |
| `ui/tests/performance.spec.ts` | Preview quality/backend controls, cache clearing, scaled gestures, Auto playback quality, genuine GPU/fallback labels, full-size PNG and delayed-response safety |
| `ui/tests/editor.spec.ts` | Composition create/settings/undo; text/shape edits; canvas gestures/lock/duplicate; grade pixels/bypass/order/keys; image/save/open/errors/exports; motion/key dragging/easing/playback; three workspaces |

The pre-existing arithmetic, blend, affine, layer, tile, effect, Op round-trip, media/cache and MCP suites are retained. Old test names that mention a historical count are not a coverage measurement.

## Exclusions and next tests

Native WebView/dialog interactions, Windows/macOS behavior, physical-hardware GPU validation, long-running memory stability, power-loss durability, complex-script typography and sustained interactive performance are **not covered**. The current tests do not substantiate the former “100% feature complete” or “forensic audit clean” claims.

Playwright artifacts are generated under `ui/test-results`; dependency/build/artifact directories are not part of the source deliverable.

## GPU and CPU-only checks

On Linux, install `mesa-vulkan-drivers` and `libvulkan1` for software Vulkan validation. `BONAPARTE_REQUIRE_GPU_TESTS=1 cargo test -p bonaparte-gpu` fails rather than skips when no adapter exists. The recorded run used this requirement and llvmpipe. Test CPU-only fallback separately with `cargo test -p bonaparte-runtime --no-default-features --test preview_workflow`. See `docs/RENDERING.md` for the executed GPU subset and tolerance policy.

## Audio Studio v0.4

- `crates/audio/tests/mixing.rs`: sample timing, panning, overlaps, chunk/seek equivalence, varispeed split/fades, reverse, mute/solo, overload reporting, nested semantics, anti-alias filtering and repeated-DAG validation.
- `crates/runtime/tests/audio_workflow.rs`: native codec import, waveform/PCM/WAV data, portable persistence, atomic failures, source range/ID handling and NTSC MP4 with AAC.
- `ui/tests/audio.spec.ts`: six real-browser audio workflows on the isolated service.
- `npm run test:audio-worklet`: five device-queue unit tests; these are excluded from Playwright discovery with `testMatch: **/*.spec.ts`.
- Avoid concurrently running heavy native/WASM compilation and Chromium in a 2 GiB sandbox. An earlier concurrent check exceeded the tool timeout under memory pressure; its temporary child processes were explicitly stopped, then checks were rerun separately. No user project sessions were stopped for that cleanup.

Audio project files write version 3 and retain original encoded sources. The browser mirror omits audio blobs; recovery fetches a full portable document only after idle time. See `docs/AUDIO.md` for measured playback/export behavior and explicit limits.

## v0.5 responsiveness and DSP

- `crates/runtime/tests/interaction.rs`: authoritative deltas, revision fallback, independent audio revisions, transient transforms and native interaction-plane packets.
- `crates/audio/tests/processing.rs`: neutral EQ, measured filter response, linked compression, compensated limiter, bus routing, feedback rejection and chunk/seek equivalence.
- Extra audio workflow tests cover processed WAV/packet parity and bounded distant-seek preparation.
- `ui/tests/interaction.spec.ts`: real pointer drag/scale/cancel against native planes, one-gesture history, delayed renderer responses and an isolated large-project measurement.
- Added audio browser cases exercise EQ/compressor/limiter/bus edits and audio hot-swaps. Worklet tests include atomic transition behavior.
- The measured v0.4 baseline uses the archived source/executable, not estimated timings. Duplicate preview services were retired only after saving their documents; active editing sessions were not used for destructive tests.

## v0.5.1 editing-stability regressions

`ui/tests/stability.spec.ts` exercises focused live text/position/color/effect edits, native picker events, delayed acknowledgements, undo grouping, visibility races, anchor-aware rotation/scaling, parent transforms, explicit Full quality, timeline view reset, duration growth and 30-second blank-project defaults. `crates/model/tests/live_edits.rs` verifies safe coalesced history; `crates/engine/tests/raster_quality.rs` verifies output-aware raster density and shared preview/export quality. These regressions are tied to reproduced behavior, not just test-count growth.
