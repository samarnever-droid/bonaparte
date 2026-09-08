# What's New — v0.6 (UI fixes, video import, auto-allocation, Atra)

Everything added in the latest two update rounds, what it does, where it
lives, and how it is tested. Apply order and patch files are in
`/HOW-TO-APPLY.md` next to the repo.

---

## 1. Timeline interaction fixes

**Problem reported:** hiding a layer made its row un-clickable ("visibility
toggle reverses"), rapid clicking the eye raced the label, layer reorder and
bar drags seemed dead, trackless layers had a disabled expand button.

| Fix | Detail |
| --- | --- |
| Row visibility class | The label row used `class:invisible`, which collided with Tailwind's global `visibility: hidden` utility — the whole row, eye button included, became un-clickable after hiding. Renamed to `.dimmed` (`ui/src/lib/components/Timeline.svelte`). |
| Stable toggle labels | Eye/lock buttons now use stable `aria-label`s (`Toggle visibility of X`, `Toggle lock on X`) with `aria-pressed` carrying state, so rapid clicking can never race a relabeling target. |
| Drag calibration | Extending `viewDuration` mid-drag resized the ruler, remapping the cursor and cancelling the drag (delta went negative). The px-per-tick mapping is now frozen at gesture start (`dragCal`) for layer drags and ruler scrubs. |
| Expand chevron | No longer disabled for layers without tracks; expanding shows a helpful "press ◇ to animate" empty row. `aria-expanded` reflects state. |
| Tactile polish | `grab`/`grabbing` cursors + lift shadow on layer bars, hover tints and transitions on eye/lock/name controls. |

**Test:** `ui/tests/timeline_audit.spec.ts` — 10 tests, every assertion
cross-checked against the Rust engine via `/api/state` (eye/lock/undo/
triple-click/reorder/drag/trim/scrub/expand/selection).

---

## 2. Gremlin Dash — the export runner is now a game

The export dialog's runner character is playable while your export renders:
**SPACE / click** to jump the frame gremlins (dropped-frame imps), grab stars,
build a streak — at a 5+ streak you get the fever cape and a ×2 multiplier.
Crashes cost your streak (with brief invincibility). Coyote time and jump
buffering make it feel fair. Score persists as a personal best; finishing the
export shows a rank card (C DAILIES → S DIRECTOR'S CUT). World speed follows
the real export fps, and reduced-motion users still get a static frame.

**Where:** `ui/src/lib/components/ExportRunner.svelte` (self-contained
canvas game; never touches the export pipeline — telemetry read-only).

---

## 3. Infinite-scroll containment (critical)

**Problem reported:** dragging a timeline bar — or shrinking the timeline
panel — made the page scroll "infinitely" and bricked the whole UI.

**Root cause:** a genuine exponential feedback loop. The timeline observed
its own scroller's width to size the track; once `viewDuration` exceeded
`fitDuration`, content → width → content compounded (measured: 1440 px →
33.5 million px and climbing).

**Fix:** the `ResizeObserver` now watches the timeline *panel* (viewport
sized), never the scroller; `trackWidth` has a hard 120 000 px cap; the
timeline region gets `min-width: 0; overflow: hidden`; the scroller is
`max-width: 100%`. Verified: drag 250 px past the comp end + shrink the
panel → page stays exactly 1440 px.

**Bonus:** a plain mouse wheel now scrolls the timeline horizontally.

---

## 4. Audio right-click menus

**Problem reported:** right-click in the audio editor did nothing — there
were no menus at all.

- **Clip menu** — Split at playhead · Duplicate · Crossfade with neighbor ·
  Reverse · Normalize peaks · Mute/Unmute · Delete.
- **Track header menu** — Mute · Solo · Lock · Add marker at playhead ·
  New audio track · Import audio file.
- **Lane empty-space menu** — Add marker · Import audio here · New track ·
  New mix bus.

Every action selects the clip/track first, so the flow is
right-click → click → done. Split-at-playhead is verified end-to-end
against the engine (1 clip → 2 clips).

**Where:** `ui/src/lib/components/AudioTimeline.svelte`
(`clipMenu` / `trackMenu` / `laneMenu`).

---

## 5. Video import

**Problem reported:** "I can't even import a video."

Drop or browse an **MP4 / WebM / MOV** and it just works:

1. The **browser's own decoder** reads the file (no codec code in the app).
2. Keyframes are sampled on a canvas; the **auto-allocation engine** (§6)
   decides how many frames at what resolution the project carries.
3. The engine got a sampled-video model (`EmbeddedVideo` in
   `crates/model/src/document.rs`) and time-aware playback
   (`DecodedImages::videos` in `crates/runtime/src/lib.rs`) — footage layers
   render **the right sample for the playhead**, modulo clip duration.
4. The poster frame doubles as the library thumbnail, so the asset also
   works as a still image anywhere.

**Where:** `importVideo` in `ui/src/lib/store.svelte.ts`,
`import_video` command in `crates/runtime/src/lib.rs`.

**Test:** `ui/tests/media_flow.spec.ts` drops a generated WebM, asserts the
engine holds sampled frames, a footage layer exists, the asset can be reused,
and the engine exports a real PNG of the video layer.

---

## 6. Auto-allocation engine (zero-prompt imports)

**Requested:** "a very fast auto-registration and auto-reason engine that
decides from the user specs what to allocate and how much."

`ui/src/lib/media/autoAlloc.ts` reads each asset's real specs — dimensions,
duration, bytes, plus device memory — and decides instantly, with no dialogs:

- **Video:** sample count (8–32, density by duration), sample width
  (downscaled harder for long sources or ≤2 GB devices), and the resulting
  in-project byte budget.
- **Audio:** files ≤45 s load whole; longer files stream in 10 s or 20 s
  chunks automatically.
- **Images:** byte budget and a 4K preview downscale note for huge sources.

The user sees one line summarizing the decision, e.g.
*"3.0s video → 9 keyframes @320px (≈3 MB in project)"*. Every import path
(video, audio, image) routes through this planner.

---

## 7. Asset reuse

**Problem reported:** "I can't reuse assets — that's important."

Every row in the sidebar **Assets** library is now alive:

- Click the row (or the + button) to add the asset to the active composition —
  images/videos become footage layers (auto-fit to comp, placed at the
  playhead), audio becomes a clip on a track.
- Videos get a film icon; dimensions show on the row.
- The drop zone now says what's really accepted: MP4 · PNG · JPG · WEBP ·
  SVG · OBJ · WAV.

**Where:** `addMediaToComp` in `ui/src/lib/store.svelte.ts`,
asset rows in `ui/src/lib/components/Sidebar.svelte`.

---

## 8. Import limits, honestly

| Asset | Before | Now |
| --- | --- | --- |
| Audio | 32 MB hard wall | **128 MB**, with an honest message (the project stays portable either way) |
| Video | not importable at all | importable, sampled per the auto-allocation plan |
| Images / SVG / OBJ | 20 / 8 / 32 MB | unchanged (sampled-video + poster keep projects small) |

---

## 9. Shortcuts

Added the missing layer-reorder shortcuts and documented everything in the
in-app sheet (Help → Shortcuts, or `?`):

| Keys | Action |
| --- | --- |
| `Ctrl/⌘ + [` / `]` | Move selected layer up / down |
| `Space` | Play / pause |
| `← / →` (+Shift) | Step 1 (10) frames |
| `Home / End` | First / last frame |
| `Ctrl/⌘ + D` | Duplicate layer (audio: duplicate / Shift = split) |
| `Ctrl/⌘ + Z` / `+Shift+Z` | Undo / redo |
| `K` | Keyframe the graph property or position |
| `V / H / R / O` | Select / hand / rotate / orbit tools |
| `1 / 2 / 3` | Design / Color / Animate workspace |
| Right-click | Context menus everywhere — timeline, audio, assets |

**Where:** keyboard map in `ui/src/App.svelte` (`keyboard()`), sheet in
`ui/src/lib/components/Dialogs.svelte`.

---

## 10. Atra — the cache engine (Meridian, vendored)

The owner's own **Meridian** engine core (from their SamarDB repo,
Apache-2.0, self-owned — no attribution required) is vendored at
**`crates/atra`** and integrated as Bonaparte's media frame cache.

- **What came over:** the sharded, seqlock-read cache engine — bounded
  probe-window reads with no locks or RMW on the read path, one writer mutex
  per shard, epoch-based reclamation, a TTL timing wheel, byte-budgeted
  eviction, and `sweep()` as the single bounded maintenance entry point.
  Upstream's own unit tests ride along (30 green).
- **What was pruned:** everything database-shaped (pgwire, CDC, CRDT, vector
  index, compute VM, cluster mesh, AOF, security) — the core is **std-only,
  zero new dependencies**.
- **Integration:** `DiskPlaybackCache` (crates/media/src/cache.rs) now has a
  **tier-0 hot cache** backed by Atra, between the LRU RAM slot tier and the
  disk tier: a 96 MB byte budget holding hundreds of sampled-video frames —
  far beyond the slot count — so timeline scrubs avoid disk I/O entirely.
  The architecture's flat-RAM slot guarantee is unchanged; `hot_stats()`
  exposes items / hits / hit-ratio / evictions for observability.
- **Tests:** engine round-trip, byte-budget eviction, TTL expiry, flush;
  hot tier serves frames beyond slot capacity; `clear()` empties it.

---

## 11. Gates (all green at `923d44e` + doc commit)

`cargo fmt` clean · **workspace 346/346** (up from 314) · wasm target check
clean · `svelte-check` 0 errors / 0 warnings · **Playwright 90/90** —
including the two new specs that lock this work in:

- `ui/tests/timeline_audit.spec.ts` — 10 engine-cross-checked timeline tests
- `ui/tests/media_flow.spec.ts` — infinite-scroll containment, video
  import→render→reuse, audio right-click→split, track menus
