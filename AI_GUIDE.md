# Bonaparte — AI Agent Guide

How to drive the Bonaparte motion-graphics editor programmatically. The
editor is **live-session unified**: anything you change through the bridge
or MCP lands in the user's open editor (undo history included) and appears
in the UI within ~1.2s.

## Golden rule

Call **`POST /api/describe`** (or the MCP tool `editor.describe`) first. It
returns the complete, current map: units, every op with a copy-paste
example, all editor commands, every effect with typed parameters, and a
summary of the user's live project. This document is the human-readable
version; `describe` is always fresher.

## Transport

- Rust bridge (dev): `http://127.0.0.1:4317/api/<command>` — every request
  is `POST` with JSON body and header `X-Bonaparte-Client: editor`.
- MCP: the same tools are exposed over JSON-RPC at `POST /api/mcp`
  (`tools/list`, `tools/call`), with panic containment per call.
- The web UI runs on the Vite dev server (5173) which proxies `/api/*` to
  the bridge.

## Units (get these wrong and nothing works)

| Quantity  | Unit                                                        |
| --------- | ----------------------------------------------------------- |
| Time      | **ticks** — 120 000 ticks = 1 second (also layer start/duration, keyframe times, comp duration) |
| Position  | pixels relative to the **comp center** (0,0 = center)       |
| Scale     | percent (100 = natural size)                                |
| Rotation  | degrees, clockwise                                          |
| Opacity   | 0..1 (not percent)                                          |
| Z (depth) | pixels; positive = farther from the 3D camera; a parented child compounds its parent chain Z |
| Colors    | `[r, g, b, a]` floats 0..1                                  |

## Editing — one op at a time

```bash
curl -s localhost:4317/api/apply -H "X-Bonaparte-Client: editor" -d '{
  "op": {"type":"setValue","comp":1,"layer":1,"property":"Position",
         "value":{"Vec2":[100.0,0.0]}}
}'
```

- Every op is **undoable**; the editor's user can Ctrl+Z your work. Group
  related edits with `{"type":"batch","label":"...","ops":[...]}` so one
  undo reverts the whole idea.
- Rapid-fire live edits can pass `"editGroup": "my-drag"` so they merge
  into one history entry instead of flooding it.
- Rejected ops return the reason **plus a hint suffix** listing valid
  properties, value shapes, and time units. Read it; fix; retry.

### Op catalog (short form — `describe` has full examples)

`setValue` · `addKeyframe` · `removeKeyframe` · `moveKeyframe` ·
`setEasing` · `setLayerTime` · `shiftLayer` · `setLayerEffects` ·
`renameLayer` · `setLayerVisible` · `setLayerLocked` · `setLayerParent` ·
`setLayerBlendMode` · `reorderLayer` · `addLayer` · `removeLayer` ·
`setCamera` (3D: fov/focus/dof) · `setTurntable` (one-click orbit) ·
`createComp` · `setCompProps` · `batch`

Properties: `Position | Scale | Rotation | Opacity | AnchorPoint | Z`.
Values are `{"Scalar": n}` or `{"Vec2": [x, y]}` — never bare numbers.

### Reading state

`POST /api/state` returns `{project, revision, canUndo, history, ...}`.
Track `revision`; if you poll, only act when it changes. The open editor
does the same and merges your edits into its UI automatically.

## Effects & grading

`POST /api/describe` lists every effect with typed params (Slider min/max/
default, Color, Point, Checkbox, Dropdown). Attach with `setLayerEffects`
(instances run top-to-bottom; `id` is any unique string you choose, omit
params for defaults). Bake any layer stack to an industry `.cube` LUT with
the MCP tool `export.lut`.

## 3D

`setCamera {fov:500, focus:0, dof:0.8}` turns the comp into a depth-sorted
diorama; give layers `Z` depth (`setValue ... "property":"Z"`), animate the
camera with keyframed `setCamera` ops, or switch on `setTurntable` for the
auto-orbit. `fov <= 0` renders exactly like classic 2D.

## Etiquette

- Locked layers (`"locked": true`) reject edits — that's the user protecting
  work; don't fight it.
- Never invent ids: read `layer_order`/`layers` from `state`/`describe`.
- Prefer `batch` over N separate applies; prefer `editGroup` over batches
  for continuous parameter sweeps.
- Verify visual results: `POST /api/export_png {compId, time}` returns a
  real PNG of the current project.

## Where things live (repo)

- Op model + undo: `crates/model/src/ops.rs`
- Op catalog shown to agents: `crates/mcp/src/tools.rs` (`editor_describe`)
- Bridge commands: `crates/runtime/src/lib.rs` + `crates/preview/src/lib.rs`
- Effects (WGSL packs): `crates/effects/packs/*`
