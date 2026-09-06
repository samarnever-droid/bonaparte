# v0.5.1 — editing stability fixes

This patch is a response to the reported rotation, scaling quality, timeline, live-input and visibility failures. Feature expansion was paused. Passing earlier broad workflow tests did not cover these interaction sequences; this is **not** a claim that every remaining UI bug has been eliminated.

## Reproduced and fixed

| Reported behavior | Cause found | Repair and verification |
|---|---|---|
| Text/position do not update until clicking elsewhere | Inspector fields used `change` rather than `input` | Live drafts render while the control stays focused. Valid edits persist after a short idle without requiring blur. Tests check both actual rendered pixels and the Rust document while focus stays in the field. |
| Color/alpha/effect values do not apply immediately | Color picker, hex/alpha and numeric effect controls committed only on change/blur | Picker input events, valid hex/alpha edits and numeric effect inputs now update live. Invalid/incomplete numeric prefixes are not converted into accidental zero edits. |
| Typing becomes inconsistent while replies arrive | Acknowledgements could replace newer UI values; independent keystrokes could fill history | Newer drafts remain visible through delayed replies. A focused editing group keeps its original inverse and latest redo state; history does not grow for every character. |
| Rotation handle behaves incorrectly | The handle offset was screen-vertical after rotation; angle math used the visual center rather than the anchor and ignored nonuniform parent scaling | Handle placement follows the transformed edge, the hit target is larger, and rotation uses the anchor in parent coordinates. Rotation and scaling both have offset-anchor regressions; rotation also has a nonuniform-parent case. A dedicated Rotation tool (`R`) is available. |
| Scaled vector/text/composition content becomes pixelated | Generated sources, glyph masks and child compositions were rasterized at their original dimensions and then enlarged | Text/generators are rasterized at output-aware density. Enlarged nested compositions use larger raster grids. Preview and native output use the same quality-aware CPU path. Tests inspect source/child raster sizes and rendered results. |
| A low-resolution image remains after scaling / visibility changes | A later revision could prevent an old transform proxy from ever being cleared | Cancelled/hidden/locked/removed targets invalidate the proxy. Refinement can complete against a newer confirmed revision instead of waiting forever for the old one. Delayed-render/visibility sequences are covered. |
| Visibility clicks behave inconsistently | Rapid clicks captured the same old boolean before the first reply arrived | Pending toggle intent drives the icon immediately and each queued operation carries the correct requested state. Rapid hide/show and lock toggles no longer reuse stale values. |
| A new timeline appears restricted to a few seconds | Timeline zoom/range survived composition changes; clip movement/trim was clamped to the old out point | New projects start at 30 seconds, timeline view resets on document/composition changes, and length/out-point are visible. Duration can be edited/extended directly. Far timecode seeks and layer out-point edits can extend the composition in an undoable transaction. |

## Quality and interaction policy

- **Full quality is honored during interactions.** It no longer silently switches to a quarter-resolution bitmap.
- In Auto mode, scaling/rotation request fresh full-quality frames rather than stretching a low-resolution cached subject. Their controls remain immediate; expensive scenes can still take time to render.
- Auto movement can use the existing cached proxy for fast feedback. Holding still triggers a native refinement, and the saved/exported result is never that temporary proxy.
- A manually selected Half/Quarter preview is intentionally lower resolution. Imported raster images cannot gain detail beyond their source pixels.
- Continuous rasterization is bounded: density is capped at 8×, individual rasters at the existing axis/pixel limits, and prepared composition intermediates at 64 megapixels. Very large magnifications/complex nesting can reach those limits; infinite-resolution or real-time rendering is not claimed.
- Spatial effect parameters scale with the nested raster grid rather than becoming visually smaller when the grid grows.

## Live editing and undo

A field's valid input is displayed as a transient draft and committed after approximately 140 ms of idle time, or on blur/Enter/save. A focus group merges compatible updates into one undoable operation; unrelated targets and failed edits cannot merge incorrectly. Save/export/undo flush pending edits. Newer drafts are not replaced by a delayed older acknowledgement.

This is input-driven preview, not a promise of zero rendering/network latency. The original document still owns validation and history. The file format remains version 4.

## Timeline behavior

The timeline is no longer visually stuck at an old zoom level. It has an editable **Length** field, a **+10s** extension action, visible output-end marker and a fit control. It can show existing content beyond the old output range, and edits can extend that range.

There is **no five-second limit**. The underlying validated composition ceiling is still 24 hours; it is not literally an unlimited-duration file format. Preview navigation and encoded output duration remain distinct concepts, made visible by the out marker.

## Regression coverage added

`ui/tests/stability.spec.ts` covers focused text/position/color/effect edits, native picker input, incomplete numeric values, delayed acknowledgements, grouped undo/redo, rapid visibility toggles, stale transform proxies, anchor-aware scale/rotation, nonuniform-parent rotation, long timeline navigation, duration extension and blank-project defaults.

`crates/model/tests/live_edits.rs` checks group coalescing, preserved inverses/redo, distinct targets and failed edit safety. `crates/engine/tests/raster_quality.rs` checks larger glyph/generator/child-composition grids and preview/output consistency. The old fixed-density renderer is retained only as an independent comparison helper, not the normal export path.

## Known limits / not claimed fixed

- No comprehensive “hundreds of bugs fixed” claim. This patch targets the concrete reports above and related chained-edit failures.
- New OS/window-manager/browser combinations, unusual input methods, complex hierarchies and extremely large projects still need broader real-use coverage.
- Native file dialogs/GUI behavior on Windows/macOS have not been end-to-end tested here.
- Cached movement proxies can approximate complex blend/adjustment stacks until native refinement; Full quality disables that proxy.
- Recording, plugin hosting, pitch-preserving time stretch, surround, masks, 3D, and advanced export-job management remain separate unfinished features. They were not added during this stabilization pass.
