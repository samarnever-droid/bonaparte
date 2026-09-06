<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import Icon from "./Icon.svelte";
  import {
    editor,
    activeComp,
    selectedLayer,
    renderTo,
    clone,
    setProperty,
    addLayer,
    selectComp,
    scrub,
  } from "../store.svelte";
  import { layerGeometry, hitTest, worldMatrix, inverse, point, type Matrix } from "../geometry";
  import {
    evaluate,
    timeToTimecode,
    formatFps,
    type Layer,
    type Property,
    type PropValue,
  } from "../model";
  let canvas = $state<HTMLCanvasElement | null>(null),
    stage = $state<HTMLDivElement | null>(null);
  let size = $state({ width: 700, height: 430 });
  let zoom = $state("fit");
  let pan = $state({ x: 0, y: 0 });
  const comp = $derived(activeComp());
  const selected = $derived(selectedLayer());
  const previewComp = $derived(
    comp && editor.previewLayer
      ? {
          ...comp,
          layers: { ...comp.layers, [String(editor.previewLayer.id)]: editor.previewLayer },
        }
      : comp,
  );
  const layer = $derived(editor.previewLayer?.id === selected?.id ? editor.previewLayer : selected);
  const scale = $derived(
    comp
      ? zoom === "fit"
        ? Math.max(
            0.05,
            Math.min((size.width - 94) / comp.width, (size.height - 68) / comp.height, 1.5),
          )
        : Number(zoom) / 100
      : 1,
  );
  const geometry = $derived(
    previewComp && layer && editor.project && !("Adjustment" in layer.kind) && layer.visible
      ? layerGeometry(previewComp, layer, editor.project, editor.currentTime)
      : null,
  );
  const points = $derived(geometry?.corners.map((p) => p.join(",")).join(" ") ?? "");
  const motion = $derived.by(() => {
    if (!comp || !selected || !editor.project || editor.workspace !== "Animate") return [];
    return (selected.tracks.Position?.keys ?? []).map((key) => ({
      time: key.time,
      center: layerGeometry(comp, selected, editor.project!, key.time).center,
    }));
  });
  $effect(() => {
    void editor.project;
    void editor.activeComp;
    void editor.currentTime;
    void editor.renderSeq;
    void editor.previewLayer;
    void editor.bypassEffects;
    if (canvas) void renderTo(canvas);
  });
  onMount(() => {
    if (!stage) return;
    const observer = new ResizeObserver((entries) => {
      const { width, height } = entries[0].contentRect;
      size = { width, height };
    });
    observer.observe(stage);
    return () => observer.disconnect();
  });
  let gesture: null | {
    mode: "move" | "scale" | "rotate" | "pan";
    layer: Layer | null;
    start: [number, number];
    screen: [number, number];
    pan: { x: number; y: number };
    matrix: Matrix | null;
    corner: number;
    base: PropValue | null;
    value: PropValue | null;
    property: Property;
    center: [number, number];
    moved: boolean;
    parentInverse: Matrix | null;
  } = null;
  function location(event: PointerEvent): [number, number] {
    if (!canvas || !comp) return [0, 0];
    const bounds = canvas.getBoundingClientRect();
    return [
      ((event.clientX - bounds.left) * comp.width) / bounds.width,
      ((event.clientY - bounds.top) * comp.height) / bounds.height,
    ];
  }
  function begin(
    event: PointerEvent,
    mode: "move" | "scale" | "rotate" | "pan",
    target: Layer | null,
    corner = 0,
  ) {
    if (event.button !== 0 || !comp || !editor.project) return;
    if (target?.locked) return;
    event.preventDefault();
    event.stopPropagation();
    const g = target ? layerGeometry(comp, target, editor.project, editor.currentTime) : null;
    const property: Property =
      mode === "scale" ? "Scale" : mode === "rotate" ? "Rotation" : "Position";
    const parent =
      target?.parent !== null && target?.parent !== undefined
        ? comp.layers[String(target.parent)]
        : null;
    gesture = {
      mode,
      layer: target ? clone(target) : null,
      start: location(event),
      screen: [event.clientX, event.clientY],
      pan: { ...pan },
      matrix: g?.inverse ?? null,
      corner,
      base: target ? evaluate(target, property, editor.currentTime) : null,
      value: null,
      property,
      center: g?.center ?? [0, 0],
      moved: false,
      parentInverse: parent ? inverse(worldMatrix(comp, parent, editor.currentTime)) : null,
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", finish);
    window.addEventListener("pointercancel", cancel);
  }
  function down(event: PointerEvent) {
    if (!comp || !editor.project || event.button !== 0) return;
    if (editor.tool === "hand") {
      begin(event, "pan", null);
      return;
    }
    const [x, y] = location(event),
      hit = hitTest(comp, editor.project, x, y, editor.currentTime);
    editor.selected = hit?.id ?? null;
    if (hit) begin(event, "move", hit);
  }
  function move(event: PointerEvent) {
    if (!gesture) return;
    const g = gesture;
    if (Math.hypot(event.clientX - g.screen[0], event.clientY - g.screen[1]) < 2 && !g.moved)
      return;
    g.moved = true;
    if (g.mode === "pan") {
      pan = { x: g.pan.x + event.clientX - g.screen[0], y: g.pan.y + event.clientY - g.screen[1] };
      return;
    }
    if (!g.layer || !g.base) return;
    const [x, y] = location(event);
    let value: PropValue;
    if (g.mode === "move" && "Vec2" in g.base) {
      let dx = x - g.start[0],
        dy = y - g.start[1];
      if (g.parentInverse) {
        const m = g.parentInverse;
        [dx, dy] = [m[0] * dx + m[2] * dy, m[1] * dx + m[3] * dy];
      }
      if (event.shiftKey) {
        if (Math.abs(dx) > Math.abs(dy)) dy = 0;
        else dx = 0;
      }
      value = { Vec2: [Math.round(g.base.Vec2[0] + dx), Math.round(g.base.Vec2[1] + dy)] };
    } else if (g.mode === "scale" && "Vec2" in g.base && g.matrix && comp && editor.project) {
      const original = layerGeometry(comp, g.layer, editor.project, editor.currentTime);
      const local = point(g.matrix, x, y);
      const sx = g.corner === 0 || g.corner === 3 ? -1 : 1,
        sy = g.corner < 2 ? -1 : 1;
      let rx = local[0] / ((original.width / 2) * sx),
        ry = local[1] / ((original.height / 2) * sy);
      if (event.shiftKey) {
        const r = Math.abs(rx) > Math.abs(ry) ? rx : ry;
        rx = r;
        ry = r;
      }
      value = {
        Vec2: [
          Math.max(-2000, Math.min(2000, g.base.Vec2[0] * rx)),
          Math.max(-2000, Math.min(2000, g.base.Vec2[1] * ry)),
        ],
      };
    } else if (g.mode === "rotate" && "Scalar" in g.base) {
      let degrees =
        ((Math.atan2(y - g.center[1], x - g.center[0]) -
          Math.atan2(g.start[1] - g.center[1], g.start[0] - g.center[0])) *
          180) /
          Math.PI +
        g.base.Scalar;
      if (event.shiftKey) degrees = Math.round(degrees / 15) * 15;
      value = { Scalar: degrees };
    } else return;
    g.value = value;
    const preview = clone(g.layer);
    delete preview.tracks[g.property];
    if (g.property === "Position" && "Vec2" in value) preview.transform.position = value.Vec2;
    if (g.property === "Scale" && "Vec2" in value) preview.transform.scale = value.Vec2;
    if (g.property === "Rotation" && "Scalar" in value) preview.transform.rotation = value.Scalar;
    editor.previewLayer = preview;
  }
  function cleanup() {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", finish);
    window.removeEventListener("pointercancel", cancel);
  }
  async function finish() {
    const g = gesture;
    gesture = null;
    cleanup();
    if (g?.moved && g.layer && g.value) await setProperty(g.layer.id, g.property, g.value);
    editor.previewLayer = null;
    editor.renderSeq++;
  }
  function cancel() {
    gesture = null;
    cleanup();
    editor.previewLayer = null;
    editor.renderSeq++;
  }
  onDestroy(() => {
    cleanup();
  });
  function nudgeHandle(event: KeyboardEvent, property: Property) {
    if (!selected || !["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(event.key))
      return;
    event.preventDefault();
    event.stopPropagation();
    const value = evaluate(selected, property, editor.currentTime),
      direction = ["ArrowLeft", "ArrowDown"].includes(event.key) ? -1 : 1;
    if ("Scalar" in value)
      void setProperty(selected.id, property, {
        Scalar: value.Scalar + direction * (event.shiftKey ? 15 : 1),
      });
    else {
      const v: [number, number] = [...value.Vec2];
      const axis = ["ArrowLeft", "ArrowRight"].includes(event.key) ? 0 : 1;
      v[axis] += direction * (event.shiftKey ? 10 : 1);
      void setProperty(selected.id, property, { Vec2: v });
    }
  }
  function fit() {
    zoom = "fit";
    pan = { x: 0, y: 0 };
  }
</script>

<section class="panel viewport" aria-label="Composition viewer">
  <div class="panel-heading viewport-heading">
    <span class="viewer-label">Composition</span><span class="tab-separator"></span><Icon
      name="film"
      size={12}
      class="accent"
    /><span class="comp-title">{comp?.name ?? "No composition"}</span><span class="spacer"
    ></span>{#if comp && Object.keys(editor.project?.comps ?? {}).length > 1}<select
        aria-label="Active composition"
        value={comp.id}
        onchange={(e) => {
          selectComp(Number(e.currentTarget.value));
          fit();
        }}
        >{#each Object.values(editor.project?.comps ?? {}) as c}<option value={c.id}
            >{c.name}</option
          >{/each}</select
      >{/if}<button
      class="icon-button small"
      title="Fit composition to viewer"
      aria-label="Fit composition"
      onclick={fit}><Icon name="maximize" size={13} /></button
    >
  </div>
  <div class="stage" bind:this={stage} class:hand={editor.tool === "hand"}>
    <div class="viewer-tools" role="toolbar" aria-label="Canvas tools">
      <button
        class="icon-button"
        class:active={editor.tool === "select"}
        title="Selection tool (V)"
        aria-label="Selection tool"
        onclick={() => (editor.tool = "select")}><Icon name="pointer" size={16} /></button
      >
      <button
        class="icon-button"
        class:active={editor.tool === "hand"}
        title="Hand tool (H)"
        aria-label="Hand tool"
        onclick={() => (editor.tool = "hand")}><Icon name="hand" size={16} /></button
      >
      <span class="tool-rule"></span>
      <button
        class="icon-button"
        title="Add text"
        aria-label="Add text layer"
        onclick={() => void addLayer("text")}><Icon name="type" size={16} /></button
      >
      <button
        class="icon-button"
        title="Add rectangle"
        aria-label="Add rectangle layer"
        onclick={() => void addLayer("rectangle")}><Icon name="square" size={15} /></button
      >
      <button
        class="icon-button"
        title="Add ellipse"
        aria-label="Add ellipse layer"
        onclick={() => void addLayer("circle")}><Icon name="circle" size={15} /></button
      >
      <span class="tool-rule"></span>
      <button
        class="icon-button"
        class:active={editor.showGuides}
        title="Composition guides (G)"
        aria-label="Toggle guides"
        onclick={() => (editor.showGuides = !editor.showGuides)}
        ><Icon name="grid" size={15} /></button
      >
    </div>
    {#if comp}
      <div
        class="canvas-wrap"
        style={`width:${comp.width * scale}px;height:${comp.height * scale}px;transform:translate(calc(-50% + ${pan.x}px),calc(-50% + ${pan.y}px))`}
      >
        <canvas
          bind:this={canvas}
          aria-label="Rendered composition"
          onpointerdown={down}
          ondblclick={() => {
            if (selected && "PreComp" in selected.kind) {
              selectComp(selected.kind.PreComp.comp);
              fit();
            }
          }}
        ></canvas>
        <svg
          class="overlay"
          viewBox={`0 0 ${comp.width} ${comp.height}`}
          aria-label="Layer transform overlay"
          role="group"
        >
          {#if editor.showGuides}<g
              stroke="#d6ecbd"
              stroke-width={0.6 / scale}
              stroke-opacity=".35"
              fill="none"
              ><rect
                x={comp.width * 0.05}
                y={comp.height * 0.05}
                width={comp.width * 0.9}
                height={comp.height * 0.9}
              /><path
                d={`M${comp.width / 3} 0V${comp.height} M${(comp.width * 2) / 3} 0V${comp.height} M0 ${comp.height / 3}H${comp.width} M0 ${(comp.height * 2) / 3}H${comp.width}`}
              /></g
            >{/if}
          {#if motion.length > 1}<polyline
              points={motion.map((k) => k.center.join(",")).join(" ")}
              stroke="#c5dbb0"
              stroke-opacity=".55"
              stroke-width={1 / scale}
              fill="none"
              stroke-dasharray={`${3 / scale} ${4 / scale}`}
            />{#each motion as key}<circle
                cx={key.center[0]}
                cy={key.center[1]}
                r={3 / scale}
                fill="#1e2817"
                stroke="#d2e2bd"
                stroke-width={1 / scale}
              />{/each}{/if}
          {#if geometry && layer && !layer.locked && editor.tool === "select"}
            <polygon
              {points}
              fill="none"
              stroke="#c0e6aa"
              stroke-width={0.8 / scale}
              stroke-opacity=".75"
            />
            {#each geometry.corners as corner, index}<rect
                role="button"
                tabindex="0"
                aria-label={`Scale handle ${index + 1}`}
                onkeydown={(e) => nudgeHandle(e, "Scale")}
                class="transform-handle"
                x={corner[0] - 3.2 / scale}
                y={corner[1] - 3.2 / scale}
                width={6.4 / scale}
                height={6.4 / scale}
                fill="#202a19"
                stroke="#d7efc1"
                stroke-width={1 / scale}
                style={`cursor:${index % 2 === 0 ? "nwse" : "nesw"}-resize`}
                onpointerdown={(e) => begin(e, "scale", layer, index)}
              />{/each}
            {@const top:[number,number]=[(geometry.corners[0][0]+geometry.corners[1][0])/2,(geometry.corners[0][1]+geometry.corners[1][1])/2]}
            <line
              x1={top[0]}
              y1={top[1]}
              x2={top[0]}
              y2={top[1] - 20 / scale}
              stroke="#c0e6aa"
              stroke-width={0.8 / scale}
            />
            <circle
              role="button"
              tabindex="0"
              aria-label="Rotation handle"
              onkeydown={(e) => nudgeHandle(e, "Rotation")}
              class="transform-handle rotate-handle"
              cx={top[0]}
              cy={top[1] - 24 / scale}
              r={3.5 / scale}
              fill="#202a19"
              stroke="#d7efc1"
              stroke-width={1 / scale}
              onpointerdown={(e) => begin(e, "rotate", layer)}
            />
            <path
              d={`M${geometry.center[0] - 5 / scale} ${geometry.center[1]}h${10 / scale} M${geometry.center[0]} ${geometry.center[1] - 5 / scale}v${10 / scale}`}
              stroke="#e3eed7"
              stroke-opacity=".75"
              stroke-width={0.8 / scale}
            />
          {/if}
        </svg>
      </div>
      <div class="canvas-meta mono">
        {comp.width} × {comp.height}<span>·</span>{formatFps(comp.fps)}
      </div>
      {#if editor.bypassEffects}<div class="bypass-banner">
          <Icon name="eye-off" size={11} />All effects bypassed
        </div>{/if}
      {#if editor.renderError}<div class="render-error" role="alert">
          <Icon name="info" size={19} /><strong>Couldn’t render this frame</strong><span
            >{editor.renderError}</span
          >
        </div>{/if}
    {:else}<div class="empty no-comp">
        <Icon name="film" size={28} /><strong>Your canvas is waiting</strong>Create a composition to
        start designing.
      </div>{/if}
  </div>
  <div class="viewer-footer">
    <select
      aria-label="Viewer zoom"
      value={zoom}
      onchange={(e) => {
        zoom = e.currentTarget.value;
        if (zoom === "fit") pan = { x: 0, y: 0 };
      }}
      ><option value="fit">Fit · {Math.round(scale * 100)}%</option
      >{#each [25, 50, 75, 100, 150, 200] as percentage}<option value={String(percentage)}
          >{percentage}%</option
        >{/each}</select
    >
    <span class="divider"></span><button
      class="compare"
      class:active={editor.bypassEffects}
      title="Bypass all effects to compare with the source"
      aria-label="Bypass all effects"
      onclick={() => (editor.bypassEffects = !editor.bypassEffects)}
      ><span class="fx">ƒx</span><span>{editor.bypassEffects ? "Bypassed" : "Effects on"}</span
      ></button
    >
    <span class="spacer"></span><span class="render-status"><span></span>Full resolution</span><span
      class="divider"
    ></span><span class="mono frame-time"
      >{comp ? timeToTimecode(editor.currentTime, comp.fps) : "00:00:00:00"}</span
    >
  </div>
</section>

<style>
  .viewport {
    background: #171817;
  }
  .viewport-heading {
    padding-left: 16px;
    background: #212221;
    gap: 8px;
  }
  .viewer-label {
    font-size: 10px;
    color: #9ea19d;
  }
  .comp-title {
    font-size: 10px;
    color: #d5d7d3;
    font-weight: 400;
  }
  .tab-separator {
    height: 12px;
    width: 1px;
    background: #414340;
    margin: 0 5px;
  }
  .viewport-heading select {
    max-width: 130px;
    background: transparent;
    border: 0;
    color: #848781;
    font-size: 9px;
    outline: 0;
    padding: 4px;
  }
  .viewport-heading select option {
    background: #2b2c2a;
  }
  .stage {
    position: relative;
    flex: 1;
    min-height: 0;
    overflow: hidden;
    background: radial-gradient(ellipse at 55% 42%, #262725 0, #1b1c1a 75%);
  }
  .stage.hand {
    cursor: grab;
  }
  .canvas-wrap {
    position: absolute;
    left: 50%;
    top: 50%;
    background-color: #282927;
    background-image:
      linear-gradient(45deg, #313331 25%, transparent 25%),
      linear-gradient(-45deg, #313331 25%, transparent 25%),
      linear-gradient(45deg, transparent 75%, #313331 75%),
      linear-gradient(-45deg, transparent 75%, #313331 75%);
    background-size: 12px 12px;
    background-position:
      0 0,
      0 6px,
      6px -6px,
      -6px 0;
    box-shadow:
      0 8px 35px #0005,
      0 0 0 1px #484a4660;
  }
  .canvas-wrap canvas {
    display: block;
    width: 100%;
    height: 100%;
    touch-action: none;
  }
  .overlay {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    overflow: visible;
    pointer-events: none;
  }
  .transform-handle {
    pointer-events: all;
    touch-action: none;
  }
  .rotate-handle {
    cursor: grab;
  }
  .viewer-tools {
    position: absolute;
    left: 12px;
    top: 16px;
    z-index: 4;
    display: flex;
    flex-direction: column;
    gap: 3px;
    background: #262725f0;
    border: 1px solid #41433f;
    border-radius: 5px;
    padding: 4px;
    box-shadow: 0 4px 12px #0002;
  }
  .viewer-tools .icon-button {
    width: 25px;
    height: 27px;
  }
  .viewer-tools .icon-button.active {
    background: #4c4f49;
    color: #d6dad2;
  }
  .tool-rule {
    height: 1px;
    background: #474945;
    margin: 3px 4px;
  }
  .canvas-meta {
    position: absolute;
    bottom: 13px;
    left: 0;
    right: 0;
    text-align: center;
    pointer-events: none;
    font-size: 8px;
    letter-spacing: 0.3px;
    color: #656763;
  }
  .canvas-meta span {
    padding: 0 8px;
  }
  .bypass-banner {
    position: absolute;
    right: 14px;
    top: 14px;
    display: flex;
    align-items: center;
    gap: 6px;
    background: #43413ddc;
    border: 1px solid #747068;
    color: #cbc7bf;
    border-radius: 4px;
    font-size: 9px;
    padding: 7px 9px;
  }
  .render-error {
    position: absolute;
    inset: 50% auto auto 50%;
    transform: translate(-50%, -50%);
    width: min(360px, 80%);
    display: flex;
    align-items: center;
    flex-direction: column;
    gap: 10px;
    padding: 25px;
    background: #252423f5;
    border: 1px solid #5e5855;
    color: #bdb3ae;
    border-radius: 6px;
    font-size: 11px;
    box-shadow: var(--shadow-lg);
    text-align: center;
  }
  .render-error span {
    font-size: 10px;
    line-height: 1.6;
    color: #b0ada9;
  }
  .no-comp {
    position: absolute;
    left: 50%;
    top: 50%;
    transform: translate(-50%, -50%);
    width: 80%;
  }
  .viewer-footer {
    height: 34px;
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 0 13px;
    background: #222321;
    border-top: 1px solid #383b37;
    flex-shrink: 0;
  }
  .viewer-footer select {
    border: 0;
    background: none;
    font: 9px monospace;
    color: #a4a8a1;
    outline: 0;
    width: 84px;
  }
  .viewer-footer option {
    background: #2c2e2b;
  }
  .compare {
    display: flex;
    align-items: center;
    gap: 6px;
    color: #969993;
    font-size: 9px;
    padding: 2px 5px;
  }
  .compare.active {
    color: #bcb7ad;
  }
  .fx {
    font-family: Georgia, serif;
    font-style: italic;
    font-size: 16px;
    color: #c6cbc1;
  }
  .frame-time {
    font-size: 10px;
    color: #cacdc6;
    letter-spacing: 0.7px;
  }
  .render-status {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 8px;
    color: #7d817a;
  }
  .render-status > span {
    width: 3px;
    height: 3px;
    border-radius: 50%;
    background: #91978c;
  }
</style>
