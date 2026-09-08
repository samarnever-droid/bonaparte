<script lang="ts">
  /** Circular HSV picker: hue ring + saturation/value square. Replaces the
   * native browser color dialog — zero browser chrome, all Bonaparte. */
  import { fromHex } from "../color";
  let {
    hex,
    alpha,
    linear,
    onchange,
    onclose,
  }: {
    hex: string;
    alpha: number;
    linear: boolean;
    onchange: (hexColor: string, alphaValue: number) => void;
    onclose: () => void;
  } = $props();

  const SIZE = 176;
  const RING = 14; // hue ring thickness
  const INNER = (SIZE / 2 - RING - 4) * 2; // SV square side

  let canvas = $state<HTMLCanvasElement | null>(null);
  let hue = $state(0);
  let sat = $state(1);
  let val = $state(1);
  let dragging: "ring" | "square" | null = null;

  function hsvToHex(hDeg: number, s: number, v: number): string {
    const c = v * s;
    const h = ((hDeg % 360) + 360) % 360 / 60;
    const x = c * (1 - Math.abs((h % 2) - 1));
    let rgb: [number, number, number] = [0, 0, 0];
    if (h < 1) rgb = [c, x, 0];
    else if (h < 2) rgb = [x, c, 0];
    else if (h < 3) rgb = [0, c, x];
    else if (h < 4) rgb = [0, x, c];
    else if (h < 5) rgb = [x, 0, c];
    else rgb = [c, 0, x];
    const m = v - c;
    const to255 = (f: number) =>
      Math.round((f + m) * 255)
        .toString(16)
        .padStart(2, "0");
    return `#${to255(rgb[0])}${to255(rgb[1])}${to255(rgb[2])}`;
  }

  function seedFromHex(h: string) {
    const parsed = h.replace("#", "");
    const r = parseInt(parsed.slice(0, 2), 16) / 255;
    const g = parseInt(parsed.slice(2, 4), 16) / 255;
    const b = parseInt(parsed.slice(4, 6), 16) / 255;
    const max = Math.max(r, g, b);
    const min = Math.min(r, g, b);
    val = max;
    sat = max <= 0 ? 0 : (max - min) / max;
    if (max - min === 0) {
      hue = 0;
      return;
    }
    const d = max - min;
    if (max === r) hue = 60 * (((g - b) / d) % 6);
    else if (max === g) hue = 60 * ((b - r) / d + 2);
    else hue = 60 * ((r - g) / d + 4);
  }

  $effect(() => {
    seedFromHex(hex);
  });

  function draw() {
    const ctx = canvas?.getContext("2d");
    if (!ctx) return;
    ctx.clearRect(0, 0, SIZE, SIZE);
    const cx = SIZE / 2;
    const cy = SIZE / 2;
    // Hue ring: 360 thin wedges.
    for (let a = 0; a < 360; a += 1) {
      ctx.beginPath();
      ctx.moveTo(cx, cy);
      const rad = ((a - 90) * Math.PI) / 180;
      ctx.arc(cx, cy, SIZE / 2, rad, rad + Math.PI / 180);
      ctx.closePath();
      ctx.fillStyle = hsvToHex(a, 1, 1);
      ctx.fill();
    }
    // Punch the center out.
    ctx.globalCompositeOperation = "destination-out";
    ctx.beginPath();
    ctx.arc(cx, cy, SIZE / 2 - RING, 0, Math.PI * 2);
    ctx.fill();
    ctx.globalCompositeOperation = "source-over";
    // SV square.
    const side = INNER;
    const x0 = cx - side / 2;
    const y0 = cy - side / 2;
    const base = ctx.createLinearGradient(x0, 0, x0 + side, 0);
    base.addColorStop(0, "#ffffff");
    base.addColorStop(1, hsvToHex(hue, 1, 1));
    ctx.fillStyle = base;
    ctx.fillRect(x0, y0, side, side);
    const shade = ctx.createLinearGradient(0, y0, 0, y0 + side);
    shade.addColorStop(0, "rgba(0,0,0,0)");
    shade.addColorStop(1, "#000000");
    ctx.fillStyle = shade;
    ctx.fillRect(x0, y0, side, side);
    // Cursor: ring dot + square dot.
    const ringR = SIZE / 2 - RING / 2;
    const hx = cx + ringR * Math.cos(((hue - 90) * Math.PI) / 180);
    const hy = cy + ringR * Math.sin(((hue - 90) * Math.PI) / 180);
    ctx.beginPath();
    ctx.arc(hx, hy, 6, 0, Math.PI * 2);
    ctx.strokeStyle = "#fff";
    ctx.lineWidth = 2;
    ctx.stroke();
    ctx.beginPath();
    ctx.arc(hx, hy, 7, 0, Math.PI * 2);
    ctx.strokeStyle = "rgba(0,0,0,.6)";
    ctx.lineWidth = 1;
    ctx.stroke();
    const sx = x0 + sat * side;
    const sy = y0 + (1 - val) * side;
    ctx.beginPath();
    ctx.arc(sx, sy, 5, 0, Math.PI * 2);
    ctx.strokeStyle = "#fff";
    ctx.lineWidth = 2;
    ctx.stroke();
    ctx.beginPath();
    ctx.arc(sx, sy, 6, 0, Math.PI * 2);
    ctx.strokeStyle = "rgba(0,0,0,.6)";
    ctx.lineWidth = 1;
    ctx.stroke();
  }

  $effect(() => {
    hue;
    sat;
    val;
    draw();
  });
  // eslint-disable-next-line svelte/no-ignored-unreachable-code
  draw();

  function emit() {
    onchange(hsvToHex(hue, sat, val), alpha);
  }

  function pointerPos(e: PointerEvent) {
    const box = canvas!.getBoundingClientRect();
    return [e.clientX - box.left, e.clientY - box.top];
  }

  function onPointerDown(e: PointerEvent) {
    const [x, y] = pointerPos(e);
    const cx = SIZE / 2;
    const cy = SIZE / 2;
    const dist = Math.hypot(x - cx, y - cy);
    const side = INNER;
    // The square's corners reach past the ring's inner edge — the square wins
    // inside its own bounds, the ring gets everything beyond them.
    const inSquare = Math.abs(x - cx) <= side / 2 && Math.abs(y - cy) <= side / 2;
    if (inSquare) dragging = "square";
    else if (dist >= SIZE / 2 - RING - 2) dragging = "ring";
    else return;
    canvas!.setPointerCapture(e.pointerId);
    onPointerMove(e);
  }

  function onPointerMove(e: PointerEvent) {
    if (!dragging) return;
    const [x, y] = pointerPos(e);
    const cx = SIZE / 2;
    const cy = SIZE / 2;
    if (dragging === "ring") {
      hue = ((Math.atan2(y - cy, x - cx) * 180) / Math.PI + 90 + 360) % 360;
    } else {
      const side = INNER;
      sat = Math.min(1, Math.max(0, (x - (cx - side / 2)) / side));
      val = Math.min(1, Math.max(0, 1 - (y - (cy - side / 2)) / side));
    }
    emit();
  }

  function onPointerUp() {
    dragging = null;
  }
</script>

<div class="wheel-pop" role="dialog" aria-label="Color picker">
  <canvas
    bind:this={canvas}
    width={SIZE}
    height={SIZE}
    aria-label="Circular color picker"
    onpointerdown={onPointerDown}
    onpointermove={onPointerMove}
    onpointerup={onPointerUp}
    onpointercancel={onPointerUp}
  ></canvas>
  <div class="wheel-row">
    <span>Alpha</span>
    <input
      type="range"
      min="0"
      max="100"
      value={Math.round(alpha * 100)}
      aria-label="Alpha percent"
      oninput={(e) => onchange(hsvToHex(hue, sat, val), Number(e.currentTarget.value) / 100)}
    />
    <button class="wheel-close" onclick={onclose} aria-label="Close color picker">Done</button>
  </div>
</div>

<style>
  .wheel-pop {
    position: absolute;
    z-index: 60;
    top: calc(100% + 6px);
    left: 0;
    background: #161715;
    border: 1px solid var(--border, #333);
    border-radius: 8px;
    padding: 8px;
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.5);
  }
  canvas {
    display: block;
    touch-action: none;
    cursor: crosshair;
  }
  .wheel-row {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 6px;
    color: var(--text-secondary, #aaa);
    font-size: 10px;
  }
  .wheel-row input[type="range"] {
    flex: 1;
    accent-color: #9aa294;
  }
  .wheel-close {
    border: 1px solid var(--border, #333);
    background: none;
    color: var(--text-secondary, #aaa);
    border-radius: 4px;
    padding: 2px 8px;
    font-size: 10px;
    cursor: pointer;
  }
</style>
