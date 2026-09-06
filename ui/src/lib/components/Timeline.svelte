<script lang="ts">
  import { onMount, onDestroy, untrack } from "svelte";
  import Icon from "./Icon.svelte";
  import CurveEditor from "./CurveEditor.svelte";
  import AudioTimeline from "./AudioTimeline.svelte";
  import {
    editor,
    activeComp,
    selectedLayer,
    scrub,
    play,
    pause,
    applyOp,
    duplicateSelected,
    deleteSelected,
    notify,
    clone,
    toggleLayerVisibility,
    toggleLayerLock,
    layerVisibility,
    layerLocked,
  } from "../store.svelte";
  import { layerIcon, layerColor } from "../geometry";
  import {
    timeToTimecode,
    timecodeToTime,
    timeToSecs,
    snapToFrame,
    ticksPerFrame,
    TICKS_PER_SEC,
    type Layer,
    type Property,
    type Keyframe,
    type Track,
    type Op,
    type Comp,
  } from "../model";
  const comp = $derived(activeComp());
  const selected = $derived(selectedLayer());
  const LABEL_WIDTH = 268;
  let scroll = $state<HTMLDivElement | null>(null),
    ruler = $state<HTMLDivElement | null>(null),
    bodyWidth = $state(1100),
    zoom = $state(1);
  let expanded = $state<Record<number, boolean>>({});
  let filter = $state("");
  let showFilter = $state(false);
  let scrubbing = false;
  const MAX_TIME = 24 * 60 * 60 * TICKS_PER_SEC;
  function contentEnd(c: Comp): number {
    let end = c.duration;
    for (const l of Object.values(c.layers)) {
      end = Math.max(end, l.start + l.duration);
      for (const track of Object.values(l.tracks))
        end = Math.max(end, track?.keys.at(-1)?.time ?? 0);
    }
    return Math.min(MAX_TIME, end);
  }

  let fitDuration = $state(30 * TICKS_PER_SEC),
    viewDuration = $state(30 * TICKS_PER_SEC),
    viewKey = "";
  $effect(() => {
    const c = comp,
      key = `${editor.documentEpoch}:${c?.id}:${editor.timelineMode}`;
    if (!c) return;
    untrack(() => {
      if (key !== viewKey) {
        viewKey = key;
        zoom = 1;
        fitDuration = contentEnd(c);
        viewDuration = fitDuration;
        if (scroll) scroll.scrollLeft = 0;
      } else if (contentEnd(c) > viewDuration) viewDuration = contentEnd(c);
    });
  });
  const trackWidth = $derived(
    (Math.max(320, bodyWidth - LABEL_WIDTH) * zoom * viewDuration) / Math.max(1, fitDuration),
  );
  function durationOp(c: Comp, end: number): Op | null {
    if (end <= c.duration) return null;
    return {
      type: "setCompProps",
      comp: c.id,
      name: c.name,
      width: c.width,
      height: c.height,
      fps: c.fps,
      background: c.background,
      duration: Math.min(MAX_TIME, Math.ceil(end / TICKS_PER_SEC) * TICKS_PER_SEC),
    };
  }
  function withDuration(op: Op, end: number): Op {
    const extra = comp ? durationOp(comp, end) : null;
    return extra
      ? { type: "batch", label: "Extended composition and edited timeline", ops: [extra, op] }
      : op;
  }
  async function seekTimeline(time: number) {
    const c = comp;
    if (!c) return;
    pause();
    const extension = durationOp(c, time + ticksPerFrame(c.fps));
    if (extension && !(await applyOp(extension))) return;
    scrub(time);
  }
  async function setDuration(seconds: number) {
    const c = comp;
    if (!c || !Number.isFinite(seconds) || seconds <= 0 || seconds > 86400) return;
    const duration = Math.max(ticksPerFrame(c.fps), Math.round(seconds * TICKS_PER_SEC));
    if (
      await applyOp({
        type: "setCompProps",
        comp: c.id,
        name: c.name,
        width: c.width,
        height: c.height,
        fps: c.fps,
        background: c.background,
        duration,
      })
    ) {
      fitDuration = duration;
      viewDuration = duration;
      zoom = 1;
      if (scroll) scroll.scrollLeft = 0;
    }
  }

  const layers = $derived(
    comp
      ? [...comp.layer_order]
          .reverse()
          .map((id) => comp.layers[String(id)])
          .filter((l) => l.name.toLowerCase().includes(filter.toLowerCase()))
      : [],
  );
  const ticks = $derived.by(() => {
    if (!comp) return [];
    const sec = timeToSecs(viewDuration),
      desired = sec / Math.max(2, Math.floor(trackWidth / 80));
    const step =
      [0.1, 0.25, 0.5, 1, 2, 5, 10, 30, 60, 120, 300, 600, 1800, 3600, 7200].find(
        (s) => s >= desired,
      ) ?? 14400;
    return Array.from({ length: Math.floor(sec / step) + 1 }, (_, i) => ({
      time: Math.round(i * step * TICKS_PER_SEC),
      label: step < 1 ? `${(i * step).toFixed(2)}s` : `${i * step}s`,
    }));
  });
  const percent = (time: number) => (time / viewDuration) * 100;
  function timeAt(e: PointerEvent): number {
    if (!ruler || !comp) return 0;
    const rect = ruler.getBoundingClientRect();
    return snapToFrame(((e.clientX - rect.left) / rect.width) * viewDuration, comp.fps);
  }
  function rulerDown(e: PointerEvent) {
    if (e.button !== 0) return;
    pause();
    editor.timelineGesture = true;
    scrubbing = true;
    ruler?.setPointerCapture(e.pointerId);
    scrub(timeAt(e));
  }
  $effect(() => {
    const node = scroll;
    if (!node) return;
    const ro = new ResizeObserver((entries) => (bodyWidth = entries[0].contentRect.width));
    ro.observe(node);
    return () => ro.disconnect();
  });
  type RowTrack = {
    label: string;
    track: Track;
    prop?: Property;
    effectId?: string;
    paramId?: string;
  };
  function tracks(layer: Layer): RowTrack[] {
    const result: RowTrack[] = Object.entries(layer.tracks)
      .filter(([, v]) => v?.keys.length)
      .map(([prop, track]) => ({ label: prop, track: track!, prop: prop as Property }));
    for (const effect of layer.effects) {
      const manifest = editor.effects.find((e) => e.id === effect.effect_id);
      for (const [id, track] of Object.entries(effect.tracks))
        if (track.keys.length)
          result.push({
            label: `${manifest?.name ?? "Effect"} · ${manifest?.params.find((p) => p.id === id)?.name ?? id}`,
            track,
            effectId: effect.id,
            paramId: id,
          });
    }
    return result;
  }
  let dragging = $state<null | {
    kind: "move" | "in" | "out" | "key";
    layer: number;
    start: number;
    duration: number;
    mouse: number;
    nextStart: number;
    nextDuration: number;
    keyTime?: number;
    nextKey?: number;
    row?: RowTrack;
  }>(null);
  function begin(
    e: PointerEvent,
    layer: Layer,
    kind: "move" | "in" | "out" | "key",
    row?: RowTrack,
    key?: Keyframe,
  ) {
    if (e.button !== 0) return;
    e.stopPropagation();
    editor.selected = layer.id;
    if (layer.locked) return;
    pause();
    editor.timelineGesture = true;
    if (key) scrub(key.time);
    e.preventDefault();
    dragging = {
      kind,
      layer: layer.id,
      start: layer.start,
      duration: layer.duration,
      mouse: timeAt(e),
      nextStart: layer.start,
      nextDuration: layer.duration,
      keyTime: key?.time,
      nextKey: key?.time,
      row,
    };
    window.addEventListener("pointermove", dragMove);
    window.addEventListener("pointerup", dragEnd);
    window.addEventListener("pointercancel", dragCancel);
  }
  let dragRaf = 0,
    pendingPointer: PointerEvent | null = null;
  function dragMove(e: PointerEvent) {
    pendingPointer = e;
    if (!dragRaf)
      dragRaf = requestAnimationFrame(() => {
        dragRaf = 0;
        const event = pendingPointer;
        pendingPointer = null;
        if (event) applyDrag(event);
      });
  }
  function applyDrag(e: PointerEvent) {
    if (!dragging || !comp) return;
    const dt = timeAt(e) - dragging.mouse,
      tpf = ticksPerFrame(comp.fps);
    if (dragging.kind === "move")
      dragging.nextStart = Math.max(0, Math.min(MAX_TIME - dragging.duration, dragging.start + dt));
    else if (dragging.kind === "in") {
      dragging.nextStart = Math.max(
        0,
        Math.min(dragging.start + dragging.duration - tpf, dragging.start + dt),
      );
      dragging.nextDuration = dragging.start + dragging.duration - dragging.nextStart;
    } else if (dragging.kind === "out")
      dragging.nextDuration = Math.max(
        tpf,
        Math.min(MAX_TIME - dragging.start, dragging.duration + dt),
      );
    else {
      dragging.nextKey = Math.max(0, Math.min(MAX_TIME - tpf, (dragging.keyTime ?? 0) + dt));
      scrub(dragging.nextKey);
    }
    const end =
      dragging.kind === "key"
        ? (dragging.nextKey ?? 0) + tpf
        : dragging.nextStart + dragging.nextDuration;
    if (end > viewDuration)
      viewDuration = Math.min(MAX_TIME, Math.ceil(end / (5 * TICKS_PER_SEC)) * 5 * TICKS_PER_SEC);
    if (scroll) {
      const bounds = scroll.getBoundingClientRect(),
        old = scroll.scrollLeft;
      if (e.clientX > bounds.right - 24) scroll.scrollLeft += 14;
      else if (e.clientX < bounds.left + LABEL_WIDTH + 24) scroll.scrollLeft -= 14;
      if (scroll.scrollLeft !== old) dragMove(e);
    }
  }
  function cleanup() {
    if (dragRaf) cancelAnimationFrame(dragRaf);
    dragRaf = 0;
    pendingPointer = null;
    editor.timelineGesture = false;
    window.removeEventListener("pointermove", dragMove);
    window.removeEventListener("pointerup", dragEnd);
    window.removeEventListener("pointercancel", dragCancel);
  }
  function dragCancel() {
    cleanup();
    dragging = null;
  }
  async function dragEnd() {
    if (pendingPointer) applyDrag(pendingPointer);
    const d = dragging,
      c = comp;
    cleanup();
    if (!d || !c) {
      dragging = null;
      return;
    }
    try {
      if (
        d.kind === "key" &&
        d.row &&
        d.keyTime !== undefined &&
        d.nextKey !== undefined &&
        d.nextKey !== d.keyTime
      ) {
        const row = d.row;
        if (row.track.keys.some((k) => k.time === d.nextKey && k.time !== d.keyTime)) {
          notify("There is already a keyframe at this time.", true);
          return;
        }
        if (row.prop)
          await applyOp(
            withDuration(
              {
                type: "moveKeyframe",
                comp: c.id,
                layer: d.layer,
                property: row.prop,
                from: d.keyTime,
                to: d.nextKey,
              },
              d.nextKey + ticksPerFrame(c.fps),
            ),
          );
        else
          await applyOp((project) => {
            const layer = project.comps[String(c.id)]?.layers[String(d.layer)];
            if (!layer) return null;
            const effects = clone(layer.effects);
            const track = effects.find((e) => e.id === row.effectId)?.tracks[row.paramId!];
            const key = track?.keys.find((k) => k.time === d.keyTime);
            if (!track || !key) return null;
            key.time = d.nextKey!;
            track.keys.sort((a, b) => a.time - b.time);
            return withDuration(
              { type: "setLayerEffects", comp: c.id, layer: d.layer, effects },
              d.nextKey! + ticksPerFrame(c.fps),
            );
          });
      } else if (d.kind === "move" && d.nextStart !== d.start)
        await applyOp(
          withDuration(
            { type: "shiftLayer", comp: c.id, layer: d.layer, delta: d.nextStart - d.start },
            d.nextStart + d.duration,
          ),
        );
      else if (d.kind !== "key" && (d.nextStart !== d.start || d.nextDuration !== d.duration))
        await applyOp(
          withDuration(
            {
              type: "setLayerTime",
              comp: c.id,
              layer: d.layer,
              start: d.nextStart,
              duration: d.nextDuration,
            },
            d.nextStart + d.nextDuration,
          ),
        );
    } finally {
      if (dragging === d) dragging = null;
    }
  }
  onDestroy(cleanup);
  function reorder(delta: number) {
    if (!comp || !selected || selected.locked) return;
    const index = comp.layer_order.indexOf(selected.id);
    void applyOp({
      type: "reorderLayer",
      comp: comp.id,
      layer: selected.id,
      newIndex: Math.max(0, Math.min(comp.layer_order.length - 1, index + delta)),
    });
  }
  function openGraph(prop?: Property) {
    editor.graphProperty =
      prop ?? (Object.keys(selected?.tracks ?? {})[0] as Property) ?? "Position";
  }
</script>

<section class="panel timeline" aria-label="Timeline">
  <div class="timeline-header">
    <button
      class="timeline-tab"
      class:active={!editor.graphProperty && editor.timelineMode === "layers"}
      onclick={() => {
        editor.graphProperty = null;
        editor.timelineMode = "layers";
      }}><Icon name="layers" size={13} />Timeline</button
    >
    <button
      class="timeline-tab"
      class:active={!!editor.graphProperty}
      disabled={!selected}
      onclick={() => {
        editor.timelineMode = "layers";
        editor.graphProperty ? (editor.graphProperty = null) : openGraph();
      }}><Icon name="graph" size={13} />Graph editor</button
    >
    {#if editor.audioProtocol}<button
        class="timeline-tab"
        class:active={editor.timelineMode === "audio"}
        aria-label="Audio timeline tab"
        onclick={() => {
          editor.graphProperty = null;
          editor.timelineMode = "audio";
          editor.workspace = "Audio";
          editor.selected = null;
          editor.sidebar = "audio";
        }}><Icon name="wave" size={13} />Audio</button
      >{/if}
    <span class="divider"></span><span class="timeline-comp truncate"
      >{comp?.name ?? "No composition"}</span
    >
    <span class="spacer"></span>
    <div class="transport">
      <button
        class="icon-button small"
        aria-label="Step backward"
        title="Previous frame (←)"
        onclick={() => comp && scrub(editor.currentTime - ticksPerFrame(comp.fps))}
        ><Icon name="back" size={13} /></button
      ><button
        class="play-control"
        aria-label={editor.audioStarting
          ? "Cancel audio start"
          : editor.playing
            ? "Pause playback"
            : "Play"}
        title="Play / pause (Space)"
        onclick={() => (editor.playing || editor.audioStarting ? pause() : void play())}
        ><Icon name={editor.playing ? "pause" : "play"} size={13} /></button
      ><button
        class="icon-button small"
        aria-label="Step forward"
        title="Next frame (→)"
        onclick={() => comp && scrub(editor.currentTime + ticksPerFrame(comp.fps))}
        ><Icon name="forward" size={13} /></button
      >
    </div>
    <input
      class="timecode mono"
      aria-label="Playhead timecode"
      value={comp ? timeToTimecode(editor.currentTime, comp.fps) : "00:00:00:00"}
      onchange={(e) => {
        if (comp) {
          const time = timecodeToTime(e.currentTarget.value, comp.fps);
          if (time !== null) {
            pause();
            void seekTimeline(time);
          } else notify("Use a timecode such as 00:00:01:15.", true);
        }
      }}
    />
    {#if comp}<label class="duration-control"
        >Length <input
          aria-label="Timeline duration seconds"
          type="number"
          min=".1"
          max="86400"
          step="1"
          value={timeToSecs(comp.duration)}
          onchange={(e) => void setDuration(Number(e.currentTarget.value))}
        /><span>s</span></label
      ><button
        class="extend-duration"
        aria-label="Extend composition by 10 seconds"
        title="Add time to the composition"
        onclick={() => void setDuration(Math.min(86400, timeToSecs(comp.duration) + 10))}
        >+10s</button
      >{/if}
    <span class="divider"></span><Icon name="search" size={12} class="dim" /><input
      class="timeline-zoom"
      type="range"
      aria-label="Timeline zoom"
      min="1"
      max={editor.timelineMode === "audio" ? 128 : 8}
      step=".25"
      bind:value={zoom}
    /><button
      class="icon-button small"
      title="Fit the entire composition in the timeline"
      aria-label="Reset timeline zoom"
      onclick={() => {
        zoom = 1;
        if (comp) {
          fitDuration = contentEnd(comp);
          viewDuration = fitDuration;
        }
        if (scroll) scroll.scrollLeft = 0;
      }}><Icon name="maximize" size={12} /></button
    >
  </div>
  {#if editor.timelineMode === "audio" && editor.audioProtocol}
    <AudioTimeline {zoom} />
  {:else if editor.graphProperty && comp && selected}
    <div class="graph-panel">
      <div class="graph-properties">
        <span class="upper dim">ANIMATED PROPERTY</span><strong>{selected.name}</strong
        >{#each ["Position", "Scale", "Rotation", "Opacity", "AnchorPoint"] as prop}<button
            class:active={editor.graphProperty === prop}
            onclick={() => (editor.graphProperty = prop as Property)}
            ><Icon name="keyframe" size={10} />{prop}<span class="spacer"></span><span class="count"
              >{selected.tracks[prop as Property]?.keys.length ?? 0}</span
            ></button
          >{/each}
      </div>
      <div class="curve-container">
        <CurveEditor
          {comp}
          layer={selected}
          property={editor.graphProperty}
          onClose={() => (editor.graphProperty = null)}
        />
      </div>
    </div>
  {:else}
    <div class="timeline-scroll" bind:this={scroll}>
      {#if comp}
        <div
          class="timeline-grid"
          style={`grid-template-columns:${LABEL_WIDTH}px ${trackWidth}px;width:${LABEL_WIDTH + trackWidth}px`}
        >
          <div class="name-heading label-cell">
            <button
              class="icon-button small"
              title="Filter layers"
              aria-label="Filter layers"
              onclick={() => (showFilter = !showFilter)}><Icon name="search" size={11} /></button
            >{#if showFilter}<input
                aria-label="Filter timeline layers"
                bind:value={filter}
                placeholder="Filter layers…"
              />{:else}<span>LAYER NAME</span><span class="count">{comp.layer_order.length}</span
              >{/if}<span class="spacer"></span><span class="mono"
              >{Math.floor(comp.duration / ticksPerFrame(comp.fps))}f</span
            >
          </div>
          <div
            class="ruler"
            bind:this={ruler}
            role="slider"
            tabindex="0"
            aria-label="Timeline ruler"
            aria-valuemin={0}
            aria-valuemax={viewDuration}
            aria-valuenow={editor.currentTime}
            onkeydown={(e) => {
              if (e.key === "ArrowRight") scrub(editor.currentTime + ticksPerFrame(comp.fps));
              if (e.key === "ArrowLeft") scrub(editor.currentTime - ticksPerFrame(comp.fps));
              e.stopPropagation();
            }}
            onpointerdown={rulerDown}
            onpointermove={(e) => {
              if (scrubbing) scrub(timeAt(e));
            }}
            onpointerup={() => {
              scrubbing = false;
              editor.timelineGesture = false;
            }}
            onpointercancel={() => {
              scrubbing = false;
              editor.timelineGesture = false;
            }}
          >
            <span
              class="comp-out"
              style={`left:${percent(comp.duration)}%`}
              title="Composition output end">OUT</span
            >
            {#each ticks as tick}<span class="tick" style={`left:${percent(tick.time)}%`}
                ><span>{tick.label}</span></span
              >{/each}
            <div class="playhead-cap" style={`left:${percent(editor.currentTime)}%`}></div>
          </div>
          {#each layers as layer, index (layer.id)}
            {@const animated = tracks(layer)}
            {@const draggingLayer = dragging?.layer === layer.id && dragging.kind !== "key"}
            {@const start = draggingLayer ? dragging!.nextStart : layer.start}
            {@const duration = draggingLayer ? dragging!.nextDuration : layer.duration}
            <div
              class="layer-label label-cell"
              class:selected={editor.selected === layer.id}
              class:invisible={!layerVisibility(comp.id, layer)}
            >
              <button
                class="tiny-button"
                aria-label={`${layerVisibility(comp.id, layer) ? "Hide" : "Show"} ${layer.name}`}
                title="Toggle visibility"
                aria-pressed={layerVisibility(comp.id, layer)}
                onclick={() => void toggleLayerVisibility(comp.id, layer.id)}
                ><Icon
                  name={layerVisibility(comp.id, layer) ? "eye" : "eye-off"}
                  size={11}
                /></button
              >
              <button
                class="tiny-button lock-button"
                class:locked={layer.locked}
                aria-label={`${layer.locked ? "Unlock" : "Lock"} ${layer.name}`}
                title="Toggle lock"
                aria-pressed={layerLocked(comp.id, layer)}
                onclick={() => void toggleLayerLock(comp.id, layer.id)}
                ><Icon name={layerLocked(comp.id, layer) ? "lock" : "unlock"} size={9} /></button
              >
              <span class="layer-swatch" style={`background:${layerColor(layer)}`}></span><span
                class="layer-index mono">{String(index + 1).padStart(2, "0")}</span
              >
              <button
                class="tiny-button disclosure"
                disabled={!animated.length}
                title="Expand animated properties"
                aria-label={`Expand ${layer.name}`}
                onclick={() => (expanded[layer.id] = !expanded[layer.id])}
                ><Icon name={expanded[layer.id] ? "down" : "right"} size={9} /></button
              >
              <button
                class="layer-name"
                aria-label={`Select layer ${layer.name}`}
                onclick={() => (editor.selected = layer.id)}
                ><Icon name={layerIcon(layer)} size={11} /><span class="truncate">{layer.name}</span
                ></button
              >
              {#if layer.effects.length}<button
                  class="layer-fx"
                  title="Open effect controls"
                  aria-label={`Effects on ${layer.name}`}
                  onclick={() => {
                    editor.selected = layer.id;
                    editor.inspector = "effects";
                  }}>ƒx</button
                >{/if}
            </div>
            <div class="track-cell" class:selected={editor.selected === layer.id}>
              {#each ticks as tick}<span
                  class="track-gridline"
                  style={`left:${percent(tick.time)}%`}
                ></span>{/each}
              <div
                class="layer-bar"
                class:locked={layer.locked}
                class:hidden-layer={!layer.visible}
                style={`left:${percent(start)}%;width:${Math.max(0.1, percent(duration))}%;--layer-color:${layerColor(layer)};background:${layerColor(layer)}26;border-color:${layerColor(layer)}77`}
                role="button"
                tabindex="0"
                aria-label={`Move ${layer.name} and its keyframes`}
                onkeydown={(e) => {
                  if (e.key === "Enter") editor.selected = layer.id;
                }}
                onpointerdown={(e) => begin(e, layer, "move")}
              >
                <span
                  class="trim-handle left"
                  role="presentation"
                  title="Trim in point"
                  onpointerdown={(e) => begin(e, layer, "in")}
                ></span><span class="bar-label truncate">{layer.name}</span>
                {#if !expanded[layer.id]}{#each animated.flatMap((t) => t.track.keys) as key}<span
                      class="summary-key"
                      style={`left:${((key.time - start) / duration) * 100}%`}
                    ></span>{/each}{/if}
                <span
                  class="trim-handle right"
                  role="presentation"
                  title="Trim out point"
                  onpointerdown={(e) => begin(e, layer, "out")}
                ></span>
              </div>
            </div>
            {#if expanded[layer.id]}
              {#each animated as row (`${row.prop ?? row.effectId}-${row.paramId ?? ""}`)}
                <div class="property-label label-cell">
                  <Icon name="keyframe" size={9} /><span class="truncate">{row.label}</span><span
                    class="spacer"
                  ></span>{#if row.prop}<button
                      class="tiny-button"
                      title="Edit easing curve"
                      aria-label={`Graph ${row.prop} on ${layer.name}`}
                      onclick={() => {
                        editor.selected = layer.id;
                        openGraph(row.prop);
                      }}><Icon name="graph" size={11} /></button
                    >{/if}
                </div>
                <div class="property-track track-cell">
                  {#each ticks as tick}<span
                      class="track-gridline"
                      style={`left:${percent(tick.time)}%`}
                    ></span>{/each}{#each row.track.keys as key (key.time)}{@const dragged =
                      dragging?.kind === "key" &&
                      dragging.layer === layer.id &&
                      dragging.row?.prop === row.prop &&
                      dragging.row?.effectId === row.effectId &&
                      dragging.row?.paramId === row.paramId &&
                      dragging.keyTime === key.time}<button
                      class="timeline-key"
                      class:at-playhead={key.time === editor.currentTime}
                      style={`left:${percent(dragged ? dragging!.nextKey! : key.time)}%`}
                      aria-label={`${row.label} keyframe at ${timeToSecs(key.time).toFixed(2)} seconds`}
                      title="Drag to move · double-click to edit easing"
                      onpointerdown={(e) => begin(e, layer, "key", row, key)}
                      ondblclick={() => {
                        editor.selected = layer.id;
                        if (row.prop) openGraph(row.prop);
                        else editor.inspector = "effects";
                      }}><Icon name="keyframe" size={9} /></button
                    >{/each}
                </div>
              {/each}
            {/if}
          {/each}
          {#if !layers.length}<div class="no-layers label-cell">
              {filter ? "No matching layers" : "No layers yet"}
            </div>
            <div class="no-layers">Add text, shapes, or an image to start.</div>{/if}
          <div
            class="playhead-line"
            style={`left:${LABEL_WIDTH + (editor.currentTime / viewDuration) * trackWidth}px`}
          ></div>
        </div>
      {/if}
    </div>
  {/if}
  <div class="timeline-footer">
    <button
      class="icon-button small"
      title="Move selected layer up"
      aria-label="Move layer up"
      disabled={!selected || selected.locked}
      onclick={() => reorder(1)}><Icon name="up" size={12} /></button
    ><button
      class="icon-button small"
      title="Move selected layer down"
      aria-label="Move layer down"
      disabled={!selected || selected.locked}
      onclick={() => reorder(-1)}><Icon name="down" size={12} /></button
    ><span class="divider"></span><button
      class="icon-button small"
      title="Duplicate selected layer (Ctrl/⌘ D)"
      aria-label="Duplicate layer"
      disabled={!selected || selected.locked}
      onclick={() => void duplicateSelected()}><Icon name="copy" size={11} /></button
    ><button
      class="icon-button small"
      title="Delete selected layer"
      aria-label="Delete layer"
      disabled={!selected || selected.locked}
      onclick={() => void deleteSelected()}><Icon name="trash" size={11} /></button
    ><span class="spacer"></span><span class="timeline-hint"
      >Drag layers to move · drag edges to trim · ◇ animate</span
    ><span class="divider"></span><span class="mono ticks-note">120,000 ticks / sec</span>
  </div>
</section>

<style>
  .timeline {
    border-top: 1px solid #4e514c;
    background: #1d1e1c;
  }
  .timeline-header {
    height: 38px;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 13px;
    border-bottom: 1px solid #3a3d39;
    background: #262825;
    flex-shrink: 0;
  }
  .timeline-tab {
    display: flex;
    align-items: center;
    gap: 7px;
    height: 100%;
    padding: 0 8px;
    font-size: 10px;
    color: #8c8f88;
    border-bottom: 2px solid transparent;
  }
  .timeline-tab.active {
    color: #d2d5d0;
    border-bottom-color: #c4c8bf;
  }
  .timeline-comp {
    font-size: 9px;
    color: #868a83;
    max-width: 200px;
  }
  .transport {
    display: flex;
    align-items: center;
    gap: 5px;
  }
  .play-control {
    display: flex;
    justify-content: center;
    align-items: center;
    width: 24px;
    height: 23px;
    color: #daddd7;
    background: #3e413c;
    border: 1px solid #5a5e57;
    border-radius: 3px;
  }
  .timecode {
    font-size: 11px;
    letter-spacing: 0.6px;
    background: none;
    border: 0;
    color: #d0d5cc;
    width: 104px;
    text-align: center;
    outline: 0;
  }
  .timecode:focus {
    background: #3b3e39;
  }
  .extend-duration {
    font: 8px monospace;
    color: #b9ceab;
    border: 1px solid #465740;
    border-radius: 3px;
    padding: 3px 5px;
    margin-left: 4px;
  }
  .comp-out {
    position: absolute;
    top: 13px;
    color: #d1aa75;
    font: 7px monospace;
    border-left: 1px solid #ccac77;
    padding: 1px 3px;
    transform: translateX(-100%);
    pointer-events: none;
  }
  .duration-control {
    display: flex;
    align-items: center;
    gap: 4px;
    color: #919a8c;
    font-size: 8px;
    margin-left: 8px;
    flex-shrink: 0;
  }
  .duration-control input {
    width: 46px;
    background: #1b2119;
    border: 1px solid #444d3c;
    border-radius: 3px;
    color: #c7d4bb;
    padding: 3px 4px;
    font: 9px monospace;
  }
  .timeline-zoom {
    width: 90px;
    height: 2px !important;
  }
  .timeline-scroll {
    overflow: auto;
    min-height: 0;
    flex: 1;
    position: relative;
  }
  .timeline-grid {
    display: grid;
    grid-auto-rows: min-content;
    min-height: 100%;
    position: relative;
    background: #1a1b19;
  }
  .label-cell {
    position: sticky;
    left: 0;
    z-index: 3;
    border-right: 1px solid #454943;
    background: #272926;
  }
  .name-heading {
    top: 0;
    z-index: 12;
    display: flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 9px 0 4px;
    border-bottom: 1px solid #40433e;
    background: #232421;
    font-size: 7px;
    letter-spacing: 1px;
    color: #858981;
  }
  .name-heading input {
    background: none;
    border: 0;
    outline: 0;
    color: #bfc3bc;
    font-size: 9px;
    letter-spacing: 0;
    width: 145px;
  }
  .name-heading > .mono {
    font-size: 7px;
    color: #6e736a;
  }
  .name-heading .count {
    height: 12px;
    font-size: 7px;
    border-color: #464944;
  }
  .ruler {
    position: sticky;
    top: 0;
    height: 28px;
    z-index: 8;
    background: #222421;
    border-bottom: 1px solid #484c46;
    cursor: ew-resize;
    overflow: hidden;
    touch-action: none;
  }
  .tick {
    position: absolute;
    top: 0;
    bottom: 0;
    border-left: 1px solid #4f524c;
  }
  .tick > span {
    position: absolute;
    top: 5px;
    left: 6px;
    font: 8px monospace;
    color: #9da19a;
    white-space: nowrap;
  }
  .playhead-cap {
    position: absolute;
    top: 5px;
    width: 9px;
    height: 17px;
    margin-left: -4px;
    clip-path: polygon(0 0, 100% 0, 100% 65%, 50% 100%, 0 65%);
    background: #cbd2c7;
    z-index: 2;
  }
  .playhead-line {
    position: absolute;
    top: 28px;
    bottom: 0;
    width: 1px;
    background: #bec4b9;
    opacity: 0.7;
    z-index: 2;
    pointer-events: none;
  }
  .layer-label {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 0 8px 0 6px;
    height: 28px;
    border-bottom: 1px solid #343732;
  }
  .layer-label.selected {
    background: #464944;
  }
  .tiny-button {
    display: grid;
    place-items: center;
    flex-shrink: 0;
    height: 20px;
    width: 15px;
    color: #a1a49e;
  }
  .tiny-button:hover {
    color: #e6e8e4;
  }
  .lock-button {
    opacity: 0.3;
    width: 13px;
  }
  .lock-button.locked {
    opacity: 1;
    color: #c0c2bc;
  }
  .layer-swatch {
    height: 15px;
    width: 3px;
    flex-shrink: 0;
    border-radius: 1px;
  }
  .layer-index {
    font-size: 7px;
    color: #81857e;
    width: 12px;
  }
  .disclosure {
    width: 9px;
  }
  .disclosure:disabled {
    opacity: 0.18;
  }
  .layer-name {
    display: flex;
    gap: 7px;
    align-items: center;
    flex: 1;
    min-width: 0;
    text-align: left;
    font-size: 10px;
    color: #c1c4bf;
  }
  .layer-name :global(svg) {
    color: #b2b6af;
  }
  .layer-fx {
    font-family: Georgia, serif;
    font-style: italic;
    color: #acb0a8;
    font-size: 12px;
  }
  .layer-label.invisible {
    opacity: 0.55;
  }
  .track-cell {
    position: relative;
    height: 28px;
    border-bottom: 1px solid #2e312c;
    overflow: hidden;
    background: #1e201d;
  }
  .track-cell.selected {
    background: #30332e;
  }
  .track-gridline {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 1px;
    background: #3a3d38;
    opacity: 0.6;
    pointer-events: none;
  }
  .layer-bar {
    position: absolute;
    top: 5px;
    height: 19px;
    border: 1px solid;
    border-radius: 2px;
    cursor: grab;
    touch-action: none;
    color: var(--layer-color);
    display: flex;
    align-items: center;
    padding: 0 9px;
    overflow: hidden;
    min-width: 4px;
  }
  .layer-bar.locked {
    cursor: default;
    opacity: 0.6;
  }
  .layer-bar.hidden-layer {
    opacity: 0.23;
  }
  .bar-label {
    font-size: 8px;
    max-width: 130px;
    opacity: 0.75;
    pointer-events: none;
  }
  .trim-handle {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 5px;
    background: var(--layer-color);
    opacity: 0.25;
    cursor: ew-resize;
    z-index: 2;
    touch-action: none;
  }
  .trim-handle:hover {
    opacity: 1;
  }
  .trim-handle.left {
    left: 0;
  }
  .trim-handle.right {
    right: 0;
  }
  .summary-key {
    position: absolute;
    top: 6px;
    width: 4px;
    height: 4px;
    background: #d0d2c7;
    transform: translateX(-50%) rotate(45deg);
    opacity: 0.6;
    pointer-events: none;
  }
  .property-label {
    height: 23px;
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 0 10px 0 72px;
    color: #979c94;
    font-size: 8px;
    background: #212320;
    border-bottom: 1px solid #31342f;
  }
  .property-label > :global(svg) {
    color: #bcbdb4;
  }
  .property-track {
    height: 23px;
    background: #1c1e1b;
  }
  .timeline-key {
    position: absolute;
    top: 0;
    height: 23px;
    width: 15px;
    transform: translateX(-50%);
    display: grid;
    place-items: center;
    color: #bfc1b6;
    cursor: ew-resize;
    touch-action: none;
  }
  .timeline-key :global(svg) {
    fill: currentColor;
  }
  .timeline-key:hover,
  .timeline-key.at-playhead {
    color: #dedccf;
    filter: drop-shadow(0 0 3px #ced2c770);
  }
  .no-layers {
    padding: 18px;
    color: #868b83;
    font-size: 10px;
  }
  .timeline-footer {
    height: 27px;
    display: flex;
    align-items: center;
    padding: 0 10px;
    gap: 5px;
    background: #232522;
    border-top: 1px solid #3a3e38;
    flex-shrink: 0;
  }
  .timeline-footer .icon-button {
    color: #8a8e86;
    width: 23px;
    height: 22px;
  }
  .timeline-hint {
    font-size: 8px;
    color: #747970;
    letter-spacing: 0.15px;
  }
  .ticks-note {
    font-size: 7px;
    color: #868b82;
  }
  .graph-panel {
    display: flex;
    flex: 1;
    min-height: 0;
    overflow: auto;
    background: #1d1f1c;
  }
  .graph-properties {
    width: 220px;
    min-width: 220px;
    display: flex;
    flex-direction: column;
    padding: 15px 15px;
    border-right: 1px solid #3e423c;
  }
  .graph-properties > .upper {
    font-size: 7px;
  }
  .graph-properties strong {
    font-size: 11px;
    font-weight: 400;
    color: #c4c8c1;
    margin: 9px 0 12px;
  }
  .graph-properties button {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 6px 8px;
    font-size: 9px;
    color: #888c84;
    border-radius: 3px;
  }
  .graph-properties button.active {
    background: #3b3e39;
    color: #d5d9d2;
  }
  .curve-container {
    flex: 1;
    min-width: 400px;
    padding: 9px 18px;
  }
  @media (max-width: 1050px) {
    .timeline-comp {
      display: none;
    }
    .timeline-hint {
      display: none;
    }
    .timeline-header {
      gap: 4px;
    }
    .timeline-zoom {
      width: 65px;
    }
  }
</style>
