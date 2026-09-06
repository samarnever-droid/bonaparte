## 2026-09-04T08:43:51Z

You are the GUI Interaction & Timeline Tweaker for Bonaparte.
Your working directory is: C:\Users\khati\.zcode\workspace\default\bonaparte\.agents\worker_ui_timeline_tweaker
You MUST read:
- C:\Users\khati\.zcode\workspace\default\bonaparte\ORIGINAL_REQUEST.md
- C:\Users\khati\.zcode\workspace\default\bonaparte\PROJECT.md

MANDATORY INTEGRITY WARNING:
DO NOT CHEAT. All implementations must be genuine.

Your write scope: `ui/src/lib/components/CurveEditor.svelte`, `ui/src/lib/components/Timeline.svelte`, `ui/src/lib/components/Viewport.svelte`, `ui/src/lib/components/Properties.svelte`, `ui/src/lib/geometry.ts`, `ui/src/lib/store.svelte.ts`, `ui/src/lib/model.ts`.

Tasks:
1. Store & Model Alignment:
   - Normalize `store.svelte.ts` and `model.ts` to integer ticks (`Time(i64)` at 120k ticks/sec) and rational FrameRates with bidirectional conversions.
   - Replace `setInterval` in `play()` with `requestAnimationFrame` loop and frame-accurate playhead advancement.
2. Viewport Canvas & Drag Gestures (`Viewport.svelte`, `geometry.ts`):
   - Fix non-reactive `$state(dragging)` warning.
   - Fix track-override bug: when dragging a layer on canvas that has animated Position keyframes, add/move keyframe at `currentTime` rather than setting static transform.
   - Implement interactive canvas drag handles: corner/edge scaling handles, rotation handle, anchor point pivot handle, motion path curve tangents.
   - High-DPI canvas rendering (`devicePixelRatio`).
   - Comprehensive hit testing for all layer kinds (Shape, Text, Solid, Footage, PreComp).
3. Properties Panel (`Properties.svelte`):
   - Keyframe diamond toggle: accurately toggles AddKeyframe / RemoveKeyframe based on whether a keyframe exists at `currentTime`.
   - Key-at-playhead indicator: filled when on keyframe, hollow when between keyframes.
   - When editing property number inputs on an animated property, commit keyframe update at playhead.
4. Timeline (`Timeline.svelte`):
   - Keyframe diamond dragging (`MoveKeyframe` op) along the ruler with frame snapping.
   - Timeline zoom & pan (sub-second / frame-level view).
   - In-place curve editor view toggle button on animated tracks.
5. In-Place Bézier Curve Editor (`CurveEditor.svelte`):
   - Render velocity and value curves via SVG.
   - Draggable tangent handles adjusting Bézier control points (x1, y1, x2, y2).
   - Easing preset buttons: `Linear`, `Ease In`, `Ease Out`, `Ease In Out`, `Bezier`.
   - Dispatches `SetEasing` ops upon handle manipulation.

Write handoff to `C:\Users\khati\.zcode\workspace\default\bonaparte\.agents\worker_ui_timeline_tweaker\handoff.md`.
Send message when done.
