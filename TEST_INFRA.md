# E2E Test Infra: Bonaparte

## Test Philosophy
- Opaque-box, requirement-driven. No dependency on internal implementation details.
- Systematic 4-tier methodology:
  - **Tier 1**: Feature Coverage (happy path, isolations, ≥5 per feature area).
  - **Tier 2**: Boundary & Corner Cases (limits, overflow, empty inputs, non-zero offsets, zero durations, extreme scales).
  - **Tier 3**: Cross-Feature Combinations (pairwise interactions, animated transforms + nested PreComps + effects + blend modes).
  - **Tier 4**: Real-World Application Scenarios (motion graphic title sequence, social media ad, multi-layer logo reveal).

## Feature Inventory Coverage Matrix
| # | Feature Area | Requirement | Tier 1 | Tier 2 | Tier 3 | Tier 4 |
|---|--------------|-------------|:------:|:------:|:------:|:------:|
| 1 | Time & FrameRate Arithmetic | R1 | 5 | 5 | ✓ | ✓ |
| 2 | Op Mutations & Invertible History | R1 | 10 | 10 | ✓ | ✓ |
| 3 | Layer Hierarchy & Transforms | R1/R2 | 5 | 5 | ✓ | ✓ |
| 4 | Tile Grid & Topological Graph | R2 | 5 | 5 | ✓ | ✓ |
| 5 | CPU Reference Renderer (All Layers & Blends) | R2 | 8 | 8 | ✓ | ✓ |
| 6 | WASM Compilation Purity | R2 | 2 | 2 | ✓ | ✓ |
| 7 | Plugin Host & 10 First-Party Effects | R3 | 10 | 10 | ✓ | ✓ |
| 8 | Pixel-Deterministic Golden Frame Tests | R3 | 10 | 10 | ✓ | ✓ |
| 9 | Media Probing & Perception Cards | R4 | 5 | 5 | ✓ | ✓ |
| 10 | Playback Cache (Flat RAM) & Frame Decode | R4 | 5 | 5 | ✓ | ✓ |
| 11 | Direct Streaming MP4 Video Export | R4 | 5 | 5 | ✓ | ✓ |
| 12 | Svelte 5 UI Shell & Timeline Scrubber | R5 | 5 | 5 | ✓ | ✓ |
| 13 | Bézier Curve Graph Editor | R5 | 5 | 5 | ✓ | ✓ |
| 14 | Properties Panel & Diamond Toggles | R5 | 5 | 5 | ✓ | ✓ |
| 15 | Headless MCP Server Tools (`op.apply`, `ops.propose`, `comp.render`) | R5 | 8 | 8 | ✓ | ✓ |

## Test Architecture
- Test runner: Rust integration test harnesses (`cargo test --test ...`) and headless CLI/MCP drivers.
- Golden-frame fixture generator: Synthetic and deterministic image comparison with PSNR / exact pixel tolerance.
- Direct-process FFmpeg validation: Inspecting exported MP4 metadata, duration, streams, and visual frames.

## Real-World Application Scenarios (Tier 4)
| # | Scenario | Features Exercised | Complexity |
|---|----------|--------------------|------------|
| 1 | Kinetic Typography Title Sequence | Text layers, Bézier scale/opacity easing, Glow effect, PreComp | High |
| 2 | Multi-Layer Logo Reveal | Vector shapes, rotation transforms, Drop Shadow, Add blend mode, Vignette | High |
| 3 | Picture-in-Picture Video Composite | Footage layer, half-res proxy decode, Solid background, Blur effect | High |
| 4 | AI-Driven Automated Promo Generator | MCP server `ops.propose`, batch `op.apply`, undo/redo verification, MP4 streaming export | High |
| 5 | Complex Nested PreComp Hierarchy | 3-level deep PreComps, time-offset parent layers, Multiply blend mode, Transform effect | Very High |
