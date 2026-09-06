<script lang="ts">
  import Icon from "./Icon.svelte";
  import { editor, arrangement } from "../store.svelte";
  import { importAudio, addAssetClip, addMixBus } from "../audio/actions";
  let { search = "" }: { search?: string } = $props();
  const assets = $derived(
    Object.values(editor.project?.media ?? {}).filter(
      (a) => a.audio && a.name.toLowerCase().includes(search.toLowerCase()),
    ),
  );
</script>

<div class="audio-library">
  <div class="library-intro">
    <span class="badge"><Icon name="wave" size={12} />AUDIO LIBRARY</span>
    <p>Sound gives<br /><strong>motion its meaning.</strong></p>
  </div>
  <button class="import-audio" disabled={editor.audioImporting} onclick={() => void importAudio()}
    ><Icon name="upload" size={18} /><strong
      >{editor.audioImporting ? "Decoding & analyzing…" : "Import a sound"}</strong
    ><span>Music, dialogue, ambience, effects</span><small
      >WAV · MP3 · FLAC · OGG · AIFF · AAC</small
    ></button
  >
  {#if editor.audioProtocol >= 2}
    <div class="library-label">
      MIX ROUTING <button aria-label="Add mix bus" onclick={() => void addMixBus()}>＋ Bus</button>
    </div>
    <button
      class="source"
      aria-label="Select master processing"
      onclick={() => {
        editor.audioSelection = null;
        editor.audioBusSelection = null;
      }}
      ><span class="source-icon"><Icon name="sliders" size={17} /></span><span
        ><strong>Master output</strong><small>EQ · COMPRESSION · LIMITER</small></span
      ></button
    >
    {#each arrangement().buses ?? [] as bus}
      <button
        class="source"
        aria-label={`Select mix bus ${bus.name}`}
        onclick={() => {
          editor.audioSelection = null;
          editor.audioBusSelection = bus.id;
        }}
        ><span class="source-icon"><Icon name="layers" size={17} /></span><span
          ><strong>{bus.name}</strong><small
            >{bus.gain_db.toFixed(1)} dB · {bus.muted ? "MUTED" : "MIX BUS"}</small
          ></span
        ></button
      >
    {/each}
  {/if}
  <div class="library-label">SOURCE AUDIO <span>{assets.length}</span></div>
  {#each assets as asset (asset.id)}
    <button
      class="source"
      aria-label={`Add audio source ${asset.name}`}
      title="Add another non-destructive clip at the audio playhead"
      onclick={() => void addAssetClip(asset.id, editor.audioSelection?.track)}
      ><span class="source-icon"><Icon name="wave" size={18} /></span><span
        ><strong>{asset.name}</strong><small
          >{(asset.audio!.frames / 48000).toFixed(2)}s · {asset.audio!.original_channels === 1
            ? "MONO"
            : "STEREO"} · {(asset.audio!.original_sample_rate / 1000).toFixed(1)}k</small
        ></span
      ><Icon name="plus" size={12} /></button
    >
  {/each}
  {#if !assets.length}<p class="note">
      Original files are retained in the project. Working sound uses 48 kHz floating-point PCM, with
      waveforms built by the native engine.
    </p>{/if}
  <div class="library-label">WORKING WITH AUDIO</div>
  <ul>
    <li>Drag a clip to move it. Trim either edge.</li>
    <li>Alt-drag to slip the source; Shift bypasses snap.</li>
    <li>Drag top-corner handles for fades.</li>
    <li>Overlap clips, then apply a crossfade.</li>
    <li>Use the inspector for sample positions and automation.</li>
  </ul>
  <p class="note">
    {arrangement().tracks.length} tracks in this composition. Preview and export use the same Rust mixer.
    Varispeed changes pitch; it is not pitch-preserving stretch.
  </p>
</div>

<style>
  .audio-library {
    color: #9ebbc2;
    padding: 0 12px 18px;
  }
  .library-intro {
    padding: 8px 0 16px;
  }
  .badge {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font: 7px monospace;
    letter-spacing: 0.7px;
    color: #86b7c2;
    border: 1px solid #486a70;
    background: #87aeba12;
    border-radius: 3px;
    padding: 5px 6px;
  }
  .library-intro p {
    font-size: 15px;
    color: #74979c;
    line-height: 1.7;
    margin: 14px 0 0;
    font-weight: 300;
  }
  .library-intro strong {
    font-weight: 400;
    color: #b7d2d5;
  }
  .import-audio {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 9px;
    padding: 19px 12px;
    border: 1px dashed #52727b;
    border-radius: 6px;
    width: 100%;
    background: #75a8b00b;
    color: #8eb9c4;
  }
  .import-audio strong {
    font-size: 11px;
    font-weight: 400;
    color: #bed3d9;
  }
  .import-audio span {
    font-size: 8px;
    color: #75949d;
  }
  .import-audio small {
    font: 6px monospace;
    letter-spacing: 0.4px;
    color: #6f8b94;
    margin-top: 4px;
  }
  .library-label {
    font: 8px monospace;
    letter-spacing: 1px;
    color: #8db0b7;
    display: flex;
    justify-content: space-between;
    margin: 22px 0 12px;
  }
  .source {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 11px 8px;
    margin-bottom: 7px;
    border: 1px solid #354c53;
    border-radius: 4px;
    background: #709fab09;
  }
  .source > span:nth-child(2) {
    flex: 1;
    min-width: 0;
    text-align: left;
  }
  .source strong {
    display: block;
    font-size: 9px;
    font-weight: 400;
    color: #bad0d5;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .source small {
    display: block;
    font: 6px monospace;
    color: #799aa5;
    margin-top: 7px;
  }
  .source-icon {
    width: 28px;
    height: 28px;
    background: #6c9ca524;
    display: flex;
    align-items: center;
    justify-content: center;
    color: #94bfca;
    border-radius: 3px;
  }
  .source :global(svg) {
    flex-shrink: 0;
  }
  .note,
  li {
    font-size: 9px;
    line-height: 1.9;
    color: #809fa8;
  }
  .note {
    margin-top: 17px;
  }
  ul {
    padding-left: 14px;
    margin: 0;
  }
  li {
    margin-bottom: 7px;
  }
</style>
