<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import Icon from "./Icon.svelte";
  import {
    editor,
    activeComp,
    selectedLayer,
    editingLayer,
    flushLiveEdits,
    renderTo,
    previewDivisor,
    setPreviewPreference,
    clone,
    setProperty,
    addLayer,
    layerContextItems,
    openContextMenu,
    compCameraAt,
    moveCamera,
    setTurntableEnabled,
    focusCameraOnSelection,
    snapshotFrame,
    selectComp,
    scrub,
    pause,
    cancelInteraction,
  } from "../store.svelte";
  import {
    ARRANGEMENTS,
    applySceneArrangement,
    arrangeInDepth,
    create3dScene,
    setFraming,
    type Framing,
  } from "../three-d";
  import type { PreviewBackend, PreviewQuality } from "../preview";
  import {
    getPlanes,
    preparePlanes,
    transformedLayer,
    cssBlend,
    type InteractionPlanes,
  } from "../interaction";
  import {
    layerGeometry,
    hitTest,
    worldMatrix,
    inverse,
    multiply,
    point,
    type Matrix,
  } from "../geometry";
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
  const selected = $derived(editingLayer());
  const poseLayer = $derived(
    selected && editor.interaction?.layer === selected.id
      ? transformedLayer(selected, editor.interaction.property, editor.interaction.value)
      : editor.previewLayer?.id === selected?.id
        ? editor.previewLayer
        : selected,
  );
  const previewComp = $derived(
    comp && poseLayer && poseLayer !== selected
      ? { ...comp, layers: { ...comp.layers, [String(poseLayer.id)]: poseLayer } }
      : comp,
  );
  const layer = $derived(poseLayer);
  let planes = $state.raw<InteractionPlanes | null>(null);
  let baseCanvas = $state<HTMLCanvasElement | null>(null),
    subjectCanvas = $state<HTMLCanvasElement | null>(null),
    topCanvas = $state<HTMLCanvasElement | null>(null);
  const proxyActive = $derived(
    !!editor.interaction &&
      editor.interaction.property === "Position" &&
      !!planes &&
      planes.layerId === editor.interaction.layer &&
      planes.compId === editor.interaction.comp &&
      planes.time === editor.interaction.time &&
      !!layer?.visible &&
      !layer.locked &&
      selected?.id === editor.interaction.layer &&
      editor.nativeInteractionSequence !== editor.interaction.sequence &&
      editor.previewQuality !== "1",
  );
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
  const rotationHandle = $derived.by(() => {
    if (!geometry) return null;
    const p = geometry.corners;
    const top: [number, number] = [(p[0][0] + p[1][0]) / 2, (p[0][1] + p[1][1]) / 2];
    let nx = p[1][1] - p[0][1],
      ny = p[0][0] - p[1][0];
    const len = Math.hypot(nx, ny) || 1;
    nx /= len;
    ny /= len;
    if (nx * (top[0] - geometry.center[0]) + ny * (top[1] - geometry.center[1]) < 0) {
      nx = -nx;
      ny = -ny;
    }
    return { top, x: top[0] + (nx * 25) / scale, y: top[1] + (ny * 25) / scale };
  });

  const proxyMatrix = $derived.by(() => {
    if (!planes || !geometry) return "none";
    const inv = inverse(planes.matrix);
    if (!inv) return "none";
    const m = multiply(geometry.matrix, inv);
    return `matrix(${m[0]},${m[1]},${m[2]},${m[3]},${m[4] * scale},${m[5] * scale})`;
  });
  $effect(() => {
    editor.interactionReady = proxyActive;
  });
  $effect(() => {
    if (!planes || !baseCanvas || !subjectCanvas || !topCanvas) return;
    [baseCanvas, subjectCanvas, topCanvas].forEach((node, i) => {
      node.width = planes!.width;
      node.height = planes!.height;
      node.getContext("2d")?.putImageData(planes!.images[i], 0, 0);
    });
  });
  async function prefetch(id: number) {
    const c = comp,
      revision = editor.revision,
      time = editor.currentTime,
      bypass = editor.bypassEffects;
    if (!c || !editor.interactionProtocol || editor.playing) return;
    const ready = await preparePlanes(c.id, id, revision, time, bypass);
    if (
      ready &&
      comp?.id === c.id &&
      editor.revision === revision &&
      editor.currentTime === time &&
      (editor.selected === id || gesture?.layer?.id === id)
    )
      planes = ready;
  }
  $effect(() => {
    const id = editor.selected,
      rev = editor.revision,
      time = editor.currentTime,
      playing = editor.playing;
    if (id === null || playing || !editor.interactionProtocol) return;
    const timer = setTimeout(() => {
      if (editor.revision === rev && editor.currentTime === time) void prefetch(id);
    }, 50);
    return () => clearTimeout(timer);
  });
  $effect(() => {
    const i = editor.interaction;
    if (i?.phase !== "drag") return;
    const timer = setTimeout(() => {
      if (editor.interaction?.sequence === i.sequence) {
        editor.refineInteractionSequence = i.sequence;
        editor.renderSeq++;
      }
    }, 90);
    return () => clearTimeout(timer);
  });
  let lastHover = 0;
  function hover(e: PointerEvent) {
    if (gesture || editor.playing || !comp || !editor.project || performance.now() - lastHover < 90)
      return;
    lastHover = performance.now();
    const [x, y] = location(e);
    const hit = hitTest(comp, editor.project, x, y, editor.currentTime);
    if (hit) void prefetch(hit.id);
  }

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
    void editor.liveEdit;
    void editor.bypassEffects;
    void editor.previewQuality;
    void editor.previewBackend;
    void editor.playing;
    void editor.interaction;
    void editor.interactionReady;
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
    token: number;
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
    width: number;
    height: number;
    compId: number;
    time: number;
    bounds: DOMRect;
    lastAngle: number;
    rotationDelta: number;
    anchor: [number, number];
  } = null;
  let gestureToken = 0;
  function location(event: { clientX: number; clientY: number }): [number, number] {
    if (!canvas || !comp) return [0, 0];
    const bounds = gesture?.bounds ?? canvas.getBoundingClientRect();
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
    if (mode !== "pan") {
      void flushLiveEdits();
      pause();
    }
    if (target) {
      planes = getPlanes(
        comp.id,
        target.id,
        editor.revision,
        editor.currentTime,
        editor.bypassEffects,
      );
      void prefetch(target.id);
    }
    const g = target ? layerGeometry(comp, target, editor.project, editor.currentTime) : null;
    const property: Property =
      mode === "scale" ? "Scale" : mode === "rotate" ? "Rotation" : "Position";
    const parent =
      target?.parent !== null && target?.parent !== undefined
        ? comp.layers[String(target.parent)]
        : null;
    const anchorValue = target
      ? evaluate(target, "AnchorPoint", editor.currentTime)
      : { Vec2: [0, 0] as [number, number] };
    const anchor: [number, number] = "Vec2" in anchorValue ? anchorValue.Vec2 : [0, 0];
    const pivot = g ? point(g.matrix, anchor[0], anchor[1]) : ([0, 0] as [number, number]);
    const parentInverse = parent ? inverse(worldMatrix(comp, parent, editor.currentTime)) : null;
    const initial = location(event);
    const dx = initial[0] - pivot[0],
      dy = initial[1] - pivot[1];
    const parentVector = parentInverse
      ? [
          parentInverse[0] * dx + parentInverse[2] * dy,
          parentInverse[1] * dx + parentInverse[3] * dy,
        ]
      : [dx, dy];
    gesture = {
      mode,
      token: ++gestureToken,
      layer: target,
      start: location(event),
      screen: [event.clientX, event.clientY],
      pan: { ...pan },
      matrix: g?.inverse ?? null,
      corner,
      base: target ? evaluate(target, property, editor.currentTime) : null,
      value: null,
      property,
      center: pivot,
      anchor,
      moved: false,
      parentInverse,
      width: g?.width ?? 0,
      height: g?.height ?? 0,
      compId: comp.id,
      time: editor.currentTime,
      bounds: canvas!.getBoundingClientRect(),
      lastAngle: Math.atan2(parentVector[1], parentVector[0]),
      rotationDelta: 0,
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", finish);
    window.addEventListener("pointercancel", cancel);
  }
  // Right-click: layer actions on a hit, quick layer creation on empty canvas.
  function framingOf(c: NonNullable<ReturnType<typeof activeComp>>): Framing {
    const fov = c.camera?.fov ?? 500;
    const z = c.camera?.z ?? 0;
    const d = (f: number, zz: number) => Math.abs(fov - f) + Math.abs(z - zz) * 0.5;
    const options: [Framing, number, number][] = [
      ["wide", 780, 260],
      ["medium", 500, 0],
      ["closeup", 330, -220],
    ];
    return options.reduce((best, o) => (d(o[1], o[2]) < d(best[1], best[2]) ? o : best))[0];
  }
  function rightClick(event: MouseEvent) {
    if (!comp || !editor.project) return;
    const [x, y] = location(event);
    const hit = hitTest(comp, editor.project, x, y, editor.currentTime);
    if (hit) {
      editor.selected = hit.id;
      openContextMenu(event, [
        ...layerContextItems(hit.id),
        { separator: true as const },
        { label: "Arrange layers in depth", icon: "graph", run: () => arrangeInDepth() },
        { label: "Focus on this layer", icon: "target", run: () => void focusCameraOnSelection() },
      ]);
      return;
    }
    openContextMenu(event, [
      { label: "New text layer", icon: "type", run: () => void addLayer("text") },
      { label: "New rectangle", icon: "square", run: () => void addLayer("rectangle") },
      { label: "New ellipse", icon: "circle", run: () => void addLayer("circle") },
      { label: "New solid color", icon: "square", run: () => void addLayer("solid") },
      { label: "New adjustment layer", icon: "adjust", run: () => void addLayer("adjustment") },
    ]);
  }
  // --- 3D camera orbit: drag rotates the camera around the comp center ---
  let orbiting: { x: number; y: number; angle: number; z: number } | null = null;
  function angleOf(position: [number, number]): number {
    return Math.atan2(position[1], position[0]);
  }
  function orbitDown(event: PointerEvent) {
    if (editor.tool !== "orbit" || !comp || event.button !== 0) return;
    const cam = compCameraAt();
    const r = Math.hypot(cam.position[0], cam.position[1]);
    orbiting = {
      x: event.clientX,
      y: event.clientY,
      angle: angleOf(cam.position),
      z: r > 1 ? cam.z : comp.width * 0.25,
    };
    event.preventDefault();
  }
  function orbitMove(event: PointerEvent) {
    if (!orbiting || !comp) return;
    const dx = event.clientX - orbiting.x,
      dy = event.clientY - orbiting.y;
    const angle = orbiting.angle + dx * 0.011;
    const r = Math.abs(orbiting.z) > 1 ? Math.abs(orbiting.z) : comp.width * 0.25;
    const cam = compCameraAt();
    void moveCamera(
      [r * Math.cos(angle), r * Math.sin(angle)],
      Math.max(-2000, Math.min(4000, orbiting.z + dy * 2)),
      cam.fov > 0 ? cam.fov : 500,
      "orbit-camera",
    );
  }
  async function orbitUp() {
    if (!orbiting) return;
    orbiting = null;
    await flushLiveEdits();
  }
  function down(event: PointerEvent) {
    if (!comp || !editor.project || event.button !== 0) return;
    if (editor.tool === "hand") {
      begin(event, "pan", null);
      return;
    }
    const [x, y] = location(event),
      hit = hitTest(comp, editor.project, x, y, editor.currentTime);
    if (editor.tool === "rotate") {
      const target = selected ?? hit;
      if (target) {
        editor.selected = target.id;
        begin(event, "rotate", target);
      }
      return;
    }
    editor.selected = hit?.id ?? null;
    if (hit) begin(event, "move", hit);
  }
  let latestPointer: PointerEvent | null = null,
    pointerRaf = 0,
    pointerSequence = 0;
  function move(event: PointerEvent) {
    latestPointer = event;
    if (!pointerRaf)
      pointerRaf = requestAnimationFrame(() => {
        pointerRaf = 0;
        const last = latestPointer;
        latestPointer = null;
        if (last) applyPointer(last);
      });
  }
  function applyPointer(event: PointerEvent) {
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
      const local = point(g.matrix, x, y);
      const sx = g.corner === 0 || g.corner === 3 ? -1 : 1,
        sy = g.corner < 2 ? -1 : 1;
      const denominatorX = (g.width / 2) * sx - g.anchor[0],
        denominatorY = (g.height / 2) * sy - g.anchor[1];
      let rx = Math.abs(denominatorX) < 1e-6 ? 1 : (local[0] - g.anchor[0]) / denominatorX,
        ry = Math.abs(denominatorY) < 1e-6 ? 1 : (local[1] - g.anchor[1]) / denominatorY;
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
      let dx = x - g.center[0],
        dy = y - g.center[1];
      if (g.parentInverse) {
        const m = g.parentInverse;
        [dx, dy] = [m[0] * dx + m[2] * dy, m[1] * dx + m[3] * dy];
      }
      const angle = Math.atan2(dy, dx);
      let delta = angle - g.lastAngle;
      while (delta > Math.PI) delta -= Math.PI * 2;
      while (delta < -Math.PI) delta += Math.PI * 2;
      g.rotationDelta += delta;
      g.lastAngle = angle;
      let degrees = g.base.Scalar + (g.rotationDelta * 180) / Math.PI;
      if (event.shiftKey) degrees = Math.round(degrees / 15) * 15;
      value = { Scalar: degrees };
    } else return;
    g.value = value;
    const sequence = ++pointerSequence;
    editor.interaction = {
      layer: g.layer.id,
      comp: g.compId,
      property: g.property,
      value,
      time: g.time,
      sequence,
      gesture: g.token,
      phase: "drag",
    };
    if (!editor.interactionProtocol)
      editor.previewLayer = transformedLayer(g.layer, g.property, value);
  }

  function cleanup() {
    if (pointerRaf) cancelAnimationFrame(pointerRaf);
    pointerRaf = 0;
    latestPointer = null;
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", finish);
    window.removeEventListener("pointercancel", cancel);
  }
  async function finish() {
    if (latestPointer) applyPointer(latestPointer);
    const g = gesture;
    gesture = null;
    cleanup();
    if (g?.moved && g.layer && g.value && comp?.id === g.compId) {
      const state = editor.interaction;
      if (state) editor.interaction = { ...state, phase: "commit" };
      const ok = await setProperty(g.layer.id, g.property, g.value);
      if (ok && state) {
        if (editor.interaction?.sequence === state.sequence)
          editor.interaction = { ...state, phase: "commit", committedRevision: editor.revision };
      } else if (!state || editor.interaction?.sequence === state.sequence) {
        editor.interaction = null;
        editor.interactionReady = false;
      }
    } else {
      editor.interaction = null;
      editor.interactionReady = false;
    }
    editor.previewLayer = null;
    editor.renderSeq++;
  }
  function cancel() {
    gesture = null;
    cleanup();
    editor.previewLayer = null;
    editor.interaction = null;
    editor.interactionReady = false;
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

<svelte:window
  onkeydown={(e) => {
    if (e.key === "Escape" && gesture) {
      e.preventDefault();
      cancel();
    }
  }}
/>
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
      <button
        class="icon-button"
        class:active={editor.tool === "rotate"}
        title="Rotation tool (R): drag around the anchor"
        aria-label="Rotation tool"
        aria-pressed={editor.tool === "rotate"}
        onclick={() => (editor.tool = "rotate")}><Icon name="rotate" size={16} /></button
      >
      <button
        class="icon-button"
        class:active={editor.tool === "orbit"}
        title="3D orbit (O): drag to circle the camera around the scene"
        aria-label="3D orbit tool"
        aria-pressed={editor.tool === "orbit"}
        onclick={() => (editor.tool = editor.tool === "orbit" ? "select" : "orbit")}
        ><Icon name="cube" size={16} /></button
      >
      <span class="tool-rule"></span>
      <button
        class="icon-button"
        class:active={!!comp?.turntable?.enabled}
        title="Turntable: orbit the camera automatically while playing"
        aria-label="Toggle turntable"
        aria-pressed={!!comp?.turntable?.enabled}
        disabled={!comp}
        onclick={() => void setTurntableEnabled(!comp?.turntable?.enabled)}
        ><Icon name="turntable" size={16} /></button
      >
      {#if comp && (comp.camera?.fov ?? 0) > 0}
        <label class="fov-chip" title="Camera focal length — higher is a longer lens">
          <span>FOV</span>
          <input
            type="range"
            min="80"
            max="1200"
            step="10"
            aria-label="Camera focal length"
            value={comp.camera?.fov ?? 500}
            onchange={(e) => {
              const cam = compCameraAt();
              void moveCamera(
                [...cam.position] as [number, number],
                cam.z,
                Number(e.currentTarget.value),
              );
            }}
          />
        </label>
        <label
          class="fov-chip"
          title="Depth of field: blur cards away from the focal plane"
        >
          <span>DoF</span>
          <input
            type="range"
            min="0"
            max="1"
            step="0.05"
            aria-label="Depth of field strength"
            value={comp.camera?.dof ?? 0}
            onchange={(e) => {
              const cam = compCameraAt();
              void moveCamera(
                [...cam.position] as [number, number],
                cam.z,
                cam.fov,
                undefined,
                undefined,
                Number(e.currentTarget.value),
              );
            }}
          />
        </label>
        {#if (comp.camera?.dof ?? 0) > 0}
          <label class="fov-chip" title="Focal plane distance (0 = the camera plane)">
            <span>Focus</span>
            <input
              type="number"
              step="10"
              aria-label="Focal plane distance"
              value={Math.round(comp.camera?.focus ?? 0)}
              onchange={(e) => {
                const cam = compCameraAt();
                void moveCamera(
                  [...cam.position] as [number, number],
                  cam.z,
                  cam.fov,
                  undefined,
                  Number(e.currentTarget.value),
                );
              }}
            />
          </label>
          <button
            class="icon-button"
            title="Focus on the selected layer"
            aria-label="Focus on selection"
            onclick={() => void focusCameraOnSelection()}><Icon name="target" size={15} /></button
          >
        {/if}
      {/if}
      <span class="tool-rule"></span>
      <button
        class="icon-button"
        title="New 3D scene: layered cards with an orbiting camera, ready to play"
        aria-label="New 3D scene"
        disabled={!comp}
        onclick={() => create3dScene()}><Icon name="cube" size={16} /></button
      >
      <button
        class="icon-button"
        title="3D scene arrangements: one-click depth layouts"
        aria-label="3D scene arrangements"
        disabled={!comp}
        onclick={(e) =>
          openContextMenu(e, [
            ...ARRANGEMENTS.map((a) => ({
              label: a.label,
              icon: "cube",
              run: () => applySceneArrangement(a.id),
            })),
            { separator: true as const },
            {
              label: "Arrange layers in depth",
              icon: "graph",
              run: () => arrangeInDepth(),
            },
          ])}><Icon name="sparkles" size={16} /></button
      >
      {#if comp && (comp.camera?.fov ?? 0) > 0}
        <div class="framing-group" role="group" aria-label="Camera framing presets">
          <button
            class:active={framingOf(comp) === "wide"}
            title="Wide framing: see the whole depth stage"
            onclick={() => setFraming("wide")}>Wide</button
          >
          <button
            class:active={framingOf(comp) === "medium"}
            title="Medium framing: balanced depth"
            onclick={() => setFraming("medium")}>Medium</button
          >
          <button
            class:active={framingOf(comp) === "closeup"}
            title="Close-up: near layers fill the frame"
            onclick={() => setFraming("closeup")}>Close-up</button
          >
        </div>
      {/if}
      <span class="tool-rule"></span>
      <button
        class="icon-button"
        title="Snapshot this frame as a PNG"
        aria-label="Snapshot frame"
        onclick={() => void snapshotFrame()}><Icon name="camera" size={16} /></button
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
          class:orbiting={editor.tool === "orbit"}
          aria-label="Rendered composition"
          onpointerdown={(e) => {
            orbitDown(e);
            if (editor.tool !== "orbit") down(e);
          }}
          onpointermove={(e) => {
            orbitMove(e);
            hover(e);
          }}
          onpointerup={() => void orbitUp()}
          onpointerleave={() => void orbitUp()}
          oncontextmenu={rightClick}
          style:opacity={proxyActive ? 0 : 1}
          ondblclick={() => {
            if (selected && "PreComp" in selected.kind) {
              selectComp(selected.kind.PreComp.comp);
              fit();
            }
          }}
        ></canvas>
        <div
          class="interaction-planes"
          class:shown={proxyActive}
          aria-hidden="true"
          data-interaction-active={proxyActive}
          data-interaction-matrix={proxyMatrix}
        >
          <canvas bind:this={baseCanvas}></canvas>
          <canvas
            bind:this={subjectCanvas}
            style={`transform:${proxyMatrix};mix-blend-mode:${planes ? cssBlend(planes.blendMode) : "normal"}`}
          ></canvas>
          <canvas bind:this={topCanvas}></canvas>
        </div>
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
          {#if geometry && layer && !layer.locked && editor.tool !== "hand"}
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
            {#if rotationHandle}
              <line
                x1={rotationHandle.top[0]}
                y1={rotationHandle.top[1]}
                x2={rotationHandle.x}
                y2={rotationHandle.y}
                stroke="#c0e6aa"
                stroke-width={0.8 / scale}
              />
              <circle
                role="button"
                tabindex="0"
                aria-label="Rotation handle"
                onkeydown={(e) => nudgeHandle(e, "Rotation")}
                class="transform-handle rotate-handle"
                cx={rotationHandle.x}
                cy={rotationHandle.y}
                r={11 / scale}
                fill="transparent"
                onpointerdown={(e) => begin(e, "rotate", layer)}
              />
              <circle
                cx={rotationHandle.x}
                cy={rotationHandle.y}
                r={4 / scale}
                fill="#202a19"
                stroke="#d7efc1"
                stroke-width={1 / scale}
                pointer-events="none"
              />
            {/if}
            <path
              d={`M${geometry.center[0] - 5 / scale} ${geometry.center[1]}h${10 / scale} M${geometry.center[0]} ${geometry.center[1] - 5 / scale}v${10 / scale}`}
              stroke="#e3eed7"
              stroke-opacity=".75"
              stroke-width={0.8 / scale}
            />
          {/if}
        </svg>
      </div>
      {#if editor.interaction}<div class="interaction-label">
          {editor.interaction.phase === "commit"
            ? "Refining final frame…"
            : proxyActive
              ? "Interactive · cached pixels"
              : "Interactive · draft updating"}
        </div>{/if}
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
    <span class="spacer"></span>
    {#if editor.previewProtocol >= 3}
      <select
        class="preview-quality"
        aria-label="Preview resolution"
        title="Preview only. Auto uses full resolution while paused and half during playback. Exports remain full resolution."
        value={editor.previewQuality}
        onchange={(e) =>
          setPreviewPreference(e.currentTarget.value as PreviewQuality, editor.previewBackend)}
      >
        <option value="auto">Auto · {previewDivisor() === 1 ? "Full" : "Half"}</option>
        <option value="1">Full</option><option value="2">Half</option><option value="4"
          >Quarter</option
        >
      </select>
      <select
        class="preview-backend"
        aria-label="Preview renderer"
        title="Requested backend. The engine badge reports actual execution and any fallback."
        value={editor.previewBackend}
        onchange={(e) =>
          setPreviewPreference(editor.previewQuality, e.currentTarget.value as PreviewBackend)}
      >
        <option value="auto">Auto engine</option><option value="cpu">CPU</option><option value="gpu"
          >{editor.previewStatus?.gpu.software ? "GPU · software" : "GPU"}</option
        >
      </select>
      <span
        class="cache-status"
        class:cached={editor.previewMetadata?.cacheHit}
        title={editor.previewMetadata
          ? `${editor.previewMetadata.width} × ${editor.previewMetadata.height} preview pixels; ${editor.previewMetadata.cacheHit ? "served from frame cache" : "rendered now"}`
          : "Waiting for preview"}>{editor.previewMetadata?.cacheHit ? "CACHED" : "LIVE"}</span
      >
    {:else}<span class="render-status"><span></span>Full resolution</span>{/if}
    <span class="divider"></span><span class="mono frame-time"
      >{comp ? timeToTimecode(editor.currentTime, comp.fps) : "00:00:00:00"}</span
    >
  </div>
</section>

<style>
  .fov-chip {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 10px;
    color: var(--text-3, #8b9284);
    background: #222422;
    border: 1px solid var(--border);
    border-radius: 7px;
    padding: 3px 8px;
  }
  .fov-chip input[type='range'] {
    width: 74px;
    accent-color: #9dc37f;
  }
  .framing-group {
    display: flex;
    align-items: center;
    background: #222422;
    border: 1px solid var(--border);
    border-radius: 7px;
    overflow: hidden;
  }
  .framing-group button {
    font-size: 10px;
    padding: 3px 8px;
    color: var(--text-3, #8b9284);
    background: none;
    border: 0;
    cursor: pointer;
  }
  .framing-group button + button {
    border-left: 1px solid var(--border);
  }
  .framing-group button.active {
    color: #d5e3c3;
    background: #2c2f2a;
  }
  canvas.orbiting {
    cursor: grab;
  }
  .interaction-planes {
    position: absolute;
    inset: 0;
    pointer-events: none;
    display: none;
    isolation: isolate;
    overflow: hidden;
  }
  .interaction-planes.shown {
    display: block;
  }
  .interaction-planes canvas {
    position: absolute;
    inset: 0;
    transform-origin: 0 0;
    will-change: transform;
  }
  .interaction-label {
    position: absolute;
    bottom: 12px;
    left: 50%;
    transform: translateX(-50%);
    font: 8px monospace;
    letter-spacing: 0.4px;
    background: #233320ee;
    border: 1px solid #53664a;
    padding: 5px 8px;
    border-radius: 4px;
    color: #bdd5ac;
    pointer-events: none;
  }

  .preview-quality,
  .preview-backend {
    max-width: 114px;
    font-size: 9px;
    color: var(--text-secondary);
  }
  .cache-status {
    color: #858e81;
    font: 7px monospace;
    letter-spacing: 0.5px;
    padding: 3px 5px;
    border: 1px solid #42483e;
    border-radius: 3px;
  }
  .cache-status.cached {
    color: var(--accent);
    border-color: #637757;
    background: #abc88a0d;
  }

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
