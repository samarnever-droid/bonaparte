<script lang="ts">
  import { editor, activeComp, selectedLayer, applyOp, renderTo } from "../store.svelte";
  import {
    hitTest,
    layerRect,
    getHandles,
    getMotionPath,
    rotatePoint,
    type HandlePoint,
    type MotionPathPoint,
    type MotionPathTangent,
    type MotionPathData,
    type LayerGeometry,
  } from "../geometry";
  import {
    findKeyframeAtTime,
    snapToFrame,
    ticksPerFrame,
    timeToTimecode,
    DEFAULT_EASING,
    evaluate,
    type Easing,
    type Keyframe,
    type PropValue,
    type Property,
  } from "../model";

  let canvas: HTMLCanvasElement | null = $state(null);
  let svgOverlay: SVGSVGElement | null = $state(null);

  const comp = $derived(activeComp());
  const layer = $derived(selectedLayer());

  const previewOverride = $derived.by(() => {
    if (!dragging) return undefined;
    return {
      position: currentPreviewPos ?? undefined,
      scale: currentPreviewScale ?? undefined,
      rotation: currentPreviewRot ?? undefined,
      anchorPoint: currentPreviewAnchor ?? undefined,
    };
  });

  const geom = $derived(
    comp && layer && layer.visible !== false
      ? layerRect(comp, layer, editor.currentTime, previewOverride)
      : null,
  );
  const handles = $derived(geom ? getHandles(geom) : []);
  const motion: MotionPathData = $derived(
    comp && layer && layer.visible !== false
      ? getMotionPath(comp, layer)
      : { path: [], keyframes: [], tangents: [] },
  );

  // Re-render canvas whenever document, time, or drag changes.
  $effect(() => {
    void editor.project;
    void editor.renderSeq;
    void editor.currentTime;
    void editor.drag;
    if (canvas) void renderTo(canvas);
  });

  function toCompSpace(e: PointerEvent): { x: number; y: number } | null {
    if (!canvas || !comp) return null;
    const rect = canvas.getBoundingClientRect();
    if (rect.width === 0 || rect.height === 0) return null;
    const scaleX = comp.width / rect.width;
    const scaleY = comp.height / rect.height;
    return {
      x: (e.clientX - rect.left) * scaleX,
      y: (e.clientY - rect.top) * scaleY,
    };
  }

  // Interactive Gesture State
  let dragging = $state(false);
  let dragMode: "move" | "scale" | "rotate" | "anchor" | "motionKey" | "motionTangent" | null =
    $state(null);
  let activeHandle: string | null = $state(null);
  let activeTangent = $state<MotionPathTangent | null>(null);
  let origin = $state({ x: 0, y: 0 });
  let basePos = $state<[number, number]>([0, 0]);
  let baseScale = $state<[number, number]>([100, 100]);
  let baseRot = $state(0);
  let baseAnchor = $state<[number, number]>([0, 0]);
  let baseGeom = $state<LayerGeometry | null>(null);
  let basePivot = $state<[number, number]>([0, 0]);
  let dragKeyTime: number | null = $state(null);
  let currentPreviewPos = $state<[number, number] | null>(null);
  let currentPreviewScale = $state<[number, number] | null>(null);
  let currentPreviewRot = $state<number | null>(null);
  let currentPreviewAnchor = $state<[number, number] | null>(null);
  let tangentDelta = $state<[number, number]>([0, 0]);

  function getLayerEasing(property: Property, time: number): Easing {
    if (!layer || !comp || !layer.tracks[property]) return DEFAULT_EASING;
    const track = layer.tracks[property];
    const key = findKeyframeAtTime(track, time, comp.fps);
    return key?.easing ?? DEFAULT_EASING;
  }

  // 1. Layer Move PointerDown
  function onCanvasDown(e: PointerEvent) {
    if (!comp) return;
    const p = toCompSpace(e);
    if (!p) return;

    const hit = hitTest(comp, p.x, p.y, editor.currentTime);
    if (!hit) {
      editor.selected = null;
      return;
    }

    editor.selected = hit.id;
    const targetGeom = layerRect(comp, hit, editor.currentTime);

    dragging = true;
    dragMode = "move";
    origin = p;
    basePos = [...targetGeom.position];
    currentPreviewPos = [...basePos];

    document.body.style.cursor = "grabbing";
    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", onPointerUp);
  }

  // 2. Scale Handle PointerDown
  function onScaleHandleDown(e: PointerEvent, handleId: string, cursor: string) {
    e.stopPropagation();
    e.preventDefault();
    if (!comp || !layer || !geom) return;
    const p = toCompSpace(e);
    if (!p) return;

    dragging = true;
    dragMode = "scale";
    activeHandle = handleId;
    origin = p;
    baseScale = [...geom.scale];
    basePos = [...geom.position];
    baseRot = geom.rotation;
    baseGeom = { ...geom };
    currentPreviewScale = [...baseScale];

    document.body.style.cursor = cursor;
    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", onPointerUp);
  }

  // 3. Rotation Handle PointerDown
  function onRotateHandleDown(e: PointerEvent) {
    e.stopPropagation();
    e.preventDefault();
    if (!comp || !layer || !geom) return;
    const p = toCompSpace(e);
    if (!p) return;

    dragging = true;
    dragMode = "rotate";
    origin = p;
    baseRot = geom.rotation;
    currentPreviewRot = baseRot;
    basePivot = [geom.cx + geom.anchorPoint[0], geom.cy + geom.anchorPoint[1]];

    document.body.style.cursor = "grabbing";
    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", onPointerUp);
  }

  // 4. Anchor Handle PointerDown
  function onAnchorHandleDown(e: PointerEvent) {
    e.stopPropagation();
    e.preventDefault();
    if (!comp || !layer || !geom) return;
    const p = toCompSpace(e);
    if (!p) return;

    dragging = true;
    dragMode = "anchor";
    origin = p;
    baseAnchor = [...geom.anchorPoint];
    currentPreviewAnchor = [...baseAnchor];

    document.body.style.cursor = "crosshair";
    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", onPointerUp);
  }

  // 5. Motion Path Keyframe PointerDown
  function onMotionKeyframeDown(e: PointerEvent, keyTime: number) {
    e.stopPropagation();
    e.preventDefault();
    if (!comp || !layer) return;
    const p = toCompSpace(e);
    if (!p) return;

    const posVal = evaluate(layer, "Position", keyTime);
    const vec = "Vec2" in posVal ? posVal.Vec2 : [0, 0];

    dragging = true;
    dragMode = "motionKey";
    dragKeyTime = keyTime;
    origin = p;
    basePos = [vec[0], vec[1]];
    currentPreviewPos = [...basePos];
    editor.currentTime = keyTime;

    document.body.style.cursor = "grabbing";
    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", onPointerUp);
  }

  // 6. Motion Path Tangent Handle PointerDown
  function onMotionTangentDown(e: PointerEvent, tangent: MotionPathTangent) {
    e.stopPropagation();
    e.preventDefault();
    if (!comp || !layer) return;
    const p = toCompSpace(e);
    if (!p) return;

    dragging = true;
    dragMode = "motionTangent";
    activeTangent = tangent;
    origin = p;
    tangentDelta = [0, 0];

    document.body.style.cursor = "grabbing";
    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", onPointerUp);
  }

  // Unified PointerMove
  function onPointerMove(e: PointerEvent) {
    if (!dragging || !comp || editor.selected === null || !layer) return;
    const p = toCompSpace(e);
    if (!p) return;

    const dx = p.x - origin.x;
    const dy = p.y - origin.y;

    if (dragMode === "move") {
      currentPreviewPos = [Math.round(basePos[0] + dx), Math.round(basePos[1] + dy)];
      editor.drag = {
        layer: editor.selected,
        dx,
        dy,
      };
    } else if (dragMode === "scale" && baseGeom && activeHandle) {
      // Transform comp-space dx/dy into layer's local coordinates
      const rad = (-baseRot * Math.PI) / 180;
      const localDx = dx * Math.cos(rad) - dy * Math.sin(rad);
      const localDy = dx * Math.sin(rad) + dy * Math.cos(rad);

      const factorX = baseGeom.w > 0 ? (localDx / baseGeom.w) * Math.abs(baseScale[0]) * 2 : 0;
      const factorY = baseGeom.h > 0 ? (localDy / baseGeom.h) * Math.abs(baseScale[1]) * 2 : 0;

      let nextSx = baseScale[0];
      let nextSy = baseScale[1];

      if (activeHandle.includes("e")) nextSx += factorX;
      if (activeHandle.includes("w")) nextSx -= factorX;
      if (activeHandle.includes("s")) nextSy += factorY;
      if (activeHandle.includes("n")) nextSy -= factorY;

      if (
        e.shiftKey &&
        (activeHandle === "nw" ||
          activeHandle === "ne" ||
          activeHandle === "se" ||
          activeHandle === "sw")
      ) {
        const avg = (Math.abs(nextSx) + Math.abs(nextSy)) / 2;
        nextSx = Math.sign(nextSx || 1) * avg;
        nextSy = Math.sign(nextSy || 1) * avg;
      }

      currentPreviewScale = [Math.max(1, nextSx), Math.max(1, nextSy)];
    } else if (dragMode === "rotate") {
      const startAngle = Math.atan2(origin.y - basePivot[1], origin.x - basePivot[0]);
      const curAngle = Math.atan2(p.y - basePivot[1], p.x - basePivot[0]);
      let angleDelta = ((curAngle - startAngle) * 180) / Math.PI;
      if (e.shiftKey) {
        const raw = baseRot + angleDelta;
        const snapped = Math.round(raw / 15) * 15;
        angleDelta = snapped - baseRot;
      }
      currentPreviewRot = Math.round((baseRot + angleDelta) * 10) / 10;
    } else if (dragMode === "anchor") {
      currentPreviewAnchor = [Math.round(baseAnchor[0] + dx), Math.round(baseAnchor[1] + dy)];
    } else if (dragMode === "motionKey") {
      currentPreviewPos = [Math.round(basePos[0] + dx), Math.round(basePos[1] + dy)];
      editor.drag = {
        layer: editor.selected,
        dx,
        dy,
      };
    } else if (dragMode === "motionTangent" && activeTangent) {
      tangentDelta = [dx, dy];
    }
  }

  // Unified PointerUp
  async function onPointerUp(e: PointerEvent) {
    if (!dragging || !comp || editor.selected === null || !layer) {
      resetGesture();
      return;
    }

    try {
      if (dragMode === "move" || dragMode === "motionKey") {
        const finalPos: [number, number] = currentPreviewPos
          ? [Math.round(currentPreviewPos[0]), Math.round(currentPreviewPos[1])]
          : [basePos[0], basePos[1]];

        if (dragMode === "motionKey" && dragKeyTime !== null) {
          const easing = getLayerEasing("Position", dragKeyTime);
          await applyOp({
            type: "addKeyframe",
            comp: comp.id,
            layer: layer.id,
            property: "Position",
            key: {
              time: dragKeyTime,
              value: { Vec2: finalPos },
              easing,
            },
          });
        } else {
          // Track-aware: if position keyframes exist, create/update keyframe at currentTime
          const hasPosTrack = !!(layer.tracks.Position && layer.tracks.Position.keys.length > 0);
          if (hasPosTrack) {
            const targetTime = snapToFrame(editor.currentTime, comp.fps);
            const easing = getLayerEasing("Position", targetTime);
            await applyOp({
              type: "addKeyframe",
              comp: comp.id,
              layer: layer.id,
              property: "Position",
              key: {
                time: targetTime,
                value: { Vec2: finalPos },
                easing,
              },
            });
          } else {
            await applyOp({
              type: "setValue",
              comp: comp.id,
              layer: layer.id,
              property: "Position",
              value: { Vec2: finalPos },
            });
          }
        }
      } else if (dragMode === "scale" && activeHandle) {
        const nextSx = currentPreviewScale ? currentPreviewScale[0] : baseScale[0];
        const nextSy = currentPreviewScale ? currentPreviewScale[1] : baseScale[1];
        const finalScale: [number, number] = [
          Math.round(nextSx * 10) / 10,
          Math.round(nextSy * 10) / 10,
        ];

        const hasScaleTrack = !!(layer.tracks.Scale && layer.tracks.Scale.keys.length > 0);
        if (hasScaleTrack) {
          const targetTime = snapToFrame(editor.currentTime, comp.fps);
          const easing = getLayerEasing("Scale", targetTime);
          await applyOp({
            type: "addKeyframe",
            comp: comp.id,
            layer: layer.id,
            property: "Scale",
            key: {
              time: targetTime,
              value: { Vec2: finalScale },
              easing,
            },
          });
        } else {
          await applyOp({
            type: "setValue",
            comp: comp.id,
            layer: layer.id,
            property: "Scale",
            value: { Vec2: finalScale },
          });
        }
      } else if (dragMode === "rotate") {
        const finalRot = currentPreviewRot !== null ? currentPreviewRot : baseRot;
        const hasRotTrack = !!(layer.tracks.Rotation && layer.tracks.Rotation.keys.length > 0);
        if (hasRotTrack) {
          const targetTime = snapToFrame(editor.currentTime, comp.fps);
          const easing = getLayerEasing("Rotation", targetTime);
          await applyOp({
            type: "addKeyframe",
            comp: comp.id,
            layer: layer.id,
            property: "Rotation",
            key: {
              time: targetTime,
              value: { Scalar: finalRot },
              easing,
            },
          });
        } else {
          await applyOp({
            type: "setValue",
            comp: comp.id,
            layer: layer.id,
            property: "Rotation",
            value: { Scalar: finalRot },
          });
        }
      } else if (dragMode === "anchor") {
        const finalAnchor: [number, number] = currentPreviewAnchor
          ? [Math.round(currentPreviewAnchor[0]), Math.round(currentPreviewAnchor[1])]
          : [baseAnchor[0], baseAnchor[1]];
        const hasAnchorTrack = !!(
          layer.tracks.AnchorPoint && layer.tracks.AnchorPoint.keys.length > 0
        );
        if (hasAnchorTrack) {
          const targetTime = snapToFrame(editor.currentTime, comp.fps);
          const easing = getLayerEasing("AnchorPoint", targetTime);
          await applyOp({
            type: "addKeyframe",
            comp: comp.id,
            layer: layer.id,
            property: "AnchorPoint",
            key: {
              time: targetTime,
              value: { Vec2: finalAnchor },
              easing,
            },
          });
        } else {
          await applyOp({
            type: "setValue",
            comp: comp.id,
            layer: layer.id,
            property: "AnchorPoint",
            value: { Vec2: finalAnchor },
          });
        }
      } else if (dragMode === "motionTangent" && activeTangent) {
        const curEasing = getLayerEasing("Position", activeTangent.keyTime);
        let p1: [number, number] = [0.42, 0.0];
        let p2: [number, number] = [0.58, 1.0];
        if (typeof curEasing === "object" && "Bezier" in curEasing) {
          p1 = [...curEasing.Bezier.p1];
          p2 = [...curEasing.Bezier.p2];
        }

        const k0 = motion.keyframes[activeTangent.keyIndex];
        const k1 = motion.keyframes[activeTangent.keyIndex + 1];
        if (k0 && k1) {
          const segDx = k1.x - k0.x;
          const segDy = k1.y - k0.y;

          const newTx = activeTangent.x + tangentDelta[0];
          const newTy = activeTangent.y + tangentDelta[1];

          const u =
            Math.abs(segDx) > 1
              ? (newTx - activeTangent.startX) / segDx
              : activeTangent.type === "p1"
                ? p1[0]
                : p2[0];
          const v =
            Math.abs(segDy) > 1
              ? (newTy - activeTangent.startY) / segDy
              : activeTangent.type === "p1"
                ? p1[1]
                : p2[1];

          const clampedU = Math.min(1, Math.max(0, Math.round(u * 100) / 100));
          const clampedV = Math.min(1.5, Math.max(-0.5, Math.round(v * 100) / 100));

          if (activeTangent.type === "p1") {
            p1 = [clampedU, clampedV];
          } else {
            p2 = [clampedU, clampedV];
          }

          await applyOp({
            type: "setEasing",
            comp: comp.id,
            layer: layer.id,
            property: "Position",
            time: activeTangent.keyTime,
            easing: {
              Bezier: { p1, p2 },
            },
          });
        }
      }
    } finally {
      resetGesture();
    }
  }

  function resetGesture() {
    window.removeEventListener("pointermove", onPointerMove);
    window.removeEventListener("pointerup", onPointerUp);
    document.body.style.cursor = "";
    dragging = false;
    dragMode = null;
    activeHandle = null;
    activeTangent = null;
    dragKeyTime = null;
    currentPreviewPos = null;
    currentPreviewScale = null;
    currentPreviewRot = null;
    currentPreviewAnchor = null;
    baseGeom = null;
    tangentDelta = [0, 0];
    editor.drag = null;
  }

  $effect(() => {
    return () => {
      resetGesture();
    };
  });
</script>

<main
  aria-label="Composition Viewport"
  class="relative grid min-h-0 place-items-center overflow-hidden bg-[var(--bg-base)] p-4 select-none"
  onpointercancel={resetGesture}
>
  {#if comp}
    <div class="relative max-h-full max-w-full">
      <!-- High-DPI Composition Preview Canvas -->
      <canvas
        bind:this={canvas}
        class="max-h-[calc(100vh-330px)] max-w-full rounded-lg shadow-2xl ring-1 ring-[var(--border)] {dragging &&
        dragMode === 'move'
          ? 'cursor-grabbing'
          : 'cursor-grab'}"
        style="aspect-ratio: {comp.width} / {comp.height}; image-rendering: auto;"
        onpointerdown={onCanvasDown}
      ></canvas>

      <!-- SVG Overlay for Crisp Vector Handles, Outlines & Motion Path -->
      <svg
        bind:this={svgOverlay}
        class="pointer-events-none absolute inset-0 h-full w-full overflow-visible"
        viewBox="0 0 {comp.width} {comp.height}"
        preserveAspectRatio="none"
      >
        <!-- Motion Path Trajectory, Tangents & Keyframe Handles -->
        {#if motion.path.length > 1}
          <polyline
            points={motion.path.map((p) => `${p.x},${p.y}`).join(" ")}
            fill="none"
            stroke="var(--accent)"
            stroke-width="2"
            stroke-dasharray="4,4"
            opacity="0.8"
          />

          <!-- Motion Path Curve Tangent Handles & Stalk Lines -->
          {#each motion.tangents as t (t.id)}
            {@const isDrag = activeTangent?.id === t.id}
            {@const tx = isDrag ? t.x + tangentDelta[0] : t.x}
            {@const ty = isDrag ? t.y + tangentDelta[1] : t.y}

            <!-- Tangent Stalk Line -->
            <line
              x1={t.startX}
              y1={t.startY}
              x2={tx}
              y2={ty}
              stroke={t.type === "p1" ? "var(--danger)" : "var(--accent)"}
              stroke-width="1.5"
              stroke-dasharray="2,2"
              opacity="0.85"
            />

            <!-- Tangent Control Point Handle Circle -->
            <circle
              role="button"
              tabindex="0"
              aria-label="Motion curve tangent {t.id}"
              cx={tx}
              cy={ty}
              r="4.5"
              fill={t.type === "p1" ? "var(--danger)" : "var(--accent)"}
              stroke="#fff"
              stroke-width="1.5"
              class="pointer-events-auto cursor-grab hover:brightness-125"
              onpointerdown={(e) => onMotionTangentDown(e, t)}
            />
          {/each}

          {#each motion.keyframes as k (k.time)}
            <!-- Keyframe diamond on motion path -->
            <g
              role="button"
              tabindex="0"
              aria-label="Motion path keyframe at {timeToTimecode(k.time, comp.fps)}"
              class="pointer-events-auto cursor-pointer"
              transform="translate({k.x},{k.y})"
              onpointerdown={(e) => onMotionKeyframeDown(e, k.time)}
            >
              <rect
                x="-5"
                y="-5"
                width="10"
                height="10"
                transform="rotate(45)"
                fill={Math.abs(editor.currentTime - k.time) <= ticksPerFrame(comp.fps) / 2
                  ? "var(--danger)"
                  : "var(--accent)"}
                stroke="#fff"
                stroke-width="1.5"
              />
            </g>
          {/each}
        {/if}

        <!-- Selected Layer Bounding Box & Handles -->
        {#if geom}
          <!-- Bounding Polygon Outline -->
          <polygon
            points={geom.corners.map((p) => `${p[0]},${p[1]}`).join(" ")}
            fill="rgba(107, 138, 253, 0.05)"
            stroke="var(--accent)"
            stroke-width="1.5"
            stroke-dasharray="4,3"
          />

          <!-- Rotation Stalk Line -->
          {@const rotHandle = handles.find((h) => h.id === "rot")}
          {@const nHandle = handles.find((h) => h.id === "n")}
          {#if rotHandle && nHandle}
            <line
              x1={nHandle.x}
              y1={nHandle.y}
              x2={rotHandle.x}
              y2={rotHandle.y}
              stroke="var(--accent)"
              stroke-width="1.5"
            />
          {/if}

          <!-- Handles: Scaling, Rotation, Anchor Pivot -->
          <g class={dragging ? "pointer-events-none" : ""}>
            {#each handles as h (h.id)}
              {#if h.id === "rot"}
                <!-- Rotation Handle Circle -->
                <circle
                  role="button"
                  tabindex="0"
                  aria-label="Rotate layer handle"
                  cx={h.x}
                  cy={h.y}
                  r="5.5"
                  fill="var(--accent)"
                  stroke="#fff"
                  stroke-width="1.5"
                  class="pointer-events-auto cursor-grab hover:fill-white hover:stroke-[var(--accent)]"
                  onpointerdown={onRotateHandleDown}
                />
              {:else if h.id === "anchor"}
                <!-- Anchor Point Pivot Crosshair -->
                <g
                  role="button"
                  tabindex="0"
                  aria-label="Anchor point pivot handle"
                  class="pointer-events-auto cursor-crosshair hover:opacity-80"
                  transform="translate({h.x},{h.y})"
                  onpointerdown={onAnchorHandleDown}
                >
                  <circle
                    cx="0"
                    cy="0"
                    r="5"
                    fill="none"
                    stroke="var(--danger)"
                    stroke-width="1.5"
                  />
                  <line x1="-7" y1="0" x2="7" y2="0" stroke="var(--danger)" stroke-width="1.5" />
                  <line x1="0" y1="-7" x2="0" y2="7" stroke="var(--danger)" stroke-width="1.5" />
                </g>
              {:else}
                <!-- 8 Scaling Corner/Edge Handle Rectangles -->
                <rect
                  role="button"
                  tabindex="0"
                  aria-label="Scale handle {h.id}"
                  x={h.x - 4}
                  y={h.y - 4}
                  width="8"
                  height="8"
                  fill="#fff"
                  stroke="var(--accent)"
                  stroke-width="1.5"
                  class="pointer-events-auto hover:fill-[var(--accent)] hover:stroke-white"
                  style="cursor: {h.cursor};"
                  onpointerdown={(e) => onScaleHandleDown(e, h.id, h.cursor)}
                />
              {/if}
            {/each}
          </g>
        {/if}
      </svg>

      <!-- Selection & Shortcuts Helper Badge -->
      {#if editor.selected !== null && layer}
        <div
          class="absolute -top-6 left-0 flex items-center gap-2 text-[11px] text-[var(--text-dim)]"
        >
          <span class="font-medium text-[var(--text)]">{layer.name}</span>
          <span>· Drag handles to scale/rotate · K to keyframe · Ctrl+Z to undo</span>
        </div>
      {/if}
    </div>
  {/if}
</main>
