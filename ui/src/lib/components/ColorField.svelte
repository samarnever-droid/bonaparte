<script lang="ts">
  import type { Color } from "../model";
  import { fromHex, toHex } from "../color";
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
    onchange: (color: Color) => unknown;
  } = $props();
  const hex = $derived(toHex(value, linear));
</script>

<div class="color-field">
  <input
    type="color"
    aria-label={`${label} color`}
    value={hex}
    {disabled}
    onchange={(e) => onchange(fromHex(e.currentTarget.value, value[3], linear))}
  />
  <input
    class="hex mono"
    aria-label={`${label} hex`}
    value={hex.toUpperCase()}
    maxlength="7"
    {disabled}
    onchange={(e) => {
      const raw = e.currentTarget.value;
      if (/^#[0-9a-f]{6}$/i.test(raw)) {
        e.currentTarget.setCustomValidity("");
        onchange(fromHex(raw, value[3], linear));
      } else {
        e.currentTarget.value = hex;
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
      onchange={(e) =>
        onchange([
          value[0],
          value[1],
          value[2],
          Math.max(0, Math.min(100, Number(e.currentTarget.value))) / 100,
        ])}
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
