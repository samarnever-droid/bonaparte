# Audio Studio — v0.4 foundation (historical scope)

**v0.5 adds EQ, compression, compensated limiting and mix buses.** See [the current update](RESPONSIVENESS-AND-DSP.md). The following records the v0.4 foundation and its original limits.

A sample-addressed multitrack audio foundation for Bonaparte, not a claim of full DAW parity.

## What works

- **Import:** WAV, MP3, FLAC, OGG/Vorbis, AIFF, M4A and AAC, decoded through FFmpeg. Mono/stereo sources, original sample rates from 8–384 kHz.
- **Portable media:** original encoded bytes are retained in the project. Working PCM is 48 kHz float, resampled with FFmpeg's SoXR precision-28 path.
- **Timeline:** independent audio tracks, multiple clips per track, clip move/trim/slip/split/duplicate/delete, reverse and 0.25–4× varispeed, track lock, waveform display and sample-addressed inspector fields.
- **Mixing:** clip/track/master gain, clip pan/stereo balance, track balance, mute and solo, clip-local gain/pan automation, fades and equal-power crossfades.
- **Navigation:** audio playhead, sample/video-frame/beat snapping, tempo grid, named markers, looping, optional scrub audition and monitor-only listening volume.
- **Meters:** real worklet output sample peaks and RMS data, per-track meter data from the mixer, and overload indication. Monitor volume does not hide master overload.
- **Output:** 48 kHz stereo float WAV, or the project mix encoded to stereo AAC at 256 kbps inside native H.264 MP4 export.
- **Persistence:** validated atomic audio arrangement edits, undo/redo, save/open and recovery, with version-2 projects still readable.

## Using the editor

Choose the **Audio** workspace or the timeline's **Audio** tab. Import a file from the audio library, toolbar, or file drop. Imports retain the complete source; the initial clip is trimmed to the active composition's remaining duration.

A clip's body moves it. Its lower side edges trim it. **Alt-drag** slips the source without moving its timeline position; **Shift** bypasses snapping. Upper-corner handles edit fade regions. Overlap a clip with a later clip on the same track, then choose **Crossfade overlapping neighbor**.

The audio inspector has exact sample start/length/source fields, gain/pan controls and automation diamonds. With automation present, changing the numeric gain/pan creates or changes the key at the audio playhead. Split uses the exact sample playhead, not the video frame. Gain and pan automation are linear between points in their respective dB/pan domains.

Mono sources use an equal-power, −3 dB center pan law. Stereo sources preserve their center level and use balance. Track balance is applied after clip panning. Master gain is part of the exported mix; monitor volume is not.

**Varispeed changes pitch.** The rate control is not pitch-preserving time stretch. Reverse changes source direction without destructive rewriting. Peak Normalize analyzes the entire source's sample peak, not integrated loudness or just the trimmed range.

## Timing and shared evaluation

Video keeps its existing 120,000-tick timebase. Audio clip positions are integer **48,000-Hz sample frames**, so one audio sample is not rounded to a video tick/frame. Source offsets can be fractional when a varispeed clip is split or trimmed.

`bonaparte-audio` is a pure Rust mixer with injected sample sources. It owns clip timing, envelopes, panning, gain, overlap sums and a 32-tap windowed-sinc resampler. Unity-rate integer reads are exact. It performs no file/device/process I/O. Tests verify that different chunk boundaries and arbitrary seek ranges produce identical sample arrays.

Fade regions and automation can extend beyond the retained portion of a clip. Splitting shifts the second clip's local envelope coordinates rather than restarting its fades. This preserves the original sound—including varispeed, fades and automation—across the split.

Nested composition audio follows precomp start/duration and visibility. **Visual opacity does not silently attenuate sound.** Composition master gains cascade; a root master mute silences the entire mix. Solo is scoped to the composition and suppresses non-solo tracks and nested audio in that composition.

The browser and Tauri request PCM from the same native mixer used by WAV/MP4 export. There is no second JavaScript implementation of the mixing rules.

## Playback engine

A bounded **AudioWorklet** queue consumes already-mixed float PCM. Its sample cursor drives the visual timeline. Device output timestamps are used when available to compensate for output-buffer latency; this is not a promise of zero acoustic latency.

The client requests a 48 kHz AudioContext to match the working mix. The browser/device can perform the final hardware-rate conversion. This avoids repeatedly resampling every voice to a 44.1 kHz context on the development server. Other 8–192 kHz output rates remain supported by the native chunk API, but their performance is not equivalent.

- Start/seek prefetches audio before starting the worklet clock.
- Generation IDs discard obsolete responses after seeking or editing.
- Loop boundaries can be crossed within a render quantum without inserted samples.
- If data is missing, the worklet emits silence and **holds the composition cursor** instead of allowing video to run ahead.
- Queue retention is bounded to 16 chunks. The currently playing chunk is protected from eviction.
- Scrub audition is optional, short and declicked. Monitoring changes are smoothed and do not change export samples.

A full five-track IGNITION playback/loop observation at 48 kHz reported **zero worklet buffer-underrun samples** over approximately 25 seconds while the real Rust video preview was active. This is local browser evidence, not a universal network/device performance guarantee. Initial testing exposed an expensive 44.1 kHz per-voice conversion path; matching the context to 48 kHz removed that observed bottleneck.

## Native acquisition and waveforms

`bonaparte-media::audio` uses bounded FFprobe/FFmpeg subprocesses, an explicit input format/protocol allowlist, and temporary files. Multi-channel sources are rejected rather than silently downmixed. Invalid encoded data, mismatched metadata and non-finite/excessive sample values fail before import commits.

Original content is identified by SHA-256, never filename. Decoded float PCM is held in immutable, memory-mapped cache files. Waveforms use a multiresolution min/max hierarchy with exact edge refinement, rather than reading an entire long source for every zoom request.

Heavy import preparation runs outside the editor document mutex. The final source/track/clip insertion is one atomic history transaction. Failed imports do not replace the document.

## Storage and resource boundaries

These are first-milestone limits, not unlimited professional-session support:

| Resource | Limit / policy |
|---|---|
| Encoded source import | 32 MiB per file |
| Portable embedded media | 256 MiB combined encoded audio/image payloads |
| Project file | 512 MiB; serialization refuses a file that cannot be reopened |
| Source duration | One hour per source |
| Source channels | Mono or stereo; surround is not downmixed implicitly |
| Tracks / clips | 256 tracks, up to 16,384 clips/track and 65,536 clips/composition |
| Expanded audio graph | 4096 composition instances, 512 buses / 16384 voices; oversized arrangements fail explicitly |
| PCM requests | At most two seconds per request, 8–192 kHz output |
| Decoded cache | 4 GiB of named PCM cache files; unpinned old entries can be removed |
| Worklet queue | At most 16 PCM chunks |
| Float WAV | RIFF 4 GiB size boundary; the bridge streams any size straight from disk |

Cache budgets are not total RAM/disk guarantees. Memory maps, decoded waveform levels, temporary decoding/output files, in-flight jobs, browser buffers and OS caches have additional costs. Linked media, streaming projects beyond the portable-file budget, and background disk-space management need a subsequent milestone.

Normal snapshots omit original audio payloads so a fader edit does not resend megabytes of audio. Explicit project save and idle recovery retrieve the complete portable document. In-memory source payloads use shared immutable strings so document/history cloning does not duplicate those bytes.

## Export semantics

Audio is mixed to a temporary 48 kHz stereo **32-bit float WAV** with a `fact` chunk. This preserves floating-point headroom; there is no secret normalization or limiter. If the mix exceeds 0 dBFS, reduce gain before AAC/device playback, which may clip.

Native video export gives the same mix to FFmpeg alongside the scheduled video frames. The audio length covers the encoded video-frame schedule; any interval beyond the composition itself is silent. No `-shortest` shortcut is used to silently drop the final video frame. No-audio projects remain video-only.

The file destination is replaced only after successful completion. Temporary audio/movie files are removed on the normal/error paths. Export progress/cancel/queue is still the existing limited UI and is **not** a completed job-management system in v0.4.

## IGNITION audio demonstration

`IGNITION-with-audio.bonaparte` adds **five tracks and fifteen clips** to the existing 22-second film. It uses original synthesized source files, imported as ordinary audio. Arrangement, gain, fades, automation, reverse cues and final mixing/muxing are native Bonaparte operations—not an external post-export soundtrack.

Observed output:

- 1920×1080, 24 fps, **528 video frames**.
- AAC stereo, **48,000 Hz**, audio and video both **22.000 seconds** in the MP4 container.
- Reference mix: **1,056,000 stereo sample frames**, no sample overloads.
- Sample peak approximately **0.865**; external analysis measured approximately **−18.8 LUFS integrated**. LUFS analysis is verification, not a new in-editor loudness feature.
- AAC/reference onset correlation lag: **0 samples** in the checked segment; full left-channel correlation approximately **0.9995**.
- AAC decoding includes codec padding beyond the container's logical end. That is not additional timeline audio.
- Native editor export completed in approximately **320.6 seconds**, with no browser or decode errors.

## Verification and next work

Automated coverage includes source offsets, sample boundaries, mono pan law, stereo balance, overlaps, gain/automation/fades, reverse, mute/solo, nested timing, split equivalence, resampling alias suppression, packet/WAV parity, original-media persistence, malformed import atomicity, common codecs and NTSC audio/video export.

Worklet unit tests cover underrun clock holding, stale generations, queue eviction, loop continuity and pre-monitor overload metering. Real-browser tests exercise importing, waveforms, save/undo, sample edits, automation, real PCM consumption, playback/mute, markers and native WAV/AAC exports.

Still missing for a best-in-class audio workstation: EQ/compression/limiter chains, sends/submix routing, VST/AU plugins, recording, noise reduction/spectral editing, pitch-preserving time stretch, LUFS/true-peak tools, surround, linked-media sessions at scale, richer automation curves and broad physical-device/native-platform testing. The current work is a tested foundation for those features, not a claim that they already exist.
