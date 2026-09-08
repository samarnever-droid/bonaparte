<script lang="ts">
  /** The export runner: "Gremlin Dash" — a tiny playable endless-runner that
   * rides the export bar. You steer the character (jump the frame gremlins,
   * collect stars) while the Rust runtime does the actual exporting; the game
   * only ever READS telemetry and never touches the export. Progress moves the
   * runner along the track, export fps drives the world speed, and finishing
   * ranks the run. Reduced motion gets a dignified static frame instead. */
  import type { ExportProgress } from "../store.svelte";

  let { progress }: { progress: ExportProgress } = $props();

  const W = 520,
    H = 140,
    GROUND = 112;
  const GRAVITY = 950,
    JUMP_V = -330,
    JUMP_CUT = -130;
  let canvas = $state<HTMLCanvasElement | null>(null);
  let raf = 0,
    lastT = 0;
  let xSmoothed = 0,
    phase = 0,
    hintT = 5;
  let finished = false,
    finishedAt = 0;
  let confetti: { x: number; y: number; vx: number; vy: number; c: string; t: number }[] = [];
  let etaSamples: number[] = [];
  let milestone = 10;

  // Game state.
  type Gremlin = { x: number; w: number; h: number; wob: number };
  type Star = { x: number; y: number; t: number; got: number };
  type Poof = { x: number; y: number; t: number; c: string; txt?: string };
  let y = 0,
    vy = 0,
    jumping = false,
    heldJump = false;
  let coyote = 0,
    jumpBuffer = 0;
  let gremlins: Gremlin[] = [],
    stars: Star[] = [],
    poofs: Poof[] = [];
  let spawnIn = 1.4,
    starIn = 2.4;
  let score = 0,
    streak = 0,
    bestStreak = 0,
    crashT = 0,
    graceT = 0;
  let best: number = 0;
  try {
    best = Number(localStorage.getItem("bonaparte.runnerBest") ?? 0) || 0;
  } catch {
    best = 0;
  }

  $effect(() => {
    const p = progress;
    if (!p) return;
    if (
      p.totalFrames > 0 &&
      p.framesDone >= p.totalFrames &&
      p.stage !== 1 &&
      !finished &&
      !p.canceled
    ) {
      finished = true;
      finishedAt = performance.now();
      if (score > best) {
        best = score;
        try {
          localStorage.setItem("bonaparte.runnerBest", String(best));
        } catch {
          /* private mode — fine */
        }
      }
      burst();
    }
    if (p.canceled) finished = false;
  });

  $effect(() => {
    const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (reduce) {
      const p = progress;
      if (p) drawStatic(p);
      return;
    }
    lastT = performance.now();
    const loop = (t: number) => {
      const dt = Math.min(0.05, (t - lastT) / 1000);
      lastT = t;
      step(progress, dt);
      paint(progress, dt, 1);
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  });

  function focusGame() {
    canvas?.focus();
  }

  function pressJump() {
    jumpBuffer = 0.11;
    heldJump = true;
    focusGame();
  }
  function releaseJump() {
    heldJump = false;
    if (jumping && vy < JUMP_CUT) vy = JUMP_CUT;
  }
  function onKey(e: KeyboardEvent) {
    if (e.code === "Space" || e.code === "ArrowUp" || e.code === "KeyW") {
      e.preventDefault();
      e.stopPropagation();
      if (e.repeat) return;
      pressJump();
    }
  }

  function burst() {
    const colors = ["#9dc37f", "#e8a04c", "#6db3d9", "#e26d5a", "#e8d26d"];
    confetti = Array.from({ length: 90 }, () => ({
      x: W * 0.5 + (Math.random() - 0.5) * 160,
      y: GROUND - 30,
      vx: (Math.random() - 0.5) * 210,
      vy: -70 - Math.random() * 170,
      c: colors[Math.floor(Math.random() * colors.length)],
      t: 1 + Math.random() * 0.8,
    }));
  }

  /** World speed tracks the real export fps — a fast render is a fast run. */
  function worldSpeed(p: ExportProgress) {
    return 128 + Math.min(150, Math.max(0, p.fps) * 7);
  }

  function step(p: ExportProgress, dt: number) {
    const target = (Math.min(100, p.percent) / 100) * (W - 46) + 8;
    if (xSmoothed === 0) xSmoothed = target;
    xSmoothed += (target - xSmoothed) * Math.min(1, dt * 9);
    const v = worldSpeed(p);
    phase += dt * (2.2 + Math.min(p.fps, 30) * 0.5);
    if (hintT > 0) hintT -= dt;
    etaSamples.push(p.etaSec);
    if (etaSamples.length > 6) etaSamples.shift();

    const canceled = p.canceled;
    const done = p.totalFrames > 0 && p.framesDone >= p.totalFrames && p.stage !== 1;
    const playing = !canceled && !done;

    // Milestone fireworks every 10%.
    if (Math.min(100, p.percent) >= milestone) {
      milestone += 10;
      poofs.push({
        x: xSmoothed,
        y: GROUND - 34,
        t: 0.55,
        c: "#e8d26d",
      });
    }

    // Runner physics.
    if (playing) {
      coyote = y === 0 ? 0.09 : Math.max(0, coyote - dt);
      jumpBuffer = Math.max(0, jumpBuffer - dt);
      if (jumpBuffer > 0 && coyote > 0 && !jumping) {
        jumping = true;
        coyote = 0;
        jumpBuffer = 0;
        vy = JUMP_V;
      }
      if (jumping || y > 0) {
        vy += GRAVITY * dt;
        y += vy * dt;
        if (y >= 0) {
          if (jumping) poofs.push({ x: xSmoothed - 6, y: GROUND - 2, t: 0.3, c: "#8b9284" });
          y = 0;
          vy = 0;
          jumping = false;
        }
      }
      if (crashT > 0) crashT -= dt;
      if (graceT > 0) graceT -= dt;

      // Spawning — fair gaps even at high speed.
      spawnIn -= dt * (v / 170);
      if (spawnIn <= 0) {
        spawnIn = 0.85 + Math.random() * 0.75;
        gremlins.push({
          x: W + 20,
          w: 13 + Math.random() * 9,
          h: 11 + Math.random() * 8,
          wob: Math.random() * Math.PI * 2,
        });
      }
      starIn -= dt * (v / 170);
      if (starIn <= 0) {
        starIn = 1.9 + Math.random() * 1.3;
        const sy = 26 + Math.random() * 26;
        for (let i = 0; i < 3; i++)
          stars.push({
            x: W + 26 + i * 22,
            y: sy - Math.sin((i / 2) * Math.PI) * 10,
            t: 0,
            got: 0,
          });
      }

      // Move + collide.
      const rx = xSmoothed,
        ry = GROUND + y;
      const runnerBox = { x: rx - 7, y: ry - 40, w: 14, h: 40 };
      for (const g of gremlins) {
        g.x -= v * dt;
        g.wob += dt * 9;
        if (graceT <= 0 && crashT <= 0 && !finished) {
          const gx = g.x - g.w / 2,
            gy = GROUND - g.h;
          if (
            runnerBox.x < gx + g.w &&
            runnerBox.x + runnerBox.w > gx &&
            runnerBox.y < gy + g.h &&
            runnerBox.y + runnerBox.h > gy
          ) {
            crashT = 0.85;
            graceT = 1.4;
            if (streak >= 3)
              poofs.push({ x: rx, y: ry - 52, t: 0.9, c: "#e26d5a", txt: `streak lost` });
            streak = 0;
          } else if (gx + g.w < rx - 8 && !(g as any).scored) {
            (g as any).scored = true;
            streak += 1;
            bestStreak = Math.max(bestStreak, streak);
            score += 5 * (streak >= 5 ? 2 : 1);
            poofs.push({
              x: rx + 4,
              y: ry - 48,
              t: 0.6,
              c: "#9dc37f",
              txt: streak >= 5 ? `+10 ×2` : `+5`,
            });
          }
        }
      }
      gremlins = gremlins.filter((g) => g.x > -30);

      for (const s of stars) {
        s.x -= v * dt;
        s.t += dt;
        if (!s.got) {
          const dx = s.x - rx,
            dy = s.y + GROUND - 26 - (ry - 26);
          if (dx * dx + dy * dy < 20 * 20) {
            s.got = 1;
            score += 2;
            poofs.push({ x: s.x, y: s.y + GROUND - 26, t: 0.45, c: "#e8d26d", txt: "+2" });
          }
        }
      }
      stars = stars.filter((s) => s.x > -20 && (!s.got || s.t < 90));
    }

    for (const pf of poofs) {
      pf.t -= dt;
      pf.y -= 26 * dt;
    }
    poofs = poofs.filter((pf) => pf.t > 0);

    // Test hook: lets e2e specs verify the game is alive without touching pixels.
    (window as any).__gremlinDash = {
      score: () => score,
      streak: () => streak,
      airborne: () => y < -2,
      gremlins: () => gremlins.length,
      stars: () => stars.length,
      jump: () => pressJump(),
      best: () => best,
    };
  }

  function drawStatic(p: ExportProgress) {
    const ctx = canvas?.getContext("2d");
    if (!ctx) return;
    paint(p, 0, 0);
  }

  function rank() {
    if (score >= 220) return { r: "S", name: "DIRECTOR'S CUT" };
    if (score >= 140) return { r: "A", name: "STEADY HAND" };
    if (score >= 80) return { r: "B", name: "ROUGH CUT" };
    return { r: "C", name: "DAILIES" };
  }

  function paint(
    ctxOrP: ExportProgress | CanvasRenderingContext2D,
    dtOrLive: number,
    liveArg?: number,
  ) {
    const ctx = canvas?.getContext("2d");
    if (!ctx) return;
    // Called as paint(p, dt, live) from the loop and paint(p, 0, 0) for static.
    const p = ctxOrP as ExportProgress;
    const _dt = dtOrLive as number;
    const live = (liveArg ??
      (typeof ctxOrP === "object" && ctxOrP !== null && "percent" in ctxOrP ? 0 : 1)) as number;
    ctx.setTransform(2, 0, 0, 2, 0, 0);
    const x = live ? xSmoothed : (Math.min(100, p.percent) / 100) * (W - 46) + 8;
    if (!live) xSmoothed = x;
    const canceled = p.canceled;
    const done = p.totalFrames > 0 && p.framesDone >= p.totalFrames && p.stage !== 1;
    const v = worldSpeed(p);

    // Sky.
    const sky = ctx.createLinearGradient(0, 0, 0, H);
    sky.addColorStop(0, "#14161a");
    sky.addColorStop(1, "#1d201b");
    ctx.fillStyle = sky;
    ctx.fillRect(0, 0, W, H);

    // Moon + twinkles.
    ctx.fillStyle = "#e8e4d2";
    ctx.globalAlpha = 0.85;
    ctx.beginPath();
    ctx.arc(W - 52, 24, 11, 0, Math.PI * 2);
    ctx.fill();
    ctx.globalAlpha = 0.35;
    ctx.fillStyle = "#c9c5b2";
    ctx.beginPath();
    ctx.arc(W - 56, 21, 3, 0, Math.PI * 2);
    ctx.arc(W - 49, 27, 2, 0, Math.PI * 2);
    ctx.fill();
    ctx.globalAlpha = 1;
    for (let i = 0; i < 14; i++) {
      const sx = ((i * 97) % W) + 6,
        sy = 8 + ((i * 53) % 46);
      const tw = 0.25 + 0.2 * Math.sin(phase * 1.7 + i * 2.1);
      ctx.fillStyle = `rgba(216,211,195,${live ? Math.max(0.08, tw) : 0.2})`;
      ctx.fillRect(sx, sy, 1.6, 1.6);
    }

    // Parallax hills.
    ctx.fillStyle = "#20241e";
    ctx.beginPath();
    ctx.moveTo(0, GROUND + 2);
    for (let hx = 0; hx <= W; hx += 8) {
      const hy =
        GROUND -
        22 -
        Math.sin((hx + (live ? phase * 9 : 0)) * 0.011) * 10 -
        Math.sin((hx + (live ? phase * 9 : 0)) * 0.031) * 4;
      ctx.lineTo(hx, hy);
    }
    ctx.lineTo(W, GROUND + 2);
    ctx.closePath();
    ctx.fill();
    ctx.fillStyle = "#262b23";
    ctx.beginPath();
    ctx.moveTo(0, GROUND + 2);
    for (let hx = 0; hx <= W; hx += 8) {
      const hy = GROUND - 10 - Math.sin((hx + (live ? phase * 22 : 40)) * 0.02) * 7;
      ctx.lineTo(hx, hy);
    }
    ctx.lineTo(W, GROUND + 2);
    ctx.closePath();
    ctx.fill();

    // Milestone markers on the ground (10%…90%).
    for (let m = 10; m < 100; m += 10) {
      const mx = (m / 100) * (W - 46) + 8;
      ctx.strokeStyle = "#31352d";
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(mx, GROUND + 2);
      ctx.lineTo(mx, GROUND + 8);
      ctx.stroke();
    }

    // Ground + hatches.
    ctx.strokeStyle = "#2c2f2a";
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.moveTo(4, GROUND + 6);
    ctx.lineTo(W - 4, GROUND + 6);
    ctx.stroke();
    const shift = live ? (phase * Math.min(p.fps, 24) * 12) % 18 : 0;
    ctx.strokeStyle = "#232622";
    ctx.lineWidth = 1;
    for (let hx = -18 + shift; hx < W; hx += 18) {
      ctx.beginPath();
      ctx.moveTo(hx, GROUND + 6);
      ctx.lineTo(hx - 7, GROUND + 14);
      ctx.stroke();
    }

    // Stars (collectibles).
    for (const s of stars) {
      const sy = s.y + GROUND - 26 + (live ? Math.sin(s.t * 4 + s.x * 0.05) * 2.4 : 0);
      ctx.save();
      if (s.got) {
        ctx.globalAlpha = Math.max(0, 1 - (s.t % 90) * 0.02);
      }
      drawStar(ctx, s.x, sy, 4.6, "#e8d26d");
      ctx.restore();
    }

    // Gremlins (dropped-frame imps).
    for (const g of gremlins) {
      const gx = g.x,
        gy = GROUND - g.h / 2 + Math.sin(g.wob) * 1.4;
      ctx.save();
      ctx.translate(gx, gy);
      ctx.fillStyle = "#e26d5a";
      ctx.beginPath();
      const spikes = 7;
      for (let i = 0; i < spikes * 2; i++) {
        const rad = i % 2 === 0 ? g.h / 2 + 2.4 : g.h / 2 - 1.6;
        const a = (i / (spikes * 2)) * Math.PI * 2 + g.wob * 0.2;
        const px = Math.cos(a) * rad * (g.w / g.h),
          py = Math.sin(a) * rad;
        i === 0 ? ctx.moveTo(px, py) : ctx.lineTo(px, py);
      }
      ctx.closePath();
      ctx.fill();
      // Angry eyes.
      ctx.fillStyle = "#1a1c19";
      ctx.fillRect(-g.w * 0.18, -2.4, 2.2, 2.6);
      ctx.fillRect(g.w * 0.06, -2.4, 2.2, 2.6);
      ctx.strokeStyle = "#1a1c19";
      ctx.lineWidth = 1.1;
      ctx.beginPath();
      ctx.moveTo(-g.w * 0.26, -4.4);
      ctx.lineTo(-g.w * 0.1, -3);
      ctx.moveTo(g.w * 0.2, -4.4);
      ctx.lineTo(g.w * 0.04, -3);
      ctx.stroke();
      ctx.restore();
    }

    // Poofs / floating score text.
    for (const pf of poofs) {
      ctx.globalAlpha = Math.min(1, pf.t * 2.4);
      if (pf.txt) {
        ctx.fillStyle = pf.c;
        ctx.font = "bold 10px ui-sans-serif, system-ui";
        ctx.textAlign = "center";
        ctx.fillText(pf.txt, pf.x, pf.y);
      } else {
        ctx.strokeStyle = pf.c;
        ctx.lineWidth = 1.6;
        ctx.beginPath();
        ctx.arc(pf.x, pf.y, (0.55 - Math.min(0.55, pf.t)) * 34 + 4, 0, Math.PI * 2);
        ctx.stroke();
      }
      ctx.globalAlpha = 1;
    }

    // Speed lines.
    const fever = streak >= 5;
    if (live && p.fps >= 8 && !canceled) {
      ctx.strokeStyle = fever ? "rgba(232,160,76,0.4)" : "rgba(157,195,127,0.35)";
      ctx.lineWidth = 2;
      for (let i = 0; i < 3; i++) {
        const ly = GROUND - 18 - i * 12;
        const lx = x - 26 - i * 12 - ((phase * 26) % 22);
        ctx.beginPath();
        ctx.moveTo(lx, ly);
        ctx.lineTo(lx - 16, ly);
        ctx.stroke();
      }
    }

    // ETA trend (sweat).
    const trend =
      etaSamples.length >= 4 && live
        ? etaSamples[etaSamples.length - 1] / Math.max(0.01, etaSamples[0])
        : 1;

    // The runner.
    ctx.save();
    ctx.translate(x, GROUND + y);
    if (canceled) {
      ctx.rotate(-1.25);
      ctx.translate(-4, -4);
    }
    if (graceT > 0 && Math.floor(graceT * 12) % 2 === 0) ctx.globalAlpha = 0.35;
    const running = !canceled && !done;
    const swing = running ? Math.sin(phase * 6) * (jumping ? 3 : 7) : 0;
    // Cape in fever mode.
    if (fever && running) {
      ctx.fillStyle = "#e8a04c";
      ctx.beginPath();
      ctx.moveTo(-4, -28);
      ctx.quadraticCurveTo(-16 - Math.sin(phase * 7) * 3, -22, -12, -8);
      ctx.quadraticCurveTo(-6, -14, -3, -20);
      ctx.closePath();
      ctx.fill();
    }
    // Legs.
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
    ctx.fillStyle = crashT > 0 ? "#e26d5a" : "#9dc37f";
    ctx.beginPath();
    ctx.roundRect(-6, -30, 12, 17, 6);
    ctx.fill();
    // Arms.
    ctx.strokeStyle = crashT > 0 ? "#e26d5a" : "#9dc37f";
    ctx.lineWidth = 3;
    if (done) {
      ctx.beginPath();
      ctx.moveTo(0, -27);
      ctx.lineTo(7, -40);
      ctx.stroke();
      ctx.fillStyle = "#e8d26d";
      ctx.beginPath();
      ctx.arc(8, -43, 3.4, 0, Math.PI * 2);
      ctx.fill();
    } else {
      const arm = canceled ? 5 : jumping ? -8 : -Math.sin(phase * 6) * 6;
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

    // Sweat when ETA creeps.
    if (live && trend > 1.08 && !canceled && !done) {
      ctx.fillStyle = "rgba(109,179,217,0.9)";
      for (let i = 0; i < 2; i++) {
        const sy = GROUND - 44 + y + ((phase * 30 + i * 9) % 16);
        ctx.beginPath();
        ctx.ellipse(x + 9, sy, 1.7, 2.6, 0, 0, Math.PI * 2);
        ctx.fill();
      }
    }

    // Confetti + banners.
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
      if (done && !canceled) {
        ctx.textAlign = "center";
        ctx.fillStyle = "#d5e3c3";
        ctx.font = "bold 13px ui-sans-serif, system-ui";
        ctx.fillText("EXPORTED!", W / 2, 20);
        const rk = rank();
        const el = (performance.now() - finishedAt) / 1000;
        ctx.globalAlpha = Math.min(1, el * 2);
        ctx.fillStyle = "rgba(20,22,25,0.85)";
        ctx.beginPath();
        ctx.roundRect(W / 2 - 86, 30, 172, 40, 9);
        ctx.fill();
        ctx.strokeStyle = "#3a3f37";
        ctx.lineWidth = 1;
        ctx.stroke();
        ctx.fillStyle = "#e8d26d";
        ctx.font = "bold 20px ui-sans-serif, system-ui";
        ctx.fillText(rk.r, W / 2 - 62, 58);
        ctx.fillStyle = "#d5e3c3";
        ctx.font = "bold 10px ui-sans-serif, system-ui";
        ctx.textAlign = "left";
        ctx.fillText(rk.name, W / 2 - 42, 46);
        ctx.font = "10px ui-sans-serif, system-ui";
        ctx.fillStyle = "#8b9284";
        ctx.fillText(`score ${score} · best streak ${bestStreak}`, W / 2 - 42, 60);
        ctx.globalAlpha = 1;
      }
    }
    if (live && p.stage === 2 && !canceled && !done) {
      ctx.fillStyle = "#8b9284";
      ctx.font = "11px ui-sans-serif, system-ui";
      ctx.textAlign = "center";
      ctx.fillText("packing the file…", W / 2, 20);
    }

    // HUD.
    ctx.textAlign = "right";
    ctx.fillStyle = "#e8d26d";
    ctx.font = "bold 11px ui-sans-serif, system-ui";
    ctx.fillText(`${score}`, W - 44, 16);
    drawStar(ctx, W - 34, 12, 4.2, "#e8d26d");
    ctx.fillStyle = "#8b9284";
    ctx.font = "9px ui-sans-serif, system-ui";
    ctx.fillText(`BEST ${Math.max(best, finished ? score : 0)}`, W - 10, 28);
    if (fever && !canceled && !done) {
      ctx.textAlign = "left";
      ctx.fillStyle = "#e8a04c";
      ctx.font = "bold 10px ui-sans-serif, system-ui";
      ctx.fillText(`STREAK ${streak} · ×2`, 10, 16);
    } else if (streak >= 2 && !canceled && !done) {
      ctx.textAlign = "left";
      ctx.fillStyle = "#8b9284";
      ctx.font = "9px ui-sans-serif, system-ui";
      ctx.fillText(`streak ${streak}`, 10, 16);
    }
    if (live && hintT > 0 && !canceled && !done) {
      ctx.textAlign = "center";
      ctx.globalAlpha = Math.min(1, hintT);
      ctx.fillStyle = "#d8d3c3";
      ctx.font = "10px ui-sans-serif, system-ui";
      ctx.fillText("SPACE / CLICK to jump the frame gremlins · grab the stars", W / 2, 34);
      ctx.globalAlpha = 1;
    }
    if (live && !finished && !canceled && document.activeElement !== canvas && hintT <= 0) {
      ctx.textAlign = "center";
      ctx.fillStyle = "rgba(139,146,132,0.7)";
      ctx.font = "9px ui-sans-serif, system-ui";
      ctx.fillText("click the track to play", W / 2, 34);
    }
  }

  function drawStar(
    ctx: CanvasRenderingContext2D,
    cx: number,
    cy: number,
    r: number,
    color: string,
  ) {
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
  tabindex="0"
  aria-label="Export runner game: jump the frame gremlins while the export renders."
  style={`width:100%;height:${H}px;display:block;border-radius:10px;outline:none;box-shadow:0 0 0 1px #2c2f2a;`}
  onkeydown={onKey}
  onpointerdown={(e) => {
    e.preventDefault();
    pressJump();
  }}
  onpointerup={releaseJump}
  onfocusout={releaseJump}
></canvas>
