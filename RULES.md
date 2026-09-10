# RULES.md — the Bonaparte constitution

> **Provenance.** Every source file in this repository cites `RULES.md §…`, but
> the file itself was never committed (verified against the full git history).
> This document reconstructs it from the citations, the crate contracts they
> point at, and the gate conventions in `TEST_INFRA.md` / `PROJECT.md`.
> Where a section number is cited in code, it is kept here verbatim.

## §1 — Architecture

- **§1.1 · Microkernel.** `bonaparte-model` owns state. `bonaparte-engine`
  renders. Nothing in the core reaches into another crate's internals;
  first-party effect packs obey the same public plugin contract as third-party
  ones (verified by a test that installs an extension with no private hooks).
- **§1.2 · Every workspace dependency needs a one-line reason.** Enforced by
  review against the comments in the root `Cargo.toml`.
- **§1.3 · The `Op` enum is the ONLY way project state changes.** Everything
  derives from it: undo/redo = stored inverses; MCP and the AI bridge send
  `Op`s; the AI-diff preview shows `Op`s before applying; the history list is
  rendered from their docs. Every variant implements `apply` + `invert`, and
  `apply; invert(apply(D)) == D` is gated per variant in the History tests.
- **§1.4 · Engine purity.** `bonaparte-engine` has no OS, filesystem, network
  or thread dependencies; decoded media pixels are *injected*. `cargo check
  -p bonaparte-engine --target wasm32-unknown-unknown` must pass clean.
- **§1.5 · One evaluation path.** Preview, playback and export render from the
  same validated snapshot; playback and the audio mixer share one evaluation
  rule, so what you hear and what you export are the same computation.

## §2 — Domain model

- **§2.1 · Time is `Time(i64)` ticks (120 000/s); frame rates are rational.**
  Float seconds are not allowed in the document. Integer ticks make 23.976 fps
  frame-exact (5005 ticks), so preview, export and MCP agree bit-for-bit.
- **§2.5 · Strongly-typed newtype IDs.** `CompId`, `LayerId`, `MediaId` are
  distinct types; they cannot be mixed up, and IDs are never silently coerced
  between domains or reused across a restore.
- **§2.6 · Typed errors per crate** (`thiserror`), with human-readable
  messages. Rejected operations answer with the reason **plus** a hint
  listing valid properties, value shapes and time units.
- **§2.7 · Transactions are atomic; the history itself spills to disk.** A
  commit validates before replacing the document; a rejected commit leaves
  the document byte-identical (pinned by the failure-atomicity tests). The
  hot window holds 1000 transactions in memory; every transaction pushed out
  of it appends to an append-only journal (`$BONAPARTE_HISTORY_DIR`, else
  `~/.bonaparte/history/`), so **the whole session stays undoable** — no
  ceiling on depth, only on memory. A journal that cannot be written
  degrades to window eviction, never to a failed edit. Opening or creating a
  project truncates the journals: history is per session. A Batch carries at
  most 8192 ops and cannot nest.

## §3 — Persistence

- Versioned, validated JSON. Unknown versions fail **before** replacing the
  session. Saves are atomic (temp file → rename). Projects stay portable:
  media embeds inline up to 16 MiB per source; larger sources become content-
  addressed Astra chunk extents (see §6). Embedded payloads share a 256 MiB
  document budget; whole project files a 512 MiB ceiling.
- **Video never bakes into the document.** Clips link by path
  (`MediaAsset::footage`): the file is the source, frames decode on demand at
  full framerate, and a half-resolution proxy — generated lazily, stored in
  the user's shelf — may serve interactive playback. Exports and PNG writes
  always read the original file; a moved clip goes `offline` and the relink
  button restores every trim because only the `footage` record changed (one
  undoable `Op`).

## §4 — Plugins and effects

- **§4.1 · Manifest-owned effects.** Types, defaults, ranges, groups and
  control kinds live in `manifest.toml` (+ WGSL for GPU programs). The engine
  never special-cases a pack by identity.
- **§4.2 · Manifest versions are checked.** A major version the host does not
  understand is refused with a human-readable message — never guessed at.
- **§7.2 · Every effect parameter carries a docstring**; manifest validation
  fails the build if one is missing.

## §5 — Rendering truth

- The CPU reference renderer is the reference: GPU output must match golden
  fixtures (executed comparisons, stated tolerances). Unsupported GPU kernels
  fall back to complete CPU frames **with a reason**, never silent
  substitution. Previews may trade resolution for latency; exports never do.

## §6 — Storage

- `Astra` (content-addressed, SHA-256, 4 MiB chunks, dedup, atomic publish)
  behind `Atra` (byte-budgeted hot cache). Projects carry hash extents;
  reading re-verifies digests. Missing chunks fail explicitly. This is what
  replaced the old 128 MB / 32 MiB walls: the workspace bound is disk, not
  heap.

## §7 — AI-native surface

- **§7 · The runtime exposes an immutable render snapshot**; the UI and MCP
  render from this, not from a mutable session.
- MCP and the HTTP bridge expose the same `Op` vocabulary. Every tool call is
  panic-contained; a crashing request never kills the editor. AI edits land in
  the user's undo history — the human can Ctrl+Z the machine.
- `POST /api/describe` is the authoritative, always-fresh capability map;
  prose docs are the human-readable mirror, never the source of truth.

## §8 — Honesty rule

- Docs and status files state what was measured, on what hardware, and what
  is explicitly **not** done. "100% complete" claims are retracted in favour
  of the acceptance tables in `PROJECT.md`.

## §9 — Gates (must all pass before a commit is "done")

1. `cargo fmt --all -- --check`
2. `cargo test --workspace --exclude bonaparte-app` — zero failures
3. `cargo check -p bonaparte-engine -p bonaparte-audio --target wasm32-unknown-unknown`
4. `cargo check -p bonaparte-runtime --no-default-features` (GPU-optional path)
5. `cd ui && npm run check` (svelte-check: zero errors **and** zero warnings)
6. `npm run build`
7. `npm run test:e2e` (real Rust, isolated ports, `BONAPARTE_API_PORT`/`VITE_PORT`)
8. `npm run test:audio-worklet`
- GPU tests are mandatory when an adapter exists: `BONAPARTE_REQUIRE_GPU_TESTS=1`.
- FFmpeg tests skip (not fake) when `ffmpeg`/`ffprobe` are absent from PATH.
