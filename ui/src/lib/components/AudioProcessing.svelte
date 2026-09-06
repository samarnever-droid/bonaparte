<script lang="ts">
  import { editor, arrangement, activeComp, clone } from "../store.svelte";
  import { editAudio, addMixBus, removeMixBus } from "../audio/actions";
  import {
    defaultProcessing,
    defaultLimiter,
    type AudioProcessing,
    type AudioArrangement,
    type AudioTrack,
    type AudioBus,
  } from "../audio/model";
  import { command } from "../bridge";
  import Icon from "./Icon.svelte";
  let { trackId = null, busId = null }: { trackId?: string | null; busId?: string | null } =
    $props();
  const audio = $derived(arrangement());
  const strip = $derived(
    trackId
      ? audio.tracks.find((t) => t.id === trackId)
      : busId
        ? audio.buses?.find((b) => b.id === busId)
        : audio,
  );
  const processing = $derived(strip?.processing ?? defaultProcessing());
  const master = $derived(!trackId && !busId);
  const limiter = $derived(audio.limiter ?? defaultLimiter());
  const meter = $derived(
    editor.audioProcessingMeters.find(
      (m) =>
        m.compId === activeComp()?.id &&
        (master ? m.kind === "master" : m.id === (trackId ?? busId)),
    ),
  );
  let curve = $state<number[][]>([]),
    curveToken = 0;
  $effect(() => {
    const eq = processing.eq;
    const token = ++curveToken;
    void command<number[][]>("audio_eq_curve", { eq })
      .then((value) => {
        if (token === curveToken) curve = value;
      })
      .catch(() => {});
  });
  const curvePath = $derived(
    curve
      .map(
        (p, i) =>
          `${i ? "L" : "M"}${(i / Math.max(1, curve.length - 1)) * 256},${48 - Math.max(-24, Math.min(24, p[1])) * 1.6}`,
      )
      .join(""),
  );
  function update(
    label: string,
    fn: (strip: AudioArrangement | AudioTrack | AudioBus, a: AudioArrangement) => void,
  ) {
    void editAudio(label, (a) => {
      const s = trackId
        ? a.tracks.find((t) => t.id === trackId)
        : busId
          ? a.buses?.find((b) => b.id === busId)
          : a;
      if (s) fn(s, a);
    });
  }
  function eq(key: string, value: number | boolean) {
    update("Edited channel EQ", (s) => {
      s.processing ??= defaultProcessing();
      (s.processing.eq as unknown as Record<string, number | boolean>)[key] = value;
    });
  }
  function compressor(key: string, value: number | boolean) {
    update("Edited channel compression", (s) => {
      s.processing ??= defaultProcessing();
      (s.processing.compressor as unknown as Record<string, number | boolean>)[key] = value;
    });
  }
  function preset(type: "voice" | "music" | "reset") {
    update("Applied audio processing preset", (s) => {
      const p = defaultProcessing();
      if (type === "voice") {
        p.eq = {
          ...p.eq,
          enabled: true,
          highpass_hz: 80,
          low_db: -1,
          mid_hz: 2500,
          mid_db: 2,
          mid_q: 0.9,
          high_db: 1,
        };
        p.compressor = {
          ...p.compressor,
          enabled: true,
          threshold_db: -20,
          ratio: 3,
          attack_ms: 15,
          release_ms: 120,
        };
      } else if (type === "music") {
        p.eq = { ...p.eq, enabled: true, highpass_hz: 25, low_db: 1.5, mid_db: -1, high_db: 0.5 };
        p.compressor = {
          ...p.compressor,
          enabled: true,
          threshold_db: -14,
          ratio: 1.6,
          attack_ms: 25,
          release_ms: 180,
        };
      }
      s.processing = p;
    });
  }
</script>

<div class="channel-strip" aria-label="Channel processing">
  <div class="strip-heading">
    <Icon name="sliders" size={13} /><strong
      >{master ? "Master processing" : busId ? "Bus processing" : "Track processing"}</strong
    ><span class="spacer"></span><span class="gr">{(meter?.reductionDb ?? 0).toFixed(1)} dB GR</span
    >
  </div>
  <div class="presets">
    <button onclick={() => preset("voice")}>Voice clarity</button><button
      onclick={() => preset("music")}>Warm music</button
    ><button onclick={() => preset("reset")}>Reset</button>
  </div>
  <div class="processor-head">
    <strong>PARAMETRIC EQ</strong><button
      class:enabled={processing.eq.enabled}
      aria-label="Enable channel EQ"
      aria-pressed={processing.eq.enabled}
      onclick={() => eq("enabled", !processing.eq.enabled)}
      >{processing.eq.enabled ? "ON" : "BYPASS"}</button
    >
  </div>
  <svg viewBox="0 0 256 96" role="img" aria-label="Native EQ frequency response" class="eq-curve"
    ><path
      d="M0 48H256 M64 0V96 M128 0V96 M192 0V96"
      stroke="#405054"
      stroke-width=".6"
      fill="none"
    /><path d={curvePath} stroke="#b4d7b6" stroke-width="1.4" fill="none" /><text x="4" y="91"
      >20 Hz</text
    ><text x="221" y="91">20k</text></svg
  >
  <div class="eq-grid">
    <label
      >High pass<input
        type="number"
        aria-label="EQ high pass Hz"
        min="0"
        max="1000"
        step="5"
        value={processing.eq.highpass_hz}
        onchange={(e) => eq("highpass_hz", Number(e.currentTarget.value))}
      /><small>Hz · 0 bypasses</small></label
    >
    <label
      >Low shelf<input
        type="number"
        aria-label="EQ low gain dB"
        min="-18"
        max="18"
        step=".5"
        value={processing.eq.low_db}
        onchange={(e) => eq("low_db", Number(e.currentTarget.value))}
      /><small>dB</small></label
    >
    <label
      >Low frequency<input
        type="number"
        aria-label="EQ low frequency Hz"
        min="20"
        max="1000"
        step="10"
        value={processing.eq.low_hz}
        onchange={(e) => eq("low_hz", Number(e.currentTarget.value))}
      /><small>Hz</small></label
    >
    <label
      >Mid bell<input
        type="number"
        aria-label="EQ mid gain dB"
        min="-18"
        max="18"
        step=".5"
        value={processing.eq.mid_db}
        onchange={(e) => eq("mid_db", Number(e.currentTarget.value))}
      /><small>dB</small></label
    >
    <label
      >Mid frequency<input
        type="number"
        aria-label="EQ mid frequency Hz"
        min="40"
        max="16000"
        step="50"
        value={processing.eq.mid_hz}
        onchange={(e) => eq("mid_hz", Number(e.currentTarget.value))}
      /><small>Hz</small></label
    >
    <label
      >Mid Q<input
        type="number"
        aria-label="EQ mid Q"
        min=".1"
        max="12"
        step=".1"
        value={processing.eq.mid_q}
        onchange={(e) => eq("mid_q", Number(e.currentTarget.value))}
      /><small>Bandwidth</small></label
    >
    <label
      >High shelf<input
        type="number"
        aria-label="EQ high gain dB"
        min="-18"
        max="18"
        step=".5"
        value={processing.eq.high_db}
        onchange={(e) => eq("high_db", Number(e.currentTarget.value))}
      /><small>dB</small></label
    >
    <label
      >High frequency<input
        type="number"
        aria-label="EQ high frequency Hz"
        min="1000"
        max="20000"
        step="100"
        value={processing.eq.high_hz}
        onchange={(e) => eq("high_hz", Number(e.currentTarget.value))}
      /><small>Hz</small></label
    >
  </div>
  <div class="processor-head">
    <strong>STEREO COMPRESSION</strong><button
      class:enabled={processing.compressor.enabled}
      aria-label="Enable channel compressor"
      aria-pressed={processing.compressor.enabled}
      onclick={() => compressor("enabled", !processing.compressor.enabled)}
      >{processing.compressor.enabled ? "ON" : "BYPASS"}</button
    >
  </div>
  <div class="comp-grid">
    {#each [{ id: "threshold_db", label: "Threshold", unit: "dB", min: -60, max: 0, step: 1 }, { id: "ratio", label: "Ratio", unit: ":1", min: 1, max: 20, step: 0.1 }, { id: "attack_ms", label: "Attack", unit: "ms", min: 0.1, max: 200, step: 1 }, { id: "release_ms", label: "Release", unit: "ms", min: 10, max: 2000, step: 10 }, { id: "knee_db", label: "Soft knee", unit: "dB", min: 0, max: 24, step: 1 }, { id: "makeup_db", label: "Makeup", unit: "dB", min: -12, max: 24, step: 0.5 }] as field}
      <label
        >{field.label}<input
          type="number"
          aria-label={`Compressor ${field.label.toLowerCase()}`}
          min={field.min}
          max={field.max}
          step={field.step}
          value={(processing.compressor as unknown as Record<string, number>)[field.id]}
          onchange={(e) => compressor(field.id, Number(e.currentTarget.value))}
        /><small>{field.unit}</small></label
      >
    {/each}
  </div>
  {#if master}
    <div class="processor-head">
      <strong>LOOKAHEAD LIMITER</strong><button
        class:enabled={limiter.enabled}
        aria-label="Enable master limiter"
        aria-pressed={limiter.enabled}
        onclick={() =>
          update("Toggled master limiter", (_, a) => {
            a.limiter ??= defaultLimiter();
            a.limiter.enabled = !a.limiter.enabled;
          })}>{limiter.enabled ? "ON" : "BYPASS"}</button
      >
    </div>
    <div class="comp-grid">
      <label
        >Ceiling<input
          type="number"
          aria-label="Limiter ceiling dB"
          min="-12"
          max="0"
          step=".1"
          value={limiter.ceiling_db}
          onchange={(e) => {
            const value = Number(e.currentTarget.value);
            update("Changed limiter ceiling", (_, a) => {
              a.limiter ??= defaultLimiter();
              a.limiter.ceiling_db = value;
            });
          }}
        /><small>dBFS</small></label
      ><label
        >Release<input
          type="number"
          aria-label="Limiter release ms"
          min="10"
          max="1000"
          step="10"
          value={limiter.release_ms}
          onchange={(e) => {
            const value = Number(e.currentTarget.value);
            update("Changed limiter release", (_, a) => {
              a.limiter ??= defaultLimiter();
              a.limiter.release_ms = value;
            });
          }}
        /><small>ms</small></label
      >
    </div>
    <p class="note">
      5 ms lookahead, compensated in playback/export. Sample-peak protection—not oversampled
      true-peak limiting.
    </p>
  {:else}
    <div class="processor-head">
      <strong>ROUTING & SENDS</strong><Icon name="layers" size={11} />
    </div>
    <label class="route-label"
      >Main output<select
        aria-label="Channel output bus"
        value={(strip as AudioTrack | AudioBus)?.output ?? ""}
        onchange={(e) => {
          const value = e.currentTarget.value;
          update("Routed audio output", (s) => {
            (s as AudioTrack | AudioBus).output = value || null;
          });
        }}
        ><option value="">Master</option>{#each audio.buses ?? [] as b}{#if b.id !== busId}<option
              value={b.id}>{b.name}</option
            >{/if}{/each}</select
      ></label
    >
    {#each (strip as AudioTrack | AudioBus)?.sends ?? [] as send, i}
      <div class="send-row">
        <select
          aria-label={`Audio send ${i + 1} target`}
          value={send.bus}
          onchange={(e) => {
            const value = e.currentTarget.value;
            update("Routed audio send", (s) => {
              (s as AudioTrack | AudioBus).sends![i].bus = value;
            });
          }}
          >{#each audio.buses ?? [] as b}{#if b.id !== busId}<option value={b.id}>{b.name}</option
              >{/if}{/each}</select
        ><input
          type="number"
          aria-label={`Audio send ${i + 1} gain dB`}
          min="-96"
          max="6"
          step=".5"
          value={send.gain_db}
          onchange={(e) => {
            const value = Number(e.currentTarget.value);
            update(
              "Changed send level",
              (s) => ((s as AudioTrack | AudioBus).sends![i].gain_db = value),
            );
          }}
        /><button
          aria-label={`Remove audio send ${i + 1}`}
          onclick={() =>
            update("Removed audio send", (s) => (s as AudioTrack | AudioBus).sends!.splice(i, 1))}
          >×</button
        >
      </div>
    {/each}
    <button
      class="send-add"
      disabled={!(audio.buses ?? []).some((b) => b.id !== busId) ||
        ((strip as AudioTrack | AudioBus)?.sends?.length ?? 0) >= 4}
      onclick={() =>
        update("Added post-strip send", (s) => {
          const b = audio.buses?.find((b) => b.id !== busId);
          if (b) {
            const t = s as AudioTrack | AudioBus;
            t.sends ??= [];
            t.sends.push({ bus: b.id, gain_db: -12, enabled: true });
          }
        })}>+ Post-strip send</button
    >
    <p class="note">
      Dry output plus post-processing sends. Feedback routes are rejected atomically.
    </p>
  {/if}
</div>

<style>
  .channel-strip {
    padding: 15px 0;
    border-top: 1px solid #435257;
    color: #b4cbd0;
  }
  .strip-heading {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 10px;
  }
  .strip-heading strong {
    font-weight: 500;
  }
  .gr {
    font: 7px monospace;
    color: #d1c08b;
  }
  .presets {
    display: flex;
    gap: 5px;
    margin: 13px 0;
  }
  .presets button {
    font-size: 7px;
    padding: 4px 6px;
    border: 1px solid #425a61;
    color: #93b6bf;
    border-radius: 3px;
  }
  .processor-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin: 15px 0 10px;
  }
  .processor-head strong {
    font: 7px monospace;
    letter-spacing: 0.8px;
    color: #90acb3;
  }
  .processor-head button {
    font: 7px monospace;
    padding: 3px 6px;
    border: 1px solid #43545c;
    color: #779099;
    border-radius: 3px;
  }
  .processor-head button.enabled {
    color: #d2e4c9;
    background: #819e6644;
    border-color: #829870;
  }
  .eq-curve {
    width: 100%;
    height: 90px;
    background: #142026;
    border: 1px solid #354b53;
    border-radius: 4px;
    margin-bottom: 12px;
  }
  .eq-curve text {
    font: 6px monospace;
    fill: #819ea6;
  }
  .eq-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  .comp-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  label {
    font-size: 8px;
    color: #8aa4ad;
    display: flex;
    flex-direction: column;
    gap: 5px;
    min-width: 0;
    position: relative;
  }
  label input {
    width: 100%;
    min-width: 0;
    background: #18262c;
    border: 1px solid #3d525a;
    border-radius: 3px;
    padding: 5px 24px 5px 6px;
    color: #c1d4db;
    font: 9px monospace;
  }
  label small {
    position: absolute;
    bottom: 6px;
    right: 6px;
    font-size: 6px;
    color: #6f8e98;
    pointer-events: none;
  }
  .route-label select {
    background: #17252b;
    border: 1px solid #405962;
    color: #bad0d7;
    padding: 6px;
    border-radius: 4px;
    font-size: 9px;
  }
  .send-row {
    display: flex;
    gap: 5px;
    margin-top: 8px;
    min-width: 0;
  }
  .send-row select {
    width: 120px;
    min-width: 0;
    color: #abc8cf;
    font-size: 8px;
    background: #17252b;
    border: 1px solid #405962;
    padding: 4px;
  }
  .send-row input {
    min-width: 0;
    width: 60px;
    background: #17252b;
    border: 1px solid #405962;
    padding: 4px;
    color: #abc8cf;
    font: 8px monospace;
  }
  .send-row button {
    color: #98b5c1;
  }
  .send-add {
    font-size: 8px;
    margin-top: 9px;
    color: #aecbd0;
    padding: 5px 7px;
    border: 1px solid #49636b;
    border-radius: 3px;
  }
  .note {
    font-size: 8px;
    color: #73919b;
    line-height: 1.75;
    margin: 10px 0 0;
  }
</style>
