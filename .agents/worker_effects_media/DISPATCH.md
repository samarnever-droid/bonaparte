## 2026-09-04T08:43:51Z

You are the Core Media & Effects Lead for Bonaparte.
Your working directory is: C:\Users\khati\.zcode\workspace\default\bonaparte\.agents\worker_effects_media
You MUST read:
- C:\Users\khati\.zcode\workspace\default\bonaparte\ORIGINAL_REQUEST.md
- C:\Users\khati\.zcode\workspace\default\bonaparte\PROJECT.md
- C:\Users\khati\.zcode\workspace\default\bonaparte\.agents\worker_model\handoff.md

MANDATORY INTEGRITY WARNING:
DO NOT CHEAT. All implementations must be genuine. DO NOT hardcode test results, create dummy/facade implementations, or circumvent the intended task.

Your exclusive write scope: crates/effects/ and crates/media/.

Tasks:
1. crates/effects/:
   - Complete all 10 core first-party GPU effect packs in crates/effects/packs/: glow, lur, drop_shadow, color_adjust, 	ransform, ignette, chromatic_aberration, invert, 	int, directional_blur.
   - Each pack must contain manifest.toml (valid types, defaults, docstrings), <effect>.wgsl (standard 5 bindings), and README.md.
   - Implement EffectRegistry in crates/effects/src/lib.rs and egistry.rs with embedded builtins and dynamic directory scanner.
   - Implement cpu_reference.rs with deterministic mathematical evaluators for all 10 effects.
   - Golden-frame tests in crates/effects/tests/ and manifest parameter validation tests.
2. crates/media/:
   - Crash-isolated FFmpeg & FFprobe child-process orchestration (probe.rs, decode.rs, proxy.rs, cache.rs, export.rs).
   - probe.rs: JSON metadata parser and PerceptionCard generator (dominant palette, mean luma, entropy, role).
   - decode.rs: Fast seek frame decoder into raw RGBA bytes.
   - proxy.rs: Half-resolution proxy video generator.
   - cache.rs: Two-tier LRU RAM (capped) + disk-backed frame store implementing MediaFrames with Flat RAM guarantee.
   - export.rs: Direct compositor-to-FFmpeg streaming MP4 video export (O(1) memory).
   - Integration tests in crates/media/tests/.
3. Run cargo test -p bonaparte-effects and cargo test -p bonaparte-media.
Write handoff to C:\Users\khati\.zcode\workspace\default\bonaparte\.agents\worker_effects_media\handoff.md.
Send message when done.

## 2026-09-04T09:04:39Z

**Context**: Finalize Milestone 4 (bonaparte-media)
**Content**: Milestone 3 (onaparte-effects) is verified 100% clean with all 10 GPU packs and golden-frame tests passing. Please proceed to complete Milestone 4 in crates/media/:
1. probe.rs: FFprobe JSON metadata parser and PerceptionCard generator.
2. decode.rs: Child-process FFmpeg raw RGBA frame decoder.
3. proxy.rs: Half-resolution proxy video generator.
4. cache.rs: Two-tier disk-backed playback cache implementing onaparte_engine::MediaFrames with Flat RAM guarantee.
5. export.rs: Direct compositor-to-FFmpeg streaming frame-by-frame MP4 export.
6. Integration tests in crates/media/tests/.
**Action**: Run cargo test -p bonaparte-media and write your handoff report to C:\Users\khati\.zcode\workspace\default\bonaparte\.agents\worker_effects_media\handoff.md.
