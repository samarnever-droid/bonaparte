<script lang="ts">
  import {
    editor,
    selectedLayer,
    activeComp,
    applyOp,
    toggleKeyframeAtPlayhead,
  } from "../store.svelte";
  import {
    evaluate,
    findKeyframeAtTime,
    snapToFrame,
    DEFAULT_EASING,
    type Property,
    type PropValue,
  } from "../model";

  const PROPS: {
    key: Property;
    label: string;
    kind: "vec2" | "scalar";
    step: number;
    max?: number;
  }[] = [
    { key: "Position", label: "Position", kind: "vec2", step: 1 },
    { key: "Scale", label: "Scale %", kind: "vec2", step: 1 },
    { key: "Rotation", label: "Rotation °", kind: "scalar", step: 1 },
    { key: "Opacity", label: "Opacity", kind: "scalar", step: 0.05, max: 1 },
    { key: "AnchorPoint", label: "Anchor Point", kind: "vec2", step: 1 },
  ];

  const layer = $derived(selectedLayer());
  const comp = $derived(activeComp());

  function currentValue(l: import("../model").Layer, key: Property): PropValue {
    return evaluate(l, key, editor.currentTime);
  }

  function asNumbers(v: PropValue): number[] {
    return "Vec2" in v ? v.Vec2 : [v.Scalar];
  }

  function hasTrack(key: Property): boolean {
    return !!layer?.tracks[key]?.keys.length;
  }

  function hasKeyAtPlayhead(key: Property): boolean {
    if (!layer?.tracks[key] || !comp) return false;
    return !!findKeyframeAtTime(layer.tracks[key], editor.currentTime, comp.fps);
  }

  async function commit(key: Property, values: number[]) {
    if (!comp || !layer) return;
    const value: PropValue =
      key === "Position" || key === "Scale" || key === "AnchorPoint"
        ? { Vec2: [values[0], values[1]] }
        : { Scalar: values[0] };

    // Track-aware: if animated track exists, add/update keyframe at currentTime
    if (hasTrack(key)) {
      const existingKey = findKeyframeAtTime(layer.tracks[key], editor.currentTime, comp.fps);
      const targetTime = snapToFrame(editor.currentTime, comp.fps);
      const easing = existingKey?.easing ?? DEFAULT_EASING;

      await applyOp({
        type: "addKeyframe",
        comp: comp.id,
        layer: layer.id,
        property: key,
        key: {
          time: targetTime,
          value,
          easing,
        },
      });
    } else {
      await applyOp({
        type: "setValue",
        comp: comp.id,
        layer: layer.id,
        property: key,
        value,
      });
    }
  }
</script>

<aside
  aria-label="Properties Panel"
  class="flex min-h-0 flex-col border-l border-[var(--border)] bg-[var(--bg-panel)] select-none"
>
  <div
    class="border-b border-[var(--border)] px-4 py-3 text-[11px] font-semibold uppercase tracking-widest text-[var(--text-dim)]"
  >
    Properties
  </div>

  {#if layer}
    <div class="border-b border-[var(--border)] px-4 py-2.5">
      <div class="flex items-center justify-between">
        <span class="text-sm font-semibold">{layer.name}</span>
        <span class="rounded bg-[var(--bg-raised)] px-1.5 py-0.5 font-mono text-[10px] text-[var(--text-dim)]">
          ID: {layer.id}
        </span>
      </div>

      {#if "Shape" in layer.kind}
        <div class="mt-1.5 flex flex-wrap items-center gap-1.5">
          {#if layer.kind.Shape.generator === "builtin.circle"}
            <span class="inline-flex items-center gap-1 rounded bg-[rgba(255,107,107,0.15)] px-2 py-0.5 text-[11px] font-medium text-[#ff6b6b]">
              <span>⭕</span> Microkernel: builtin.circle
            </span>
          {:else}
            <span class="inline-flex items-center gap-1 rounded bg-[rgba(107,138,253,0.15)] px-2 py-0.5 text-[11px] font-medium text-[var(--accent)]">
              <span>⬛</span> Vector Rectangle
            </span>
          {/if}
        </div>
      {:else if "Solid" in layer.kind}
        <div class="mt-1.5 flex items-center gap-1.5">
          <span class="inline-flex items-center gap-1 rounded bg-[rgba(81,207,102,0.15)] px-2 py-0.5 text-[11px] font-medium text-[#51cf66]">
            <span>🎨</span> Solid Color Fill
          </span>
        </div>
      {:else if "Text" in layer.kind}
        <div class="mt-1.5 flex items-center gap-1.5">
          <span class="inline-flex items-center gap-1 rounded bg-[rgba(255,224,102,0.15)] px-2 py-0.5 text-[11px] font-medium text-[#ffd43b]">
            <span>📝</span> "{layer.kind.Text.text}" ({layer.kind.Text.size}px)
          </span>
        </div>
      {/if}
    </div>

    <div class="min-h-0 overflow-y-auto px-4 pb-4 pt-2">
      {#each PROPS as p (p.key)}
        {@const v = asNumbers(currentValue(layer, p.key))}
        {@const isKeyed = hasKeyAtPlayhead(p.key)}
        {@const animated = hasTrack(p.key)}

        <div class="mb-3">
          <div class="mb-1 flex items-center justify-between">
            <span class="text-[11px] text-[var(--text-dim)]">{p.label}</span>
            <button
              class="rounded px-1.5 text-[13px] leading-5 transition-colors {isKeyed
                ? 'text-[var(--danger)] font-bold scale-110'
                : animated
                  ? 'text-[var(--accent)] hover:scale-110'
                  : 'text-[var(--text-dim)] hover:text-[var(--text)]'}"
              title={isKeyed
                ? "Remove keyframe at playhead"
                : animated
                  ? "Add keyframe at playhead"
                  : "Enable animation track / Add keyframe"}
              onclick={() => void toggleKeyframeAtPlayhead(layer.id, p.key)}
            >
              {isKeyed ? "◆" : "◇"}
            </button>
          </div>
          <div class="flex gap-1.5">
            {#each p.kind === "vec2" ? [0, 1] : [0] as i (i)}
              <input
                type="number"
                class="w-full rounded-md border border-[var(--border)] bg-[var(--bg-raised)] px-2 py-1 font-mono text-xs outline-none focus:border-[var(--accent)]"
                value={v[i] !== undefined ? v[i].toFixed(2) : "0.00"}
                step={p.step}
                onchange={async (e) => {
                  const next = [...asNumbers(currentValue(layer, p.key))];
                  next[i] = Number(e.currentTarget.value);
                  await commit(p.key, next);
                }}
              />
            {/each}
          </div>
        </div>
      {/each}
    </div>
  {:else}
    <div class="px-4 py-6 text-xs text-[var(--text-dim)]">
      Select a layer to edit its properties.
    </div>
  {/if}
</aside>
