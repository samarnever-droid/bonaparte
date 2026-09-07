/**
 * Deep color + advanced keyframing, end to end through the real UI and the
 * real Rust bridge: the export dialog's 16-bit / wide-gamut choices must
 * produce genuinely deep PNGs, the graph editor's Hold step and bulk easing
 * must reach Rust state exactly, and the upgraded grading stack must surface
 * its new wheels through the manifest-driven controls.
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
    name: "Deep color",
    comps: {
      "1": {
        id: 1,
        name: "Deep comp",
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

async function state(page: Page) {
  return api(page.request, "state");
}
async function source(page: Page) {
  return (await state(page)).project.comps["1"].layers["1"];
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

test("the export dialog offers 16-bit wide-gamut PNGs and delivers real deep files", async ({
  page,
}, info) => {
  await page.getByRole("button", { name: "Export", exact: true }).click();
  await page.getByRole("button", { name: /PNG image/ }).click();
  await page.getByLabel("Bit depth").selectOption("16");
  await page.getByLabel("Color space").selectOption("display-p3");
  await expect(page.locator(".export-facts")).toContainText("16-bit Display P3 + alpha");
  const download = page.waitForEvent("download");
  await page.getByRole("button", { name: "Export PNG", exact: true }).click();
  const path = info.outputPath("deep.png");
  await (await download).saveAs(path);
  const bytes = await readFile(path);
  expect(bytes.readUInt32BE(16)).toBe(240);
  expect(bytes.readUInt32BE(20)).toBe(135);
  // IHDR bit depth is byte 24: 16 means the deep path really encoded u16s.
  expect(bytes[24]).toBe(16);
  expect(bytes[25]).toBe(6); // truecolor + alpha
  // The default flow stays 8-bit sRGB.
  await page.getByRole("button", { name: "Export", exact: true }).click();
  await page.getByRole("button", { name: /PNG image/ }).click();
  await expect(page.locator(".export-facts")).toContainText("8-bit sRGB + alpha");
});

test("the graph editor commits Hold steps and bulk-applies curves in one undo", async ({
  page,
  request,
}) => {
  // A three-key position track, seeded through the same apply transport.
  const keys = [
    { time: 0, value: { Vec2: [0, 0] } },
    { time: 120000, value: { Vec2: [40, -20] } },
    { time: 240000, value: { Vec2: [80, 40] } },
  ];
  for (const key of keys) {
    await api(request, "apply", {
      op: {
        type: "addKeyframe",
        comp: 1,
        layer: 1,
        property: "Position",
        key: { ...key, easing: { Bezier: { p1: [0.42, 0], p2: [0.58, 1] } } },
      },
    });
  }
  await page.reload();
  await expect(page.locator(".loading-screen")).toHaveCount(0);
  await page.getByLabel("Expand Source", { exact: true }).click();
  await page.getByLabel("Graph Position on Source", { exact: true }).click();

  // Hold replaces the smooth curve on the first segment.
  await page.getByRole("button", { name: "Hold", exact: true }).click();
  await expect
    .poll(async () => (await source(page)).tracks.Position.keys[0].easing)
    .toEqual("Hold");
  await expect(page.getByRole("img", { name: "Cubic Bézier easing curve" })).toBeVisible();

  // Anticipate on segment two, then bulk-apply to everything: one transaction.
  await page.getByLabel("Outgoing keyframe").selectOption({ index: 1 });
  await page.getByRole("button", { name: "Anticipate", exact: true }).click();
  await expect
    .poll(async () => {
      const easing = (await source(page)).tracks.Position.keys[1].easing;
      return easing === "Hold" || easing === "Linear"
        ? easing
        : [
            easing.Bezier.p1[0],
            easing.Bezier.p1[1],
            easing.Bezier.p2[0],
            easing.Bezier.p2[1],
          ].map((n) => Math.round(n * 1000) / 1000);
    })
    .toEqual([0.36, 0, 0.66, -0.3]);
  await page.getByRole("button", { name: /Apply this curve to all 2 segments/ }).click();
  await expect
    .poll(async () => {
      const easing = (await source(page)).tracks.Position.keys;
      return easing
        .slice(0, -1)
        .every((k) => JSON.stringify(k.easing) === JSON.stringify(easing[0].easing));
    })
    .toBe(true);
  // …and that bulk edit is a single undo step back to the mixed state.
  await page.keyboard.press("Control+z");
  await expect
    .poll(async () => {
      const keys2 = (await source(page)).tracks.Position.keys;
      return keys2[0].easing === "Hold" && keys2[1].easing !== keys2[0].easing;
    })
    .toBe(true);
});

test("the upgraded grading stack exposes lift, gain, filmic and split toning", async ({
  page,
}) => {
  await page.getByRole("button", { name: "Color", exact: true }).click();
  await page.getByRole("button", { name: "Add Color Grade", exact: true }).click();
  const wheels = ["Lift", "Gain", "Filmic rolloff", "Split strength"];
  for (const wheel of wheels) {
    await expect(page.getByLabel(wheel, { exact: true })).toBeVisible();
  }
  const split = page.getByLabel("Split strength", { exact: true });
  await split.fill("0.5");
  await split.press("Tab");
  await expect
    .poll(async () => (await source(page)).effects[0].params.split_strength.Float)
    .toBeCloseTo(0.5);
  // The wheels actually grade: filmic rolloff reshapes the lit shape pixels
  // (sampled at canvas center, inside the 80×60 shape — black background
  // pixels are legitimately unchanged by filmic tonemapping).
  const sample = () =>
    page.getByLabel("Rendered composition").evaluate((node: HTMLCanvasElement) =>
      Array.from(node.getContext("2d")!.getImageData(120, 67, 1, 1).data),
    );
  const before = await sample();
  const filmic = page.getByLabel("Filmic rolloff", { exact: true });
  await filmic.fill("1");
  await filmic.press("Tab");
  await expect
    .poll(async () => (await source(page)).effects[0].params.filmic.Float)
    .toBeCloseTo(1);
  await expect.poll(async () => (await sample()).join(",")).not.toBe(before.join(","));
});
