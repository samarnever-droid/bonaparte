<script lang="ts">
  import Icon from "./Icon.svelte";
  import AudioProcessing from "./AudioProcessing.svelte";
  import {
    editor,
    arrangement,
    seekAudio,
    pause,
    audioTransport,
    exportFile,
  } from "../store.svelte";
  import {
    selectedAudio,
    updateClip,
    updateTrack,
    editAudio,
    reverseAudio,
    normalizeAudio,
    toggleGainKey,
    deleteAudio,
    splitAudio,
    crossfadeAudio,
    addMixBus,
    removeMixBus,
  } from "../audio/actions";
  import { AUDIO_RATE, envelope, peakDb, dbLabel, type AudioClip } from "../audio/model";
  const selected = $derived(selectedAudio()),
    track = $derived(selected.track),
    clip = $derived(selected.clip),
    audio = $derived(arrangement());
  const bus = $derived(!track ? audio.buses?.find((b) => b.id === editor.audioBusSelection) : null);
  const asset = $derived(clip ? editor.project?.media[String(clip.media)] : null);
  const local = $derived(
    clip
      ? Math.max(0, Math.min(clip.duration_frames - 1, editor.audioCursor - clip.start_frame))
      : 0,
  );
  const gain = $derived(clip ? envelope(clip.gain_points, local, clip.gain_db) : 0);
  const pan = $derived(clip ? envelope(clip.pan_points, local, clip.pan) : 0);
  function gainChange(value: number) {
    void updateClip("Changed clip gain", (c) => {
      if (c.gain_points.length) {
        c.gain_points = c.gain_points.filter((p) => p.frame !== local);
        c.gain_points.push({ frame: local, value });
        c.gain_points.sort((a, b) => a.frame - b.frame);
      } else c.gain_db = value;
    });
  }
  function panChange(value: number) {
    void updateClip("Changed clip pan", (c) => {
      if (c.pan_points.length) {
        c.pan_points = c.pan_points.filter((p) => p.frame !== local);
        c.pan_points.push({ frame: local, value });
        c.pan_points.sort((a, b) => a.frame - b.frame);
      } else c.pan = value;
    });
  }
  function togglePan() {
    void updateClip("Edited pan automation", (c) => {
      if (c.pan_points.some((p) => p.frame === local))
        c.pan_points = c.pan_points.filter((p) => p.frame !== local);
      else c.pan_points.push({ frame: local, value: pan });
      c.pan_points.sort((a, b) => a.frame - b.frame);
    });
  }
  function setFade(incoming: boolean, ms: number) {
    void updateClip(incoming ? "Changed fade in" : "Changed fade out", (c) => {
      const n = Math.max(0, Math.min(c.duration_frames, Math.round(ms * 48)));
      const value = n
        ? {
            start: incoming ? 0 : c.duration_frames - n,
            end: incoming ? n : c.duration_frames,
            shape: "equal_power" as const,
          }
        : null;
      if (incoming) c.fade_in = value;
      else c.fade_out = value;
    });
  }
  function rate(value: number) {
    void updateClip("Changed varispeed", (c) => {
      if (!Number.isFinite(value) || value < 0.25 || value > 4) return;
      c.duration_frames = Math.max(
        1,
        Math.floor(((c.duration_frames - 1) * Math.abs(c.rate)) / value) + 1,
      );
      c.rate = value * Math.sign(c.rate);
    });
  }
  const meterWidth = (value: number) =>
    Math.max(0, Math.min(100, ((peakDb(value) + 60) / 60) * 100));
</script>

<aside class="panel inspector audio-inspector" aria-label="Audio inspector">
  <div class="panel-heading">
    Audio studio<span class="spacer"></span><Icon name="headphones" size={14} />
  </div>
  <div class="audio-inspector-body">
    <div class="audio-heading">
      <span class="audio-icon"><Icon name="wave" size={20} /></span>
      <div>
        <strong
          >{clip
            ? "Clip inspector"
            : track
              ? "Track mixer"
              : bus
                ? "Mix bus"
                : "Master output"}</strong
        ><span>{clip?.name ?? track?.name ?? bus?.name ?? "48 kHz floating-point mix"}</span>
      </div>
    </div>
    <section class="audio-section">
      <div class="section-name">PLAYHEAD <span>Sample-addressed</span></div>
      <div class="sample-field">
        <input
          type="number"
          aria-label="Audio playhead sample"
          min="0"
          step="1"
          value={editor.audioCursor}
          onchange={(e) => {
            pause();
            seekAudio(Number(e.currentTarget.value));
          }}
        /><span>SAMPLES</span>
      </div>
      <small
        >{(editor.audioCursor / AUDIO_RATE).toFixed(6)} seconds · independent of video-frame snapping</small
      >
    </section>
    {#if bus && editor.audioProtocol >= 2}
      <section class="audio-section">
        <div class="section-name">
          MIX BUS <button class="small-action" onclick={() => void removeMixBus(bus.id)}
            >Delete bus</button
          >
        </div>
        <label class="field full"
          >Name<input
            aria-label="Mix bus name"
            value={bus.name}
            onchange={(e) => {
              const value = e.currentTarget.value;
              void editAudio(
                "Renamed mix bus",
                (a) => (a.buses!.find((b) => b.id === bus.id)!.name = value),
              );
            }}
          /></label
        >
        <div class="field-pair">
          <label class="field"
            >Gain · dB<input
              type="number"
              aria-label="Mix bus gain dB"
              min="-96"
              max="24"
              step=".1"
              value={bus.gain_db}
              onchange={(e) => {
                const value = Number(e.currentTarget.value);
                void editAudio(
                  "Changed mix bus gain",
                  (a) => (a.buses!.find((b) => b.id === bus.id)!.gain_db = value),
                );
              }}
            /></label
          ><label class="field"
            >Balance<input
              type="number"
              aria-label="Mix bus pan"
              min="-1"
              max="1"
              step=".05"
              value={bus.pan}
              onchange={(e) => {
                const value = Number(e.currentTarget.value);
                void editAudio(
                  "Changed mix bus pan",
                  (a) => (a.buses!.find((b) => b.id === bus.id)!.pan = value),
                );
              }}
            /></label
          >
        </div>
        <button
          class="audio-button"
          class:active={bus.muted}
          aria-label="Mute mix bus"
          aria-pressed={bus.muted}
          onclick={() =>
            void editAudio("Toggled mix bus mute", (a) => {
              const b = a.buses!.find((b) => b.id === bus.id)!;
              b.muted = !b.muted;
            })}>Mute bus</button
        >
      </section>
    {/if}
    {#if clip && track}
      <section class="audio-section">
        <div class="section-name">
          CLIP <button
            class="small-action"
            aria-label="Mute selected audio clip"
            aria-pressed={clip.muted}
            onclick={() => void updateClip("Toggled clip mute", (c) => (c.muted = !c.muted))}
            >{clip.muted ? "MUTED" : "AUDIBLE"}</button
          >
        </div>
        <label class="field full"
          >Name<input
            aria-label="Audio clip name"
            value={clip.name}
            disabled={track.locked}
            onchange={(e) => {
              const value = e.currentTarget.value;
              void updateClip("Renamed audio clip", (c) => (c.name = value));
            }}
          /></label
        >
        <div class="field-pair">
          <label class="field"
            >Start · samples<input
              aria-label="Audio clip start sample"
              type="number"
              min="0"
              step="1"
              value={clip.start_frame}
              disabled={track.locked}
              onchange={(e) => {
                const value = Math.round(Number(e.currentTarget.value));
                void updateClip("Moved audio clip", (c) => (c.start_frame = value));
              }}
            /></label
          ><label class="field"
            >Length · samples<input
              aria-label="Audio clip duration samples"
              type="number"
              min="1"
              step="1"
              value={clip.duration_frames}
              disabled={track.locked}
              onchange={(e) => {
                const value = Math.round(Number(e.currentTarget.value));
                void updateClip("Trimmed audio duration", (c) => (c.duration_frames = value));
              }}
            /></label
          >
        </div>
        <label class="field full"
          >Source offset · samples<input
            aria-label="Audio source offset samples"
            type="number"
            min="0"
            step="1"
            value={clip.source_offset}
            disabled={track.locked}
            onchange={(e) => {
              const value = Number(e.currentTarget.value);
              void updateClip("Slipped audio source", (c) => (c.source_offset = value));
            }}
          /></label
        >
        <div class="field-pair">
          <label class="field"
            >Rate · pitch follows<input
              aria-label="Audio playback rate"
              type="number"
              min=".25"
              max="4"
              step=".05"
              value={Math.abs(clip.rate)}
              disabled={track.locked}
              onchange={(e) => rate(Number(e.currentTarget.value))}
            /></label
          ><button
            class="audio-button"
            class:active={clip.rate < 0}
            aria-label="Reverse audio clip"
            disabled={track.locked}
            onclick={() => void reverseAudio()}
            ><Icon name="rotate" size={12} />{clip.rate < 0 ? "Reversed" : "Reverse"}</button
          >
        </div>
      </section>
      <section class="audio-section">
        <div class="section-name">LEVEL & PAN <span>Non-destructive</span></div>
        <div class="gain-head">
          <label for="clip-gain">Gain</label><span class="spacer"></span><input
            id="clip-gain"
            aria-label="Audio clip gain dB"
            type="number"
            min="-96"
            max="24"
            step=".1"
            value={gain.toFixed(2)}
            disabled={track.locked}
            onchange={(e) => gainChange(Number(e.currentTarget.value))}
          /><span>dB</span><button
            class="diamond"
            class:active={clip.gain_points.some((p) => p.frame === local)}
            aria-label="Toggle audio gain keyframe"
            disabled={track.locked}
            onclick={() => void toggleGainKey()}>◇</button
          >
        </div>
        <input
          class="range"
          aria-label="Audio clip gain fader"
          type="range"
          min="-60"
          max="12"
          step=".1"
          value={gain}
          disabled={track.locked}
          onchange={(e) => gainChange(Number(e.currentTarget.value))}
        />
        <div class="gain-head">
          <label for="clip-pan">Pan / balance</label><span class="spacer"></span><input
            id="clip-pan"
            aria-label="Audio clip pan"
            type="number"
            min="-1"
            max="1"
            step=".05"
            value={pan.toFixed(2)}
            disabled={track.locked}
            onchange={(e) => panChange(Number(e.currentTarget.value))}
          /><button
            class="diamond"
            class:active={clip.pan_points.some((p) => p.frame === local)}
            aria-label="Toggle audio pan keyframe"
            disabled={track.locked}
            onclick={togglePan}>◇</button
          >
        </div>
        <input
          class="range"
          aria-label="Audio clip pan fader"
          type="range"
          min="-1"
          max="1"
          step=".01"
          value={pan}
          disabled={track.locked}
          onchange={(e) => panChange(Number(e.currentTarget.value))}
        />
        <div class="automation-count">
          <span>{clip.gain_points.length} gain / {clip.pan_points.length} pan keys</span><button
            disabled={track.locked}
            onclick={() =>
              void updateClip("Cleared audio automation", (c) => {
                c.gain_points = [];
                c.pan_points = [];
              })}>Clear keys</button
          >
        </div>
        <button
          class="audio-button normalize"
          disabled={track.locked}
          onclick={() => void normalizeAudio()}>Peak normalize to −1 dBFS</button
        >
        <small
          >Source-peak normalization, not loudness normalization. Master overload is still possible
          when clips overlap.</small
        >
      </section>
      <section class="audio-section">
        <div class="section-name">FADES <span>Equal-power default</span></div>
        <div class="field-pair">
          <label class="field"
            >Fade in · ms<input
              aria-label="Audio fade in milliseconds"
              type="number"
              min="0"
              step="1"
              value={Math.max(0, clip.fade_in?.end ?? 0) / 48}
              disabled={track.locked}
              onchange={(e) => setFade(true, Number(e.currentTarget.value))}
            /></label
          ><label class="field"
            >Fade out · ms<input
              aria-label="Audio fade out milliseconds"
              type="number"
              min="0"
              step="1"
              value={Math.max(
                0,
                clip.duration_frames - (clip.fade_out?.start ?? clip.duration_frames),
              ) / 48}
              disabled={track.locked}
              onchange={(e) => setFade(false, Number(e.currentTarget.value))}
            /></label
          >
        </div>
        <button class="audio-button" disabled={track.locked} onclick={() => void crossfadeAudio()}
          >Crossfade overlapping neighbor</button
        >
        <div class="button-row">
          <button class="audio-button" disabled={track.locked} onclick={() => void splitAudio()}
            ><Icon name="scissors" size={12} />Split at playhead</button
          ><button class="audio-button" disabled={track.locked} onclick={() => void deleteAudio()}
            ><Icon name="trash" size={12} />Delete</button
          >
        </div>
      </section>
      {#if asset?.audio}<section class="audio-section source-info">
          <div class="section-name">SOURCE <span>{asset.audio.codec.toUpperCase()}</span></div>
          <strong>{asset.name}</strong>
          <p>
            {asset.audio.original_sample_rate.toLocaleString()} Hz · {asset.audio
              .original_channels === 1
              ? "Mono"
              : "Stereo"} · {(asset.audio.frames / AUDIO_RATE).toFixed(3)}s
          </p>
          <small
            >Working PCM: 48 kHz float · original encoded file retained in the project. Mono uses a
            −3 dB center pan law; stereo uses balance.</small
          >
        </section>{/if}
    {:else if track}
      <section class="audio-section">
        <div class="section-name">TRACK MIXER <span>Post-clip controls</span></div>
        <label class="field full"
          >Name<input
            aria-label="Selected audio track name"
            value={track.name}
            onchange={(e) => {
              const value = e.currentTarget.value;
              void updateTrack("Renamed audio track", (t) => (t.name = value));
            }}
          /></label
        >
        <div class="field-pair">
          <label class="field"
            >Gain · dB<input
              aria-label="Audio track gain dB"
              type="number"
              min="-96"
              max="24"
              step=".1"
              value={track.gain_db}
              onchange={(e) => {
                const value = Number(e.currentTarget.value);
                void updateTrack("Changed track gain", (t) => (t.gain_db = value));
              }}
            /></label
          ><label class="field"
            >Balance<input
              aria-label="Audio track pan"
              type="number"
              min="-1"
              max="1"
              step=".05"
              value={track.pan}
              onchange={(e) => {
                const value = Number(e.currentTarget.value);
                void updateTrack("Changed track pan", (t) => (t.pan = value));
              }}
            /></label
          >
        </div>
        <div class="button-row">
          <button
            class="audio-button"
            class:active={track.muted}
            onclick={() => void updateTrack("Toggled track mute", (t) => (t.muted = !t.muted))}
            >Mute</button
          ><button
            class="audio-button"
            class:active={track.solo}
            onclick={() => void updateTrack("Toggled track solo", (t) => (t.solo = !t.solo))}
            >Solo</button
          ><button
            class="audio-button"
            class:active={track.locked}
            onclick={() => void updateTrack("Toggled track lock", (t) => (t.locked = !t.locked))}
            >Lock</button
          >
        </div>
        <label class="field full"
          >Track color<input
            type="color"
            aria-label="Audio track color"
            value={track.color}
            onchange={(e) => {
              const value = e.currentTarget.value;
              void updateTrack("Changed track color", (t) => (t.color = value));
            }}
          /></label
        ><button class="audio-button" disabled={track.locked} onclick={() => void deleteAudio()}
          >Delete track</button
        >
      </section>
    {/if}
    {#if editor.audioProtocol >= 2}<AudioProcessing
        trackId={track?.id ?? null}
        busId={bus?.id ?? null}
      />{/if}
    <section class="audio-section master">
      <div class="section-name">
        MASTER OUTPUT <span class:over={editor.audioMeter.peak.some((v) => v > 1)}
          >{editor.audioMeter.peak.some((v) => v > 1) ? "OVERLOAD" : "SAMPLE PEAK"}</span
        >
      </div>
      {#each [0, 1] as channel}<div class="meter-row">
          <span>{channel === 0 ? "L" : "R"}</span>
          <div class="meter-bar">
            <i
              class:over={editor.audioMeter.peak[channel] > 1}
              style={`width:${meterWidth(editor.audioMeter.peak[channel])}%`}
            ></i>
          </div>
          <span
            >{peakDb(editor.audioMeter.peak[channel]) <= -100
              ? "−∞"
              : peakDb(editor.audioMeter.peak[channel]).toFixed(1)}</span
          >
        </div>{/each}
      <label class="field full"
        >Master gain · dB<input
          aria-label="Audio master gain dB"
          type="number"
          min="-96"
          max="24"
          step=".1"
          value={audio.gain_db}
          onchange={(e) => {
            const value = Number(e.currentTarget.value);
            void editAudio("Changed master gain", (a) => (a.gain_db = value));
          }}
        /></label
      >
      <label class="monitor"
        >Monitor volume<input
          type="range"
          aria-label="Audio monitor volume"
          min="0"
          max="1"
          step=".01"
          value={editor.monitorVolume}
          oninput={(e) => {
            editor.monitorVolume = Number(e.currentTarget.value);
            audioTransport.setMonitor(editor.monitorVolume);
          }}
        /></label
      >
      <small
        >Monitor volume affects listening only. Float mixing preserves headroom; reduce master gain
        if overload is shown. No hidden limiter.</small
      >
      <button class="audio-button export-audio" onclick={() => void exportFile("wav")}
        ><Icon name="download" size={12} />Export float WAV mix</button
      >
    </section>
    {#if audio.markers.length}<section class="audio-section">
        <div class="section-name">MARKERS <span>{audio.bpm} BPM grid</span></div>
        {#each audio.markers as marker}<div class="marker-row">
            <button
              onclick={() => {
                pause();
                seekAudio(marker.frame);
              }}>{(marker.frame / AUDIO_RATE).toFixed(3)}s</button
            ><input
              aria-label={`Marker name ${marker.name}`}
              value={marker.name}
              onchange={(e) => {
                const value = e.currentTarget.value;
                void editAudio(
                  "Renamed marker",
                  (a) => (a.markers.find((m) => m.id === marker.id)!.name = value),
                );
              }}
            /><button
              aria-label={`Delete marker ${marker.name}`}
              onclick={() =>
                void editAudio(
                  "Deleted marker",
                  (a) => (a.markers = a.markers.filter((m) => m.id !== marker.id)),
                )}>×</button
            >
          </div>{/each}
      </section>{/if}
  </div>
</aside>

<style>
  .audio-inspector {
    background: #202729;
  }
  .audio-inspector :global(.panel-heading) {
    background: #242d2f;
    color: #b7ced0;
  }
  .audio-inspector-body {
    overflow: auto;
    flex: 1;
    min-height: 0;
    padding: 0 14px 20px;
  }
  .audio-heading {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 18px 0;
  }
  .audio-heading > div {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .audio-heading strong {
    font-size: 11px;
    font-weight: 500;
    color: #cadddf;
  }
  .audio-heading span:not(.audio-icon) {
    font-size: 9px;
    color: #7e9ca2;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .audio-icon {
    width: 33px;
    height: 33px;
    background: #699da324;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 5px;
    color: #9dc8d0;
  }
  .audio-section {
    padding: 15px 0;
    border-top: 1px solid #3a484c;
  }
  .section-name {
    font: 8px monospace;
    letter-spacing: 1px;
    color: #aecbd0;
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 13px;
  }
  .section-name > span {
    font: 7px sans-serif;
    color: #768f96;
    letter-spacing: 0;
  }
  .sample-field {
    display: flex;
    border: 1px solid #47616a;
    border-radius: 4px;
    align-items: center;
    padding: 6px 8px;
    background: #152126;
  }
  .sample-field > input {
    background: transparent;
    border: 0;
    color: #bed8de;
    width: 100%;
    font: 12px monospace;
  }
  .sample-field > span {
    font: 7px monospace;
    color: #668891;
  }
  .audio-section small {
    display: block;
    color: #758e95;
    font-size: 8px;
    line-height: 1.7;
    margin-top: 8px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 5px;
    color: #90aab0;
    font-size: 9px;
    min-width: 0;
  }
  .field input {
    width: 100%;
    border: 1px solid #3e5259;
    background: #172328;
    border-radius: 3px;
    padding: 6px 7px;
    font: 10px monospace;
    color: #c6dce1;
    min-width: 0;
  }
  .field.full {
    margin-bottom: 10px;
  }
  .field-pair {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
    margin-bottom: 10px;
  }
  .audio-button {
    font-size: 8px;
    color: #b4cfd4;
    padding: 7px;
    border: 1px solid #455d65;
    border-radius: 4px;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    background: #26373d;
  }
  .audio-button.active {
    background: #52777a;
    color: #d9eaeb;
  }
  .button-row {
    display: flex;
    gap: 7px;
    margin-top: 10px;
  }
  .button-row > button {
    flex: 1;
  }
  .gain-head {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 9px;
    color: #a0bac0;
  }
  .gain-head input {
    width: 54px;
    border: 1px solid #3f535a;
    background: #172328;
    border-radius: 3px;
    padding: 4px;
    color: #d6e3db;
    font: 10px monospace;
    text-align: right;
  }
  .gain-head > span:not(.spacer) {
    color: #708b92;
    font-size: 8px;
  }
  .diamond {
    font-size: 15px;
    color: #617e86;
    width: 18px;
  }
  .diamond.active {
    color: #e9cd83;
  }
  .range {
    width: 100%;
    height: 19px;
    accent-color: #83b9c3;
    margin: 6px 0 12px;
  }
  .automation-count {
    display: flex;
    justify-content: space-between;
    font: 8px monospace;
    color: #70949d;
  }
  .automation-count button {
    font-size: 8px;
    color: #afcdd3;
  }
  .normalize {
    margin-top: 14px;
    width: 100%;
  }
  .source-info strong {
    font-size: 9px;
    font-weight: 400;
    color: #c1d5d9;
    overflow-wrap: anywhere;
  }
  .source-info p {
    font: 8px monospace;
    color: #8aadb6;
    margin: 8px 0;
  }
  .master .over {
    color: #f1997e;
  }
  .meter-row {
    display: flex;
    align-items: center;
    gap: 5px;
    height: 17px;
    font: 7px monospace;
    color: #91afb8;
  }
  .meter-row > span:first-child {
    width: 9px;
  }
  .meter-row > span:last-child {
    width: 27px;
    text-align: right;
  }
  .meter-bar {
    height: 5px;
    background: #132126;
    flex: 1;
    border-radius: 1px;
    overflow: hidden;
  }
  .meter-bar i {
    display: block;
    height: 100%;
    background: linear-gradient(90deg, #669991, #bcd297, #dfc67d);
  }
  .meter-bar i.over {
    background: #d38065;
  }
  .master .field {
    margin-top: 13px;
  }
  .monitor {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 8px;
    color: #94b1ba;
    margin-top: 12px;
  }
  .monitor input {
    flex: 1;
    width: 100%;
    min-width: 0;
    accent-color: #79adb5;
  }
  .export-audio {
    width: 100%;
    margin-top: 13px;
  }
  .small-action {
    font: 7px monospace;
    color: #acc9d0;
    border: 1px solid #43575e;
    padding: 3px 5px;
    border-radius: 3px;
  }
  .marker-row {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 8px 0;
  }
  .marker-row button {
    font: 8px monospace;
    color: #b6c4ba;
  }
  .marker-row input {
    font-size: 9px;
    color: #b6cfd7;
    background: #18282e;
    border: 1px solid #334c56;
    border-radius: 3px;
    padding: 4px;
    width: 100%;
    min-width: 0;
  }
</style>
