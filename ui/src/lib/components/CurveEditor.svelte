<script lang="ts">
  import { applyOp } from "../store.svelte";
  import {
    timeToSecs,
    timeToTimecode,
    type Comp,
    type Layer,
    type Property,
    type Keyframe,
    type Easing,
  } from "../model";

  interface Props {
    comp: Comp;
    layer: Layer;
    property: Property;
    onClose?: () => void;
  }

  let { comp, layer, property, onClose }: Props = $props();

  const track = $derived(layer.tracks[property]);
  const keys = $derived(track?.keys ?? []);

  // Active keyframe index being edited (default to first keyframe or key before playhead)
  let activeKeyIndex = $state(0);
  const activeKey = $derived(keys[activeKeyIndex] ?? keys[0] ?? null);

  // Control points state in unit space [0..1]
  let p1 = $state<[number, number]>([0.42, 0.0]);
  let p2 = $state<[number, number]>([0.58, 1.0]);
  let isLinear = $state(false);

  // Sync control points when active keyframe changes
  $effect(() => {
    if (activeKey) {
      if (activeKey.easing === "Linear") {
        isLinear = true;
        p1 = [0.0, 0.0];
        p2 = [1.0, 1.0];
      } else if (typeof activeKey.easing === "object" && "Bezier" in activeKey.easing) {
        isLinear = false;
        p1 = [...activeKey.easing.Bezier.p1];
        p2 = [...activeKey.easing.Bezier.p2];
      }
    }
  });

  // SVG Graph Coordinate Mapping
  // Width: 260px, Height: 140px, Padding: 25px
  const gw = 260;
  const gh = 140;
  const padX = 30;
  const padY = 25;
  const plotW = gw - padX * 2;
  const plotH = gh - padY * 2;

  // Convert unit [u, v] where u in [0,1], v in [0,1] to SVG coordinates
  // (0,0) is bottom-left of plot, (1,1) is top-right of plot
  function toSvg(u: number, v: number): [number, number] {
    const x = padX + u * plotW;
    const y = padY + (1 - v) * plotH;
    return [x, y];
  }

  function fromSvg(x: number, y: number): [number, number] {
    const u = Math.min(1, Math.max(0, (x - padX) / plotW));
    const v = (padY + plotH - y) / plotH;
    return [u, v];
  }

  const startPt = $derived(toSvg(0, 0));
  const endPt = $derived(toSvg(1, 1));
  const cp1 = $derived(toSvg(p1[0], p1[1]));
  const cp2 = $derived(toSvg(p2[0], p2[1]));

  const pathD = $derived(
    isLinear
      ? `M ${startPt[0]} ${startPt[1]} L ${endPt[0]} ${endPt[1]}`
      : `M ${startPt[0]} ${startPt[1]} C ${cp1[0]} ${cp1[1]}, ${cp2[0]} ${cp2[1]}, ${endPt[0]} ${endPt[1]}`,
  );

  // Velocity Curve Points (sampling derivative dv/du)
  const velocityPathD = $derived.by(() => {
    if (isLinear) {
      const vY = padY + plotH / 2;
      return `M ${padX} ${vY} L ${padX + plotW} ${vY}`;
    }
    const pts: string[] = [];
    const samples = 30;
    for (let i = 0; i <= samples; i++) {
      const t = i / samples;
      const it = 1 - t;
      const dx = 3 * it * it * p1[0] + 6 * it * t * (p2[0] - p1[0]) + 3 * t * t * (1 - p2[0]);
      const dy = 3 * it * it * p1[1] + 6 * it * t * (p2[1] - p1[1]) + 3 * t * t * (1 - p2[1]);
      const speed = Math.max(0, Math.min(3, dx > 0.0001 ? dy / dx : 1));
      const u = t;
      const v = speed / 3;
      const [sx, sy] = toSvg(u, v);
      pts.push(`${i === 0 ? "M" : "L"} ${sx.toFixed(1)} ${sy.toFixed(1)}`);
    }
    return pts.join(" ");
  });

  // Handle Dragging State
  let draggingHandle: "p1" | "p2" | null = $state(null);
  let svgEl: SVGSVGElement | null = $state(null);

  function onHandleDown(e: PointerEvent, handle: "p1" | "p2") {
    e.stopPropagation();
    draggingHandle = handle;
    isLinear = false;
    (e.currentTarget as Element)?.setPointerCapture(e.pointerId);
  }

  function onPointerMove(e: PointerEvent) {
    if (!draggingHandle || !svgEl) return;
    const rect = svgEl.getBoundingClientRect();
    const clientX = e.clientX - rect.left;
    const clientY = e.clientY - rect.top;
    const [u, v] = fromSvg(clientX, clientY);

    const clampedU = Math.min(1, Math.max(0, Math.round(u * 100) / 100));
    const clampedV = Math.min(1.5, Math.max(-0.5, Math.round(v * 100) / 100));

    if (draggingHandle === "p1") {
      p1 = [clampedU, clampedV];
    } else if (draggingHandle === "p2") {
      p2 = [clampedU, clampedV];
    }
  }

  async function onPointerUp() {
    if (!draggingHandle || !activeKey) {
      draggingHandle = null;
      return;
    }
    draggingHandle = null;
    await commitEasing({
      Bezier: {
        p1: [p1[0], p1[1]],
        p2: [p2[0], p2[1]],
      },
    });
  }

  async function commitEasing(easing: Easing) {
    if (!activeKey) return;
    await applyOp({
      type: "setEasing",
      comp: comp.id,
      layer: layer.id,
      property,
      time: activeKey.time,
      easing,
    });
  }

  // Presets
  const PRESETS: { name: string; easing: Easing; p1: [number, number]; p2: [number, number] }[] = [
    { name: "Linear", easing: "Linear", p1: [0.0, 0.0], p2: [1.0, 1.0] },
    {
      name: "Ease In",
      easing: { Bezier: { p1: [0.42, 0.0], p2: [1.0, 1.0] } },
      p1: [0.42, 0.0],
      p2: [1.0, 1.0],
    },
    {
      name: "Ease Out",
      easing: { Bezier: { p1: [0.0, 0.0], p2: [0.58, 1.0] } },
      p1: [0.0, 0.0],
      p2: [0.58, 1.0],
    },
    {
      name: "Ease In Out",
      easing: { Bezier: { p1: [0.42, 0.0], p2: [0.58, 1.0] } },
      p1: [0.42, 0.0],
      p2: [0.58, 1.0],
    },
    {
      name: "Bezier",
      easing: { Bezier: { p1: [0.25, 0.1], p2: [0.25, 1.0] } },
      p1: [0.25, 0.1],
      p2: [0.25, 1.0],
    },
  ];

  async function applyPreset(preset: (typeof PRESETS)[0]) {
    if (preset.name === "Linear") {
      isLinear = true;
      p1 = [0.0, 0.0];
      p2 = [1.0, 1.0];
    } else {
      isLinear = false;
      p1 = [...preset.p1];
      p2 = [...preset.p2];
    }
    await commitEasing(preset.easing);
  }

  function isPresetActive(preset: (typeof PRESETS)[0]): boolean {
    if (preset.name === "Linear") return isLinear;
    if (isLinear) return false;
    return (
      Math.abs(p1[0] - preset.p1[0]) < 0.02 &&
      Math.abs(p1[1] - preset.p1[1]) < 0.02 &&
      Math.abs(p2[0] - preset.p2[0]) < 0.02 &&
      Math.abs(p2[1] - preset.p2[1]) < 0.02
    );
  }
</script>

<div
  role="group"
  aria-label="Bézier Curve Editor"
  class="relative flex flex-col gap-2 rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] p-3 text-xs shadow-xl select-none"
  onpointermove={onPointerMove}
  onpointerup={onPointerUp}
  onpointercancel={onPointerUp}
>
  <!-- Header with Property & Keyframe Selector -->
  <div class="flex items-center justify-between border-b border-[var(--border)] pb-2">
    <div class="flex items-center gap-2">
      <span class="font-semibold text-[var(--accent)]">{property} Curve</span>
      {#if keys.length > 1}
        <div class="flex items-center gap-1">
          {#each keys as k, idx (k.time)}
            <button
              class="rounded px-1.5 py-0.5 text-[10px] font-mono {activeKeyIndex === idx
                ? 'bg-[var(--accent)] text-white font-bold'
                : 'bg-[var(--bg-panel)] text-[var(--text-dim)] hover:text-[var(--text)]'}"
              onclick={() => (activeKeyIndex = idx)}
            >
              K{idx + 1} ({timeToSecs(k.time).toFixed(2)}s)
            </button>
          {/each}
        </div>
      {:else if activeKey}
        <span class="font-mono text-[10px] text-[var(--text-dim)]">
          @ {timeToSecs(activeKey.time).toFixed(2)}s
        </span>
      {/if}
    </div>
    {#if onClose}
      <button
        class="rounded p-1 text-[var(--text-dim)] hover:bg-[var(--bg-panel)] hover:text-[var(--text)]"
        title="Close Curve Editor"
        onclick={onClose}
      >
        ✕
      </button>
    {/if}
  </div>

  <!-- SVG Curve Graph Viewport -->
  <div class="relative flex justify-center">
    <svg
      bind:this={svgEl}
      width={gw}
      height={gh}
      class="rounded border border-[var(--border)] bg-[var(--bg-panel)] overflow-visible"
    >
      <!-- Grid Lines -->
      <line
        x1={padX}
        y1={padY}
        x2={padX + plotW}
        y2={padY}
        stroke="var(--border)"
        stroke-dasharray="2,2"
      />
      <line
        x1={padX}
        y1={padY + plotH}
        x2={padX + plotW}
        y2={padY + plotH}
        stroke="var(--border)"
      />
      <line
        x1={padX}
        y1={padY}
        x2={padX}
        y2={padY + plotH}
        stroke="var(--border)"
      />
      <line
        x1={padX + plotW}
        y1={padY}
        x2={padX + plotW}
        y2={padY + plotH}
        stroke="var(--border)"
        stroke-dasharray="2,2"
      />
      <!-- Diagonal Reference Guide -->
      <line
        x1={startPt[0]}
        y1={startPt[1]}
        x2={endPt[0]}
        y2={endPt[1]}
        stroke="rgba(255,255,255,0.08)"
        stroke-dasharray="3,3"
      />

      <!-- Velocity Profile Curve (translucent cyan) -->
      <path
        d={velocityPathD}
        fill="none"
        stroke="rgba(78, 201, 176, 0.4)"
        stroke-width="1.5"
        stroke-dasharray="3,2"
      />

      <!-- Value Interpolation Bézier Curve (Accent) -->
      <path
        d={pathD}
        fill="none"
        stroke="var(--accent)"
        stroke-width="2.5"
      />

      <!-- Tangent Handle Stalks & Circles -->
      {#if !isLinear}
        <!-- Stalk 1 -->
        <line
          x1={startPt[0]}
          y1={startPt[1]}
          x2={cp1[0]}
          y2={cp1[1]}
          stroke="var(--danger)"
          stroke-width="1.5"
        />
        <!-- Handle 1 Circle -->
        <circle
          role="button"
          tabindex="0"
          aria-label="Start tangent handle (P1)"
          cx={cp1[0]}
          cy={cp1[1]}
          r="5"
          fill="var(--danger)"
          stroke="#fff"
          stroke-width="1.5"
          class="cursor-grab hover:scale-125 transition-transform"
          onpointerdown={(e) => onHandleDown(e, "p1")}
        />

        <!-- Stalk 2 -->
        <line
          x1={endPt[0]}
          y1={endPt[1]}
          x2={cp2[0]}
          y2={cp2[1]}
          stroke="var(--accent)"
          stroke-width="1.5"
        />
        <!-- Handle 2 Circle -->
        <circle
          role="button"
          tabindex="0"
          aria-label="End tangent handle (P2)"
          cx={cp2[0]}
          cy={cp2[1]}
          r="5"
          fill="var(--accent)"
          stroke="#fff"
          stroke-width="1.5"
          class="cursor-grab hover:scale-125 transition-transform"
          onpointerdown={(e) => onHandleDown(e, "p2")}
        />
      {/if}

      <!-- Start & End Keyframe Points -->
      <circle cx={startPt[0]} cy={startPt[1]} r="4" fill="#fff" />
      <circle cx={endPt[0]} cy={endPt[1]} r="4" fill="#fff" />
    </svg>
  </div>

  <!-- Legend & Values -->
  <div class="flex items-center justify-between px-1 text-[10px] text-[var(--text-dim)]">
    <div class="flex items-center gap-3">
      <span class="flex items-center gap-1 font-mono">
        <span class="inline-block h-2 w-2 rounded-full bg-[var(--accent)]"></span>
        Value Curve
      </span>
      <span class="flex items-center gap-1 font-mono">
        <span class="inline-block h-2 w-2 rounded-full bg-[#4ec9b0]"></span>
        Speed
      </span>
    </div>
    {#if !isLinear}
      <span class="font-mono">
        p1: [{p1[0].toFixed(2)}, {p1[1].toFixed(2)}] · p2: [{p2[0].toFixed(2)}, {p2[1].toFixed(2)}]
      </span>
    {/if}
  </div>

  <!-- Numerical Control Inputs -->
  {#if !isLinear}
    <div class="flex items-center gap-2 border-t border-[var(--border)] pt-1.5 font-mono text-[10px]">
      <div class="flex items-center gap-1">
        <span class="text-[var(--danger)]">p1:</span>
        <input
          type="number"
          step="0.05"
          min="0"
          max="1"
          class="w-12 rounded border border-[var(--border)] bg-[var(--bg-panel)] px-1 py-0.5"
          bind:value={p1[0]}
          onchange={() => commitEasing({ Bezier: { p1: [p1[0], p1[1]], p2: [p2[0], p2[1]] } })}
        />
        <input
          type="number"
          step="0.05"
          min="-0.5"
          max="1.5"
          class="w-12 rounded border border-[var(--border)] bg-[var(--bg-panel)] px-1 py-0.5"
          bind:value={p1[1]}
          onchange={() => commitEasing({ Bezier: { p1: [p1[0], p1[1]], p2: [p2[0], p2[1]] } })}
        />
      </div>
      <div class="flex items-center gap-1">
        <span class="text-[var(--accent)]">p2:</span>
        <input
          type="number"
          step="0.05"
          min="0"
          max="1"
          class="w-12 rounded border border-[var(--border)] bg-[var(--bg-panel)] px-1 py-0.5"
          bind:value={p2[0]}
          onchange={() => commitEasing({ Bezier: { p1: [p1[0], p1[1]], p2: [p2[0], p2[1]] } })}
        />
        <input
          type="number"
          step="0.05"
          min="-0.5"
          max="1.5"
          class="w-12 rounded border border-[var(--border)] bg-[var(--bg-panel)] px-1 py-0.5"
          bind:value={p2[1]}
          onchange={() => commitEasing({ Bezier: { p1: [p1[0], p1[1]], p2: [p2[0], p2[1]] } })}
        />
      </div>
    </div>
  {/if}

  <!-- Easing Preset Buttons -->
  <div class="flex flex-wrap gap-1 pt-1 border-t border-[var(--border)]">
    {#each PRESETS as preset (preset.name)}
      <button
        class="flex-1 rounded px-2 py-1 text-center font-medium text-[11px] transition-colors {isPresetActive(
          preset,
        )
          ? 'bg-[var(--accent)] text-white'
          : 'bg-[var(--bg-panel)] text-[var(--text-dim)] hover:bg-[var(--border)] hover:text-[var(--text)]'}"
        onclick={() => void applyPreset(preset)}
      >
        {preset.name}
      </button>
    {/each}
  </div>
</div>
