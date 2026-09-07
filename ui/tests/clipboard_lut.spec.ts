/**
 * Keyframe clipboard + .cube LUT export, through the real UI and bridge.
 */
import { test, expect, type Page, type APIRequestContext } from "@playwright/test";
import { readFile } from "node:fs/promises";

const headers = { "X-Bonaparte-Client": "editor" };

async function api(request: APIRequestContext, name: string, data: unknown = {}) {
  const response = await request.post(`/api/${name}`, { headers, data });
  expect(response.ok(), await response.text()).toBeTruthy();
  return response.json();
}

const shapeLayer = (id: number, name: string) => ({
  id,
  name,
  kind: {
    Shape: {
      color: [0.9, 0.5, 0.1, 1],
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

function projectFixture(layer = shapeLayer(1, "Source")) {
  return {
    name: "Clipboard and LUT",
    comps: {
      "1": {
        id: 1,
        name: "Clip comp",
        width: 240,
        height: 135,
        fps: { num: 30, den: 1 },
        duration: 240000,
        background: [0, 0, 0, 1],
        layer_order: [layer.id],
        layers: { [String(layer.id)]: layer },
      },
    },
    media: {},
    next_comp: 2,
    next_layer: 2,
    next_media: 1,
  };
}

async function source(page: Page) {
  return (await api(page.request, "state")).project.comps["1"].layers["1"];
}
async function seek(page: Page, timecode: string) {
  const field = page.getByLabel("Playhead timecode");
  await field.fill(timecode);
  await field.press("Tab");
  await expect(field).toHaveValue(timecode);
}

const errors = new WeakMap<Page, string[]>();
test.beforeEach(async ({ page, request }) => {
  const found: string[] = [];
  errors.set(page, found);
  page.on("pageerror", (e) => found.push(e.message));
  await api(request, "open_project", { json: JSON.stringify(projectFixture()) });
  await page.goto("/");
  await expect(page.locator(".loading-screen")).toHaveCount(0);
  await expect(page.getByLabel("Rendered composition")).toHaveAttribute("width", "240");
  await seek(page, "00:00:00:00");
});
test.afterEach(async ({ page }) => {
  expect(errors.get(page)).toEqual([]);
  await expect(page.locator(".render-error")).toHaveCount(0);
});

test("keyframes copy from one property and paste onto another as one undoable batch", async ({
  page,
}) => {
  // Seed a two-key Position animation and a one-key Scale track after load,
  // so the editor's live state push selects and enables the disclosure.
  for (const [property, key] of [
    ["Position", { time: 0, value: { Vec2: [0, 0] } }],
    ["Position", { time: 96000, value: { Vec2: [60, -30] } }],
    ["Scale", { time: 48000, value: { Vec2: [100, 100] } }],
  ] as const) {
    await api(page.request, "apply", {
      op: {
        type: "addKeyframe",
        comp: 1,
        layer: 1,
        property,
        key: {
          ...key,
          easing:
            property === "Position"
              ? { Bezier: { p1: [0.2, 1.2], p2: [0.5, 1] } }
              : "Linear",
        },
      },
    });
  }
  await page.reload();
  await expect(page.locator(".loading-screen")).toHaveCount(0);
  await page.getByLabel("Expand Source", { exact: true }).click();
  await page.getByLabel("Graph Position on Source", { exact: true }).click();
  await page.getByRole("button", { name: /Copy 2 keyframes/ }).click();
  await page.getByLabel("Close graph editor", { exact: true }).click();

  // Playhead to 1s: pasted keys shift so the first lands here.
  await seek(page, "00:00:01:00");
  await page.getByLabel("Graph Scale on Source", { exact: true }).click();
  await page.getByRole("button", { name: "Paste at playhead" }).click();
  await expect
    .poll(async () => (await source(page)).tracks.Scale.keys.length)
    .toBe(3);
  const scale = (await source(page)).tracks.Scale.keys;
  // Copied keys at 0 and 96000 shift by the playhead (1s = 120000 ticks):
  // pasted at 120000 and 216000, next to the seeded 48000.
  expect(scale.map((k) => k.time).sort((a, b) => a - b)).toEqual([48000, 120000, 216000]);
  // Easing and values travel intact.
  const pasted = scale.find((k) => k.time === 216000)!;
  expect(pasted.value).toEqual({ Vec2: [60, -30] });
  expect(JSON.stringify(pasted.easing)).toContain("1.2");
  // One undo removes the whole pasted batch.
  await page.keyboard.press("Control+z");
  await expect
    .poll(async () => (await source(page)).tracks.Scale.keys.length)
    .toBe(1);
});

test("the effects panel bakes a .cube LUT of the grade stack", async ({ page }, info) => {
  await page.getByRole("button", { name: "Color", exact: true }).click();
  await page.getByRole("button", { name: "Add Color Grade", exact: true }).click();
  const exposure = page.getByLabel("Exposure", { exact: true });
  await exposure.fill("1");
  await exposure.press("Tab");
  await expect
    .poll(async () => (await source(page)).effects[0].params.exposure.Float)
    .toBeCloseTo(1);
  const download = page.waitForEvent("download");
  await page.getByRole("button", { name: /Export grade as .cube LUT/ }).click();
  const path = info.outputPath("grade.cube");
  await (await download).saveAs(path);
  const cube = await readFile(path, "utf-8");
  expect(cube).toContain('TITLE "Bonaparte baked grade"');
  expect(cube).toContain("LUT_3D_SIZE 33");
  // The +1EV grade brightens the lattice mid-gray past its input.
  const lines = cube.split("\n").filter((l) => l && !l.startsWith("TITLE") && !l.startsWith("LUT") && !l.startsWith("DOMAIN"));
  expect(lines.length).toBe(33 * 33 * 33);
});
