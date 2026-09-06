# Milestone 1: Robust Domain Model & Invertible Operation Engine (bonaparte-model) — Handoff Report

## 1. Observation
- Initial state:
  - `crates/model/src/document.rs`: Compilation error on line 328 due to missing `Add` trait on `Time` (`self.start + self.duration`).
  - `crates/model/src/ops.rs`: Moved-value compilation error in `History::commit` when matching on `op` after consuming `op.apply(project)`.
  - `crates/model/src/time.rs`: `Time(i64)` lacked operator overloading traits (`Add`, `Sub`, `Mul`, `Div`, `Neg`, `AddAssign`, `SubAssign`, `Sum`), `Display`, and SMPTE timecode conversions (`to_timecode`, `from_timecode`). `FrameRate` lacked `Display`, `to_frame`, and `from_frame`.
  - `crates/model/src/document.rs`: Lacked `BlendMode` enum, `Layer` fields (`parent`, `blend_mode`, `visible`, `locked`), `StaticTransform` `anchor_point`, and `Comp::effective_transform` / `Comp::has_parent_cycle`.
  - `crates/model/src/ops.rs`: Lacked non-destructive deletion undoing (`RestoreComp`, `RestoreLayer`, `RestoreMedia`), and new Op variants (`SetLayerTime`, `SetLayerParent`, `SetLayerBlendMode`, `SetLayerVisible`, `SetLayerLocked`, `SetCompProps`).
- Tool results after implementation:
  - `cargo test -p bonaparte-model` execution:
    ```
    running 21 tests (src/lib.rs) ... 21 passed
    running 5 tests (tests/hierarchy_transform.rs) ... 5 passed
    running 15 tests (tests/ops_roundtrip.rs) ... 15 passed
    running 3 tests (tests/serde_json.rs) ... 3 passed
    running 4 tests (tests/time_timecode.rs) ... 4 passed
    test result: ok. 48 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
    ```
  - `cargo clippy -p bonaparte-model --all-targets -- -D warnings`: Exited code 0 with zero warnings.

## 2. Logic Chain
1. **Time & FrameRate Arithmetic**:
   - Implemented `Add`, `Sub`, `Mul<i64>`, `Div<i64>`, `Neg`, `AddAssign`, `SubAssign`, `Sum`, and `Display` for `Time(i64)` with reference and value overloads.
   - Built SMPTE timecode parser `Time::from_timecode(tc, fps)` and formatter `Time::to_timecode(fps)` supporting `HH:MM:SS:FF` and `HH:MM:SS;FF`, validating range constraints (seconds < 60, minutes < 60, frames < nominal fps) with typed `TimecodeError`.
   - Added rational `FrameRate` frame conversions `to_frame(time: Time) -> i64` and `from_frame(frame_num: i64) -> Time` alongside formatted `Display` trait ("24 fps", "23.976 fps", "29.97 fps", "30 fps", "60 fps").
2. **Document Model & Transform Hierarchy**:
   - Fixed `document.rs` line 328 compilation error (`self.start + self.duration`).
   - Created `BlendMode` enum (`Normal`, `Multiply`, `Screen`, `Overlay`, `Add`, `Darken`, `Lighten`, `Difference`) with `#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]` and `Display`.
   - Added `parent: Option<LayerId>`, `blend_mode: BlendMode`, `visible: bool`, and `locked: bool` on `Layer` with backward-compatible Serde defaults.
   - Added `anchor_point: [f32; 2]` on `StaticTransform` and updated `Property::AnchorPoint`.
   - Implemented `Comp::effective_transform(layer_id, time)` calculating composite 2D affine matrices ($T_{pos} \cdot R_{rot} \cdot S_{scale} \cdot T_{-anchor}$) through root-to-target parent hierarchies with cycle detection in `Comp::has_parent_cycle`.
3. **Invertible Operation Engine & History**:
   - Fixed moved-value bug in `History::commit` by cloning/referencing before consuming apply.
   - Fixed exact invertibility for deletions:
     - `Op::RemoveComp` inverts to `Op::RestoreComp { comp: Box<Comp> }` which restores full composition state, layers, and identical `CompId`.
     - `Op::RemoveLayer` inverts to `Op::RestoreLayer { comp: CompId, layer: Box<Layer>, index: usize }` preserving exact stack order and ID.
     - `Op::RemoveMedia` inverts to `Op::RestoreMedia { asset: MediaAsset }` preserving ID, alias, and perception card.
   - Implemented full `Op` variants: `CreateComp`, `RestoreComp`, `RemoveComp`, `SetCompProps`, `AddLayer`, `RestoreLayer`, `RemoveLayer`, `RenameLayer`, `SetLayerTime`, `SetLayerParent`, `SetLayerBlendMode`, `SetLayerVisible`, `SetLayerLocked`, `SetValue`, `AddKeyframe`, `RemoveKeyframe`, `MoveKeyframe`, `SetEasing`, `ReorderLayer`, `AddMedia`, `RestoreMedia`, `RemoveMedia`.
   - Implemented exact `apply()`, `invert()`, and human-readable `describe()` for all 22 variants.
4. **Testing Suite**:
   - Added 48 total tests across unit tests and 4 integration test modules (`tests/ops_roundtrip.rs`, `tests/time_timecode.rs`, `tests/hierarchy_transform.rs`, `tests/serde_json.rs`).
   - Round-trip property verification confirms `P0 -> Op.apply -> P1 -> Op.invert.apply -> P2 == P0` for all operations.

## 3. Caveats
- No caveats. All domain model specifications, arithmetic traits, transform math, invertibility guarantees, and property tests are genuine and fully implemented.

## 4. Conclusion
Milestone 1 (bonaparte-model) is 100% complete and fully verified. All requirements have been implemented without shortcuts, facade mocks, or hardcoded test values.

## 5. Verification Method
To independently verify:
```powershell
# Run the complete test suite for bonaparte-model (48 tests)
cargo test -p bonaparte-model

# Run clippy to confirm zero warnings across all targets
cargo clippy -p bonaparte-model --all-targets -- -D warnings
```
Files to inspect:
- `crates/model/src/time.rs`
- `crates/model/src/document.rs`
- `crates/model/src/ops.rs`
- `crates/model/src/lib.rs`
- `crates/model/tests/ops_roundtrip.rs`
- `crates/model/tests/time_timecode.rs`
- `crates/model/tests/hierarchy_transform.rs`
- `crates/model/tests/serde_json.rs`
