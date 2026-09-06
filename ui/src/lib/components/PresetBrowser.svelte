<script lang="ts">
  import { editor, activeComp, selectedLayer, applyOp } from "../store.svelte";
  import { evaluate, type Layer, type Property, type PropValue, type Easing, type Comp, TICKS_PER_SEC } from "../model";

  interface Props {
    onClose?: () => void;
  }

  let { onClose }: Props = $props();

  const comp = $derived(activeComp());
  const layer = $derived(selectedLayer());

  type Category = "All" | "Entrance" | "Exit" | "Emphasis" | "Motion";

  interface PresetKeyframe {
    offsetSec: number;
    property: Property;
    getValue: (layer: Layer, comp: Comp) => PropValue;
    easing: Easing;
  }

  interface AnimationPreset {
    id: string;
    name: string;
    category: "Entrance" | "Exit" | "Emphasis" | "Motion";
    description: string;
    durationSec: number;
    tracks: Property[];
    curvePath: string;
    icon: string;
    keyframes: PresetKeyframe[];
  }

  const PRESETS: AnimationPreset[] = [
    {
      id: "fade-in",
      name: "Fade In",
      category: "Entrance",
      description: "Smooth opacity fade in with natural deceleration ease",
      durationSec: 0.6,
      tracks: ["Opacity"],
      icon: "🌓",
      curvePath: "M 8,36 C 24,32 38,8 72,8",
      keyframes: [
        {
          offsetSec: 0.0,
          property: "Opacity",
          getValue: () => ({ Scalar: 0.0 }),
          easing: { Bezier: { p1: [0.25, 0.1], p2: [0.25, 1.0] } },
        },
        {
          offsetSec: 0.6,
          property: "Opacity",
          getValue: (l) => ({ Scalar: l.transform.opacity ?? 1.0 }),
          easing: "Linear",
        },
      ],
    },
    {
      id: "fade-out",
      name: "Fade Out",
      category: "Exit",
      description: "Smooth opacity fade out into full transparency",
      durationSec: 0.6,
      tracks: ["Opacity"],
      icon: "🌘",
      curvePath: "M 8,8 C 36,8 50,32 72,36",
      keyframes: [
        {
          offsetSec: 0.0,
          property: "Opacity",
          getValue: (l) => ({ Scalar: l.transform.opacity ?? 1.0 }),
          easing: { Bezier: { p1: [0.42, 0.0], p2: [1.0, 1.0] } },
        },
        {
          offsetSec: 0.6,
          property: "Opacity",
          getValue: () => ({ Scalar: 0.0 }),
          easing: "Linear",
        },
      ],
    },
    {
      id: "pop-in",
      name: "Pop In",
      category: "Entrance",
      description: "Snappy multi-track scale overshoot with simultaneous opacity ramp",
      durationSec: 0.5,
      tracks: ["Scale", "Opacity"],
      icon: "💥",
      curvePath: "M 8,36 C 24,10 34,4 44,4 C 54,4 62,12 72,10",
      keyframes: [
        {
          offsetSec: 0.0,
          property: "Scale",
          getValue: () => ({ Vec2: [0, 0] }),
          easing: { Bezier: { p1: [0.17, 0.89], p2: [0.32, 1.25] } },
        },
        {
          offsetSec: 0.32,
          property: "Scale",
          getValue: (l) => {
            const s = l.transform.scale ?? [100, 100];
            return { Vec2: [s[0] * 1.15, s[1] * 1.15] };
          },
          easing: { Bezier: { p1: [0.25, 0.1], p2: [0.25, 1.0] } },
        },
        {
          offsetSec: 0.5,
          property: "Scale",
          getValue: (l) => {
            const s = l.transform.scale ?? [100, 100];
            return { Vec2: [s[0], s[1]] };
          },
          easing: "Linear",
        },
        {
          offsetSec: 0.0,
          property: "Opacity",
          getValue: () => ({ Scalar: 0.0 }),
          easing: { Bezier: { p1: [0.2, 0.0], p2: [0.4, 1.0] } },
        },
        {
          offsetSec: 0.22,
          property: "Opacity",
          getValue: (l) => ({ Scalar: l.transform.opacity ?? 1.0 }),
          easing: "Linear",
        },
      ],
    },
    {
      id: "slide-in-left",
      name: "Slide In (Left)",
      category: "Entrance",
      description: "Multi-track position glide from off-screen left with opacity fade",
      durationSec: 0.6,
      tracks: ["Position", "Opacity"],
      icon: "➡",
      curvePath: "M 8,36 C 18,10 38,8 72,8",
      keyframes: [
        {
          offsetSec: 0.0,
          property: "Position",
          getValue: (l, c) => {
            const p = l.transform.position ?? [0, 0];
            const offset = c.width ? c.width * 0.45 : 320;
            return { Vec2: [p[0] - offset, p[1]] };
          },
          easing: { Bezier: { p1: [0.16, 1.0], p2: [0.3, 1.0] } },
        },
        {
          offsetSec: 0.6,
          property: "Position",
          getValue: (l) => {
            const p = l.transform.position ?? [0, 0];
            return { Vec2: [p[0], p[1]] };
          },
          easing: "Linear",
        },
        {
          offsetSec: 0.0,
          property: "Opacity",
          getValue: () => ({ Scalar: 0.0 }),
          easing: { Bezier: { p1: [0.2, 0.0], p2: [0.5, 1.0] } },
        },
        {
          offsetSec: 0.35,
          property: "Opacity",
          getValue: (l) => ({ Scalar: l.transform.opacity ?? 1.0 }),
          easing: "Linear",
        },
      ],
    },
    {
      id: "slide-in-bottom",
      name: "Slide In (Bottom)",
      category: "Entrance",
      description: "Smooth vertical entrance gliding upward into final position",
      durationSec: 0.6,
      tracks: ["Position", "Opacity"],
      icon: "⬆",
      curvePath: "M 8,36 C 20,12 40,8 72,8",
      keyframes: [
        {
          offsetSec: 0.0,
          property: "Position",
          getValue: (l, c) => {
            const p = l.transform.position ?? [0, 0];
            const offset = c.height ? c.height * 0.4 : 200;
            return { Vec2: [p[0], p[1] + offset] };
          },
          easing: { Bezier: { p1: [0.16, 1.0], p2: [0.3, 1.0] } },
        },
        {
          offsetSec: 0.6,
          property: "Position",
          getValue: (l) => {
            const p = l.transform.position ?? [0, 0];
            return { Vec2: [p[0], p[1]] };
          },
          easing: "Linear",
        },
        {
          offsetSec: 0.0,
          property: "Opacity",
          getValue: () => ({ Scalar: 0.0 }),
          easing: { Bezier: { p1: [0.2, 0.0], p2: [0.5, 1.0] } },
        },
        {
          offsetSec: 0.35,
          property: "Opacity",
          getValue: (l) => ({ Scalar: l.transform.opacity ?? 1.0 }),
          easing: "Linear",
        },
      ],
    },
    {
      id: "bounce",
      name: "Bounce",
      category: "Emphasis",
      description: "Organic multi-keyframe elastic bounce with decaying oscillation",
      durationSec: 0.8,
      tracks: ["Scale"],
      icon: "🏀",
      curvePath: "M 8,36 C 16,6 22,4 28,4 C 34,4 38,22 44,22 C 50,22 56,10 62,10 C 67,10 70,12 72,12",
      keyframes: [
        {
          offsetSec: 0.0,
          property: "Scale",
          getValue: () => ({ Vec2: [0, 0] }),
          easing: { Bezier: { p1: [0.34, 1.56], p2: [0.64, 1.0] } },
        },
        {
          offsetSec: 0.28,
          property: "Scale",
          getValue: (l) => {
            const s = l.transform.scale ?? [100, 100];
            return { Vec2: [s[0] * 1.25, s[1] * 1.25] };
          },
          easing: { Bezier: { p1: [0.25, 0.1], p2: [0.25, 1.0] } },
        },
        {
          offsetSec: 0.48,
          property: "Scale",
          getValue: (l) => {
            const s = l.transform.scale ?? [100, 100];
            return { Vec2: [s[0] * 0.9, s[1] * 0.9] };
          },
          easing: { Bezier: { p1: [0.25, 0.1], p2: [0.25, 1.0] } },
        },
        {
          offsetSec: 0.65,
          property: "Scale",
          getValue: (l) => {
            const s = l.transform.scale ?? [100, 100];
            return { Vec2: [s[0] * 1.05, s[1] * 1.05] };
          },
          easing: { Bezier: { p1: [0.25, 0.1], p2: [0.25, 1.0] } },
        },
        {
          offsetSec: 0.8,
          property: "Scale",
          getValue: (l) => {
            const s = l.transform.scale ?? [100, 100];
            return { Vec2: [s[0], s[1]] };
          },
          easing: "Linear",
        },
      ],
    },
    {
      id: "spin-in",
      name: "Spin & Zoom",
      category: "Entrance",
      description: "Full 360-degree rotation spin combined with scale-up and fade",
      durationSec: 0.7,
      tracks: ["Rotation", "Scale", "Opacity"],
      icon: "💫",
      curvePath: "M 8,36 C 22,26 36,12 72,8",
      keyframes: [
        {
          offsetSec: 0.0,
          property: "Rotation",
          getValue: (l) => ({ Scalar: (l.transform.rotation ?? 0) - 360 }),
          easing: { Bezier: { p1: [0.16, 1.0], p2: [0.3, 1.0] } },
        },
        {
          offsetSec: 0.7,
          property: "Rotation",
          getValue: (l) => ({ Scalar: l.transform.rotation ?? 0 }),
          easing: "Linear",
        },
        {
          offsetSec: 0.0,
          property: "Scale",
          getValue: () => ({ Vec2: [0, 0] }),
          easing: { Bezier: { p1: [0.16, 1.0], p2: [0.3, 1.0] } },
        },
        {
          offsetSec: 0.7,
          property: "Scale",
          getValue: (l) => {
            const s = l.transform.scale ?? [100, 100];
            return { Vec2: [s[0], s[1]] };
          },
          easing: "Linear",
        },
        {
          offsetSec: 0.0,
          property: "Opacity",
          getValue: () => ({ Scalar: 0.0 }),
          easing: "Linear",
        },
        {
          offsetSec: 0.35,
          property: "Opacity",
          getValue: (l) => ({ Scalar: l.transform.opacity ?? 1.0 }),
          easing: "Linear",
        },
      ],
    },
    {
      id: "pulse",
      name: "Pulse / Heartbeat",
      category: "Emphasis",
      description: "Rhythmic scale expansion and recoil to emphasize layer presence",
      durationSec: 0.6,
      tracks: ["Scale"],
      icon: "💓",
      curvePath: "M 8,24 C 18,4 26,4 32,24 C 38,14 44,14 50,24 C 58,24 66,24 72,24",
      keyframes: [
        {
          offsetSec: 0.0,
          property: "Scale",
          getValue: (l) => {
            const s = l.transform.scale ?? [100, 100];
            return { Vec2: [s[0], s[1]] };
          },
          easing: { Bezier: { p1: [0.25, 0.1], p2: [0.25, 1.0] } },
        },
        {
          offsetSec: 0.18,
          property: "Scale",
          getValue: (l) => {
            const s = l.transform.scale ?? [100, 100];
            return { Vec2: [s[0] * 1.2, s[1] * 1.2] };
          },
          easing: { Bezier: { p1: [0.25, 0.1], p2: [0.25, 1.0] } },
        },
        {
          offsetSec: 0.32,
          property: "Scale",
          getValue: (l) => {
            const s = l.transform.scale ?? [100, 100];
            return { Vec2: [s[0] * 0.95, s[1] * 0.95] };
          },
          easing: { Bezier: { p1: [0.25, 0.1], p2: [0.25, 1.0] } },
        },
        {
          offsetSec: 0.46,
          property: "Scale",
          getValue: (l) => {
            const s = l.transform.scale ?? [100, 100];
            return { Vec2: [s[0] * 1.08, s[1] * 1.08] };
          },
          easing: { Bezier: { p1: [0.25, 0.1], p2: [0.25, 1.0] } },
        },
        {
          offsetSec: 0.6,
          property: "Scale",
          getValue: (l) => {
            const s = l.transform.scale ?? [100, 100];
            return { Vec2: [s[0], s[1]] };
          },
          easing: "Linear",
        },
      ],
    },
  ];

  let searchQuery = $state("");
  let selectedCategory = $state<Category>("All");
  let applyingId = $state<string | null>(null);
  let feedback = $state<{ type: "success" | "error"; message: string } | null>(null);

  const filteredPresets = $derived(
    PRESETS.filter((p) => {
      const matchCat = selectedCategory === "All" || p.category === selectedCategory;
      const matchQuery =
        searchQuery.trim() === "" ||
        p.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
        p.description.toLowerCase().includes(searchQuery.toLowerCase()) ||
        p.tracks.some((t) => t.toLowerCase().includes(searchQuery.toLowerCase()));
      return matchCat && matchQuery;
    }),
  );

  /** One-click multi-track insertion of preset keyframes into selected layer. */
  async function applyPreset(preset: AnimationPreset) {
    if (!comp) {
      feedback = { type: "error", message: "No active composition found." };
      return;
    }
    if (!layer) {
      feedback = { type: "error", message: "Select a layer to apply this preset." };
      return;
    }

    applyingId = preset.id;
    try {
      const isTicks = comp.duration > 1000;
      const startTime = editor.currentTime;

      // Sequentially apply each keyframe Op so all tracks are updated cleanly
      for (const kf of preset.keyframes) {
        const keyTime = isTicks
          ? Math.round(startTime + kf.offsetSec * TICKS_PER_SEC)
          : Number((startTime + kf.offsetSec).toFixed(4));

        const value = kf.getValue(layer, comp);

        await applyOp({
          type: "addKeyframe",
          comp: comp.id,
          layer: layer.id,
          property: kf.property,
          key: {
            time: keyTime,
            value,
            easing: kf.easing,
          },
        });
      }

      feedback = {
        type: "success",
        message: `Applied "${preset.name}" (${preset.tracks.join(", ")}) to ${layer.name}`,
      };
      setTimeout(() => {
        if (feedback?.message.includes(preset.name)) feedback = null;
      }, 3500);
    } catch (err) {
      feedback = {
        type: "error",
        message: `Failed to apply preset: ${err instanceof Error ? err.message : String(err)}`,
      };
    } finally {
      applyingId = null;
    }
  }
</script>

<aside
  class="flex h-full min-h-0 w-full flex-col border-r border-[var(--border)] bg-[var(--bg-panel)] select-none"
  aria-label="Animation Preset Browser"
>
  <!-- Header with Title, Layer Target, and Close Button -->
  <div class="flex items-center justify-between border-b border-[var(--border)] px-4 py-2.5">
    <div class="flex items-center gap-2">
      <span class="text-sm">✨</span>
      <span class="text-xs font-bold uppercase tracking-wider text-[var(--text)]">Animation Presets</span>
    </div>
    {#if onClose}
      <button
        type="button"
        class="flex h-6 w-6 items-center justify-center rounded text-xs text-[var(--text-dim)] hover:bg-[var(--bg-raised)] hover:text-[var(--text)]"
        title="Close Preset Browser"
        onclick={onClose}
      >
        ✕
      </button>
    {/if}
  </div>

  <!-- Target Layer Indicator & Status -->
  <div class="border-b border-[var(--border-subtle)] bg-[var(--bg-raised)] px-4 py-2 text-[11px]">
    <div class="flex items-center justify-between">
      <span class="text-[var(--text-dim)]">Target:</span>
      {#if layer}
        <span class="flex items-center gap-1 font-semibold text-[var(--accent)]">
          <span class="inline-block h-1.5 w-1.5 rounded-full bg-[var(--accent)]"></span>
          {layer.name}
        </span>
      {:else}
        <span class="text-[var(--warning)]">No layer selected</span>
      {/if}
    </div>
  </div>

  <!-- Search & Category Filters -->
  <div class="space-y-2 border-b border-[var(--border)] p-3">
    <!-- Search Input -->
    <div class="relative">
      <input
        type="text"
        placeholder="Search presets or tracks..."
        bind:value={searchQuery}
        class="w-full rounded-md border border-[var(--border)] bg-[var(--bg-base)] px-2.5 py-1.5 text-xs text-[var(--text)] placeholder-[var(--text-muted)] outline-none transition-colors focus:border-[var(--accent)]"
      />
      {#if searchQuery}
        <button
          type="button"
          class="absolute right-2 top-1/2 -translate-y-1/2 text-xs text-[var(--text-dim)] hover:text-[var(--text)]"
          onclick={() => (searchQuery = "")}
        >
          ✕
        </button>
      {/if}
    </div>

    <!-- Category Filter Pills -->
    <div class="flex flex-wrap gap-1">
      {#each ["All", "Entrance", "Exit", "Emphasis", "Motion"] as cat (cat)}
        <button
          type="button"
          class="rounded px-2 py-0.5 text-[10px] font-medium transition-colors {selectedCategory === cat
            ? 'bg-[var(--accent)] text-white'
            : 'bg-[var(--bg-raised)] text-[var(--text-dim)] hover:bg-[var(--bg-hover)] hover:text-[var(--text)]'}"
          onclick={() => (selectedCategory = cat as Category)}
        >
          {cat}
        </button>
      {/each}
    </div>
  </div>

  <!-- Feedback Banner (Toast) -->
  {#if feedback}
    <div
      class="mx-3 mt-2 flex items-center justify-between rounded px-2.5 py-1.5 text-[11px] {feedback.type === 'success'
        ? 'border border-[var(--success)] bg-[var(--success-soft)] text-[var(--success)]'
        : 'border border-[var(--danger)] bg-[var(--danger-soft)] text-[var(--danger)]'}"
    >
      <span>{feedback.message}</span>
      <button
        type="button"
        class="ml-2 text-xs opacity-70 hover:opacity-100"
        onclick={() => (feedback = null)}
      >
        ✕
      </button>
    </div>
  {/if}

  <!-- Preset List Cards -->
  <div class="min-h-0 flex-1 space-y-2.5 overflow-y-auto p-3">
    {#if filteredPresets.length === 0}
      <div class="py-8 text-center text-xs text-[var(--text-dim)]">
        No presets match "{searchQuery}"
      </div>
    {/if}

    {#each filteredPresets as preset (preset.id)}
      <div
        class="group relative rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] p-3 transition-all hover:border-[var(--border-hover)] hover:bg-[var(--bg-surface)] hover:shadow-md"
      >
        <!-- Card Header: Icon, Name & Category Badge -->
        <div class="mb-1.5 flex items-start justify-between">
          <div class="flex items-center gap-2">
            <span class="text-base">{preset.icon}</span>
            <div>
              <div class="text-xs font-semibold text-[var(--text)]">{preset.name}</div>
              <div class="text-[10px] text-[var(--text-dim)]">{preset.description}</div>
            </div>
          </div>
          <span class="rounded bg-[var(--bg-base)] px-1.5 py-0.5 text-[9px] font-medium text-[var(--text-dim)]">
            {preset.category}
          </span>
        </div>

        <!-- Curve Visualizer & Animation Preview -->
        <div class="my-2 flex items-center gap-3 rounded border border-[var(--border-subtle)] bg-[var(--bg-base)] p-2">
          <!-- SVG Curve Line Graph (80x44) -->
          <div class="relative h-11 w-20 flex-shrink-0">
            <svg class="h-full w-full overflow-visible" viewBox="0 0 80 44">
              <!-- Grid lines -->
              <line x1="8" y1="8" x2="72" y2="8" stroke="var(--border)" stroke-width="1" stroke-dasharray="2,2" />
              <line x1="8" y1="36" x2="72" y2="36" stroke="var(--border)" stroke-width="1" stroke-dasharray="2,2" />
              <!-- Motion Ease Curve -->
              <path
                d={preset.curvePath}
                fill="none"
                stroke="var(--accent)"
                stroke-width="2.5"
                stroke-linecap="round"
              />
              <!-- Origin and destination dots -->
              <circle cx="8" cy="36" r="2.5" fill="var(--accent-hover)" />
              <circle cx="72" cy="8" r="2.5" fill="var(--accent)" />
            </svg>
          </div>

          <!-- Preset Details: Duration, Key Count, Tracks -->
          <div class="min-w-0 flex-1 space-y-1 text-[10px]">
            <div class="flex items-center gap-2 text-[var(--text-dim)]">
              <span>⏱ {preset.durationSec}s</span>
              <span>·</span>
              <span>{preset.keyframes.length} keys</span>
            </div>
            <!-- Multi-track badges -->
            <div class="flex flex-wrap gap-1">
              {#each preset.tracks as track (track)}
                <span class="rounded bg-[var(--bg-raised)] px-1.5 py-0.5 font-mono text-[9px] font-medium text-[var(--accent)]">
                  {track}
                </span>
              {/each}
            </div>
          </div>
        </div>

        <!-- One-Click Apply Button -->
        <button
          type="button"
          class="mt-1 flex w-full items-center justify-center gap-1.5 rounded-md bg-[var(--bg-surface)] py-1.5 text-xs font-semibold text-[var(--text)] transition-all group-hover:bg-[var(--accent)] group-hover:text-white disabled:pointer-events-none disabled:opacity-40"
          disabled={!layer || applyingId === preset.id}
          onclick={() => applyPreset(preset)}
          title={layer ? `Insert ${preset.name} keyframes into ${layer.name} at playhead` : "Select a layer to apply"}
        >
          {#if applyingId === preset.id}
            <span>Applying…</span>
          {:else}
            <span>⚡ Apply Preset</span>
          {/if}
        </button>
      </div>
    {/each}
  </div>

  <!-- Bottom Tip -->
  <div class="border-t border-[var(--border)] bg-[var(--bg-raised)] px-3 py-2 text-center text-[10px] text-[var(--text-dim)]">
    Applies multi-track keyframes starting at current playhead.
  </div>
</aside>
