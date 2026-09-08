# Bonaparte — Implementation Record

What was built, where it lives, and how it was proven. Baseline first, then
one section per shipped update in commit order. Every section ends with the
gates it passed. Update 6 (Astra) is the current head: `ec4d002`.

Full commit chain (oldest → newest):
`…b6f23c9 → 013fa82 (v0.5.1 baseline) → 9b87ae3 → 24b2cc2 → cd79f61 → 3ed5675 → 923d44e → a2e36b0 (docs) → ec4d002 (Astra)`

---

## Baseline — v0.5.1 (`013fa82`)

Audio Studio, GPU preview, interaction proxies, and the editor stability
suite. Details live in `docs/STABILITY-0.5.1.md`, `docs/AUDIO.md`,
`docs/RESPONSIVENESS-AND-DSP.md`.

Key structure:

- `crates/engine` — render graph, tiles, typography, vector rasterizer,
  SVG/OBJ importers, cancel/trace plumbing.
- `crates/gpu` — wgpu layer; parity-tested against the CPU reference
  (`crates/effects/src/cpu_reference.rs`).
- `crates/audio` — mixing/processing with offline tests.
- `crates/runtime` — patch-based session (`src/patch.rs`), preview bridge,
  audio workflow (`src/audio.rs`).
- `crates/model` — document model, ops, keyframes; serde round-trip tests.
- `crates/mcp` — unified session + tools for the AI bridge.
- `ui/` — Svelte 5 studio; `store.svelte.ts` is the editor state machine.

---

## Update 1 — `9b87ae3` · File imports + 3D killer features (88 files, +13 093)

- **SVG import** (`crates/engine/src/import_svg.rs`): real path parsing
  (M/L/C/Q/A, transforms) → vector layers; no bitmap tracing.
- **OBJ import** (`crates/engine/src/import_obj.rs`): mesh parse → 3D layer
  with materials and normals.
- **Raster-to-vector** (`crates/engine/src/import_raster.rs`): posterized
  contour trace → editable vector paths.
- **3D camera system** (`crates/model/src/camera3d.rs`,
  `crates/engine/src/camera_depth.rs`): perspective camera keyframable in
  the model; depth pass feeds DoF.
- **Depth of field** (`crates/engine/src/depth_of_field.rs`): focal distance
  + aperture from camera; GPU blur by CoC.
- **3D lighting/shadows** (`crates/engine/src/lights.rs`): directional and
  point lights, shadow maps.
- **Viewport gizmos** (`ui/src/lib/components/Viewport.svelte`): drag-based
  move/rotate/scale proxies for 3D layers.
- **Vector raster optimization** (`crates/engine/src/vector.rs`): scanline
  tiling cache; parit fewer re-rasters on transforms.
- Tests: `crates/engine/tests/import.rs`, `camera_depth.rs`,
  `depth_of_field.rs`, `three_d_killer` coverage in `ui/tests/three_d_killer.spec.ts`.

Gates: fmt · workspace tests · wasm · svelte-check · Playwright.

## Update 2 — `24b2cc2` · Export overhaul (7 files, +1 026)

- **Parallel export pipeline** (`crates/engine/src/export.rs`): frame
  batches render concurrently; deterministic ordering.
- **Live progress + ETA** (`crates/runtime/src/export_runner.rs`,
  `ui/src/lib/export/*`): per-frame progress events stream to the UI;
  smoothed ETA.
- **Cancel support** (`crates/engine/src/cancel.rs`): cooperative
  cancellation token threaded through the graph.
- Tests: `crates/engine/tests/export.rs`,
  `ui/tests/performance.spec.ts` progress assertions.

Gates: full suite green.

## Update 3 — `cd79f61` · Scanline fix + Night Owl demo + product tour (31 files, +5 803)

- **Scanline artifact fix** (`crates/engine/src/vector.rs`): seam handling
  between tiles — rasterizer output is pixel-identical to untiled.
- **Export ETA hardening**: ETA ignores stalled samples; progress never
  regresses.
- **Night Owl demo project** (`assets/demos/night-owl.json`): ships with the
  studio; exercises video, audio, effects, 3D.
- **Guided product tour** (`ui/src/lib/tour.ts`): step-through overlay for
  first launch, skippable, remembers completion.
- Tests: rasterizer seam parity (`crates/engine/tests/trace.rs`).

Gates: full suite green.

## Update 4 — `3ed5675` · Timeline interaction fixes + Gremlin Dash (3 files, +836)

- **Infinite-scroll containment**: ResizeObserver on the panel element only;
  track width capped at 120 000 ticks; `.timeline-scroll{min-width:0;
  max-width:100%}`; wheel handler prefers |deltaX| over |deltaY|.
- **Audio right-click menus** (`ui/src/lib/components/Timeline.svelte`):
  clip → Split / Duplicate / Crossfade / Reverse / Normalize / Mute /
  Delete; track → Mute / Solo / Lock / Marker / New track / Import; lane →
  Marker / Import here / New track / New bus. Menus scope to
  `role="menu"` and set `editor.audioSelection` first.
- **`mod+[` / `mod+]` reorder** shortcuts ("[" moves the layer toward the
  top; the top row is the LAST of `layer_order`); documented in the
  shortcuts dialog.
- **Gremlin Dash** (`ui/src/lib/gremlin.ts`): platformer minigame on the
  export-runner gremlin — gravity 950, jump −330, coyote 0.09 s, input
  buffer 0.11 s, personal best in `bonaparte.runnerBest`, hook
  `__gremlinDash` for tests.
- Tests: `ui/tests/timeline_audit.spec.ts` (menus, split, solo).

Gates: full suite green.

## Update 5 — `923d44e` · Video import + auto-allocation + Atra hot tier (39 files, +3 516)

- **Video import** (`crates/engine/src/import_video.rs`): ffmpeg extracts
  poster + sampled frames (`timeMs` + RGBA base64, strictly ascending);
  contract `{compId,name,width,height,fpsNum,fpsDen,durationTicks,
  posterRgbaBase64,frames:[…]}`, applied atomically. Engine playback via
  `VideoFrames::sample_at(ms)` (mod duration, binary search) in
  `crates/media/src/video.rs`.
- **Auto-allocation engine** (`ui/src/lib/media/autoAlloc.ts`): video
  sampleCount = min(32, max(8, round(dur·3))) with 45 s / 120 s steps down
  to 16 / 12; sampleWidth = min(w, 480), 320 when > 90 s or
  `deviceMemory ≤ 2`; audio chunkSeconds 0 / 10 / 20.
- **Asset reuse** (`ui/src/lib/store.svelte.ts` `addMediaToComp`): audio
  drops as a clip, image/video as a fitted Footage layer at the playhead —
  re-imports reuse the existing asset instead of duplicating.
- **Atra hot tier** (`crates/media/src/disk_cache.rs` + vendored
  `crates/atra`): 16-byte hot key = media id u64 LE ‖ time i64 LE; hot
  value = w u32 ‖ h u32 ‖ RGBA; 96 MiB budget; `hot_stats()` → `HotStats`.
  Atra (SamarDB/Meridian lineage, owned and vendored) gives sharded
  in-memory probes with byte-budgeted eviction.
- **Timeline containment hardening** in `ui/src/lib/components/Timeline.svelte`.
- Tests: 30 atra unit tests; `ui/tests/media_flow.spec.ts` (video import
  auto-creates a footage layer; reuse poll expects +1 more).

Gates: fmt · workspace 346/346 · wasm · svelte-check 0 · Playwright 90/90.

## Docs — `a2e36b0` · `docs/WHATS-NEW-0.6.md`

Ten-section release notes covering updates 4–6 scope: timeline fixes,
Gremlin Dash, containment, audio menus, video import, autoAlloc, reuse,
limits, shortcuts, Atra.

## Update 6 — `ec4d002` · Astra: dedicated storage engine (16 files, +629)

The directive: Atra earns its keep where it has impact; nothing stays a
stub; the 128 MB UI / 32 MiB engine walls are unacceptable — workspace
projects will grow ~1000×. Answer: **Astra**, a content-addressed storage
engine (`crates/astra`) with Atra as its hot block cache.

- **Chunk store** (`crates/astra/src/store.rs`): 4 MiB chunks, layout
  `chunks/<2hex>/<sha256>`. `put_chunk` dedups by digest and publishes
  atomically (write `.tmp{pid}` → rename). `put_stream(impl Read)` /
  `put_bytes` → `PutReport{chunks, logical_bytes, deduped_bytes}`.
  `read_chunk(hash)` probes Atra first, falls back to disk, repopulates the
  hot tier on miss. `extent_len`, `verify(chunks)`, `gc(keep)` (skips
  `.tmp`), `stats()` → `AstraStats`, `flush_cache`. Engine config:
  `EngineOptions{shard_hint:None, total_entries:4096, cores:None,
  memory_bytes:64 MiB, min_buckets:1024}`. `Store::global()` is a
  pid-scoped tempdir behind a `OnceLock`; `Store::open(dir)` for tests.
- **Model contract** (`crates/model/src/audio.rs`):
  `EmbeddedAudio.astra_chunks: Option<Arc<[String]>>` — serde-defaulted,
  skipped when `None`. `Some` ⇒ `data_base64` is empty and the source is
  the concatenation of the store chunks in order. Old projects load
  unchanged; small saves gain no new key.
- **Decode routing** (`crates/media/src/audio.rs`): sources ≤ 16 MiB
  (`EMBED_INLINE_LIMIT`) embed inline as before — single-file portability
  stays the default. Bigger sources `put_bytes` into Astra and the project
  carries only the hash extent. `decode_embedded` reassembles extents
  through the hot cache and re-verifies the SHA-256 against the recorded
  hash (missing chunk ⇒ explicit error). Sanity bound is now 2 GiB.
- **Import wall removed** (`crates/runtime/src/audio.rs` `prepare_import`):
  no more 32 MiB rejection — same bounds as decode.
- **UI cap removed** (`ui/src/lib/audio/actions.ts`): the 128 MB import
  rejection is gone; `AudioAsset.astra_chunks` added to the TS model.
- Tests: 5 engine tests (roundtrip+integrity, multi-chunk stream, dedup,
  GC reclaim, hot-cache hit ratio); 2 serde-compat tests; 2 runtime
  end-to-end tests — a 95-second (~17.6 MiB) WAV imports with an empty
  `data_base64`, ≥4 chunks, verifies, and decodes from the store, while a
  small WAV stays fully inline.

Gates: fmt · workspace **355/355** (+9) · wasm · svelte-check **0** ·
Playwright **90/90**. Bundle `bonaparte-backup.bundle` @ `ec4d002`;
`bonaparte-both-updates.patch` = `013fa82→ec4d002` (71 677 lines),
fresh-clone `git am` verified tree-identical.

---

## Update 7 — `6a94fd5` · Beat Cut ⚡: one-click music-video editing (16 files, +629)

The Premiere jaw-dropper: drop a music track, right-click it, **Detect
beats** — then **Cut video to beat** and watch the timeline edit itself.

- **Detection** (`crates/audio/src/beats.rs`, pure DSP, no deps, no ML):
  RMS onset envelope (`DecodedAudio::onset_envelope` reads the decode-cache
  mmap at 200 buckets/s) → adaptive-threshold onset picking (200 ms rolling
  mean + variance floor, 100 ms refractory) → autocorrelation tempo over
  60–190 BPM with **octave folding** (a perfectly periodic train scores
  equally at the measure level; the half-lag wins whenever it scores ≥ 85%)
  → phase-locked grid (slide one period, keep the offset with the most
  onset energy) → meter anchored at the earliest strongest onset, bars
  tiled in 4/4 from it.
- **Persistence**: `Op::SetMediaBeatGrid` stores the grid rotated so index 0
  is always a downbeat (position alone reads the meter); markers named
  `beat-*` paint the arrangement ruler for free.
- **Remix cut**: one Footage clip per beat span (≤128, ≥100 ms), source
  windows walking the footage on a golden-ratio sequence — real jump cuts —
  with downbeat clips scaled 108%. The original layer stays untouched
  underneath; undo is one step.
- **Pulse cut**: no cuts — `Property::Scale` keyframes pop the layer on
  every beat (104; 112 on downbeats) with a 70 ms Bezier snap-back.
- **Enabler**: `LayerKind::Footage` gains `source_start` (serde-defaulted)
  and media time became layer-local + offset (reference.rs sampling,
  preview.rs) — clips now start at their own beginning, consistent with
  PreComp. Old projects load unchanged.
- Tests: 3 detection tests (120 BPM + downbeat spacing, 90 BPM,
  non-rhythmic/too-short rejection); 3 runtime e2e tests — detect → cut →
  **jump-cut offsets proven to differ**, downbeat pops counted, pulse track
  keys asserted, single-step undo; `ui/tests/beat_cut.spec.ts` drives the
  full browser flow (drop video + click track → clip menu → detect → grid
  + markers in state → cut → ≥4 Beat layers with differing `source_start`
  → engine still renders PNG).

Gates: fmt · workspace **361/361** (+6) · wasm · svelte-check **0** ·
Playwright **91/91**.

---

## Update 8 — `378d56c` · Kinetic ⚡: the lyric-video subsystem (10 files, +530)

The Premiere/AE jaw-dropper: right-click the music → **Lyric video ⚡** →
type the lines → **Generate**. Words land on the beat with keyframed
motion; synthesized impacts hit the downbeats; one undo removes it all.

- **kinetic_lyrics** (`crates/runtime/src/lib.rs`): one text layer per
  word. Phrases quantize to the beat grid (comp-visible beats only — a
  grid longer than the comp no longer eats lines); words stagger inside
  the phrase; three motion styles — **pop** (8 → peak 112, downbeats 120,
  settle 100, Bezier snap), **rise** (glide up + fade in), **wave** (words
  bob across the line). Layer ids are deterministic within the batch, so
  word keyframes ride the same transaction — one undo, guaranteed.
- **sound_design** + `crates/audio/src/fx.rs`: procedural Foley —
  **impact** (95→42 Hz sweeping body + 8 ms noise transient, tanh-soft),
  **whoosh** (rising one-pole noise, bell envelope), **riser** (band-shaped
  noise building to a hard stop) — seeded xorshift, deterministic at the
  engine's 48 kHz, 4 unit tests. The synthesized WAV rides the normal
  decode pipeline into a real asset, dropped on its own **"Sound design ⚡"
  lane** with a clip per downbeat. No sample packs, no downloads.
- **Batch cap 256 → 2048**: Kinetic generates ~6 ops per word; the
  atomicity proof in `foundation.rs` now pins 2049-op rejection.
- **Domain fix found en route**: beat ruler markers and sound-design clips
  live in the FRAMES domain (48 kHz) — markers had been sitting 2.5× too
  far right; they now land exactly on the audio ruler.
- **UI**: "Lyric video ⚡" in the audio clip menu opens the dialog (lines
  textarea, motion style select, sync + auto-foley toggles); Generate runs
  both commands and toasts the result.
- Tests: 4 runtime e2e (grid layout + stagger + one-undo; even-spread
  fallback without a grid; downbeat lane + synth asset + undo; helpful
  error without a grid); 4 fx unit tests; `ui/tests/kinetic.spec.ts`
  drives the browser flow: detect → generate → 8 word layers with 3-key
  scale tracks → impact lane with clips → engine PNG render → double undo.

Gates: fmt · workspace **369/369** (+8) · wasm · svelte-check **0** ·
Playwright **92/92**.

---

## Update 9 — `852b3a9` · Vision-Native MCP + muscle memory + assembled SVG + Kaya ⚡

Four fronts, one thesis: the editor gives AI eyes and ears, and gives
humans the reflexes they already own.

**Vision-Native MCP** — the AI selling point:
- `frame.view` — renders the composition and returns real PNGs inline
  (MCP image blocks): a vision-native model LOOKS at the timeline.
  Storyboard mode (`count` frames over a range), width downscale for
  token economy. 3 tests: inline PNG magic, storyboard spacing,
  meta shape.
- `audio.extract` — renders the actual mix to an inline WAV (MCP audio
  block) + loudness summary (peak/rms dB, rate, channels): an
  audio-native model HEARS it. Mono mixdown by default.
- `crates/mcp/src/protocol.rs` gains image/audio content blocks
  (`mimeType` + base64 `data`) beside text.

**Muscle memory**: Shift/Ctrl/⌘-click toggles rows into a
multi-selection overlay (`multiSelected` + `selectLayerAdvanced`), mod+A
grabs the whole stack, Escape clears (guarded during gestures — the
interaction proxies keep Escape), Delete/Duplicate drive every selected
layer, rows light up. Playwright: shift-click both → delete leaves one;
Escape → delete is a no-op.

**Assembled SVG**: imports land ONE PreComp group (poster-size docs fold
into a 4000 px-capped sub-comp with an inner scale, so the 16 MP comp
limit never trips). Right-click → **Disassemble into layers** composes
the group transform onto each child (position rotates/scales with the
group, scale/rotation/opacity multiply, tracks shift by the group's
start), restores the exact stacking slot, removes the sub-comp — one
undo regroups. Old exploded-import tests updated to the assembled
contract; 3 new runtime tests.

**Kaya ⚡** — first plugin on the new plugin API:
- `crates/runtime/src/kaya.rs`: plugin manifests (id, capabilities,
  settings with `secret` kind) are the loader's contract; `plugins.list`
  exposes them; keys live client-side and ride per-call — never in the
  project file.
- The Kaya line format, exactly as specced: `Hi~~1:20+2~~` — word, start
  m:ss, duration seconds. `annotate` + `parse` round-trip.
- `kaya.analyze` — offline scene sense: silence spans (envelope floor)
  + tempo/downbeats (the Beat Cut engine), zero keys.
- `kaya.transcribe` — OpenAI Whisper (`timestamp_granularities[]=word`,
  16 kHz mono multipart) → `KayaWord[]` persisted on the asset via the
  `SetMediaTranscript` op. Kinetic/captions/AI can read what was said
  when.
- `narrator.speak` — Sarvam AI Bulbul or ElevenLabs (BYO keys, ureq on
  rustls, wasm-gated) → decoded, placed at the requested second, word
  plan attached. HTTP lives behind `cfg(not(wasm32))` so the wasm gate
  stays clean.
- Kaya dialog in the audio clip menu; keys persisted to localStorage.

Gates: fmt · workspace **387/387** (+18) · wasm · svelte-check **0** ·
Playwright **95/95**.

## Update 10 — `2d2a82f` · The Vault + first-class Lottie + desktop feel

Three threads: a shared asset brain, a real animation interchange format,
and the feel of a native desktop app.

**The Vault** — one global shelf for every project (human *and* AI agents
use the exact same commands):
- `crates/runtime/src/vault.rs`: fixed folders `logos / images / audio /
  video / lottie / fonts / effects`; root is `$BONAPARTE_VAULT` else
  `~/.bonaparte/vault`; `list` (newest-first, bytes + mtime, dotfiles
  skipped), `save` (2 GiB cap), `read` (256 MiB cap). Names are the final
  path component, sanitized to `[A-Za-z0-9._ ()+- ]` — `../escape/name.svg`
  lands as a plain `name.svg`; folders are whitelisted, so nothing escapes
  the shelf.
- Commands `vault_list` / `vault_save` / `vault_read`; the UI gets a
  Vault button on the Assets header: folder-grouped listing, per-folder
  upload, click a file and it flows through the normal import pipeline
  (assembled-import rules apply). Drop anything in once, use it in every
  project.

**Lottie (Bodymovin) import — "Larry" is first-class**:
- `crates/engine/src/import_lottie.rs` parses Bodymovin v5 JSON: shapes
  (ellipse / rect / path → native geometry, fill + stroke styles), solids
  from hex, and **animated** position / scale / rotation / opacity →
  native keyframe tracks with real Bezier easing (`o`/`i` control points,
  x clamped to [0, 1], y to [-0.5, 1.5]), positions center-relative, each
  keyframe carrying its own segment-start value. Precomps (ty 0), images
  (2), nulls (3) and text (5) are skipped and reported by kind.
- The `import_lottie` command assembles the result like SVG: one
  PreComp group in the parent (fit-scaled, oversized docs folded under a
  4000-unit inner cap), while the generated sub-comp keeps **Lottie's own
  timeline** — clamping it to the parent used to silently drop trailing
  keyframes. Explicit disassemble explodes it; one undo regroups.
- `.lottie` / `.json` sniffing on drop and file open. Reply carries
  `{layers, skipped, skippedKinds}`.

**Desktop feel**:
- Silent save: `⌘/Ctrl+S` writes straight to the last path — no OS dialog
  once one exists; `⇧⌘S` / menu = Save As; a 60-second autosave sweeps up
  unsaved work.
- Touchpad: pinch (`ctrl/⌘ + wheel`) zooms anchored at the cursor
  (10–800 %, free of the fixed ladder — the zoom readout shows arbitrary
  values), two-finger scroll pans.
- Bonaparte's own **circular HSV color wheel** (`ColorWheel.svelte`)
  replaces the native browser color input in every color field: hue ring +
  saturation/value square + alpha slider, full pointer-captured dragging;
  the square's corners win hit-testing over the ring band (they overlap).

Gates: fmt · workspace **395/395** (+8) · wasm · svelte-check **0** ·
Playwright **99/99** (+4).

---

---

## What "storage" means now

| Tier | Mechanism | Holds |
| --- | --- | --- |
| Project file | inline base64 | audio ≤ 16 MiB (portable default) |
| Astra store | content-addressed 4 MiB chunks on disk | audio > 16 MiB, hash extents in the project |
| Atra hot cache | 64 MiB sharded in-memory (store) / 96 MiB (media frames) | chunk + decoded-frame reads |

The 128 MB and 32 MiB class limits are gone; the remaining 2 GiB bound is
per-source sanity, not a storage wall — Astra's chunking scales with disk,
and extents dedup so re-imports of the same source cost nothing.
