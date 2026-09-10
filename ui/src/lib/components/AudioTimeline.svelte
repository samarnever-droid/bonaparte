<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import Icon from "./Icon.svelte";
  import AudioWaveform from "./AudioWaveform.svelte";
  import {
    editor,
    activeComp,
    arrangement,
    seekAudio,
    pause,
    audioTransport,
  } from "../store.svelte";
  import {
    importAudio,
    addAudioTrack,
    editAudio,
    splitAudio,
    deleteAudio,
    duplicateAudio,
    addMarker,
    snapAudio,
    crossfadeAudio,
    addMixBus,
    reverseAudio,
    normalizeAudio,
    detectBeats,
    cutToBeat,
  } from "../audio/actions";
  import { openContextMenu, type ContextMenuItem } from "../store.svelte";
  import {
    AUDIO_RATE,
    trimLeft,
    dbLabel,
    peakDb,
    envelope,
    type AudioClip,
    type AudioTrack,
  } from "../audio/model";
  let { zoom = 1 }: { zoom?: number } = $props();
  const comp = $derived(activeComp()),
    audio = $derived(arrangement());
  const samples = $derived(Math.max(1, Math.ceil((comp?.duration ?? 1) * 0.4)));
  let root = $state<HTMLDivElement | null>(null),
    ruler = $state<HTMLDivElement | null>(null),
    visibleWidth = $state(1000);
  const width = $derived(Math.max(400, visibleWidth - 252) * zoom);
  const pct = (frame: number) => (frame / samples) * 100;
  const ticks = $derived.by(() => {
    const seconds = samples / AUDIO_RATE,
      step =
        [0.01, 0.05, 0.1, 0.25, 0.5, 1, 2, 5, 10, 30, 60, 120, 300, 600, 1800, 3600].find(
          (s) => s >= seconds / Math.max(2, width / 85),
        ) ?? 7200;
    return Array.from({ length: Math.min(4000, Math.floor(seconds / step) + 1) }, (_, i) => ({
      frame: i * step * AUDIO_RATE,
      label: step < 1 ? `${(i * step).toFixed(2)}s` : `${i * step}s`,
    }));
  });
  const beats = $derived.by(() => {
    const step = (AUDIO_RATE * 60) / audio.bpm;
    if (samples / step > 2000 || width / (samples / step) < 7) return [];
    return Array.from({ length: Math.ceil(samples / step) + 1 }, (_, i) => ({
      frame: audio.beat_offset + i * step,
      major: i % 4 === 0,
    }));
  });
  function at(e: PointerEvent) {
    const b = ruler?.getBoundingClientRect();
    return b ? ((e.clientX - b.left) / b.width) * samples : 0;
  }
  let scrub = false;
  type Drag = {
    kind: "move" | "left" | "right" | "slip" | "fadeIn" | "fadeOut";
    track: string;
    target: string;
    original: AudioClip;
    next: AudioClip;
    mouse: number;
  };
  let drag = $state<Drag | null>(null);
  function begin(e: PointerEvent, track: AudioTrack, clip: AudioClip, kind: Drag["kind"] = "move") {
    if (e.button !== 0) return;
    e.stopPropagation();
    editor.audioSelection = { track: track.id, clip: clip.id };
    if (track.locked) return;
    e.preventDefault();
    pause();
    editor.timelineGesture = true;
    drag = {
      kind: kind === "move" && e.altKey ? "slip" : kind,
      track: track.id,
      target: track.id,
      original: structuredClone($state.snapshot(clip)),
      next: structuredClone($state.snapshot(clip)),
      mouse: at(e),
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", finish);
    window.addEventListener("pointercancel", cancel);
  }
  function cleanup() {
    if (dragRaf) cancelAnimationFrame(dragRaf);
    dragRaf = 0;
    pendingPointer = null;
    editor.timelineGesture = false;
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", finish);
    window.removeEventListener("pointercancel", cancel);
  }
  function cancel() {
    drag = null;
    cleanup();
  }
  let pendingPointer: PointerEvent | null = null,
    dragRaf = 0;
  function move(e: PointerEvent) {
    pendingPointer = e;
    if (!dragRaf)
      dragRaf = requestAnimationFrame(() => {
        dragRaf = 0;
        const event = pendingPointer;
        pendingPointer = null;
        if (event) applyDrag(event);
      });
  }
  function applyDrag(e: PointerEvent) {
    if (!drag) return;
    const d = drag,
      c = d.original;
    const media = editor.project?.media[String(c.media)]?.audio;
    if (!media) return;
    const delta = snapAudio(at(e) - d.mouse, e.shiftKey);
    const next = structuredClone($state.snapshot(c));
    if (d.kind === "move") {
      next.start_frame = Math.max(
        0,
        Math.min(
          Math.max(0, samples - c.duration_frames),
          snapAudio(c.start_frame + at(e) - d.mouse, e.shiftKey),
        ),
      );
      const target = document
        .elementFromPoint(e.clientX, e.clientY)
        ?.closest("[data-audio-track]")
        ?.getAttribute("data-audio-track");
      if (target && !audio.tracks.find((t) => t.id === target)?.locked) d.target = target;
    } else if (d.kind === "left") {
      const a = (0 - c.source_offset) / c.rate,
        b = (media.frames - 1 - c.source_offset) / c.rate;
      const dt = Math.max(
        Math.ceil(Math.min(a, b)),
        -c.start_frame,
        Math.min(c.duration_frames - 1, delta),
      );
      d.next = trimLeft(c, Math.min(dt, Math.floor(Math.max(a, b))));
      return;
    } else if (d.kind === "right") {
      const max =
        c.rate > 0
          ? Math.floor((media.frames - 1 - c.source_offset) / c.rate) + 1
          : Math.floor(c.source_offset / -c.rate) + 1;
      next.duration_frames = Math.max(
        1,
        Math.min(max, samples - c.start_frame, c.duration_frames + delta),
      );
    } else if (d.kind === "slip") {
      const distance = (c.duration_frames - 1) * c.rate;
      const low = Math.max(0, -distance),
        high = Math.min(media.frames - 1, media.frames - 1 - distance);
      next.source_offset = Math.max(low, Math.min(high, c.source_offset + delta * c.rate));
    } else if (d.kind === "fadeIn")
      next.fade_in = {
        start: 0,
        end: Math.max(1, Math.min(c.duration_frames, snapAudio(at(e) - c.start_frame, e.shiftKey))),
        shape: c.fade_in?.shape ?? "equal_power",
      };
    else
      next.fade_out = {
        start: Math.max(
          0,
          Math.min(c.duration_frames - 1, snapAudio(at(e) - c.start_frame, e.shiftKey)),
        ),
        end: c.duration_frames,
        shape: c.fade_out?.shape ?? "equal_power",
      };
    d.next = next;
  }
  async function finish() {
    if (pendingPointer) applyDrag(pendingPointer);
    const d = drag;
    cleanup();
    if (!d) return;
    const ok = await editAudio(
      {
        move: "Moved audio clip",
        left: "Trimmed audio in point",
        right: "Trimmed audio out point",
        slip: "Slipped audio source",
        fadeIn: "Adjusted fade in",
        fadeOut: "Adjusted fade out",
      }[d.kind],
      (a) => {
        const from = a.tracks.find((t) => t.id === d.track),
          to = a.tracks.find((t) => t.id === d.target);
        if (!from || !to || from.locked || to.locked) return;
        from.clips = from.clips.filter((c) => c.id !== d.original.id);
        to.clips.push(d.next);
        to.clips.sort((a, b) => a.start_frame - b.start_frame);
      },
    );
    if (drag === d) drag = null;
    if (ok && editor.audioSelection?.clip === d.original.id)
      editor.audioSelection = { track: d.target, clip: d.next.id };
  }
  const shown = (clip: AudioClip) => (drag?.original.id === clip.id ? drag.next : clip);
  function clipMenu(e: MouseEvent, track: AudioTrack, clip: AudioClip) {
    e.preventDefault();
    e.stopPropagation();
    editor.audioSelection = { track: track.id, clip: clip.id };
    const items: ContextMenuItem[] = [
      { label: "Split at playhead", icon: "scissors", run: () => void splitAudio() },
      { label: "Duplicate clip", icon: "copy", run: () => void duplicateAudio() },
      { label: "Crossfade with neighbor", icon: "wave", run: () => void crossfadeAudio() },
      { label: "Reverse clip", icon: "rewind", run: () => void reverseAudio() },
      { label: "Normalize peaks", icon: "graph", run: () => void normalizeAudio() },
      { separator: true },
      { label: "Detect beats", icon: "graph", run: () => void detectBeats() },
      {
        label: "Cut video to beat ⚡",
        icon: "scissors",
        run: () => void cutToBeat("remix"),
      },
      {
        label: "Beat pulse (no cuts)",
        icon: "star",
        run: () => void cutToBeat("pulse"),
      },
      {
        label: "Lyric video ⚡",
        icon: "bolt",
        run: () =>
          (editor.dialog = {
            kind: "lyrics",
            assetId: clip.media,
            hasGrid: !!editor.project?.media[String(clip.media)]?.audio?.beat_grid?.length,
          }),
      },
      {
        label: "Kaya ⚡ speech & narrator",
        icon: "wave",
        run: () => (editor.dialog = { kind: "kaya", assetId: clip.media }),
      },
      { separator: true },
      {
        label: clip.muted ? "Unmute clip" : "Mute clip",
        icon: clip.muted ? "eye" : "eye-off",
        run: () =>
          void editAudio(clip.muted ? "Unmuted clip" : "Muted clip", (a) => {
            const c = a.tracks.find((t) => t.id === track.id)?.clips.find((c) => c.id === clip.id);
            if (c) c.muted = !clip.muted;
          }),
      },
      { separator: true },
      { label: "Delete clip", icon: "trash", danger: true, run: () => void deleteAudio() },
    ];
    openContextMenu(e, items);
  }
  function trackMenu(e: MouseEvent, track: AudioTrack) {
    e.preventDefault();
    editor.audioSelection = { track: track.id, clip: null };
    const items: ContextMenuItem[] = [
      {
        label: track.muted ? "Unmute track" : "Mute track",
        icon: track.muted ? "eye" : "eye-off",
        run: () =>
          void editAudio(track.muted ? "Unmuted track" : "Muted track", (a) => {
            const t = a.tracks.find((t) => t.id === track.id);
            if (t) t.muted = !track.muted;
          }),
      },
      {
        label: track.solo ? "Unsolo track" : "Solo track",
        icon: "star",
        run: () =>
          void editAudio(track.solo ? "Unsoloed track" : "Soloed track", (a) => {
            const t = a.tracks.find((t) => t.id === track.id);
            if (t) t.solo = !track.solo;
          }),
      },
      {
        label: track.locked ? "Unlock track" : "Lock track",
        icon: track.locked ? "unlock" : "lock",
        run: () =>
          void editAudio(track.locked ? "Unlocked track" : "Locked track", (a) => {
            const t = a.tracks.find((t) => t.id === track.id);
            if (t) t.locked = !track.locked;
          }),
      },
      { separator: true },
      { label: "Add marker at playhead", icon: "target", run: () => void addMarker() },
      { label: "New audio track", icon: "plus", run: () => void addAudioTrack() },
      { label: "Import audio file…", icon: "upload", run: () => void importAudio() },
    ];
    openContextMenu(e, items);
  }
  function laneMenu(e: MouseEvent, track: AudioTrack) {
    e.preventDefault();
    editor.audioSelection = { track: track.id, clip: null };
    openContextMenu(e, [
      { label: "Add marker at playhead", icon: "target", run: () => void addMarker() },
      {
        label: "Import audio here…",
        icon: "upload",
        run: () => void importAudio(undefined, track.id),
      },
      { separator: true },
      { label: "New audio track", icon: "plus", run: () => void addAudioTrack() },
      { label: "New mix bus", icon: "graph", run: () => void addMixBus() },
    ]);
  }
  function pointLine(clip: AudioClip) {
    const points = [
      { frame: 0, value: envelope(clip.gain_points, 0, clip.gain_db) },
      ...clip.gain_points.filter((p) => p.frame > 0 && p.frame < clip.duration_frames),
      {
        frame: clip.duration_frames,
        value: envelope(clip.gain_points, clip.duration_frames, clip.gain_db),
      },
    ];
    return points
      .map(
        (p) =>
          `${pctLocal(p.frame, clip)},${54 - ((Math.max(-60, Math.min(12, p.value)) + 60) / 72) * 48}`,
      )
      .join(" ");
  }
  const pctLocal = (frame: number, clip: AudioClip) => (frame / clip.duration_frames) * 1000;
  function trackMeter(id: string) {
    return editor.audioTrackMeters.find((m) => m.compId === comp?.id && m.trackId === id)?.meter;
  }
  onMount(() => {
    if (!root) return;
    const ro = new ResizeObserver((entries) => (visibleWidth = entries[0].contentRect.width));
    ro.observe(root);
    return () => ro.disconnect();
  });
  onDestroy(cleanup);
</script>

<div
  class="audio-view"
  bind:this={root}
  aria-label="Audio timeline"
  data-playhead-sample={editor.audioCursor}
  data-audio-status={editor.audioStatus}
  data-audio-underruns={editor.audioUnderruns}
  data-device-rate={editor.audioDeviceRate}
  data-master-peak={Math.max(...editor.audioMeter.peak)}
>
  <div class="audio-tools">
    <button class="audio-import" disabled={editor.audioImporting} onclick={() => void importAudio()}
      ><Icon name="plus" size={12} />{editor.audioImporting ? "Decoding…" : "Import audio"}</button
    >
    <button
      class="tool"
      title="Add empty audio track"
      aria-label="Add audio track"
      onclick={() => void addAudioTrack()}
      ><Icon name="layers" size={13} /><span>Track</span></button
    >
    {#if editor.audioProtocol >= 2}<button
        class="tool"
        aria-label="Create mix bus"
        onclick={() => void addMixBus()}><Icon name="sliders" size={12} /><span>Bus</span></button
      >{/if}
    <span class="sep"></span>
    <button
      class="tool"
      aria-label="Split audio at playhead"
      disabled={!editor.audioSelection?.clip}
      onclick={() => void splitAudio()}><Icon name="scissors" size={13} /></button
    >
    <button
      class="tool"
      aria-label="Duplicate audio clip"
      disabled={!editor.audioSelection?.clip}
      onclick={() => void duplicateAudio()}><Icon name="copy" size={13} /></button
    >
    <button
      class="tool"
      aria-label="Crossfade overlapping audio"
      disabled={!editor.audioSelection?.clip}
      onclick={() => void crossfadeAudio()}><span>⋈</span></button
    >
    <button
      class="tool"
      aria-label="Delete selected audio"
      disabled={!editor.audioSelection}
      onclick={() => void deleteAudio()}><Icon name="trash" size={12} /></button
    >
    <span class="sep"></span><label class="snap"
      >Snap <select aria-label="Audio snap" bind:value={editor.audioSnap}
        ><option value="sample">Sample</option><option value="frame">Video frame</option><option
          value="beat">Beat</option
        ></select
      ></label
    >
    <label class="tempo"
      ><input
        type="number"
        aria-label="Composition tempo"
        min="20"
        max="400"
        step="1"
        value={audio.bpm}
        onchange={(e) => {
          const value = Number(e.currentTarget.value);
          void editAudio("Changed tempo grid", (a) => (a.bpm = value));
        }}
      /> BPM</label
    >
    <button class="tool" aria-label="Add audio marker" onclick={() => void addMarker()}
      ><Icon name="marker" size={12} /></button
    >
    <span class="spacer"></span>
    <button
      class="tool"
      class:active={editor.audioLoop}
      aria-label="Loop playback"
      aria-pressed={editor.audioLoop}
      onclick={() => {
        pause();
        editor.audioLoop = !editor.audioLoop;
      }}><Icon name="loop" size={13} /></button
    >
    <button
      class="tool"
      class:active={editor.audioScrub}
      aria-label="Scrub audition"
      aria-pressed={editor.audioScrub}
      onclick={() => {
        editor.audioScrub = !editor.audioScrub;
        if (editor.audioScrub) void audioTransport.unlock();
      }}><Icon name="headphones" size={13} /></button
    >
    <span class="engine">48K <span>FLOAT</span></span>
  </div>
  <div class="audio-scroll">
    <div class="audio-grid" style={`width:${252 + width}px;grid-template-columns:252px ${width}px`}>
      <div class="lane-title sticky">
        <span>AUDIO TRACKS</span><span class="spacer"></span><span>{audio.tracks.length} / 256</span
        >
      </div>
      <div
        class="audio-ruler"
        bind:this={ruler}
        role="slider"
        tabindex="0"
        aria-label="Audio sample ruler"
        aria-valuemin="0"
        aria-valuemax={samples}
        aria-valuenow={editor.audioCursor}
        onkeydown={(e) => {
          if (e.key === "ArrowRight") {
            pause();
            seekAudio(editor.audioCursor + 1);
          }
          if (e.key === "ArrowLeft") {
            pause();
            seekAudio(editor.audioCursor - 1);
          }
          e.stopPropagation();
        }}
        onpointerdown={(e) => {
          if (e.button !== 0) return;
          pause();
          scrub = true;
          ruler?.setPointerCapture(e.pointerId);
          seekAudio(Math.max(0, snapAudio(at(e), e.shiftKey)));
        }}
        onpointermove={(e) => {
          if (scrub) seekAudio(Math.max(0, snapAudio(at(e), e.shiftKey)));
        }}
        onpointerup={() => (scrub = false)}
        onpointercancel={() => (scrub = false)}
      >
        {#each ticks as t}<span class="audio-tick" style={`left:${pct(t.frame)}%`}>{t.label}</span
          >{/each}
        {#each audio.markers as marker}<button
            class="marker"
            style={`left:${pct(marker.frame)}%`}
            aria-label={`Marker ${marker.name}`}
            title={marker.name}
            onpointerdown={(e) => e.stopPropagation()}
            onclick={() => {
              pause();
              seekAudio(marker.frame);
            }}><Icon name="marker" size={9} /><span>{marker.name}</span></button
          >{/each}
        <span class="audio-playhead head" style={`left:${pct(editor.audioCursor)}%`}></span>
      </div>
      {#each audio.tracks as track (track.id)}
        <div
          class="track-label sticky"
          class:selected={editor.audioSelection?.track === track.id}
          style={`--track-color:${track.color}`}
          role="group"
          oncontextmenu={(e) => trackMenu(e, track)}
        >
          <button
            class="track-select"
            aria-label={`Select audio track ${track.name}`}
            onclick={() => (editor.audioSelection = { track: track.id, clip: null })}
            ><span></span><Icon name="wave" size={15} /></button
          >
          <div class="track-details">
            <input
              aria-label={`Audio track name ${track.name}`}
              value={track.name}
              onchange={(e) => {
                const name = e.currentTarget.value;
                void editAudio("Renamed audio track", (a) => {
                  const t = a.tracks.find((t) => t.id === track.id);
                  if (t) t.name = name;
                });
              }}
            />
            <div class="track-controls">
              <button
                class:pressed={track.muted}
                aria-label={`Mute audio track ${track.name}`}
                aria-pressed={track.muted}
                onclick={() =>
                  void editAudio("Toggled track mute", (a) => {
                    a.tracks.find((t) => t.id === track.id)!.muted = !track.muted;
                  })}>M</button
              >
              <button
                class:solo={track.solo}
                aria-label={`Solo audio track ${track.name}`}
                aria-pressed={track.solo}
                onclick={() =>
                  void editAudio("Toggled track solo", (a) => {
                    a.tracks.find((t) => t.id === track.id)!.solo = !track.solo;
                  })}>S</button
              >
              <button
                class:pressed={track.locked}
                aria-label={`Lock audio track ${track.name}`}
                aria-pressed={track.locked}
                onclick={() =>
                  void editAudio("Toggled audio track lock", (a) => {
                    a.tracks.find((t) => t.id === track.id)!.locked = !track.locked;
                  })}><Icon name={track.locked ? "lock" : "unlock"} size={9} /></button
              >
              <button
                class="gain"
                aria-label={`Mix audio track ${track.name}`}
                onclick={() => (editor.audioSelection = { track: track.id, clip: null })}
                >{dbLabel(track.gain_db)}</button
              >
            </div>
          </div>
          <div class="mini-meter" title="Post-track sample peak">
            <span
              style={`height:${Math.max(0, Math.min(100, ((peakDb(trackMeter(track.id)?.peak[0] ?? 0) + 60) / 60) * 100))}%`}
            ></span><span
              style={`height:${Math.max(0, Math.min(100, ((peakDb(trackMeter(track.id)?.peak[1] ?? 0) + 60) / 60) * 100))}%`}
            ></span>
          </div>
        </div>
        <div
          class="audio-lane"
          data-audio-track={track.id}
          class:muted={track.muted}
          class:drop-target={drag && drag.target === track.id && drag.track !== track.id}
          role="group"
          oncontextmenu={(e) => {
            if (e.target === e.currentTarget) laneMenu(e, track);
          }}
        >
          {#each beats as beat}<span
              class:major={beat.major}
              class="beat"
              style={`left:${pct(beat.frame)}%`}
            ></span>{/each}
          {#each track.clips as original (original.id)}
            {@const clip = shown(original)}
            <div
              class="audio-clip"
              class:selected={editor.audioSelection?.clip === clip.id}
              class:clip-muted={clip.muted}
              role="button"
              tabindex="0"
              aria-label={`Audio clip ${clip.name}`}
              title={`${clip.name} · ${clip.duration_frames} samples · Alt-drag to slip source; Shift bypasses snap`}
              style={`left:${pct(clip.start_frame)}%;width:${Math.max(0.015, pct(clip.duration_frames))}%;--track-color:${track.color}`}
              onpointerdown={(e) => begin(e, track, original)}
              oncontextmenu={(e) => clipMenu(e, track, original)}
              onkeydown={(e) => {
                if (e.key === "Enter") editor.audioSelection = { track: track.id, clip: clip.id };
                e.stopPropagation();
              }}
            >
              <span class="clip-title"
                >{clip.rate < 0 ? "↶ " : ""}{clip.name}<small
                  >{Math.abs(clip.rate) !== 1 ? `${Math.abs(clip.rate)}×` : ""}</small
                ></span
              >
              <AudioWaveform {clip} color={track.color} />
              <svg
                class="gain-envelope"
                viewBox="0 0 1000 60"
                preserveAspectRatio="none"
                aria-label={`Gain envelope for ${clip.name}`}
                ><polyline
                  points={pointLine(clip)}
                  fill="none"
                  stroke="#efcf83"
                  stroke-width="1"
                  vector-effect="non-scaling-stroke"
                />{#each clip.gain_points.filter((p) => p.frame >= 0 && p.frame <= clip.duration_frames) as p}<circle
                    cx={pctLocal(p.frame, clip)}
                    cy={54 - ((Math.max(-60, Math.min(12, p.value)) + 60) / 72) * 48}
                    r="3"
                    fill="#efcf83"
                    vector-effect="non-scaling-stroke"
                  />{/each}</svg
              >
              {#if clip.fade_in}<div
                  class="fade fade-in"
                  style={`width:${Math.max(0, Math.min(100, (clip.fade_in.end / clip.duration_frames) * 100))}%`}
                ></div>{/if}
              {#if clip.fade_out}<div
                  class="fade fade-out"
                  style={`width:${Math.max(0, Math.min(100, ((clip.duration_frames - clip.fade_out.start) / clip.duration_frames) * 100))}%`}
                ></div>{/if}
              <button
                class="trim left"
                aria-label={`Trim audio start ${clip.name}`}
                onpointerdown={(e) => begin(e, track, original, "left")}
              ></button>
              <button
                class="trim right"
                aria-label={`Trim audio end ${clip.name}`}
                onpointerdown={(e) => begin(e, track, original, "right")}
              ></button>
              <button
                class="fade-handle in"
                style={`left:${Math.max(0, Math.min(95, ((clip.fade_in?.end ?? 0) / clip.duration_frames) * 100))}%`}
                aria-label={`Fade in ${clip.name}`}
                onpointerdown={(e) => begin(e, track, original, "fadeIn")}
              ></button>
              <button
                class="fade-handle out"
                style={`right:${Math.max(0, Math.min(95, ((clip.duration_frames - (clip.fade_out?.start ?? clip.duration_frames)) / clip.duration_frames) * 100))}%`}
                aria-label={`Fade out ${clip.name}`}
                onpointerdown={(e) => begin(e, track, original, "fadeOut")}
              ></button>
            </div>
          {/each}
          <span class="audio-playhead" style={`left:${pct(editor.audioCursor)}%`}></span>
        </div>
      {/each}
    </div>
    {#if !audio.tracks.length}<div class="audio-empty">
        <Icon name="wave" size={26} /><strong>Give your motion a voice.</strong><span
          >Import music, dialogue or effects. Every clip stays non-destructive.</span
        ><button onclick={() => void importAudio()}>Import audio</button><small
          >WAV · MP3 · FLAC · OGG · AIFF · M4A · AAC</small
        >
      </div>{/if}
  </div>
  <div class="audio-status">
    <span class="clock-dot" class:buffering={editor.audioStatus.startsWith("Buffering")}
    ></span><span>{editor.audioStatus}</span>{#if editor.audioUnderruns}<span class="underruns"
        >{editor.audioUnderruns} stalled device samples</span
      >{/if}<span class="spacer"></span><span class="sample-readout"
      >{editor.audioCursor.toLocaleString()} samples</span
    ><span>Shift: precise · Alt-drag: slip</span>
  </div>
</div>

<style>
  .audio-view {
    display: flex;
    flex-direction: column;
    min-height: 0;
    flex: 1;
    background: #191e20;
    color: #bac7c8;
  }
  .audio-tools {
    height: 34px;
    min-height: 34px;
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 0 12px;
    border-bottom: 1px solid #303c3e;
    background: #21282a;
    font-size: 9px;
  }
  .audio-import {
    display: flex;
    align-items: center;
    gap: 5px;
    background: #6c9ba322;
    border: 1px solid #4d7379;
    color: #afd2d8;
    border-radius: 4px;
    font-size: 9px;
    padding: 4px 8px;
  }
  .tool {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    min-width: 24px;
    height: 23px;
    font-size: 10px;
    color: #93a8ac;
  }
  .tool.active {
    background: #719c9e25;
    color: #a4d5d7;
    border-radius: 3px;
  }
  .sep {
    height: 13px;
    width: 1px;
    background: #465254;
    margin: 0 5px;
  }
  .snap {
    display: flex;
    align-items: center;
    gap: 5px;
    color: #7f979b;
  }
  .snap select {
    font-size: 9px;
    width: 87px;
    color: #adc6ca;
  }
  .tempo {
    display: flex;
    align-items: center;
    font-size: 8px;
    color: #779094;
  }
  .tempo input {
    width: 39px;
    background: #172022;
    border: 1px solid #354548;
    font-size: 9px;
    color: #b3cdd0;
    padding: 3px;
    text-align: center;
    border-radius: 3px;
  }
  .engine {
    font: 8px monospace;
    color: #a9c7c9;
    margin-left: 5px;
    letter-spacing: 0.5px;
  }
  .engine span {
    color: #6e949a;
    margin-left: 3px;
  }
  .audio-scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
    position: relative;
  }
  .audio-grid {
    display: grid;
    position: relative;
    min-height: 26px;
  }
  .sticky {
    position: sticky;
    left: 0;
    z-index: 4;
    background: #232b2d;
    border-right: 1px solid #465355;
  }
  .lane-title {
    height: 27px;
    display: flex;
    align-items: center;
    padding: 0 14px;
    font: 8px monospace;
    letter-spacing: 0.8px;
    color: #7d9296;
    border-bottom: 1px solid #3b494c;
  }
  .audio-ruler {
    height: 27px;
    position: relative;
    background: #20282a;
    border-bottom: 1px solid #405055;
    cursor: ew-resize;
    overflow: hidden;
  }
  .audio-tick {
    position: absolute;
    top: 0;
    bottom: 0;
    padding: 4px 5px;
    border-left: 1px solid #405055;
    font: 8px monospace;
    color: #8ba5aa;
    white-space: nowrap;
  }
  .marker {
    position: absolute;
    display: flex;
    gap: 3px;
    top: 12px;
    color: #dec384;
    z-index: 3;
    font: 7px monospace;
    white-space: nowrap;
    max-width: 100px;
    overflow: hidden;
  }
  .track-label {
    height: 84px;
    display: flex;
    align-items: center;
    gap: 7px;
    border-bottom: 1px solid #354447;
    padding: 8px 10px;
  }
  .track-label.selected {
    background: #2b383c;
  }
  .track-select {
    display: flex;
    gap: 8px;
    align-items: center;
    color: var(--track-color);
    height: 100%;
  }
  .track-select > span {
    width: 3px;
    height: 44px;
    background: var(--track-color);
    border-radius: 2px;
    opacity: 0.8;
  }
  .track-details {
    flex: 1;
    min-width: 0;
  }
  .track-details > input {
    width: 100%;
    font-size: 10px;
    background: transparent;
    border: none;
    color: #c4d7da;
    padding: 3px 0;
  }
  .track-controls {
    display: flex;
    gap: 4px;
    align-items: center;
    margin-top: 7px;
  }
  .track-controls > button {
    border: 1px solid #48565a;
    border-radius: 3px;
    color: #819aa0;
    min-width: 20px;
    height: 18px;
    font: 8px monospace;
  }
  .track-controls > button.pressed {
    background: #777f7d;
    color: #e2e6e5;
  }
  .track-controls > button.solo {
    background: #baa35b;
    color: #171d1b;
    border-color: #baa35b;
  }
  .track-controls > button.gain {
    font: 8px monospace;
    border: none;
    padding: 0 4px;
    color: #a9c3c7;
    white-space: nowrap;
  }
  .mini-meter {
    height: 44px;
    width: 10px;
    display: flex;
    align-items: flex-end;
    gap: 2px;
    background: #122125;
  }
  .mini-meter > span {
    display: block;
    width: 4px;
    background: #92c9b0;
    min-height: 1px;
  }
  .audio-lane {
    height: 84px;
    position: relative;
    background: #1a2224;
    border-bottom: 1px solid #344245;
    overflow: hidden;
  }
  .audio-lane.muted {
    opacity: 0.48;
  }
  .audio-lane.drop-target {
    background: #344947;
  }
  .beat {
    position: absolute;
    top: 0;
    bottom: 0;
    border-left: 1px solid #82989c0c;
    pointer-events: none;
  }
  .beat.major {
    border-color: #8399a125;
  }
  .audio-clip {
    position: absolute;
    top: 8px;
    bottom: 8px;
    border-radius: 5px;
    border: 1px solid color-mix(in srgb, var(--track-color) 60%, #233437);
    background: color-mix(in srgb, var(--track-color) 18%, #152326);
    min-width: 4px;
    overflow: hidden;
    cursor: grab;
    outline: none;
  }
  .audio-clip.selected {
    border-color: #d9ebd5;
    box-shadow: 0 0 0 1px #d4e4c63d;
  }
  .audio-clip.clip-muted {
    opacity: 0.42;
  }
  .clip-title {
    position: absolute;
    left: 10px;
    right: 8px;
    top: 5px;
    font-size: 9px;
    color: #d0e1e2;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    z-index: 1;
    pointer-events: none;
  }
  .clip-title small {
    margin-left: 8px;
    font-size: 7px;
    color: #89acb2;
  }
  .gain-envelope {
    position: absolute;
    inset: 18px 0 0;
    width: 100%;
    height: calc(100% - 18px);
    opacity: 0.65;
    pointer-events: none;
  }
  .trim {
    position: absolute;
    top: 9px;
    bottom: 0;
    width: 6px;
    background: transparent;
    cursor: ew-resize;
    z-index: 6;
  }
  .trim:hover {
    background: #d0e5db66;
  }
  .trim.left {
    left: 0;
  }
  .trim.right {
    right: 0;
  }
  .fade {
    position: absolute;
    inset: 0 auto 0 0;
    pointer-events: none;
    background: linear-gradient(90deg, #06131888, transparent);
    clip-path: polygon(0 0, 100% 0, 0 100%);
  }
  .fade-out {
    left: auto;
    right: 0;
    transform: scaleX(-1);
  }
  .fade-handle {
    position: absolute;
    top: 0;
    width: 10px;
    height: 10px;
    z-index: 7;
    background: #d1c394;
    clip-path: polygon(0 0, 100% 0, 0 100%);
    cursor: ew-resize;
    opacity: 0.85;
  }
  .fade-handle.out {
    transform: scaleX(-1);
  }
  .audio-playhead {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 1px;
    background: #cfe8d1aa;
    z-index: 8;
    pointer-events: none;
  }
  .audio-playhead.head:before {
    content: "";
    position: absolute;
    top: 0;
    left: -4px;
    width: 9px;
    height: 8px;
    background: #d6e8cf;
    clip-path: polygon(0 0, 100% 0, 100% 60%, 50% 100%, 0 60%);
  }
  .audio-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 24px 20px;
    color: #6d999f;
  }
  .audio-empty strong {
    font-size: 13px;
    font-weight: 400;
    color: #a8c5c8;
  }
  .audio-empty > span {
    font-size: 10px;
    color: #6f939a;
  }
  .audio-empty button {
    border: 1px solid #547b81;
    border-radius: 4px;
    color: #accfd2;
    font-size: 10px;
    padding: 5px 12px;
    margin-top: 4px;
  }
  .audio-empty small {
    font: 7px monospace;
    letter-spacing: 0.4px;
    color: #6b878d;
  }
  .audio-status {
    display: flex;
    align-items: center;
    gap: 7px;
    height: 23px;
    min-height: 23px;
    border-top: 1px solid #334347;
    font-size: 8px;
    color: #77959b;
    padding: 0 12px;
    background: #1d2729;
  }
  .clock-dot {
    width: 4px;
    height: 4px;
    background: #81b9a6;
    border-radius: 50%;
  }
  .clock-dot.buffering {
    background: #d4af67;
  }
  .sample-readout {
    font: 8px monospace;
    color: #a8c1c4;
    margin-right: 14px;
  }
</style>
