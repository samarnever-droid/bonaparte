/**
 * UI no-mercy suite. Everything runs against the real Rust bridge and the
 * real Svelte workspace — no mock state, no fake pixels. These tests try to
 * break the session through the exact transport the editor uses: command
 * storms, malformed payloads, adversarial open/save round trips, concurrent
 * preview races, and undo/redo torture. A session that survives this suite
 * is trusted to survive humans.
 */
import { test, expect, type Page, type APIRequestContext } from "@playwright/test";
import { createHash } from "node:crypto";

const headers = { "X-Bonaparte-Client": "editor" };

async function call(request: APIRequestContext, name: string, data: unknown = {}) {
  const response = await request.post(`/api/${name}`, { headers, data });
  return { response, body: await response.text() };
}

async function command(request: APIRequestContext, name: string, data: unknown = {}) {
  const { response, body } = await call(request, name, data);
  expect(response.ok(), body).toBeTruthy();
  return JSON.parse(body);
}

const setOpacity = (value: number) => ({
  op: { type: "setValue", comp: 1, layer: 1, property: "Opacity", value: { Scalar: value } },
});
const setRotation = (value: number) => ({
  op: { type: "setValue", comp: 1, layer: 1, property: "Rotation", value: { Scalar: value } },
});

const shapeLayer = (id: number, name: string, color: number[]) => ({
  id,
  name,
  kind: {
    Shape: {
      color,
      generator: null,
      style: { size: [80, 60], corner_radius: 4, stroke_width: 0, stroke_color: [1, 1, 1, 1] },
    },
  },
  start: 0,
  duration: 240000,
  transform: { position: [0, 0], scale: [100, 100], rotation: 0, opacity: 1, anchor_point: [0, 0] },
  tracks: {},
  effects: [],
  visible: true,
  locked: false,
  parent: null,
  blend_mode: "Normal",
});

function projectFixture(layers = [shapeLayer(1, "Base", [0.9, 0.4, 0.1, 1])]) {
  return {
    name: "No mercy",
    comps: {
      "1": {
        id: 1,
        name: "Torture comp",
        width: 240,
        height: 135,
        fps: { num: 30, den: 1 },
        duration: 240000,
        background: [0, 0, 0, 1],
        layer_order: layers.map((l) => l.id),
        layers: Object.fromEntries(layers.map((l) => [String(l.id), l])),
      },
    },
    media: {},
    next_comp: 2,
    next_layer: layers.length + 1,
    next_media: 1,
  };
}

const errors = new WeakMap<Page, string[]>();
test.beforeEach(async ({ page, request }) => {
  const found: string[] = [];
  errors.set(page, found);
  page.on("pageerror", (e) => found.push(e.message));
  await command(request, "open_project", { json: JSON.stringify(projectFixture()) });
  await page.goto("/");
  await expect(page.locator(".loading-screen")).toHaveCount(0);
  await expect(page.getByLabel("Rendered composition")).toHaveAttribute("width", "240");
});
test.afterEach(async ({ page }) => {
  expect(errors.get(page)).toEqual([]);
  await expect(page.locator(".render-error")).toHaveCount(0);
});

const sha256 = (bytes: Buffer | string) => createHash("sha256").update(bytes).digest("hex");

/** BPF3 packets: "BPF3" + u32 LE metadata length + JSON metadata + raw RGBA.
 * Metadata carries timing/cache flags, so determinism is asserted on pixels. */
function packetPixels(packet: Buffer): Buffer {
  expect(packet.subarray(0, 4).toString()).toBe("BPF3");
  const metaLen = packet.readUInt32LE(4);
  const metadata = JSON.parse(packet.subarray(8, 8 + metaLen).toString());
  expect(metadata).toHaveProperty("width");
  return packet.subarray(8 + metaLen);
}

test("a 150-command storm stays atomic under undo and redo", async ({ request }) => {
  const initial = (await command(request, "state")).project;
  for (let i = 0; i < 150; i++) {
    await command(request, "apply", setOpacity((i % 100) / 100));
  }
  const after = (await command(request, "state")).project;
  expect(after).not.toEqual(initial);
  // Undo the entire storm; the document must land exactly on the initial
  // state, then redo must land exactly on the storm result.
  for (let i = 0; i < 150; i++) await command(request, "undo");
  expect((await command(request, "state")).project).toEqual(initial);
  for (let i = 0; i < 150; i++) await command(request, "redo");
  expect((await command(request, "state")).project).toEqual(after);
});

test("the disk journal keeps the FULL session undoable past the window", async ({
  request,
}) => {
  for (let i = 0; i < 1250; i++) {
    await command(request, "apply", setRotation(i));
  }
  // Memory keeps a 1000-entry window; the spill lives on disk. Undo must
  // walk the WHOLE depth — 1250 exact steps, never an underflow, every step
  // a real transition. The undo reply names what it undid; the bottom
  // carries no name.
  const state = await command(request, "state");
  expect(state.historyDepth).toBe(1250);
  expect(state.historyOverflow).toBe(250);
  let steps = 0;
  for (let i = 0; i < 1300; i++) {
    const reply = await command(request, "undo");
    if (reply.lastUndone === undefined) break;
    steps++;
  }
  expect(steps).toBe(1250);
  const last = (await command(request, "state")).project;
  // Undoing past the bottom is a silent no-op that keeps the document stable.
  await command(request, "undo");
  expect((await command(request, "state")).project).toEqual(last);
  // And the full ring comes back home through the redo journal.
  let redone = 0;
  for (let i = 0; i < 1300; i++) {
    const reply = await command(request, "redo");
    if (reply.lastRedone === undefined) break;
    redone++;
  }
  expect(redone).toBe(1250);
});

test("malformed commands are rejected with errors and never poison the session", async ({
  request,
}) => {
  const healthy = (await command(request, "state")).project;
  const cruel: Array<[string, unknown]> = [
    ["apply", { op: { type: "noSuchOpIsRejectedByTheTaggedParser" } }],
    ["apply", {}],
    ["apply", { op: null }],
    ["totally_unknown_endpoint", {}],
    ["apply", { op: { type: "renameLayer", comp: 1, layer: 9999, name: "ghost" } }],
    ["apply", { op: { type: "setValue", comp: 1, layer: 1, property: "Opacity", value: { Vec2: [1, 2] } } }],
    ["apply", { op: { type: "setValue", comp: 77, layer: 1, property: "Opacity", value: { Scalar: 0.5 } } }],
    ["open_project", { json: "not json at all {{{" }],
    ["open_project", {}],
  ];
  for (const [name, payload] of cruel) {
    const { response, body } = await call(request, name, payload as never);
    expect(response.ok(), `${name} should fail: ${body}`).toBeFalsy();
    expect(response.status()).toBe(400);
    expect((JSON.parse(body) as { error?: string }).error).toBeTruthy();
  }
  // The session is exactly as it was, and still fully usable.
  expect((await command(request, "state")).project).toEqual(healthy);
  await command(request, "apply", { op: { type: "renameProject", name: "still alive" } });
  expect((await command(request, "state")).project.name).toBe("still alive");
});

test("save/open round trip is lossless and garbage cannot displace the live session", async ({
  request,
}) => {
  await command(request, "apply", { op: { type: "renameProject", name: "roundtrip" } });
  await command(request, "apply", setRotation(23.5));
  const saved = await command(request, "save_project");
  const savedJson: string = typeof saved === "string" ? saved : saved.json ?? saved.contents;
  expect(typeof savedJson).toBe("string");
  const before = (await command(request, "state")).project;
  // Garbage open attempts are refused whole.
  for (const garbage of ["{", "null", '{"comps":', JSON.stringify({ name: "empty" })]) {
    const { response, body } = await call(request, "open_project", { json: garbage });
    expect(response.ok(), `garbage should be refused: ${body}`).toBeFalsy();
    expect((await command(request, "state")).project).toEqual(before);
  }
  // The real save reopens losslessly.
  await command(request, "open_project", { json: savedJson });
  expect((await command(request, "state")).project).toEqual(before);
});

test("preview frames are deterministic across repeats and resolution-divisible", async ({
  request,
}) => {
  const frame = async (divisor: number, time = 120000) => {
    const response = await request.post(`/api/preview_frame`, {
      headers,
      data: { compId: 1, time, divisor },
    });
    expect(response.ok(), await response.text()).toBeTruthy();
    return Buffer.from(await response.body());
  };
  const a = packetPixels(await frame(1));
  const b = packetPixels(await frame(1));
  expect(sha256(a)).toBe(sha256(b));
  const half = await frame(2);
  const quarter = await frame(4);
  expect(quarter.length).toBeLessThan(half.length);
  expect(half.length).toBeLessThan(a.length);
  // The fixture is a static shape, so time alone cannot change pixels — but an
  // edit must. After rotating the layer the same request renders different
  // pixels (cache invalidated), and undoing restores the original bytes.
  await command(request, "apply", setRotation(45));
  const rotated = packetPixels(await frame(1));
  expect(sha256(rotated)).not.toBe(sha256(a));
  await command(request, "undo");
  expect(sha256(packetPixels(await frame(1)))).toBe(sha256(a));
});

test("ten concurrent mixed-resolution previews all resolve and leave a consistent editor", async ({
  page,
  request,
}) => {
  const canvas = page.getByLabel("Rendered composition");
  const results = await Promise.all(
    [1, 2, 4, 1, 2, 4, 1, 2, 4, 1].map((divisor) =>
      request
        .post(`/api/preview_frame`, { headers, data: { compId: 1, time: 0, divisor } })
        .then(async (r) => ({
          ok: r.ok(),
          divisor,
          pixels: sha256(packetPixels(Buffer.from(await r.body()))),
        })),
    ),
  );
  expect(results.every((r) => r.ok)).toBe(true);
  expect(
    results.filter((r) => r.divisor === 1).every((r, _, all) => r.pixels === all[0].pixels),
  ).toBe(true);
  // The editor still renders, and the document is untouched by the race.
  await expect(canvas).toHaveAttribute("width", "240");
  expect((await command(request, "state")).project.comps["1"].layers["1"].effects).toEqual([]);
});

test("an edit storm interleaved with resolution flips keeps document and preview orthogonal", async ({
  page,
  request,
}) => {
  const canvas = page.getByLabel("Rendered composition");
  for (let i = 0; i < 20; i++) {
    await command(request, "apply", setRotation(i * 3));
    await page.getByLabel("Preview resolution").selectOption(i % 2 === 0 ? "2" : "1");
  }
  await expect(canvas).toHaveAttribute("width", "240");
  const tracked = (await command(request, "state")).project.comps["1"].layers["1"];
  expect(tracked.transform.rotation).toBe(57);
  // All 20 edits undo in order regardless of the preview churn between them.
  for (let i = 0; i < 20; i++) await command(request, "undo");
  const undone = (await command(request, "state")).project.comps["1"].layers["1"];
  expect(undone.transform.rotation).toBe(0);
  // The preview pipeline survived the churn: full resolution, no error state.
  await expect(canvas).toHaveAttribute("width", "240");
  await expect(canvas).toHaveAttribute("data-divisor", "1");
});

test("two distinct edits stay two undo steps; drained undo is a stable no-op", async ({
  request,
}) => {
  const initial = (await command(request, "state")).project;
  await command(request, "apply", setOpacity(0.25));
  await command(request, "apply", setOpacity(0.75));
  await command(request, "undo");
  const once = (await command(request, "state")).project;
  expect(once.comps["1"].layers["1"].transform.opacity).toBe(0.25);
  await command(request, "undo");
  expect((await command(request, "state")).project).toEqual(initial);
  await command(request, "redo");
  expect((await command(request, "state")).project).toEqual(once);
});

test("export PNG through the bridge is deterministic across cache clears", async ({ request }) => {
  const render = async () => {
    const response = await request.post(`/api/export_png`, {
      headers,
      data: { compId: 1, time: 48000 },
    });
    expect(response.ok(), await response.text()).toBeTruthy();
    return Buffer.from(await response.body());
  };
  const a = await render();
  await command(request, "clear_preview_cache");
  const b = await render();
  expect(a.readUInt32BE(16)).toBe(240);
  expect(a.readUInt32BE(20)).toBe(135);
  expect(sha256(a)).toBe(sha256(b));
});

test("a canvas gesture travels the same transport and undoes in one step", async ({
  page,
  request,
}) => {
  const initial = (await command(request, "state")).project;
  const canvas = page.getByLabel("Rendered composition");
  const box = (await canvas.boundingBox())!;
  // Fraction-of-canvas drags map to exact composition pixels (comp 240×135).
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2 + box.width / 8, box.y + box.height / 2 + box.height / 9, {
    steps: 5,
  });
  await page.mouse.up();
  await expect
    .poll(async () => (await command(request, "state")).project.comps["1"].layers["1"].transform.position)
    .toEqual([30, 15]);
  await page.keyboard.press("Control+z");
  await expect
    .poll(async () => (await command(request, "state")).project.comps["1"].layers["1"].transform.position)
    .toEqual([0, 0]);
  expect((await command(request, "state")).project).toEqual(initial);
});
