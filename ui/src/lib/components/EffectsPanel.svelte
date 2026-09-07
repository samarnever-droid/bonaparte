<script lang="ts">
  import {
    editor,
    selectedLayer,
    updateEffects,
    addEffect,
    activeComp,
    download,
    notify,
  } from "../store.svelte";
  import { binary } from "../bridge";
  import Icon from "./Icon.svelte";
  import ParamControl from "./ParamControl.svelte";
  let expanded = $state<Record<string, boolean>>({});
  const layer = $derived(selectedLayer());
  const channelColors = ["#eaa194", "#a2c799", "#8caebd"];
  const histPaths = $derived.by(() => {
    const max = Math.max(1, ...editor.histogram.flat().map((n) => Math.log1p(n)));
    return editor.histogram.map((channel) =>
      channel
        .map(
          (n, i) => `${i === 0 ? "M" : "L"} ${(i * 252) / 63} ${64 - (Math.log1p(n) / max) * 57}`,
        )
        .join(" "),
    );
  });
  function move(id: string, delta: number) {
    void updateEffects((effects) => {
      const index = effects.findIndex((e) => e.id === id);
      if (index < 0 || index + delta < 0 || index + delta >= effects.length) return;
      const [item] = effects.splice(index, 1);
      effects.splice(index + delta, 0, item);
    });
  }
</script>

{#if editor.workspace === "Color"}
  <section class="scopes">
    <div class="scope-title">
      <span>RGB HISTOGRAM</span><span class="spacer"></span><span class="scope-live">FRAME</span>
    </div>
    <svg
      class="histogram"
      viewBox="0 0 252 72"
      aria-label="RGB histogram of the rendered frame"
      role="img"
      ><path
        d="M0 18H252M0 40H252M0 63H252M63 0V64M126 0V64M189 0V64"
        stroke="#343c2e"
        stroke-width=".5"
      />{#each histPaths as path, i}<path
          d={`${path} L252 64 L0 64Z`}
          fill={channelColors[i]}
          fill-opacity=".13"
        /><path d={path} fill="none" stroke={channelColors[i]} stroke-width="1" />{/each}<text
        x="0"
        y="72">0</text
      ><text x="234" y="72">255</text></svg
    >
    <div class="scope-note">
      <span>8-bit sRGB</span><span
        >{editor.previewMetadata?.divisor && editor.previewMetadata.divisor > 1
          ? `1/${editor.previewMetadata.divisor} preview · sampled`
          : "Display-referred output"}</span
      >
    </div>
  </section>
{/if}
<div class="effect-stack-toolbar">
  <span class="upper">Effect stack</span><span class="count">{layer?.effects.length ?? 0}</span
  ><span class="spacer"></span><button
    class="icon-button small"
    title="Browse effects"
    aria-label="Browse effects"
    onclick={() => (editor.sidebar = "effects")}><Icon name="plus" size={14} /></button
  >
</div>
{#if layer}
  {#if !layer.effects.length}
    <div class="empty">
      <Icon name="sliders" size={28} /><strong>A little finishing touch?</strong>Add an effect from
      the library.<br />Every change stays editable.<button
        class="btn full"
        style="margin-top:18px"
        disabled={layer.locked}
        onclick={() => void addEffect("builtin.color_grade")}
        ><Icon name="palette" size={14} />Add Color Grade</button
      >
    </div>
  {/if}
  {#each layer.effects as effect, index (effect.id)}
    {@const manifest = editor.effects.find((m) => m.id === effect.effect_id)}
    <section class="effect-card" class:bypassed={!effect.enabled} data-effect={effect.effect_id}>
      <div class="effect-card-header">
        <button
          class="icon-button small disclosure"
          aria-label={`Toggle ${manifest?.name ?? effect.effect_id} controls`}
          onclick={() => (expanded[effect.id] = !(expanded[effect.id] ?? true))}
          ><Icon name={expanded[effect.id] === false ? "right" : "down"} size={11} /></button
        >
        <input
          type="checkbox"
          checked={effect.enabled}
          aria-label={`Enable ${manifest?.name ?? effect.effect_id}`}
          disabled={layer.locked}
          onchange={(e) => {
            const enabled = e.currentTarget.checked;
            void updateEffects((effects) => {
              const item = effects.find((f) => f.id === effect.id);
              if (item) item.enabled = enabled;
            });
          }}
        />
        <span class="fx-symbol">ƒx</span><strong>{manifest?.name ?? effect.effect_id}</strong><span
          class="spacer"
        ></span>
        <button
          class="icon-button small"
          title="Move effect up"
          aria-label={`Move ${manifest?.name} up`}
          disabled={index === 0 || layer.locked}
          onclick={() => move(effect.id, -1)}><Icon name="up" size={12} /></button
        >
        <button
          class="icon-button small"
          title="Move effect down"
          aria-label={`Move ${manifest?.name} down`}
          disabled={index === layer.effects.length - 1 || layer.locked}
          onclick={() => move(effect.id, 1)}><Icon name="down" size={12} /></button
        >
        <button
          class="icon-button small"
          title="Remove effect"
          aria-label={`Remove ${manifest?.name}`}
          disabled={layer.locked}
          onclick={() =>
            void updateEffects((effects) => {
              const i = effects.findIndex((e) => e.id === effect.id);
              if (i >= 0) effects.splice(i, 1);
            })}><Icon name="x" size={12} /></button
        >
      </div>
      {#if expanded[effect.id] !== false && manifest}
        <div class="effect-description">
          <span class="mono">{String(index + 1).padStart(2, "0")}</span><span
            >{manifest.category}</span
          ><span class="spacer"></span><button
            title="Reset parameters and animation"
            disabled={layer.locked}
            onclick={() =>
              void updateEffects((effects) => {
                const item = effects.find((e) => e.id === effect.id);
                if (item) {
                  item.params = {};
                  item.tracks = {};
                }
              })}>Reset</button
          >
        </div>
        <div class="effect-body">
          {#each [...new Set(manifest.params.map((p) => p.group || "Parameters"))] as group}
            <details open>
              <summary>{group}<Icon name="down" size={10} /></summary>
              <div class="parameter-group">
                {#each manifest.params.filter((p) => (p.group || "Parameters") === group) as param (param.id)}<ParamControl
                    {param}
                    instance={effect}
                    disabled={layer.locked || !effect.enabled}
                  />{/each}
              </div>
            </details>
          {/each}
        </div>
      {/if}
    </section>
  {/each}
  {#if layer.effects.length}<button
      class="add-effect"
      onclick={() => (editor.sidebar = "effects")}
      disabled={layer.locked}><Icon name="plus" size={13} />Add another effect</button
    >
    <p class="stack-note">
      Evaluated from top to bottom.<br />◇ Animate a parameter at the playhead.
    </p>
    {#if layer.effects.length}
      <button
        class="lut-export"
        disabled={editor.exporting}
        onclick={() =>
          binary("export_lut", {
            compId: activeComp()?.id,
            layerId: layer.id,
            size: 33,
          })
            .then((data) => {
              download(new Blob([data], { type: "text/plain" }), `${layer.name}.cube`);
              notify("LUT exported.");
            })
            .catch((error) =>
              notify(
                error instanceof Error ? error.message : String(error ?? "LUT export failed"),
                true,
              ),
            )}
        ><Icon name="download" size={12} />Export grade as .cube LUT</button
      >
    {/if}{/if}
{:else}
  <div class="empty">
    Select a layer to edit its effects.<button
      class="btn full"
      style="margin-top:16px"
      onclick={() => void addEffect("builtin.color_grade")}
      ><Icon name="adjust" size={14} />Create grading adjustment</button
    >
  </div>
{/if}

<style>
  .lut-export {
    margin: 0 16px 13px;
    width: calc(100% - 32px);
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    border: 1px solid var(--border);
    background: transparent;
    color: var(--text-2);
    border-radius: 7px;
    padding: 6px 8px;
    font: inherit;
    font-size: 11px;
    cursor: pointer;
  }
  .lut-export:hover:not(:disabled) { background: #26291f; }
  .scopes {
    padding: 17px 16px 13px;
    border-bottom: 1px solid var(--border);
    background: #1c1d1c;
  }
  .scope-title {
    display: flex;
    align-items: center;
    font-size: 8px;
    letter-spacing: 1px;
    color: #9fa29d;
  }
  .scope-live {
    font: 7px monospace;
    color: #a3a99f;
    letter-spacing: 0.7px;
  }
  .histogram {
    width: 100%;
    height: 81px;
    margin-top: 15px;
    overflow: visible;
  }
  .histogram text {
    font: 6px monospace;
    fill: #747871;
  }
  .scope-note {
    display: flex;
    justify-content: space-between;
    margin-top: 6px;
    font-size: 7px;
    color: #7c8079;
  }
  .effect-stack-toolbar {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 14px 13px 10px;
  }
  .effect-stack-toolbar .upper {
    font-size: 8px;
    color: #8a8c88;
  }
  .effect-card {
    border-top: 1px solid #3c3e3b;
    border-bottom: 1px solid #343633;
    margin-bottom: 10px;
    background: #252624;
  }
  .effect-card.bypassed {
    background: #232322;
  }
  .effect-card-header {
    display: flex;
    align-items: center;
    gap: 4px;
    min-height: 38px;
    padding: 0 8px 0 4px;
    background: #2e302d;
  }
  .effect-card-header strong {
    font-size: 10px;
    font-weight: 500;
    color: #d3d6d2;
    white-space: nowrap;
  }
  .effect-card-header input {
    width: 10px;
    height: 10px;
    margin: 0 2px;
  }
  .fx-symbol {
    font-family: Georgia, serif;
    font-size: 14px;
    font-style: italic;
    color: #b6bab2;
    margin: 0 3px;
  }
  .effect-card-header .icon-button {
    width: 18px;
  }
  .effect-description {
    display: flex;
    align-items: center;
    padding: 10px 16px 0;
    gap: 7px;
    color: #7a7e76;
    font-size: 8px;
  }
  .effect-description .mono {
    font-size: 7px;
    border: 1px solid #494c47;
    padding: 1px 3px;
    border-radius: 2px;
  }
  .effect-description button {
    color: #a9ada6;
    font-size: 8px;
  }
  .effect-body {
    padding: 4px 16px 0;
  }
  .effect-body details {
    border-bottom: 1px solid #373935;
  }
  .effect-body details:last-child {
    border-bottom: 0;
  }
  .effect-body summary {
    display: flex;
    align-items: center;
    justify-content: space-between;
    cursor: pointer;
    list-style: none;
    color: #c2c4c0;
    font-size: 10px;
    font-weight: 500;
    padding: 12px 0;
  }
  .effect-body summary::-webkit-details-marker {
    display: none;
  }
  .effect-body details:not([open]) summary :global(svg) {
    transform: rotate(-90deg);
  }
  .parameter-group {
    padding: 3px 0 0;
  }
  .add-effect {
    width: calc(100% - 28px);
    margin: 0 14px;
    display: flex;
    justify-content: center;
    gap: 7px;
    padding: 10px;
    border: 1px dashed #4e524c;
    border-radius: 4px;
    color: #aeb2aa;
    font-size: 10px;
  }
  .add-effect:hover {
    background: #383a36;
  }
  .stack-note {
    font-size: 8px;
    line-height: 1.8;
    color: #747871;
    text-align: center;
    margin: 15px 10px;
  }
</style>
