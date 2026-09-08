<script lang="ts">
  /** The export runner: a tiny character sprinting along the export bar.
   * Purely a spectator — it reads telemetry and never touches the export.
   * Position = percent, leg speed = measured fps, mood = ETA trend, and it
   * trips if the export is canceled or throws confetti at 100%. */
  import type { ExportProgress } from "../store.svelte";

  let { progress }: { progress: ExportProgress } = $props();

  const W = 520,
    H = 84,
    GROUND = 62;
  let canvas = $state<HTMLCanvasElement | null>(null);
  let raf = 0,
    lastT = 0;
  let xSmoothed = 0,
    phase = 0,
    finished = false,
    confetti: { x: number; y: number; vx: number; vy: number; c: string; t: number }[] = [];
  let etaSamples: number[] = [];
  const stars = Array.from({ length: 10 }, (_, i) => ({ at: (i + 1) * 10, popped: false, t: 0 }));

  $effect(() => {
    const p = progress;
    if (!p) return;
    if (p.framesDone >= p.totalFrames && p.stage !== 1 && !finished && !p.canceled) {
      finished = true;
      burst();
    }
    if (p.canceled) finished = false;
  });

  $effect(() => {
    const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (reduce) {
      // Static frame, redrawn as progress moves.
      const p = progress;
      if (p) drawStatic(p);
      return;
    }
    lastT = performance.now();
    const loop = (t: number) => {
      const dt = Math.min(0.05, (t - lastT) / 1000);
      lastT = t;
      draw(progress, dt);
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  });

  function burst() {
    const colors = ["#9dc37f", "#e8a04c", "#6db3d9", "#e26d5a", "#e8d26d"];
    confetti = Array.from({ length: 70 }, () => ({
      x: W * 0.5 + (Math.random() - 0.5) * 120,
      y: GROUND - 30,
      vx: (Math.random() - 0.5) * 190,
      vy: -60 - Math.random() * 150,
      c: colors[Math.floor(Math.random() * colors.length)],
      t: 0.9 + Math.random() * 0.7,
    }));
  }

  function drawStatic(p: ExportProgress) {
    const ctx = canvas?.getContext("2d");
    if (!ctx) return;
    paint(ctx, p, 0, 0);
  }

  function draw(p: ExportProgress, dt: number) {
    const ctx = canvas?.getContext("2d");
    if (!ctx) return;
    const target = (Math.min(100, p.percent) / 100) * (W - 46) + 8;
    xSmoothed += (target - xSmoothed) * Math.min(1, dt * 9);
    phase += dt * (3 + Math.min(p.fps, 30) * 0.9);
    for (const star of stars) {
      if (!star.popped && xSmoothed >= (star.at / 100) * (W - 46) + 8) {
        star.popped = true;
        star.t = 0.45;
      }
      if (star.popped && star.t > 0) star.t = Math.max(0, star.t - dt);
    }
    etaSamples.push(p.etaSec);
    if (etaSamples.length > 6) etaSamples.shift();
    paint(ctx, p, dt, 1);
  }

  function paint(ctx: CanvasRenderingContext2D, p: ExportProgress, _dt: number, live: number) {
    // Canvas is 2× for retina; draw in logical coordinates.
    ctx.setTransform(2, 0, 0, 2, 0, 0);
    const x = live ? xSmoothed : (Math.min(100, p.percent) / 100) * (W - 46) + 8;
    const canceled = p.canceled;
    const done = p.framesDone >= p.totalFrames && p.stage !== 1;
    ctx.clearRect(0, 0, W, H);
    // Sky strip + starscape milestones.
    ctx.fillStyle = "#1a1c19";
    ctx.fillRect(0, 0, W, H);
    for (const star of stars) {
      const sx = (star.at / 100) * (W - 46) + 8;
      ctx.save();
      if (star.popped) {
        if (star.t > 0) {
          const k = 1 - star.t / 0.45;
          ctx.globalAlpha = star.t / 0.45;
          ctx.strokeStyle = "#e8d26d";
          ctx.lineWidth = 2;
          ctx.beginPath();
          ctx.arc(sx, GROUND - 26, 6 + k * 16, 0, Math.PI * 2);
          ctx.stroke();
        }
        ctx.globalAlpha = 0.25;
      }
      drawStar(ctx, sx, GROUND - 26, 5, "#e8d26d");
      ctx.restore();
    }
    // Ground with moving hatch marks.
    ctx.strokeStyle = "#2c2f2a";
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.moveTo(4, GROUND + 6);
    ctx.lineTo(W - 4, GROUND + 6);
    ctx.stroke();
    const hatchSpeed = Math.min(p.fps, 24) * 14;
    ctx.strokeStyle = "#232622";
    ctx.lineWidth = 1;
    const shift = (phase * hatchSpeed) % 18;
    for (let hx = -18 + (live ? shift : 0); hx < W; hx += 18) {
      ctx.beginPath();
      ctx.moveTo(hx, GROUND + 6);
      ctx.lineTo(hx - 7, GROUND + 14);
      ctx.stroke();
    }
    // Speed lines when the export really moves.
    if (live && p.fps >= 8 && !canceled) {
      ctx.strokeStyle = "rgba(157,195,127,0.35)";
      ctx.lineWidth = 2;
      for (let i = 0; i < 3; i++) {
        const ly = GROUND - 18 - i * 12;
        const lx = x - 26 - i * 12 - (phase * 26) % 22;
        ctx.beginPath();
        ctx.moveTo(lx, ly);
        ctx.lineTo(lx - 16, ly);
        ctx.stroke();
      }
    }
    // ETA trend mood: rising ETA → sweat drops.
    const trend =
      etaSamples.length >= 4 && live
        ? etaSamples[etaSamples.length - 1] / Math.max(0.01, etaSamples[0])
        : 1;
    // The runner.
    ctx.save();
    ctx.translate(x, GROUND);
    if (canceled) {
      ctx.rotate(-1.25);
      ctx.translate(-4, -4);
    }
    // Legs.
    const swing = canceled || done ? 0 : Math.sin(phase * 6) * 7;
    ctx.strokeStyle = "#d8d3c3";
    ctx.lineWidth = 3;
    ctx.lineCap = "round";
    ctx.beginPath();
    ctx.moveTo(0, -14);
    ctx.lineTo(swing, 0);
    ctx.moveTo(0, -14);
    ctx.lineTo(-swing, 0);
    ctx.stroke();
    // Body.
    ctx.fillStyle = "#9dc37f";
    ctx.beginPath();
    ctx.roundRect(-6, -30, 12, 17, 6);
    ctx.fill();
    // Arms.
    ctx.strokeStyle = "#9dc37f";
    ctx.lineWidth = 3;
    if (done) {
      // Trophy held high.
      ctx.beginPath();
      ctx.moveTo(0, -27);
      ctx.lineTo(7, -40);
      ctx.stroke();
      ctx.fillStyle = "#e8d26d";
      ctx.beginPath();
      ctx.arc(8, -43, 3.4, 0, Math.PI * 2);
      ctx.fill();
    } else {
      const arm = canceled ? 5 : -Math.sin(phase * 6) * 6;
      ctx.beginPath();
      ctx.moveTo(0, -27);
      ctx.lineTo(arm, -20);
      ctx.moveTo(0, -27);
      ctx.lineTo(-arm, -20);
      ctx.stroke();
    }
    // Head + face.
    ctx.fillStyle = "#e8d5b5";
    ctx.beginPath();
    ctx.arc(0, -37, 6.5, 0, Math.PI * 2);
    ctx.fill();
    if (p.fps >= 10 && !canceled && !done) {
      // Sunglasses for fast exports.
      ctx.fillStyle = "#222";
      ctx.fillRect(-6, -39.5, 12, 3.4);
    } else {
      ctx.fillStyle = "#222";
      ctx.beginPath();
      ctx.arc(-2.2, -38, 1, 0, Math.PI * 2);
      ctx.arc(2.2, -38, 1, 0, Math.PI * 2);
      ctx.fill();
    }
    if (canceled) {
      ctx.strokeStyle = "#222";
      ctx.lineWidth = 1.2;
      ctx.beginPath();
      ctx.arc(0, -34, 2.4, Math.PI, 0);
      ctx.stroke();
      // Dizzy stars.
      for (let i = 0; i < 3; i++) {
        const a = (phase * 2 + (i * Math.PI * 2) / 3) % (Math.PI * 2);
        drawStar(ctx, Math.cos(a) * 9, -46 + Math.sin(a) * 3, 2.4, "#e8d26d");
      }
    } else {
      ctx.strokeStyle = "#222";
      ctx.lineWidth = 1.2;
      ctx.beginPath();
      ctx.arc(0, -35.4, 2.6, 0, Math.PI);
      ctx.stroke();
    }
    ctx.restore();
    // Sweat when the ETA is creeping up.
    if (live && trend > 1.08 && !canceled && !done) {
      ctx.fillStyle = "rgba(109,179,217,0.9)";
      for (let i = 0; i < 2; i++) {
        const sy = GROUND - 44 + ((phase * 30 + i * 9) % 16);
        ctx.beginPath();
        ctx.ellipse(x + 9, sy, 1.7, 2.6, 0, 0, Math.PI * 2);
        ctx.fill();
      }
    }
    // Confetti + banner on completion.
    if (live && finished) {
      for (const c of confetti) {
        c.x += c.vx * _dt;
        c.y += c.vy * _dt;
        c.vy += 420 * _dt;
        c.t -= _dt;
        if (c.t > 0) {
          ctx.fillStyle = c.c;
          ctx.globalAlpha = Math.min(1, c.t);
          ctx.fillRect(c.x, c.y, 4, 6);
          ctx.globalAlpha = 1;
        }
      }
      confetti = confetti.filter((c) => c.t > 0);
      ctx.fillStyle = "#d5e3c3";
      ctx.font = "bold 13px ui-sans-serif, system-ui";
      ctx.textAlign = "center";
      ctx.fillText(done && !canceled ? "EXPORTED!" : "", W / 2, 18);
    }
    if (live && p.stage === 2 && !canceled) {
      ctx.fillStyle = "#8b9284";
      ctx.font = "11px ui-sans-serif, system-ui";
      ctx.textAlign = "center";
      ctx.fillText("packing the file…", W / 2, 18);
    }
    // Star score.
    const collected = stars.filter((s) => s.popped).length;
    ctx.fillStyle = "#e8d26d";
    ctx.font = "11px ui-sans-serif, system-ui";
    ctx.textAlign = "right";
    ctx.fillText(`★ ${collected}/10`, W - 10, 16);
  }

  function drawStar(ctx: CanvasRenderingContext2D, cx: number, cy: number, r: number, color: string) {
    ctx.fillStyle = color;
    ctx.beginPath();
    for (let i = 0; i < 10; i++) {
      const rad = i % 2 === 0 ? r : r * 0.45;
      const a = (i / 10) * Math.PI * 2 - Math.PI / 2;
      const px = cx + Math.cos(a) * rad,
        py = cy + Math.sin(a) * rad;
      i === 0 ? ctx.moveTo(px, py) : ctx.lineTo(px, py);
    }
    ctx.closePath();
    ctx.fill();
  }
</script>

<canvas
  bind:this={canvas}
  width={W * 2}
  height={H * 2}
  style={`width:100%;height:${H}px;display:block;border-radius:10px;`}
  aria-label="Export runner: a tiny character sprinting along the export progress bar"
></canvas>
