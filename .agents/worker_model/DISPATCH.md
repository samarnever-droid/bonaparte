## 2026-08-30T09:40:51Z
You are Worker 1 implementing Milestone 1: Robust Domain Model & Invertible Operation Engine (bonaparte-model).
Your working directory is: C:\Users\khati\.zcode\workspace\default\bonaparte\.agents\worker_model
You MUST read the original request first at: C:\Users\khati\.zcode\workspace\default\bonaparte\ORIGINAL_REQUEST.md
Also read: C:\Users\khati\.zcode\workspace\default\bonaparte\PROJECT.md

MANDATORY INTEGRITY WARNING:
DO NOT CHEAT. All implementations must be genuine. DO NOT hardcode test results, create dummy/facade implementations, or circumvent the intended task. A teamwork_preview_auditor will independently verify your work. Integrity violations WILL be detected and your work WILL be rejected.

You have exclusive write ownership of `crates/model/`.

Implementation Requirements:
1. `crates/model/src/time.rs`:
   - Implement `Add`, `Sub`, `Mul<i64>`, `Div<i64>`, `Neg`, `AddAssign`, `SubAssign`, `Sum`, `Display` for `Time(i64)`.
   - Implement SMPTE timecode conversions on `Time`: `to_timecode(&self, fps: FrameRate) -> String` and `from_timecode(tc: &str, fps: FrameRate) -> Result<Self, TimecodeError>`.
   - Implement `Display` on `FrameRate` (e.g. "24 fps", "23.976 fps"), `to_frame(time: Time) -> i64`, `from_frame(frame_num: i64) -> Time`.
2. `crates/model/src/document.rs`:
   - Fix line 328 compilation error (`self.start + self.duration`).
   - Add `BlendMode` enum with `#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]`: `Normal` (default), `Multiply`, `Screen`, `Overlay`, `Add`, `Darken`, `Lighten`, `Difference`.
   - On `Layer`: Add `pub parent: Option<LayerId>`, `pub blend_mode: BlendMode`, `pub visible: bool`, `pub locked: bool`. Default `parent = None`, `blend_mode = BlendMode::Normal`, `visible = true`, `locked = false`.
   - On `StaticTransform`: Add `pub anchor_point: [f32; 2]`. Default `[0.0, 0.0]`.
   - On `Comp`: Add methods `effective_transform(layer_id, time)` computing inherited parent translation, rotation, scale with cycle guard.
3. `crates/model/src/ops.rs`:
   - Fix moved-value bug in `History::commit`.
   - Fix exact invertibility on deletions:
     - When `Op::RemoveComp` is committed, store exact `Comp` state in inverse `Op::RestoreComp { comp: Box<Comp> }`.
     - When `Op::RemoveLayer` is committed, store exact `Layer` and its index in `layer_order` in inverse `Op::RestoreLayer { comp: CompId, layer: Box<Layer>, index: usize }`.
     - When `Op::RemoveMedia` is committed, store exact `MediaAsset` in inverse `Op::RestoreMedia { asset: MediaAsset }`.
   - Add new `Op` variants:
     - `SetLayerTime { comp: CompId, layer: LayerId, start: Time, duration: Time }`
     - `SetLayerParent { comp: CompId, layer: LayerId, parent: Option<LayerId> }`
     - `SetLayerBlendMode { comp: CompId, layer: LayerId, blend_mode: BlendMode }`
     - `SetLayerVisible { comp: CompId, layer: LayerId, visible: bool }`
     - `SetLayerLocked { comp: CompId, layer: LayerId, locked: bool }`
     - `SetCompProps { comp: CompId, name: String, width: u32, height: u32, fps: FrameRate, duration: Time, background: [f32; 4] }`
     - `RestoreComp { comp: Box<Comp> }`
     - `RestoreLayer { comp: CompId, layer: Box<Layer>, index: usize }`
     - `RestoreMedia { asset: MediaAsset }`
   - Implement `invert()` and `apply()` for all new and existing variants.
   - Implement `describe()` providing human-readable descriptions for all `Op` variants.
4. Comprehensive test suite:
   - Add unit tests in `crates/model/tests/` and in modules.
   - Add round-trip property tests verifying `P0 -> Op.apply -> P1 -> Op.invert.apply -> P2 == P0` for ALL Op variants.
   - Run `cargo test -p bonaparte-model` and document build/test commands and results in your handoff report at `C:\Users\khati\.zcode\workspace\default\bonaparte\.agents\worker_model\handoff.md`.
Send a message when done.

## 2026-08-30T09:57:23Z
**Context**: Strict Write Boundary Enforcement
**Content**: You have exclusive write ownership ONLY for `crates/model/`. You may read anywhere across the repository, but you must NOT write to any other crate or folder.
**Action**: Continue implementing bonaparte-model within `crates/model/` and report when tests pass.
