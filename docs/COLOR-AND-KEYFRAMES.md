# Color & keyframes — deep-color milestone

## What "not limited to 8-bit sRGB" means here (honestly)

The editing canvas and live preview remain 8-bit sRGB — that is a deliberate
performance boundary, not a format ceiling. Everything *downstream of the
canvas* can now exceed it:

| Stage | Before | Now |
|---|---|---|
| Preview canvas | 8-bit sRGB | unchanged (performance boundary) |
| Grade math | f32 linear internally, one 8-bit store | unchanged core, +new wheels |
| PNG export | 8-bit sRGB only | **8 or 16-bit**, four output spaces |
| Effect parameters | — | **animated like any property**, still |

### Output spaces (`RenderRequest.outputSpace`)

- `srgb` — default, byte-compatible with every existing export.
- `display-p3` — wide-gamut primaries (D65, Bradford-adapted); sRGB transfer.
- `rec2020` — ultra-wide-gamut masters; sRGB transfer.
- `linear` — the transfer function removed entirely; for downstream
  compositing/grading pipelines that want scene-referred data.

Conversion is real color science, not a tag: pixels are linearized, pushed
through 3×3 primary matrices (round-trip identity is unit-tested to 1e-3),
re-encoded, and quantized to the requested depth. Wide-gamut spaces stay
PNG-tagged correctly (no sRGB chunk), and 16-bit files carry genuinely finer
steps — a dark gradient that quantizes to one 8-bit level keeps ~256 levels.

### API surface

`export_png` (bridge POST) and `RenderRequest` gained optional
`bitDepth: 8 | 16` and `outputSpace` fields. Absent fields default exactly as
before, so every existing caller — desktop Tauri, MCP, old saved scripts — is
untouched. The MCP export path pins 8-bit sRGB until its tools grow options.

### What is still 8-bit (milestone 2 candidates)

Layer rasters and the blend accumulator quantize at 8 bits; a full
`FloatFrame` master (float end-to-end through precomps and blends) is the
next structural step and slots in behind `render_comp` without API churn.
HDR transfer functions (PQ/HL) need a float master first — shipping them
over an 8-bit pipeline would be a lie, so they are deliberately absent.

## Grading v2 — the new wheels

`builtin.color_grade` grew from 12 to 19 parameters, all manifest-driven
(they appear in the UI automatically, are animatable, and the GPU mirror is
parity-tested against the CPU reference on nontrivial RGBA):

- **Lift** (±0.25) — raise/crush the floor after contrast.
- **Gain** (0.5–1.5×) — multiplicative gain in linear light.
- **Filmic rolloff** (0–1) — ACES-fitted tonemap blend; super-whites curve
  instead of hard-clipping.
- **Split toning** — shadow hue, highlight hue, balance (shifts the
  crossover), strength. Hue table matches the WGSL exactly.

Two invariants are test-enforced: an all-defaults grade is byte-identical to
the input (old projects render pixel-for-pixel the same), and gain/filmic
apply even when exposure and white balance are neutral (the linear-stage
fast path only skips when *every* stage input is identity — a regression
test pins this).

## Keyframes — Hold, Anticipate, bulk apply

- `Easing::Hold` — a first-class step: the outgoing value persists for the
  whole segment and jumps exactly at the next key. Modeled in Rust and the
  TS interaction proxy identically; serde format is the external tag
  `"Hold"`, fully backward-compatible.
- **Anticipate** preset — a y-negative cubic bézier dip before motion.
- **"Apply this curve to all N segments"** — one batched, single-undo
  operation that stamps the active key's easing onto every segment.
- Validation tightened: bézier handles must be finite and time-handles in
  [0,1] (existing rule), and NaN handles now fail project validation
  deterministically (new no-mercy coverage).

## Test map

- `crates/runtime/src/color.rs` — matrix round trips, red-through-every-space,
  16-bit precision vs 8-bit, neutral-gray invariance.
- `crates/runtime/tests/color_depth.rs` — end-to-end exports through
  `render_input`: depth in decoded IHDR, wide-gamut green terms, gradient
  depth comparison, invalid-depth/space refusal.
- `crates/effects/tests/color_grading.rs` — grade v2 wheels incl. the
  fast-path regression.
- `crates/gpu/tests/parity.rs` — all 19 grade params CPU↔GPU.
- `crates/model/tests/no_mercy.rs` — Hold step semantics, serde round trip,
  NaN-handle validation.
- `ui/tests/color_depth.spec.ts` — dialog → real downloaded deep PNG; Hold +
  bulk apply through the graph editor with one-step undo; new wheels change
  real canvas pixels.
