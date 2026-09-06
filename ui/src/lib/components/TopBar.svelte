<script lang="ts">
  import { editor, activeComp, play, pause, undoOp, redoOp, scrub, applyOp } from "../store.svelte";
  import {
    fpsAsNumber,
    formatFps,
    ticksPerFrame,
    TICKS_PER_SEC,
    type Layer,
    type LayerKind,
  } from "../model";

  interface Props {
    onTogglePresets?: () => void;
    presetsOpen?: boolean;
  }

  let { onTogglePresets, presetsOpen = false }: Props = $props();

  let addMenuOpen = $state(false);

  const comp = $derived(activeComp());
  const isPlaying = $derived(editor.playing);
  const fpsNum = $derived(fpsAsNumber(comp?.fps) || 30);
  const formattedFps = $derived(comp ? formatFps(comp.fps) : "30 fps");

  async function createLayer(type: "circle" | "rect" | "text" | "solid") {
    if (!comp) return;
    addMenuOpen = false;

    let kind: LayerKind;
    let name: string;
    const count = comp.layer_order.length + 1;

    if (type === "circle") {
      name = `Circle ${count}`;
      kind = {
        Shape: {
          color: [0.95, 0.35, 0.35, 1.0],
          generator: "builtin.circle",
        },
      };
    } else if (type === "rect") {
      name = `Rectangle ${count}`;
      kind = {
        Shape: {
          color: [0.42, 0.54, 1.0, 1.0],
        },
      };
    } else if (type === "text") {
      name = `Text ${count}`;
      kind = {
        Text: {
          text: "Text Layer",
          size: 48,
        },
      };
    } else {
      name = `Solid ${count}`;
      kind = {
        Solid: {
          color: [0.15, 0.18, 0.25, 1.0],
        },
      };
    }

    const newLayer: Layer = {
      id: 0,
      name,
      kind,
      start: 0,
      duration: comp.duration,
      transform: {
        position: [0, 0],
        scale: type === "circle" ? [100, 100] : [120, 80],
        rotation: 0,
        opacity: 1,
        anchor_point: [0, 0],
      },
      tracks: {},
      visible: true,
      locked: false,
    };

    await applyOp({
      type: "addLayer",
      comp: comp.id,
      layer: newLayer,
    });

    const updated = activeComp();
    if (updated && updated.layer_order.length > 0) {
      editor.selected = updated.layer_order[updated.layer_order.length - 1];
    }
  }

  // Step duration in timeline units (ticks if duration > 1000, seconds otherwise)
  const isTicks = $derived(Boolean(comp && comp.duration > 1000));
  const frameStep = $derived(() => {
    if (!comp) return 1 / 30;
    if (isTicks) {
      return ticksPerFrame(comp.fps);
    }
    return 1 / fpsNum;
  });

  function stepFrame(delta: number) {
    if (!comp) return;
    const step = frameStep();
    const duration = comp.duration;
    const nextTime = Math.min(duration, Math.max(0, editor.currentTime + delta * step));
    scrub(nextTime);
  }

  /** Formats time into SMPTE timecode string HH:MM:SS:FF. */
  function formatTimecode(time: number): string {
    if (!comp) return "00:00:00:00";
    const fps = fpsNum > 0 ? fpsNum : 30;
    let totalFrames: number;
    if (isTicks) {
      totalFrames = Math.floor((time * fps) / TICKS_PER_SEC);
    } else {
      totalFrames = Math.floor(time * fps + 1e-4);
    }

    const isNeg = totalFrames < 0;
    const absFrames = Math.abs(totalFrames);
    const nomFps = Math.max(1, Math.round(fps));

    const ff = absFrames % nomFps;
    const totalSec = Math.floor(absFrames / nomFps);
    const ss = totalSec % 60;
    const totalMin = Math.floor(totalSec / 60);
    const mm = totalMin % 60;
    const hh = Math.floor(totalMin / 60);

    const pad = (n: number) => String(n).padStart(2, "0");
    const prefix = isNeg ? "-" : "";
    return `${prefix}${pad(hh)}:${pad(mm)}:${pad(ss)}:${pad(ff)}`;
  }

  const currentTimecode = $derived(formatTimecode(editor.currentTime));
  const durationTimecode = $derived(comp ? formatTimecode(comp.duration) : "00:00:00:00");
  const currentFrame = $derived(() => {
    if (!comp) return 0;
    const fps = fpsNum > 0 ? fpsNum : 30;
    if (isTicks) {
      return Math.floor((editor.currentTime * fps) / TICKS_PER_SEC);
    }
    return Math.floor(editor.currentTime * fps + 1e-4);
  });
</script>

<header class="flex h-12 select-none items-center justify-between border-b border-[var(--border)] bg-[var(--bg-panel)] px-4">
  <!-- Brand & Composition Info -->
  <div class="flex items-center gap-3">
    <div class="flex items-baseline gap-2">
      <span class="bg-gradient-to-r from-[var(--accent)] to-[#9bb2ff] bg-clip-text text-[15px] font-extrabold tracking-wider text-transparent">
        BONAPARTE
      </span>
      <span class="rounded bg-[var(--bg-raised)] px-1.5 py-0.5 text-[9px] font-semibold uppercase tracking-widest text-[var(--text-dim)]">
        v0.1
      </span>
    </div>

    {#if comp}
      <div class="hidden items-center gap-2 border-l border-[var(--border)] pl-3 text-xs text-[var(--text-dim)] sm:flex">
        <span class="font-medium text-[var(--text)]">{comp.name}</span>
        <span>·</span>
        <span>{comp.width}×{comp.height}</span>
        <span>·</span>
        <!-- FPS Indicator badge -->
        <span
          class="flex items-center gap-1 rounded bg-[var(--bg-raised)] px-1.5 py-0.5 font-mono text-[11px] font-medium text-[var(--accent)]"
          title={`Composition frame rate: ${formattedFps}`}
        >
          <span class="inline-block h-1.5 w-1.5 rounded-full bg-[var(--accent)]"></span>
          {formattedFps}
        </span>
      </div>
    {/if}
  </div>

  <!-- Center Playback & Timecode Controls -->
  <div class="flex items-center gap-2">
    <!-- Step Backward 1 Frame -->
    <button
      type="button"
      class="flex h-8 w-8 items-center justify-center rounded-md text-xs text-[var(--text-dim)] transition-colors hover:bg-[var(--bg-raised)] hover:text-[var(--text)] disabled:pointer-events-none disabled:opacity-30"
      title="Step Backward 1 Frame (Left Arrow)"
      disabled={!comp}
      onclick={() => stepFrame(-1)}
    >
      ⏮
    </button>

    <!-- Play / Pause Button -->
    <button
      type="button"
      class="flex h-8 min-w-10 items-center justify-center gap-1 rounded-md px-3 text-xs font-semibold transition-all disabled:pointer-events-none disabled:opacity-30 {isPlaying
        ? 'bg-[var(--accent)] text-white shadow-[0_0_12px_rgba(107,138,253,0.4)]'
        : 'bg-[var(--bg-raised)] text-[var(--text)] hover:bg-[var(--bg-hover)]'}"
      title={isPlaying ? "Pause (Space)" : "Play (Space)"}
      disabled={!comp}
      onclick={() => (isPlaying ? pause() : play())}
    >
      <span class="text-sm">{isPlaying ? "⏸" : "▶"}</span>
      <span class="hidden text-[11px] sm:inline">{isPlaying ? "Pause" : "Play"}</span>
    </button>

    <!-- Step Forward 1 Frame -->
    <button
      type="button"
      class="flex h-8 w-8 items-center justify-center rounded-md text-xs text-[var(--text-dim)] transition-colors hover:bg-[var(--bg-raised)] hover:text-[var(--text)] disabled:pointer-events-none disabled:opacity-30"
      title="Step Forward 1 Frame (Right Arrow)"
      disabled={!comp}
      onclick={() => stepFrame(1)}
    >
      ⏭
    </button>

    <!-- Timecode Display: HH:MM:SS:FF / Duration -->
    <div
      class="flex items-baseline gap-1.5 rounded-md border border-[var(--border)] bg-[var(--bg-base)] px-2.5 py-1 font-mono text-xs shadow-inner"
      title={`Current frame: ${currentFrame()}`}
    >
      <span class="font-bold tracking-wider text-[var(--text)]">
        {currentTimecode}
      </span>
      <span class="text-[10px] text-[var(--text-muted)]">/</span>
      <span class="text-[11px] tracking-wider text-[var(--text-dim)]">
        {durationTimecode}
      </span>
    </div>
  </div>

  <!-- Right Controls: Add Layer, Presets Toggle, Undo, Redo -->
  <div class="flex items-center gap-2">
    <!-- Add Layer Dropdown -->
    <div class="relative">
      <button
        type="button"
        class="flex h-8 items-center gap-1.5 rounded-md border border-[var(--border)] bg-[var(--bg-raised)] px-2.5 text-xs font-medium text-[var(--text)] transition-colors hover:bg-[var(--bg-hover)] disabled:pointer-events-none disabled:opacity-30"
        title="Add a new layer to composition"
        disabled={!comp}
        onclick={() => (addMenuOpen = !addMenuOpen)}
      >
        <span class="text-sm font-bold text-[var(--accent)]">+</span>
        <span class="hidden sm:inline">Add Layer</span>
        <span class="text-[9px] text-[var(--text-dim)]">▼</span>
      </button>

      {#if addMenuOpen}
        <!-- Click outside to dismiss -->
        <div
          class="fixed inset-0 z-40"
          onclick={() => (addMenuOpen = false)}
          role="presentation"
        ></div>

        <div
          class="absolute right-0 top-full z-50 mt-1 w-52 rounded-lg border border-[var(--border)] bg-[var(--bg-panel)] p-1.5 shadow-xl select-none"
        >
          <div class="px-2 py-1 text-[10px] font-semibold uppercase tracking-wider text-[var(--text-dim)]">
            Plugin Generators & Shapes
          </div>

          <button
            type="button"
            class="flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-xs text-[var(--text)] transition-colors hover:bg-[var(--bg-raised)]"
            onclick={() => void createLayer("circle")}
          >
            <span class="text-base leading-none text-[#ff6b6b]">⭕</span>
            <div class="flex flex-col">
              <span class="font-medium">Circle Shape</span>
              <span class="text-[10px] text-[var(--accent)] font-mono">builtin.circle plugin</span>
            </div>
          </button>

          <button
            type="button"
            class="flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-xs text-[var(--text)] transition-colors hover:bg-[var(--bg-raised)]"
            onclick={() => void createLayer("rect")}
          >
            <span class="text-base leading-none text-[#6b8afd]">⬛</span>
            <div class="flex flex-col">
              <span class="font-medium">Rectangle Shape</span>
              <span class="text-[10px] text-[var(--text-dim)]">Vector shape primitive</span>
            </div>
          </button>

          <div class="my-1 border-t border-[var(--border)]"></div>

          <div class="px-2 py-1 text-[10px] font-semibold uppercase tracking-wider text-[var(--text-dim)]">
            Standard Layers
          </div>

          <button
            type="button"
            class="flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-xs text-[var(--text)] transition-colors hover:bg-[var(--bg-raised)]"
            onclick={() => void createLayer("text")}
          >
            <span class="text-base leading-none text-[#ffe066]">📝</span>
            <div class="flex flex-col">
              <span class="font-medium">Text Layer</span>
              <span class="text-[10px] text-[var(--text-dim)]">Vector typography</span>
            </div>
          </button>

          <button
            type="button"
            class="flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-xs text-[var(--text)] transition-colors hover:bg-[var(--bg-raised)]"
            onclick={() => void createLayer("solid")}
          >
            <span class="text-base leading-none text-[#51cf66]">🎨</span>
            <div class="flex flex-col">
              <span class="font-medium">Solid Color Layer</span>
              <span class="text-[10px] text-[var(--text-dim)]">Canvas backdrop fill</span>
            </div>
          </button>
        </div>
      {/if}
    </div>

    <!-- Preset Browser Toggle -->
    <button
      type="button"
      class="flex h-8 items-center gap-1.5 rounded-md px-2.5 text-xs font-medium transition-all disabled:pointer-events-none disabled:opacity-30 {presetsOpen
        ? 'border border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)] shadow-sm'
        : 'border border-transparent bg-[var(--bg-raised)] text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text)]'}"
      title="Toggle Animation Presets Browser (P)"
      disabled={!comp}
      onclick={() => onTogglePresets?.()}
    >
      <span class="text-xs">✨</span>
      <span class="hidden sm:inline">Presets</span>
    </button>

    <div class="h-4 w-[1px] bg-[var(--border)]"></div>

    <!-- Undo -->
    <button
      type="button"
      class="flex h-8 w-8 items-center justify-center rounded-md text-sm text-[var(--text-secondary)] transition-colors hover:bg-[var(--bg-raised)] hover:text-[var(--text)] disabled:pointer-events-none disabled:opacity-30"
      title="Undo (Ctrl+Z)"
      disabled={!editor.project}
      onclick={() => void undoOp()}
    >
      ↺
    </button>

    <!-- Redo -->
    <button
      type="button"
      class="flex h-8 w-8 items-center justify-center rounded-md text-sm text-[var(--text-secondary)] transition-colors hover:bg-[var(--bg-raised)] hover:text-[var(--text)] disabled:pointer-events-none disabled:opacity-30"
      title="Redo (Ctrl+Shift+Z)"
      disabled={!editor.project}
      onclick={() => void redoOp()}
    >
      ↻
    </button>
  </div>
</header>
