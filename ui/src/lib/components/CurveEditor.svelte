<script lang="ts">
  import { onDestroy } from "svelte";
  import { applyOp, editor } from "../store.svelte";
  import { timeToSecs, type Comp, type Layer, type Property, type Easing } from "../model";
  import Icon from "./Icon.svelte";
  let {
    comp,
    layer,
    property,
    onClose,
  }: { comp: Comp; layer: Layer; property: Property; onClose?: () => void } = $props();
  const keys = $derived(layer.tracks[property]?.keys ?? []);
  let activeTime = $state<number | null>(null),
    p1 = $state<[number, number]>([0.42, 0]),
    p2 = $state<[number, number]>([0.58, 1]),
    linear = $state(false);
  const activeKey = $derived(keys.find((k) => k.time === activeTime) ?? keys[0]);
  const index = $derived(activeKey ? keys.indexOf(activeKey) : -1);
  const nextKey = $derived(keys[index + 1]);
  let lastSelection = "";
  $effect(() => {
    const token = `${layer.id}:${property}`;
    if (token !== lastSelection) {
      activeTime = null;
      lastSelection = token;
    }
  });
  $effect(() => {
    if (activeKey) {
      if (activeKey.easing === "Linear") {
        linear = true;
        p1 = [0, 0];
        p2 = [1, 1];
      } else {
        linear = false;
        p1 = [...activeKey.easing.Bezier.p1];
        p2 = [...activeKey.easing.Bezier.p2];
      }
    }
  });
  const W = 340,
    H = 155,
    P = 26,
    PW = W - P * 2,
    PH = H - 25;
  const xy = (u: number, v: number): [number, number] => [P + u * PW, 8 + ((1.4 - v) / 1.8) * PH];
  const start = $derived(xy(0, 0)),
    end = $derived(xy(1, 1)),
    a = $derived(xy(p1[0], p1[1])),
    b = $derived(xy(p2[0], p2[1]));
  const curve = $derived(linear ? `M${start}L${end}` : `M${start}C${a} ${b} ${end}`);
  const presets: { name: string; easing: Easing }[] = [
    { name: "Linear", easing: "Linear" },
    { name: "Ease in", easing: { Bezier: { p1: [0.42, 0], p2: [1, 1] } } },
    { name: "Ease out", easing: { Bezier: { p1: [0, 0], p2: [0.58, 1] } } },
    { name: "Smooth", easing: { Bezier: { p1: [0.42, 0], p2: [0.58, 1] } } },
    { name: "Overshoot", easing: { Bezier: { p1: [0.2, 1.4], p2: [0.4, 1] } } },
  ];
  let svg = $state<SVGSVGElement | null>(null),
    dragging: "p1" | "p2" | null = null;
  async function commit(easing: Easing) {
    if (activeKey && nextKey && !layer.locked)
      await applyOp({
        type: "setEasing",
        comp: comp.id,
        layer: layer.id,
        property,
        time: activeKey.time,
        easing,
      });
  }
  function startDrag(e: PointerEvent, handle: "p1" | "p2") {
    if (layer.locked || !nextKey) return;
    e.preventDefault();
    dragging = handle;
    linear = false;
    window.addEventListener("pointermove", drag);
    window.addEventListener("pointerup", endDrag);
  }
  function drag(e: PointerEvent) {
    if (!dragging || !svg) return;
    const r = svg.getBoundingClientRect();
    const x = ((e.clientX - r.left) / r.width) * W,
      y = ((e.clientY - r.top) / r.height) * H;
    const point: [number, number] = [
      Math.max(0, Math.min(1, (x - P) / PW)),
      Math.max(-0.4, Math.min(1.4, 1.4 - ((y - 8) / PH) * 1.8)),
    ];
    if (dragging === "p1") p1 = point;
    else p2 = point;
  }
  function cleanup() {
    window.removeEventListener("pointermove", drag);
    window.removeEventListener("pointerup", endDrag);
  }
  async function endDrag() {
    if (!dragging) return;
    dragging = null;
    cleanup();
    await commit({ Bezier: { p1: [...p1], p2: [...p2] } });
  }
  onDestroy(cleanup);
  function handleKey(e: KeyboardEvent, handle: "p1" | "p2") {
    const delta = e.shiftKey ? 0.1 : 0.02;
    const p = handle === "p1" ? [...p1] : [...p2];
    if (e.key === "ArrowLeft") p[0] -= delta;
    else if (e.key === "ArrowRight") p[0] += delta;
    else if (e.key === "ArrowUp") p[1] += delta;
    else if (e.key === "ArrowDown") p[1] -= delta;
    else return;
    e.preventDefault();
    e.stopPropagation();
    const next: [number, number] = [
      Math.max(0, Math.min(1, p[0])),
      Math.max(-0.4, Math.min(1.4, p[1])),
    ];
    if (handle === "p1") p1 = next;
    else p2 = next;
    linear = false;
    void commit({ Bezier: { p1: [...p1], p2: [...p2] } });
  }
</script>

<div class="curve-editor">
  <div class="curve-heading">
    <Icon name="graph" size={12} /><span>Easing curve</span><span class="curve-context"
      >{property} · normalized segment</span
    ><span class="spacer"></span><button
      class="icon-button small"
      title="Close graph editor"
      aria-label="Close graph editor"
      onclick={onClose}><Icon name="x" size={12} /></button
    >
  </div>
  {#if keys.length < 2}
    <div class="curve-empty">
      <Icon name="keyframe" size={24} /><strong>Give your animation a beginning and an end.</strong
      ><span
        >Add at least two {property.toLowerCase()} keyframes using the inspector diamonds, or apply a
        motion preset.</span
      >
    </div>
  {:else}
    <div class="curve-content">
      <svg
        bind:this={svg}
        viewBox={`0 0 ${W} ${H}`}
        class="curve-plot"
        role="img"
        aria-label="Cubic Bézier easing curve"
      >
        {#each [0, 0.25, 0.5, 0.75, 1] as u}<line
            x1={xy(u, 0)[0]}
            y1={xy(u, 0)[1]}
            x2={xy(u, 1)[0]}
            y2={xy(u, 1)[1]}
            stroke="#40463b"
            stroke-width=".5"
          />{/each}
        {#each [0, 0.5, 1] as v}<line
            x1={xy(0, v)[0]}
            y1={xy(0, v)[1]}
            x2={xy(1, v)[0]}
            y2={xy(1, v)[1]}
            stroke="#4b5543"
            stroke-width=".5"
          /><text x="11" y={xy(0, v)[1] + 2}>{v}</text>{/each}
        <path d={`M${start}L${end}`} stroke="#6b7961" stroke-width=".6" stroke-dasharray="3 4" />
        <path d={curve} fill="none" stroke="#c0e6aa" stroke-width="2" />
        {#if !linear && nextKey}
          <path d={`M${start}L${a} M${end}L${b}`} fill="none" stroke="#a6b48d" stroke-width=".8" />
          <circle
            cx={a[0]}
            cy={a[1]}
            r="4"
            fill="#d5c89b"
            stroke="#1e2519"
            stroke-width="1.5"
            role="button"
            tabindex="0"
            aria-label="First easing handle"
            class="handle"
            onpointerdown={(e) => startDrag(e, "p1")}
            onkeydown={(e) => handleKey(e, "p1")}
          />
          <circle
            cx={b[0]}
            cy={b[1]}
            r="4"
            fill="#c0e6aa"
            stroke="#1e2519"
            stroke-width="1.5"
            role="button"
            tabindex="0"
            aria-label="Second easing handle"
            class="handle"
            onpointerdown={(e) => startDrag(e, "p2")}
            onkeydown={(e) => handleKey(e, "p2")}
          />
        {/if}
        <rect
          x={start[0] - 2.5}
          y={start[1] - 2.5}
          width="5"
          height="5"
          fill="#dae5ce"
          transform={`rotate(45 ${start[0]} ${start[1]})`}
        /><rect
          x={end[0] - 2.5}
          y={end[1] - 2.5}
          width="5"
          height="5"
          fill="#dae5ce"
          transform={`rotate(45 ${end[0]} ${end[1]})`}
        />
        <text x={P} y={H - 1}>0</text><text x={W / 2} y={H - 1} text-anchor="middle">TIME →</text
        ><text x={W - P} y={H - 1}>1</text>
      </svg>
      <div class="curve-controls">
        <div class="segment">
          <label for="curve-key">Outgoing keyframe</label><select
            id="curve-key"
            class="field"
            value={activeKey?.time}
            onchange={(e) => (activeTime = Number(e.currentTarget.value))}
            >{#each keys as key, i}<option value={key.time}
                >{String(i + 1).padStart(2, "0")} · {timeToSecs(key.time).toFixed(2)}s{!keys[i + 1]
                  ? " (last)"
                  : ""}</option
              >{/each}</select
          >
        </div>
        <div class="ease-presets">
          {#each presets as preset}<button
              class:active={JSON.stringify(activeKey?.easing) === JSON.stringify(preset.easing)}
              disabled={!nextKey || layer.locked}
              onclick={() => void commit(preset.easing)}>{preset.name}</button
            >{/each}
        </div>
        {#if nextKey && !linear}<div class="handle-values">
            {#each ["p1", "p2"] as name}{@const point = name === "p1" ? p1 : p2}
              <div>
                <span>{name.toUpperCase()}</span>{#each [0, 1] as axis}<input
                    type="number"
                    step=".05"
                    aria-label={`${name} ${axis === 0 ? "time" : "progress"}`}
                    min={axis === 0 ? 0 : -0.4}
                    max={axis === 0 ? 1 : 1.4}
                    value={Number(point[axis].toFixed(2))}
                    disabled={layer.locked}
                    onchange={(e) => {
                      const v = Number(e.currentTarget.value);
                      if (name === "p1")
                        p1 = p1.map((n, i) => (i === axis ? v : n)) as [number, number];
                      else p2 = p2.map((n, i) => (i === axis ? v : n)) as [number, number];
                      void commit({ Bezier: { p1: [...p1], p2: [...p2] } });
                    }}
                  />{/each}
              </div>{/each}
          </div>{/if}
        <p>
          {nextKey
            ? `${timeToSecs(nextKey.time - activeKey!.time).toFixed(2)}s segment · drag handles to shape the motion.`
            : "The last keyframe has no outgoing segment."}
        </p>
      </div>
    </div>
  {/if}
</div>

<style>
  .curve-editor {
    max-width: 950px;
    margin: 0 auto;
  }
  .curve-heading {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 10px;
    color: #c1c4bf;
  }
  .curve-context {
    font-size: 8px;
    color: #828580;
    margin-left: 9px;
  }
  .curve-content {
    display: flex;
    align-items: center;
    gap: 20px;
  }
  .curve-plot {
    width: 55%;
    max-width: 450px;
    min-width: 200px;
    flex-shrink: 1;
    height: 155px;
  }
  .curve-plot text {
    fill: #82857f;
    font: 6px monospace;
  }
  .handle {
    cursor: grab;
    touch-action: none;
  }
  .handle:focus {
    outline: none;
    stroke: #fff;
    stroke-width: 2px;
  }
  .curve-controls {
    flex: 1;
    max-width: 345px;
    padding: 5px 0;
  }
  .segment {
    display: flex;
    gap: 12px;
    align-items: center;
    font-size: 8px;
    color: #9fa39c;
  }
  .segment select {
    width: 135px;
    font-size: 9px;
    padding: 4px;
    background: #20221f;
  }
  .ease-presets {
    display: flex;
    gap: 4px;
    margin: 12px 0;
    flex-wrap: wrap;
  }
  .ease-presets button {
    font-size: 8px;
    padding: 5px 7px;
    border: 1px solid #454843;
    border-radius: 3px;
    color: #a5a9a2;
  }
  .ease-presets button.active {
    background: #4e524b;
    color: #daddd7;
    border-color: #82887e;
  }
  .handle-values {
    display: flex;
    gap: 14px;
  }
  .handle-values > div {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .handle-values span {
    font: 7px monospace;
    color: #9b9e97;
    margin-right: 3px;
  }
  .handle-values input {
    width: 37px;
    outline: 0;
    background: #1e201d;
    border: 1px solid #484b45;
    border-radius: 2px;
    font: 9px monospace;
    color: #cbcec7;
    padding: 4px;
  }
  .curve-controls p {
    font-size: 8px;
    line-height: 1.6;
    color: #898d86;
    margin: 10px 0 0;
  }
  .curve-empty {
    min-height: 130px;
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 10px;
    align-items: center;
    text-align: center;
    color: #92968e;
  }
  .curve-empty strong {
    font-size: 11px;
    font-weight: 400;
    color: #bdc1ba;
  }
  .curve-empty span {
    max-width: 450px;
    font-size: 9px;
    line-height: 1.8;
  }
</style>
