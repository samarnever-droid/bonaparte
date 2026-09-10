<script lang="ts">
  import Icon from "./Icon.svelte";
  import { editor, arrangement } from "../store.svelte";
  import { addAssetClip } from "../audio/actions";
  let { search = "" }: { search?: string } = $props();
  const assets = $derived(
    Object.values(editor.project?.media ?? {}).filter(
      (a) => a.audio && a.name.toLowerCase().includes(search.toLowerCase()),
    ),
  );
</script>

<!-- Single home per control: this sidebar panel only picks the selection target and adds
     clips. Import lives in the audio toolbar (+ its context menus), and mix buses are
     created with "Bus" there too — nothing here repeats another panel's action. -->
<div class="audio-library">
  {#if editor.audioProtocol >= 2}
    <div class="library-label">
      MIX ROUTING <span>{(arrangement().buses ?? []).length + 1}</span>
    </div>
    <button
      class="source"
      aria-label="Select master processing"
      title="Edit the master strip in the inspector"
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
        title="Edit this bus in the inspector"
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
  {#if !assets.length}
    <p class="note">
      No sounds yet. Import from the audio toolbar above the tracks (or right-click any track).
    </p>
  {/if}
  <div class="library-label">WORKING WITH AUDIO</div>
  <ul>
    <li>Drag a clip to move it. Trim either edge.</li>
    <li>Alt-drag to slip the source; Shift bypasses snap.</li>
    <li>Drag top-corner handles for fades.</li>
    <li>Overlap clips, then apply a crossfade.</li>
    <li>Use the inspector for sample positions and automation.</li>
  </ul>
  <p class="note">
    Original files are retained; working sound is 48 kHz float. Preview and export use the same Rust
    mixer. Varispeed changes pitch; it is not pitch-preserving stretch.
  </p>
</div>

<style>
  .audio-library {
    color: #9ebbc2;
    padding: 4px 12px 18px;
  }
  .library-label {
    font: 8px monospace;
    letter-spacing: 1px;
    color: #8db0b7;
    display: flex;
    justify-content: space-between;
    margin: 18px 0 10px;
  }
  .library-label span {
    color: #6f8b94;
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
  .source:hover {
    border-color: #4d707a;
    background: #709fab14;
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
    margin-top: 12px;
  }
  ul {
    padding-left: 14px;
    margin: 0;
  }
  li {
    margin-bottom: 7px;
  }
</style>
