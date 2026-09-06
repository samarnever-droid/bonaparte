<script lang="ts">
  import type { Color } from "../model";
  import { fromHex, toHex } from "../color";
  import { inputGroup, releaseInputGroup, finiteInput } from "../live-input";
  import { flushLiveEdits } from "../store.svelte";
  let {
    value,
    label,
    linear = true,
    disabled = false,
    onchange,
  }: {
    value: Color;
    label: string;
    linear?: boolean;
    disabled?: boolean;
    onchange: (color: Color, group?: string) => unknown;
  } = $props();
  const hex = $derived(toHex(value, linear));
  let focused = $state(false),
    draft = $state("");
  function end(node: HTMLElement) {
    releaseInputGroup(node);
    focused = false;
    void flushLiveEdits();
  }
</script>

<div class="color-field">
  <input
    type="color"
    aria-label={`${label} color`}
    value={hex}
    {disabled}
    oninput={(e) =>
      onchange(fromHex(e.currentTarget.value, value[3], linear), inputGroup(e.currentTarget))}
    onchange={() => void flushLiveEdits()}
    onblur={(e) => end(e.currentTarget)}
  />
  <input
    class="hex mono"
    aria-label={`${label} hex`}
    value={focused ? draft : hex.toUpperCase()}
    maxlength="7"
    {disabled}
    onfocus={(e) => {
      focused = true;
      draft = e.currentTarget.value;
    }}
    oninput={(e) => {
      draft = e.currentTarget.value;
      if (/^#[0-9a-f]{6}$/i.test(draft))
        onchange(fromHex(draft, value[3], linear), inputGroup(e.currentTarget));
    }}
    onblur={(e) => end(e.currentTarget)}
    onkeydown={(e) => {
      if (e.key === "Enter") {
        e.preventDefault();
        void flushLiveEdits();
      }
    }}
  />
  <span class="alpha"
    ><input
      type="number"
      aria-label={`${label} alpha`}
      value={Math.round(value[3] * 100)}
      min="0"
      max="100"
      {disabled}
      oninput={(e) => {
        const n = finiteInput(e.currentTarget.value);
        if (n !== null && n >= 0 && n <= 100)
          onchange([value[0], value[1], value[2], n / 100], inputGroup(e.currentTarget));
      }}
      onblur={(e) => end(e.currentTarget)}
      onkeydown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          void flushLiveEdits();
        }
      }}
    /><span>%</span></span
  >
</div>

<style>
  .color-field {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }
  .hex {
    width: 67px;
    border: 1px solid var(--border);
    background: #1a1b1a;
    padding: 5px 4px;
    font-size: 9px;
    border-radius: 3px;
    outline: 0;
    color: var(--text-secondary);
  }
  .alpha {
    display: flex;
    align-items: center;
    gap: 2px;
    border: 1px solid var(--border);
    border-radius: 3px;
    background: #1a1b1a;
    padding: 5px;
    color: var(--text-muted);
    font-size: 9px;
  }
  .alpha input {
    width: 25px;
    border: 0;
    outline: 0;
    background: none;
    color: var(--text-secondary);
    font: 9px monospace;
    text-align: right;
  }
  .color-field input:focus {
    border-color: #939990;
  }
</style>
