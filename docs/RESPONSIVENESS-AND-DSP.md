# v0.5 — responsive editing and native audio processing

This milestone prioritizes the reported drag/scale lag, then adds real EQ, dynamics and mix-bus routing. It does not claim to complete recording, external plugin hosting, pitch-preserving stretching, surround or a full DAW.

## Interaction changes

### Lightweight pointer path

The project mirror is now an immutable, shallow-reactive object rather than a deeply proxied tree. Pointer moves no longer clone an animated layer and all of its keys. Gesture geometry, parent transforms and canvas bounds are captured at pointer-down; move/scale/rotation calculations use that snapshot and update once per animation frame.

A drag has one persistent commit/undo transaction. The provisional position is held through the asynchronous commit instead of snapping back to the old position while waiting. Timeline/audio gestures are similarly coalesced. Expensive waveform fetches wait until a gesture finishes, and recovery serialization waits for idle time.

Text bounds are cached, and keyframe lookup uses binary search. Image decode/resize/base64 work moves into a worker where OffscreenCanvas is available, with a compatible fallback.

### Cached interaction planes

The new native `interaction_planes` endpoint renders background, subject and foreground planes in Rust. Hover/selection prefetches these into a bounded client cache. During a transform, the browser applies an affine CSS transform to those **already-rendered pixels**, so object feedback does not wait for a fresh full composition render. Release requests the authoritative final frame.

This is explicitly an **interactive proxy**, not a second authoritative renderer. Above-subject adjustment layers and complex blend/effect relationships can settle differently in the final composite. Interleaved parent/child stacks fall back to the draft renderer. Before a proxy is ready, selection/handles respond immediately while a reduced-resolution draft updates. The viewport labels cached/draft interaction and final refinement.

The proxy is bounded to four entries / 32 MiB client retention, and native packets are capped at 24 MiB. These are cache limits, not total process/GPU memory guarantees.

### Authoritative state deltas

Opt-in edit/undo/redo replies return actual changed model objects rather than serializing the entire project. The client merges these objects; it does not reimplement Op validation or allocation. Mismatched base revisions receive a full snapshot. Old clients continue to work with full replies.

Visual and audio revisions are separate. Position/scale/rotation, image changes, marker edits and tempo-grid changes no longer restart the audio stream or discard its mixer plan.

### Local measurements

A repeatable isolated browser test loaded **IGNITION plus an editable probe: 487 layers across the project**. The comparison used the archived v0.4 UI/executable and the new code in the same sandbox, after retiring an unused duplicate preview server.

| Measurement | Archived v0.4 | Revised interaction path |
|---|---:|---:|
| Mutation response | 1,773,010 bytes | about 758–761 bytes |
| Observed commit confirmation | 782 ms | 52–148 ms |
| Pointer-to-next-RAF p95 | 14.4 ms | about 0.3 ms |
| RAF interval p95 during the drag | 16.8 ms | about 16.7 ms |
| Sampled intervals over 50 ms | 1 | 0 |
| Preview-frame requests in the scenario | 26 | 4 |

These are local scenario measurements, **not portable hardware guarantees or full-render times**. Pointer event dispatch can coincide with an animation frame; the sub-millisecond statistic is not a claim of sub-millisecond human-to-display latency. The substantial changes are the much smaller commit payload and cached object feedback instead of waiting for each Rust render.

An earlier run while several old servers were resident hit memory pressure, including a failed large-project load. Those measurements are not used as the comparison baseline. Existing documents were backed up; the unused performance-only session was retired, while the original editing/audio sessions were retained.

## Native audio channel strips

### EQ

Every track, mix bus and composition master can use a real native channel strip:

- Optional second-order high-pass filter.
- Low shelf, parametric mid bell with Q, and high shelf.
- Bounded frequencies/gains/Q, with explicit bypass.
- Frequency-response display calculated from the actual native filter coefficients.

The implementation uses stereo biquads. Flat/bypassed configurations are tested for neutrality; bell gain and low-frequency attenuation are measured in tests.

### Stereo-linked compressor

Threshold, ratio, soft knee, attack, release and makeup gain are implemented in Rust. Both channels use one linked peak detector/gain-reduction envelope, preserving stereo balance. Channel-strip meters expose gain reduction. Settings are persisted and undoable; these processor parameters are static settings, not a newly claimed plugin-automation engine.

### Compensated lookahead limiter

Composition masters can use a stereo-linked **sample-peak** limiter with a configurable ceiling/release and fixed 5 ms lookahead. Its latency is compensated in playback/export. A direct path and a nested limited path are aligned at their mix point; an impulse remains at the original timeline sample.

This is not oversampled true-peak limiting. AAC/device reconstruction can have inter-sample peaks, so this does not replace a professional true-peak delivery check.

### Mix buses and sends

Up to 16 named mix buses per composition support gain, balance, mute, EQ/compression and output routing. Tracks/buses have a main output and up to four post-strip sends. Dry output is not secretly duplicated; explicit sends add additional routes. Feedback cycles, missing targets and invalid processor parameters are rejected before committing the document.

Each nested composition instance gets its own processing state. Track/clip mixing feeds channel processing, then routes/sends; bus/master gain is applied before that strip's processors. Incoming paths are delay-aligned when lookahead latency differs. Existing source timing, clip automation, fades and solo behavior remain part of the same mixer.

## Stateful DSP without seek/export discrepancies

Stateful filtering/compression cannot be reset at every network chunk. The new graph processes canonical 256-sample quanta, with bounded PCM caches and DSP-state checkpoints. Arbitrary seeks and differently sized requests are tested against contiguous output, including limiter compensation and nested routes.

PCM retention is 32 MiB and checkpoint retention 16 MiB per rate-cache entry, with at most two output rates retained per processed plan. Multiple plans/sessions, source maps, worklet buffers and temporary exports add memory; these numbers are not a total RAM bound.

A cold distant seek may need earlier processing. Work is divided into bounded **15 seconds of source-time per request**, not 15 seconds of wall time. The transport receives a `BAW1` preparation response and retries while its generation is current. Requests beyond the composition return silence immediately instead of replaying an unbounded timeline.

On processing edits, the old queued sound continues while replacement PCM is prepared. An atomic worklet swap preserves the sample cursor and uses a short monitoring crossfade to reduce clicks. This transition is playback-only; exported samples reflect the saved settings. The preparation is cancellable between bounded requests, not a hard real-time guarantee under arbitrary network/CPU load.

## File and API compatibility

New projects write **format version 4**. Versions 2 and 3 remain readable. Older applications reject the new format rather than silently dropping routing/processing fields.

- Existing `BAP1` packets remain mixed stereo float PCM, with additional processing-meter/latency metadata.
- `BAW1` reports staged DSP preparation.
- `BIP1` supplies native interaction planes.
- Transient transform overrides carry one property/value and never enter history or saved projects.
- Project deltas require a matching base revision and are opt-in.

The included `examples/ignition-audio.bonaparte` demonstrates Music/Effects buses, channel processing, a post-strip send and a master limiter. `cargo run -p bonaparte-runtime --example render_audio -- project.bonaparte mix.wav` renders through the same native mixer.

## Still deferred

Recording; VST/AU hosting; pitch-preserving time stretch; surround; LUFS/true-peak workflows; noise reduction/spectral editing; automated plugin-parameter curves; linked-media sessions beyond portable-file limits; full video editing/masks/3D; export queue/cancellation; physical-GPU/native-device coverage across platforms.

No blank UI, pass-through DSP placeholder or external post-export mix is counted as an implemented feature.
