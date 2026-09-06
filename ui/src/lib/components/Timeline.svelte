<script lang="ts">
  import { editor, activeComp, scrub, applyOp } from "../store.svelte";
  import {
    ticksPerFrame,
    snapToFrame,
    timeToSecs,
    formatFps,
    timeToTimecode,
    TICKS_PER_SEC,
    type Comp,
    type Layer,
    type Property,
    type Keyframe,
  } from "../model";
  import CurveEditor from "./CurveEditor.svelte";

  const comp = $derived(activeComp());
  let rulerEl: HTMLElement | null = $state(null);
  let scrollContainer: HTMLElement | null = $state(null);

  // Zoom & Pan state
  let zoom = $state(1.0); // 1.0x to 15.0x zoom
  let scrubbing = $state(false);
  let isPanning = $state(false);
  let panStartX = 0;
  let panStartScroll = 0;

  // Keyframe Dragging state
  let draggingKey = $state<{
    layerId: number;
    prop: Property;
    origTime: number;
    currentTime: number;
  } | null>(null);

  // In-place Curve Editor and expanded layer tracks state
  let expandedLayers = $state<Record<number, boolean>>({});
  let openCurveEditor = $state<{ layerId: number; prop: Property } | null>(null);

  function snapTime(t: number): number {
    if (!comp) return t;
    return Math.min(comp.duration, Math.max(0, snapToFrame(t, comp.fps)));
  }

  function timeAt(e: PointerEvent): number {
    if (!rulerEl || !comp) return 0;
    const rect = rulerEl.getBoundingClientRect();
    if (rect.width === 0) return 0;
    const frac = (e.clientX - rect.left) / rect.width;
    return Math.min(comp.duration, Math.max(0, Math.round(frac * comp.duration)));
  }

  function onRulerDown(e: PointerEvent) {
    if (e.button !== 0) return;
    scrubbing = true;
    rulerEl?.setPointerCapture(e.pointerId);
    scrub(timeAt(e));
  }

  function onRulerMove(e: PointerEvent) {
    if (scrubbing) {
      scrub(timeAt(e));
    }
  }

  function onRulerUp(e?: PointerEvent) {
    if (scrubbing && comp) {
      scrub(snapTime(editor.currentTime));
    }
    scrubbing = false;
  }

  function onKeyframeDown(e: PointerEvent, layerId: number, prop: Property, key: Keyframe) {
    if (e.button !== 0) return;
    e.stopPropagation();
    editor.selected = layerId;
    draggingKey = {
      layerId,
      prop,
      origTime: key.time,
      currentTime: key.time,
    };
    (e.currentTarget as Element)?.setPointerCapture(e.pointerId);
  }

  function onKeyframeMove(e: PointerEvent) {
    if (!draggingKey || !comp) return;
    const rawTime = timeAt(e);
    const snapped = snapTime(rawTime);
    draggingKey.currentTime = snapped;
    scrub(snapped);
  }

  async function onKeyframeUp() {
    if (!draggingKey || !comp) {
      draggingKey = null;
      return;
    }
    const { layerId, prop, origTime, currentTime } = draggingKey;
    draggingKey = null;

    if (currentTime !== origTime) {
      await applyOp({
        type: "moveKeyframe",
        comp: comp.id,
        layer: layerId,
        property: prop,
        from: origTime,
        to: currentTime,
      });
    }
  }

  function pct(t: number): string {
    if (!comp || comp.duration <= 0) return "0%";
    return `${(t / comp.duration) * 100}%`;
  }

  function kindLabel(kind: import("../model").LayerKind): string {
    if ("Solid" in kind) return "▦";
    if ("Shape" in kind) return "◆";
    if ("Text" in kind) return "T";
    if ("Footage" in kind) return "▶";
    return "◫";
  }

  function toggleCurveEditor(layerId: number, prop: Property) {
    if (openCurveEditor?.layerId === layerId && openCurveEditor?.prop === prop) {
      openCurveEditor = null;
    } else {
      openCurveEditor = { layerId, prop };
    }
  }

  function toggleLayerExpand(layerId: number) {
    expandedLayers[layerId] = !expandedLayers[layerId];
  }

  function stepFrame(delta: number) {
    if (!comp) return;
    const tpf = ticksPerFrame(comp.fps);
    if (tpf <= 0) return;
    const currentFrame = Math.round(editor.currentTime / tpf);
    const totalFrames = Math.round(comp.duration / tpf);
    const targetFrame = Math.max(0, Math.min(totalFrames, currentFrame + delta));
    scrub(targetFrame * tpf);
  }

  function onTimelineWheel(e: WheelEvent) {
    if (e.ctrlKey) {
      e.preventDefault();
      const factor = e.deltaY < 0 ? 1.2 : 0.8;
      zoom = Math.min(15.0, Math.max(1.0, Math.round(zoom * factor * 10) / 10));
    }
  }

  function onPanDown(e: PointerEvent) {
    if (e.button === 1 || (e.altKey && e.button === 0)) {
      e.preventDefault();
      isPanning = true;
      panStartX = e.clientX;
      panStartScroll = scrollContainer?.scrollLeft ?? 0;
      (e.currentTarget as Element)?.setPointerCapture(e.pointerId);
    }
  }

  function onPanMove(e: PointerEvent) {
    if (isPanning && scrollContainer) {
      const dx = e.clientX - panStartX;
      scrollContainer.scrollLeft = panStartScroll - dx;
    }
  }

  function onPanUp() {
    isPanning = false;
  }

  // Generate ruler tick marks depending on zoom level
  const rulerTicks = $derived.by(() => {
    if (!comp || comp.duration <= 0) return [];
    const ticks: { time: number; label: string; isMajor: boolean }[] = [];
    const tpf = ticksPerFrame(comp.fps);
    const totalDuration = comp.duration;
    const totalSecs = Math.ceil(totalDuration / TICKS_PER_SEC);
    const totalFrames = tpf > 0 ? Math.round(totalDuration / tpf) : 0;

    if (zoom < 2.5) {
      // 1-second major intervals
      for (let s = 0; s <= totalSecs; s++) {
        const t = s * TICKS_PER_SEC;
        if (t <= totalDuration) {
          ticks.push({ time: t, label: `${s}s`, isMajor: true });
        }
      }
    } else if (zoom < 6) {
      // Half-second / 15-frame intervals
      const stepTicks = Math.round(TICKS_PER_SEC / 2);
      for (let t = 0; t <= totalDuration; t += stepTicks) {
        const isSec = t % TICKS_PER_SEC === 0;
        const s = Math.floor(t / TICKS_PER_SEC);
        const f = tpf > 0 ? Math.round((t % TICKS_PER_SEC) / tpf) : 0;
        ticks.push({
          time: t,
          label: isSec ? `${s}s` : `:${String(f).padStart(2, "0")}`,
          isMajor: isSec,
        });
      }
    } else {
      // Frame-by-frame / sub-frame intervals
      const frameStep = zoom >= 12 ? 1 : zoom >= 8 ? 2 : 5;
      for (let f = 0; f <= totalFrames; f += frameStep) {
        const t = f * tpf;
        if (t <= totalDuration) {
          const isSec = t % TICKS_PER_SEC === 0;
          const s = Math.floor(t / TICKS_PER_SEC);
          const framesPerSec = tpf > 0 ? Math.round(TICKS_PER_SEC / tpf) : 30;
          const frameInSec = f % framesPerSec;
          ticks.push({
            time: t,
            label: isSec ? `${s}s` : `${frameInSec}f`,
            isMajor: isSec,
          });
        }
      }
    }
    return ticks;
  });
</script>

<section
  aria-label="Timeline Tracks and Ruler"
  class="flex min-h-0 flex-col border-t border-[var(--border)] bg-[var(--bg-panel)] select-none"
  onpointermove={(e) => {
    onKeyframeMove(e);
    onPanMove(e);
  }}
  onpointerup={() => {
    void onKeyframeUp();
    onPanUp();
  }}
  onpointercancel={() => {
    void onKeyframeUp();
    onPanUp();
  }}
>
  <!-- Timeline Toolbar (Zoom, Frame Stepping, Playhead Timecode) -->
  <div
    class="flex h-8 items-center justify-between border-b border-[var(--border)] bg-[var(--bg-raised)] px-3 text-xs"
  >
    <div class="flex items-center gap-2">
      <span class="text-[11px] font-semibold uppercase tracking-widest text-[var(--text-dim)]">
        Timeline
      </span>
      {#if comp}
        <span
          class="rounded bg-[var(--bg-panel)] px-1.5 py-0.5 font-mono text-[10px] text-[var(--text-dim)]"
        >
          {formatFps(comp.fps)} · {timeToSecs(comp.duration).toFixed(1)}s ({timeToTimecode(
            editor.currentTime,
            comp.fps,
          )})
        </span>
      {/if}
    </div>

    <!-- Frame step buttons & Zoom controls -->
    <div class="flex items-center gap-3">
      <div class="flex items-center gap-1">
        <button
          class="rounded px-1.5 py-0.5 text-xs hover:bg-[var(--bg-panel)] text-[var(--text-dim)] hover:text-[var(--text)]"
          title="Previous Frame (Left Arrow)"
          onclick={() => stepFrame(-1)}
        >
          ◀
        </button>
        <button
          class="rounded px-1.5 py-0.5 text-xs hover:bg-[var(--bg-panel)] text-[var(--text-dim)] hover:text-[var(--text)]"
          title="Next Frame (Right Arrow)"
          onclick={() => stepFrame(1)}
        >
          ▶
        </button>
      </div>

      <!-- Zoom Controls -->
      <div class="flex items-center gap-1.5 border-l border-[var(--border)] pl-3">
        <span class="text-[10px] text-[var(--text-dim)]">Zoom</span>
        <button
          class="rounded px-1.5 py-0.5 font-mono text-xs hover:bg-[var(--bg-panel)] text-[var(--text-dim)] hover:text-[var(--text)]"
          title="Zoom Out"
          onclick={() => (zoom = Math.max(1.0, Math.round((zoom - 0.5) * 10) / 10))}
        >
          -
        </button>
        <input
          type="range"
          min="1"
          max="15"
          step="0.5"
          bind:value={zoom}
          class="h-1.5 w-16 accent-[var(--accent)] cursor-pointer"
          title="Zoom: {zoom}x (Ctrl+Wheel to zoom, Middle-click to pan)"
        />
        <button
          class="rounded px-1.5 py-0.5 font-mono text-xs hover:bg-[var(--bg-panel)] text-[var(--text-dim)] hover:text-[var(--text)]"
          title="Zoom In"
          onclick={() => (zoom = Math.min(15.0, Math.round((zoom + 0.5) * 10) / 10))}
        >
          +
        </button>
        {#if zoom > 1}
          <button
            class="rounded bg-[var(--bg-panel)] px-1.5 py-0.5 text-[10px] text-[var(--accent)] hover:underline"
            onclick={() => (zoom = 1.0)}
          >
            Fit
          </button>
        {/if}
      </div>
    </div>
  </div>

  <!-- Main Timeline Body (Layer List + Zoomable Track Area) -->
  <div class="flex min-h-0 flex-1 overflow-hidden">
    <!-- Left: Layer Header / Track Tree -->
    <div class="w-64 shrink-0 overflow-y-auto border-r border-[var(--border)] bg-[var(--bg-panel)]">
      {#if comp}
        {#each [...comp.layer_order].reverse() as layerId (layerId)}
          {@const layer = comp.layers[String(layerId)]}
          {@const hasTracks = Object.keys(layer.tracks).length > 0}
          {@const isExpanded = expandedLayers[layerId]}

          <div class="border-b border-[var(--border)]">
            <!-- Layer Main Row -->
            <div
              role="button"
              tabindex="0"
              class="flex h-10 w-full items-center justify-between px-2 text-xs transition-colors {editor.selected ===
              layerId
                ? 'bg-[var(--accent-soft)]'
                : 'hover:bg-[var(--bg-raised)]'}"
              onclick={() => (editor.selected = layerId)}
              onkeydown={(e) => e.key === "Enter" && (editor.selected = layerId)}
            >
              <div class="flex min-w-0 items-center gap-1.5">
                {#if hasTracks}
                  <button
                    class="rounded p-0.5 text-[10px] text-[var(--text-dim)] hover:text-[var(--text)]"
                    title={isExpanded ? "Collapse Tracks" : "Expand Tracks"}
                    onclick={(e) => {
                      e.stopPropagation();
                      toggleLayerExpand(layerId);
                    }}
                  >
                    {isExpanded ? "▼" : "▶"}
                  </button>
                {:else}
                  <span class="w-3"></span>
                {/if}
                <span class="text-[var(--text-dim)]">{kindLabel(layer.kind)}</span>
                <span class="truncate font-medium">{layer.name}</span>
              </div>

              <!-- Quick Add Keyframe / Curve Editor indicator -->
              <div class="flex items-center gap-1">
                {#if hasTracks}
                  <span class="rounded bg-[var(--bg-raised)] px-1 py-0.5 font-mono text-[9px] text-[var(--accent)]">
                    {Object.values(layer.tracks).reduce((acc, t) => acc + t.keys.length, 0)} keys
                  </span>
                {/if}
              </div>
            </div>

            <!-- Sub-tracks list when expanded -->
            {#if isExpanded && hasTracks}
              <div class="bg-[var(--bg-base)] pb-1 pl-6">
                {#each Object.entries(layer.tracks) as [prop, track] (prop)}
                  <div class="flex h-7 items-center justify-between pr-2 text-[11px] text-[var(--text-dim)]">
                    <span class="font-mono">{prop}</span>
                    <button
                      class="flex items-center gap-1 rounded px-1.5 py-0.5 text-[10px] transition-colors {openCurveEditor?.layerId ===
                        layerId && openCurveEditor?.prop === prop
                        ? 'bg-[var(--accent)] text-white'
                        : 'bg-[var(--bg-panel)] text-[var(--accent)] hover:bg-[var(--border)]'}"
                      title="Toggle in-place Bézier curve editor"
                      onclick={() => toggleCurveEditor(layerId, prop as Property)}
                    >
                      <span>📈</span>
                      <span>Curve</span>
                    </button>
                  </div>
                {/each}
              </div>
            {/if}
          </div>
        {/each}
      {/if}
    </div>

    <!-- Right: Zoomable Timeline Ruler & Tracks Area -->
    <div
      bind:this={scrollContainer}
      role="group"
      aria-label="Timeline track scroll area"
      class="relative min-w-0 flex-1 overflow-x-auto overflow-y-auto bg-[var(--bg-base)] {isPanning
        ? 'cursor-grab'
        : ''}"
      onwheel={onTimelineWheel}
      onpointerdown={onPanDown}
    >
      <div
        class="relative min-h-full"
        style="width: {zoom * 100}%; min-width: 100%;"
      >
        {#if comp}
          <!-- Ruler -->
          <div
            bind:this={rulerEl}
            role="slider"
            tabindex="0"
            aria-label="Timeline Ruler"
            aria-valuemin={0}
            aria-valuemax={comp.duration}
            aria-valuenow={editor.currentTime}
            class="sticky top-0 z-20 h-7 w-full cursor-ew-resize border-b border-[var(--border)] bg-[var(--bg-panel)] shadow-sm select-none"
            onpointerdown={onRulerDown}
            onpointermove={onRulerMove}
            onpointerup={onRulerUp}
          >
            {#each rulerTicks as tick (tick.time)}
              <div
                class="absolute top-0 bottom-0 flex flex-col justify-between {tick.isMajor
                  ? 'border-l border-[var(--border)]'
                  : 'border-l border-[rgba(255,255,255,0.05)]'}"
                style="left: {pct(tick.time)}"
              >
                <span
                  class="pl-1 font-mono text-[9px] {tick.isMajor
                    ? 'text-[var(--text-dim)] font-semibold'
                    : 'text-[rgba(255,255,255,0.25)]'}"
                >
                  {tick.label}
                </span>
                <div class="h-1.5 w-px bg-[var(--border)]"></div>
              </div>
            {/each}
          </div>

          <!-- Layer Track Rows with Bars & Keyframe Diamonds -->
          <div class="relative">
            {#each [...comp.layer_order].reverse() as layerId (layerId)}
              {@const layer = comp.layers[String(layerId)]}
              {@const hasTracks = Object.keys(layer.tracks).length > 0}
              {@const isExpanded = expandedLayers[layerId]}

              <!-- Main Layer Row Bar -->
              <div
                class="relative flex h-10 items-center border-b border-[var(--border)] {editor.selected ===
                layerId
                  ? 'bg-[var(--accent-soft)]'
                  : ''}"
              >
                <!-- Layer duration bar -->
                <div
                  class="absolute h-6 rounded-sm bg-[var(--bg-raised)] ring-1 ring-[var(--border)] opacity-80"
                  style="left: {pct(layer.start)}; width: {pct(layer.duration)};"
                ></div>

                <!-- Main layer row keyframe diamonds -->
                {#each Object.entries(layer.tracks) as [prop, track] (prop)}
                  {#each track.keys as key (key.time)}
                    {@const isDragged =
                      draggingKey &&
                      draggingKey.layerId === layerId &&
                      draggingKey.prop === prop &&
                      draggingKey.origTime === key.time}
                    {@const displayTime = isDragged ? draggingKey.currentTime : key.time}

                    <div
                      role="button"
                      tabindex="0"
                      aria-label="{prop} Keyframe at {timeToTimecode(displayTime, comp.fps)}"
                      class="absolute z-10 h-3.5 w-3.5 -translate-x-1/2 rotate-45 border border-[var(--bg-base)] {isDragged
                        ? 'bg-[var(--danger)] ring-2 ring-white'
                        : 'bg-[var(--accent)] hover:brightness-125'} cursor-ew-resize"
                      style="left: {pct(displayTime)}; top: calc(50% - 7px);"
                      title="{prop} @ {timeToTimecode(
                        displayTime,
                        comp.fps,
                      )} — Drag to move (snaps to frames)"
                      onpointerdown={(e) => onKeyframeDown(e, layerId, prop as Property, key)}
                      onclick={(e) => {
                        e.stopPropagation();
                        editor.selected = layerId;
                        scrub(key.time);
                      }}
                      onkeydown={(e) => e.key === "Enter" && scrub(key.time)}
                    ></div>
                  {/each}
                {/each}
              </div>

              <!-- Expanded Sub-track Rows -->
              {#if isExpanded && hasTracks}
                {#each Object.entries(layer.tracks) as [prop, track] (prop)}
                  <div
                    class="relative flex h-7 items-center border-b border-[var(--border)] bg-[rgba(0,0,0,0.2)]"
                  >
                    {#each track.keys as key (key.time)}
                      {@const isDragged =
                        draggingKey &&
                        draggingKey.layerId === layerId &&
                        draggingKey.prop === prop &&
                        draggingKey.origTime === key.time}
                      {@const displayTime = isDragged ? draggingKey.currentTime : key.time}

                      <div
                        role="button"
                        tabindex="0"
                        aria-label="{prop} Keyframe at {timeToTimecode(displayTime, comp.fps)}"
                        class="absolute z-10 h-3 w-3 -translate-x-1/2 rotate-45 border border-[var(--bg-base)] {isDragged
                          ? 'bg-[var(--danger)] ring-2 ring-white'
                          : 'bg-[var(--accent)] hover:brightness-125'} cursor-ew-resize"
                        style="left: {pct(displayTime)}; top: calc(50% - 6px);"
                        title="{prop} @ {timeToTimecode(displayTime, comp.fps)}"
                        onpointerdown={(e) => onKeyframeDown(e, layerId, prop as Property, key)}
                        onclick={(e) => {
                          e.stopPropagation();
                          editor.selected = layerId;
                          scrub(key.time);
                        }}
                        onkeydown={(e) => e.key === "Enter" && scrub(key.time)}
                      ></div>
                    {/each}
                  </div>

                  <!-- In-Place Curve Editor View Embedded Below Sub-track -->
                  {#if openCurveEditor?.layerId === layerId && openCurveEditor?.prop === prop}
                    <div class="border-b border-[var(--border)] bg-[var(--bg-panel)] p-3 shadow-inner">
                      <CurveEditor
                        {comp}
                        {layer}
                        property={prop as Property}
                        onClose={() => (openCurveEditor = null)}
                      />
                    </div>
                  {/if}
                {/each}
              {/if}
            {/each}
          </div>

          <!-- Playhead Needle & Scrubber Line -->
          <div
            class="pointer-events-none absolute bottom-0 top-0 z-30 w-px bg-[var(--danger)] shadow-[0_0_8px_rgba(255,107,107,0.5)]"
            style="left: {pct(editor.currentTime)};"
          >
            <!-- Playhead Thumb Diamond -->
            <div
              class="absolute -left-1.5 top-0 h-3 w-3 rotate-45 bg-[var(--danger)] shadow-md"
            ></div>
          </div>
        {/if}
      </div>
    </div>
  </div>
</section>
