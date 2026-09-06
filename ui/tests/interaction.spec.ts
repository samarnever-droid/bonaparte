import { test, expect } from "@playwright/test";
import { readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

test("large-project pointer interaction measurements and one-gesture undo", async ({
  page,
  request,
}, info) => {
  const project = JSON.parse(
    await readFile(
      fileURLToPath(new URL("../../examples/ignition.bonaparte.json", import.meta.url)),
      "utf8",
    ),
  );
  const comp = project.comps["1"];
  const id = project.next_layer++;
  comp.layers[String(id)] = {
    id,
    name: "Interaction probe",
    kind: {
      Shape: {
        color: [1, 0, 1, 1],
        generator: null,
        style: { size: [260, 180], corner_radius: 0, stroke_width: 0, stroke_color: [1, 1, 1, 1] },
      },
    },
    start: 0,
    duration: comp.duration,
    transform: {
      position: [0, 0],
      scale: [100, 100],
      rotation: 0,
      opacity: 1,
      anchor_point: [0, 0],
    },
    tracks: {},
    effects: [],
    visible: true,
    locked: false,
    parent: null,
    blend_mode: "Normal",
  };
  comp.layer_order.push(id);
  const api = async (name: string, data: unknown = {}) => {
    const r = await request.post("/api/" + name, {
      headers: { "X-Bonaparte-Client": "editor" },
      data,
    });
    expect(r.ok()).toBeTruthy();
    return r.json();
  };
  await api("open_project", { json: JSON.stringify(project) });
  const before = await api("state");
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  let responseBytes = 0,
    previewRequests = 0;
  page.on("response", async (r) => {
    if (r.url().endsWith("/api/apply")) {
      try {
        responseBytes += (await r.body()).length;
      } catch {}
    }
  });
  page.on("request", (r) => {
    if (r.url().endsWith("/api/preview_frame")) previewRequests++;
  });
  await page.goto("/");
  await expect(page.locator(".loading-screen")).toHaveCount(0);
  await page.getByLabel("Preview resolution").selectOption("4");
  await page.getByLabel("Select layer Interaction probe", { exact: true }).click();
  await expect(page.getByLabel("Rendered composition")).toHaveAttribute("width", "480");
  await page.waitForTimeout(350);
  await page.evaluate(() => {
    const w = window as any;
    w.__interactionSamples = [];
    w.__rafIntervals = [];
    let last = performance.now();
    w.__benchActive = true;
    function tick(now: number) {
      if (!w.__benchActive) return;
      w.__rafIntervals.push(now - last);
      last = now;
      requestAnimationFrame(tick);
    }
    requestAnimationFrame(tick);
    document.addEventListener(
      "pointermove",
      () => {
        if (!w.__benchActive) return;
        const t = performance.now();
        requestAnimationFrame(() => w.__interactionSamples.push(performance.now() - t));
      },
      { capture: true },
    );
  });
  const canvas = page.getByLabel("Rendered composition");
  const box = (await canvas.boundingBox())!;
  const x = box.x + box.width / 2,
    y = box.y + box.height / 2;
  await page.mouse.move(x, y);
  await page.mouse.down();
  for (let i = 1; i <= 40; i++) {
    await page.mouse.move(x + i * 2, y + Math.sin(i / 7) * 14);
    await page.waitForTimeout(8);
  }
  const samples = await page.evaluate(() => {
    const w = window as any;
    w.__benchActive = false;
    return { eventRafMs: w.__interactionSamples, rafIntervalsMs: w.__rafIntervals };
  });
  const beforeUp = Date.now();
  await page.mouse.up();
  await expect(page.getByLabel("Position X", { exact: true })).toHaveValue(
    String(Math.round((80 * 1920) / box.width)),
  );
  const commitObservedMs = Date.now() - beforeUp;
  await expect
    .poll(
      async () =>
        Object.values((await api("state")).project.comps["1"].layers).find(
          (l: any) => l.name === "Interaction probe",
        ) as any,
    )
    .toMatchObject({
      transform: {
        position: [
          Math.round((80 * 1920) / box.width),
          Math.round((Math.sin(40 / 7) * 14 * 1080) / box.height),
        ],
      },
    });
  await expect
    .poll(async () => (await api("state")).history.length)
    .toBe(before.history.length + 1);
  const median = (v: number[]) => {
    const a = [...v].sort((a, b) => a - b);
    return a[Math.floor(a.length * 0.5)] ?? 0;
  };
  const p95 = (v: number[]) => {
    const a = [...v].sort((a, b) => a - b);
    return a[Math.floor(a.length * 0.95)] ?? 0;
  };
  const metrics = {
    project: "IGNITION, plus one editable probe",
    layers: Object.values(project.comps).reduce((n: number, c: any) => n + c.layer_order.length, 0),
    eventToNextRafMedianMs: median(samples.eventRafMs),
    eventToNextRafP95Ms: p95(samples.eventRafMs),
    rafIntervalP95Ms: p95(samples.rafIntervalsMs),
    framesOver50ms: samples.rafIntervalsMs.filter((v: number) => v > 50).length,
    commitObservedMs,
    mutationResponseBytes: responseBytes,
    previewRequests,
    ...samples,
  };
  await writeFile(info.outputPath("interaction-metrics.json"), JSON.stringify(metrics, null, 2));
  console.log(
    "INTERACTION_METRICS",
    JSON.stringify({ ...metrics, eventRafMs: undefined, rafIntervalsMs: undefined }),
  );
  await page.getByRole("button", { name: "Undo", exact: true }).click();
  await expect
    .poll(async () => (await api("state")).project.comps["1"].layers[String(id)].transform.position)
    .toEqual([0, 0]);
  expect(errors).toEqual([]);
});

test("cached transform pixels respond without waiting for delayed authoritative frames", async ({
  page,
  request,
}) => {
  const project = {
    name: "Proxy test",
    comps: {
      "1": {
        id: 1,
        name: "Main",
        width: 320,
        height: 180,
        fps: { num: 24, den: 1 },
        duration: 120000,
        background: [0.02, 0.02, 0.02, 1],
        layers: {
          "1": {
            id: 1,
            name: "Magenta",
            kind: {
              Shape: {
                color: [1, 0, 1, 1],
                generator: null,
                style: {
                  size: [100, 80],
                  corner_radius: 0,
                  stroke_width: 0,
                  stroke_color: [1, 1, 1, 1],
                },
              },
            },
            start: 0,
            duration: 120000,
            transform: {
              position: [0, 0],
              scale: [100, 100],
              rotation: 0,
              opacity: 1,
              anchor_point: [0, 0],
            },
            tracks: {},
            parent: null,
            blend_mode: "Normal",
            visible: true,
            locked: false,
            effects: [],
          },
        },
        layer_order: [1],
      },
    },
    media: {},
    next_comp: 2,
    next_layer: 2,
    next_media: 1,
  };
  const h = { "X-Bonaparte-Client": "editor" };
  await request.post("/api/open_project", { headers: h, data: { json: JSON.stringify(project) } });
  await page.goto("/");
  await expect(page.locator(".loading-screen")).toHaveCount(0);
  await page.getByLabel("Select layer Magenta", { exact: true }).click();
  await page.waitForTimeout(300);
  await page.route("**/api/preview_frame", async (route) => {
    const response = await route.fetch();
    await new Promise((r) => setTimeout(r, 350));
    await route.fulfill({ response });
  });
  const canvas = page.getByLabel("Rendered composition");
  const box = (await canvas.boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2 + 40, box.y + box.height / 2 + 10);
  await expect(page.locator(".interaction-planes")).toHaveAttribute(
    "data-interaction-active",
    "true",
  );
  await expect(page.locator(".interaction-planes")).not.toHaveAttribute(
    "data-interaction-matrix",
    "matrix(1,0,0,1,0,0)",
  );
  await page.keyboard.press("Escape");
  await page.mouse.up();
  await expect(page.locator(".interaction-planes")).toHaveAttribute(
    "data-interaction-active",
    "false",
  );
  const state = await (await request.post("/api/state", { headers: h, data: {} })).json();
  expect(state.history).toHaveLength(0);
  expect(state.project.comps["1"].layers["1"].transform.position).toEqual([0, 0]);
  const handle = page.getByLabel("Scale handle 3", { exact: true });
  const hb = (await handle.boundingBox())!;
  await page.mouse.move(hb.x + hb.width / 2, hb.y + hb.height / 2);
  await page.mouse.down();
  await page.mouse.move(hb.x + hb.width / 2 + 25, hb.y + hb.height / 2 + 15);
  await expect(page.getByLabel("Rendered composition")).toHaveCSS("opacity", "1");
  await page.mouse.up();
  await expect
    .poll(async () => {
      const s = await (await request.post("/api/state", { headers: h, data: {} })).json();
      return s.project.comps["1"].layers["1"].transform.scale[0];
    })
    .toBeGreaterThan(100);
  await page.getByRole("button", { name: "Undo", exact: true }).click();
  await expect(page.getByLabel("Scale X", { exact: true })).toHaveValue("100");
});
