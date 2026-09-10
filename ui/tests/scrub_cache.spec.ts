/**
 * The static-prefix compositing cache in action: a 240-layer text wall with
 * one animated title scrubs frame-by-frame at full resolution — each new
 * time skips compositing for every unchanged layer — and the frames stay
 * byte-for-byte deterministic.
 */
import { test, expect, type APIRequestContext } from "@playwright/test";
import { createHash } from "node:crypto";

const H = { "X-Bonaparte-Client": "editor" };

async function api(request: APIRequestContext, name: string, data: unknown = {}) {
  const response = await request.post(`/api/${name}`, { headers: H, data });
  const body = await response.text();
  expect(response.ok(), `${name}: ${body.slice(0, 200)}`).toBeTruthy();
  return JSON.parse(body);
}

function posterProject(layers: number) {
  const comp = {
    id: 1,
    name: "Plate wall",
    width: 640,
    height: 360,
    fps: { num: 30, den: 1 },
    duration: 1_200_000,
    background: [0.04, 0.04, 0.07, 1],
    layer_order: [] as number[],
    layers: {} as Record<string, unknown>,
  };
  let id = 1;
  for (let i = 0; i < layers; i++) {
    // Full-frame translucent plates: every one of them costs a complete
    // blend pass. A grading stack is exactly this shape.
    comp.layers[String(id)] = {
      id,
      name: `plate-${i}`,
      kind: { Solid: { color: [0.05, 0.06, 0.1, 0.02] } },
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
    id++;
  }
  const keys = Array.from({ length: 12 }, (_, i) => ({
    time: i * 10_000,
    value: { Vec2: [200 + i * 12, 100 + (i % 3) * 20] },
    easing: "Linear",
  }));
  comp.layers[String(id)] = {
    id,
    name: "title",
    kind: {
      Text: {
        text: "THE ANIMATED ONE",
        size: 26,
        style: { color: [1, 1, 1, 1], bold: true, tracking: 0 },
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
    tracks: { Position: { keys } },
    effects: [],
    visible: true,
    locked: false,
    parent: null,
    blend_mode: "Normal",
  };
  comp.layer_order.push(id);
  return {
    name: "Scrub cache",
    comps: { "1": comp },
    media: {},
    next_comp: 2,
    next_layer: id + 1,
    next_media: 1,
  };
}

test("a 240-layer plate wall scrubs full-res and stays deterministic", async ({
  page,
  request,
}) => {
  test.setTimeout(120_000);
  await api(request, "open_project", { json: JSON.stringify(posterProject(240)) });

  const frame = async (time: number) => {
    const response = await request.post("/api/preview_frame", {
      headers: H,
      data: { compId: 1, time, divisor: 1 },
    });
    expect(response.ok(), await response.text()).toBeTruthy();
    return Buffer.from(await response.body());
  };

  const pixelsOf = (bytes: Buffer) => {
    // BPF3 packet: magic, meta length, JSON metadata, then raw pixels.
    expect(bytes.subarray(0, 4).toString()).toBe("BPF3");
    const metaLen = bytes.readUInt32LE(4);
    return bytes.subarray(8 + metaLen);
  };

  const started = Date.now();
  const seen: number[] = [];
  const hashes = new Map<number, string>();
  for (let f = 0; f < 10; f++) {
    const t0 = Date.now();
    const bytes = await frame(2_000 + f * 4_000); // ten DISTINCT new times
    seen.push(Date.now() - t0);
    hashes.set(2_000 + f * 4_000, createHash("sha256").update(pixelsOf(bytes)).digest("hex"));
  }
  const total = Date.now() - started;
  const median = seen.slice().sort((a, b) => a - b)[Math.floor(seen.length / 2)];
  test.info().annotations.push({
    type: "timing",
    description: `10 full-res frames of a 241-layer comp: median ${median} ms, total ${total} ms`,
  });
  // eslint-disable-next-line no-console
  console.log(`SCRUB-TIMING total=${total}ms median=${median}ms per-frame`);
  expect(total, "scrubbing 10 fresh frames must stay interactive").toBeLessThan(10_000);
  expect(median, "warm frames must beat 1000 ms").toBeLessThan(1_000);

  // Determinism: repeats (warm cache + frame LRU) are byte-identical, and a
  // revisit of an earlier time matches its original hash.
  const again = await frame(2_000 + 4 * 4_000);
  expect(createHash("sha256").update(pixelsOf(again)).digest("hex")).toBe(
    hashes.get(2_000 + 4 * 4_000),
  );

  // The live editor rides the same path: arrow-scrub with no errors.
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto("/");
  await expect(page.locator(".loading-screen")).toHaveCount(0, { timeout: 60_000 });
  for (let i = 0; i < 4; i++) await page.keyboard.press("ArrowRight");
  await expect(page.locator(".render-error")).toHaveCount(0);
  expect(errors).toEqual([]);

  await api(request, "new_project");
});
