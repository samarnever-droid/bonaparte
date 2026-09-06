<script lang="ts">
  import type { ParamDef, EffectInstance, EffectValue } from "../model";
  import {
    editor,
    effectValue,
    setEffectParam,
    previewEffect,
    toggleEffectKey,
  } from "../store.svelte";
  import Icon from "./Icon.svelte";
  import ColorField from "./ColorField.svelte";
  let {
    param,
    instance,
    disabled = false,
  }: { param: ParamDef; instance: EffectInstance; disabled?: boolean } = $props();
  const current = $derived(
    editor.previewLayer?.effects.find((e) => e.id === instance.id) ?? instance,
  );
  const value = $derived(effectValue(current, param));
  const animated = $derived(!!instance.tracks[param.id]?.keys.length);
  const keyed = $derived(
    !!instance.tracks[param.id]?.keys.some((k) => k.time === editor.currentTime),
  );
  const animatable = $derived("Slider" in param.kind || "Point" in param.kind);
  function commit(value: EffectValue) {
    void setEffectParam(instance.id, param, value);
  }
  function scalar(raw: string) {
    return "Slider" in param.kind
      ? Math.max(param.kind.Slider.min, Math.min(param.kind.Slider.max, Number(raw)))
      : Number(raw);
  }
</script>

<div class="param" class:disabled title={param.doc}>
  <div class="param-header">
    <label for={`${instance.id}-${param.id}`}>{param.name}</label>
    <span class="spacer"></span>
    {#if "Float" in value && "Slider" in param.kind}
      <input
        id={`${instance.id}-${param.id}`}
        class="scalar mono"
        type="number"
        aria-label={param.name}
        value={Number(value.Float.toFixed(3))}
        min={param.kind.Slider.min}
        max={param.kind.Slider.max}
        step={param.step ?? 0.01}
        {disabled}
        onchange={(e) => commit({ Float: scalar(e.currentTarget.value) })}
      /><span class="unit">{param.unit}</span>
    {/if}
    {#if animatable}<button
        class="key-button"
        class:animated
        class:keyed
        {disabled}
        aria-label={`Keyframe ${param.name}`}
        title={keyed ? "Remove keyframe at playhead" : "Animate this parameter"}
        onclick={() => void toggleEffectKey(instance.id, param)}
        ><Icon name="keyframe" size={10} /></button
      >{/if}
  </div>
  {#if "Slider" in param.kind && "Float" in value}
    <input
      class="range"
      type="range"
      aria-label={`${param.name} slider`}
      min={param.kind.Slider.min}
      max={param.kind.Slider.max}
      step={param.step ?? (param.kind.Slider.max - param.kind.Slider.min) / 200}
      value={value.Float}
      {disabled}
      oninput={(e) => previewEffect(instance.id, param, { Float: scalar(e.currentTarget.value) })}
      onchange={(e) => commit({ Float: scalar(e.currentTarget.value) })}
      onpointercancel={() => (editor.previewLayer = null)}
    />
  {:else if "Color" in value}
    <ColorField
      value={value.Color}
      label={param.name}
      linear={false}
      {disabled}
      onchange={(color) => commit({ Color: color })}
    />
  {:else if "Point" in value}
    <div class="point-fields">
      {#each [0, 1] as axis}<div class="number-field">
          <span>{axis === 0 ? "X" : "Y"}</span><input
            type="number"
            aria-label={`${param.name} ${axis === 0 ? "X" : "Y"}`}
            {disabled}
            value={value.Point[axis]}
            onchange={(e) => {
              if ("Point" in value) {
                const next: [number, number] = [...value.Point];
                next[axis] = Number(e.currentTarget.value);
                commit({ Point: next });
              }
            }}
          />
        </div>{/each}
    </div>
  {:else if "Bool" in value}
    <label class="checkbox"
      ><input
        id={`${instance.id}-${param.id}`}
        type="checkbox"
        checked={value.Bool}
        {disabled}
        onchange={(e) => commit({ Bool: e.currentTarget.checked })}
      /><span>{value.Bool ? "Enabled" : "Disabled"}</span></label
    >
  {:else if "Index" in value && "Dropdown" in param.kind}
    <select
      id={`${instance.id}-${param.id}`}
      class="field"
      aria-label={param.name}
      value={value.Index}
      {disabled}
      onchange={(e) => commit({ Index: Number(e.currentTarget.value) })}
      >{#each param.kind.Dropdown.options as option, index}<option value={index}>{option}</option
        >{/each}</select
    >
  {/if}
</div>

<style>
  .param {
    margin: 0 0 15px;
  }
  .param-header {
    display: flex;
    align-items: center;
    gap: 2px;
    min-height: 20px;
    margin-bottom: 5px;
  }
  .param-header label {
    font-size: 10px;
    color: #adafab;
  }
  .scalar {
    background: none;
    outline: 0;
    border: 0;
    border-bottom: 1px solid #494b48;
    width: 51px;
    text-align: right;
    padding: 2px 3px;
    color: #c5c9c2;
    font-size: 10px;
  }
  .scalar:focus {
    border-color: var(--accent);
  }
  .unit {
    font: 8px monospace;
    color: #7c807a;
    min-width: 12px;
    text-align: center;
  }
  .range {
    width: calc(100% - 6px);
    margin: 5px 3px 2px;
    display: block;
  }
  .point-fields {
    display: flex;
    gap: 6px;
  }
  .checkbox {
    display: flex;
    gap: 7px;
    align-items: center;
    color: var(--text-dim);
    font-size: 10px;
  }
  .disabled {
    opacity: 0.45;
  }
</style>
