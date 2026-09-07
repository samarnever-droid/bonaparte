/**
 * Cinematic depth of field on the 3D camera and one-click grade looks.
 * DoF is verified against real rendered pixels through the bridge.
 */
import { test, expect, type Page, type APIRequestContext } from "@playwright/test";

const headers = { "X-Bonaparte-Client": "editor" };

async function api(request: APIRequestContext, name: string, data: unknown = {}) {
  const response = await request.post(`/api/${name}`, { headers, data });
  expect(response.ok(), await response.text()).toBeTruthy();
  return response.json();
}

const card = (id: number, z: number, color: [number, number, number, number]) => ({
  id,
  name: `Card ${id}`,
  kind: {
    Shape: { color, generator: null, style: { size: [30, 30], corner_radius: 2, stroke_width: 0, stroke_color: [1, 1, 1, 1] } },
  },
  start: 0,
  duration: 240000,
  transform: {
    position: [id === 1 ? -60 : 60, 0],
    scale: [100, 100],
    rotation: 0,
    opacity: 1,
    anchor_point: [0, 0],
    z,
  },
  tracks: {},
  effects: [],
  visible: true,
  locked: false,
  parent: null,
  blend_mode: "Normal",
});

function projectFixture() {
  return {
    name: "Cinematic",
    comps: {
      "1": {
        id: 1,
        name: "Scene",
        width: 240,
        height: 135,
        fps: { num: 30, den: 1 },
        duration: 240000,
        background: [0, 0, 0, 1],
        layer_order: [1, 2],
        layers: {
          "1": card(1, 0, [1, 0.1, 0.1, 1]),
          "2": card(2, 600, [0.1, 1, 0.1, 1]),
        },
      },
    },
    media: {},
    next_comp: 2,
    next_layer: 3,
    next_media: 1,
  };
}

async function comp(page: Page) {
  return (await api(page.request, "state")).project.comps["1"];
}

function peakChannel(page: Page, x0: number, x1: number, y: number, channel: 0 | 1) {
  return page.getByLabel("Rendered composition").evaluate(
    (node: HTMLCanvasElement, [a, b, py, ch]: number[]) => {
      const ctx = node.getContext("2d")!;
      let peak = 0;
      for (let x = a; x <= b; x++) peak = Math.max(peak, ctx.getImageData(x, py, 1, 1).data[ch]);
      return peak;
    },
    [x0, x1, y, channel],
  );
}

const errors = new WeakMap<Page, string[]>();
test.beforeEach(async ({ page, request }) => {
  const found: string[] = [];
  errors.set(page, found);
  page.on("pageerror", (e) => found.push(e.message));
  page.on("dialog", (dialog) => void dialog.accept("Renamed from menu"));
  await api(request, "open_project", { json: JSON.stringify(projectFixture()) });
  await page.goto("/");
  await expect(page.locator(".loading-screen")).toHaveCount(0);
  await expect(page.getByLabel("Rendered composition")).toHaveAttribute("width", "240");
});
test.afterEach(async ({ page }) => {
  expect(errors.get(page)).toEqual([]);
  await expect(page.locator(".render-error")).toHaveCount(0);
});

test("depth of field blurs the far card and focus-on-selection rescues it", async ({ page }) => {
  // Camera straight on, everything sharp first.
  await api(page.request, "apply", {
    op: { type: "setCamera", comp: 1, position: [0, 0], z: 0, fov: 500, focus: 0, dof: 0 },
  });
  await page.reload();
  await expect(page.locator(".loading-screen")).toHaveCount(0);
  await expect(page.getByLabel("3D orbit tool", { exact: true })).toBeVisible();
  await expect
    .poll(async () => peakChannel(page, 136, 158, 67, 1))
    .toBeGreaterThan(200);
  const sharpFar = await peakChannel(page, 136, 158, 67, 1);

  // Rack focus: full DoF with the focal plane on the near card.
  await page.getByLabel("Depth of field strength", { exact: true }).fill("1");
  await page
    .getByLabel("Depth of field strength", { exact: true })
    .dispatchEvent("change");
  await expect
    .poll(async () => (await comp(page)).camera?.dof ?? 0)
    .toBe(1);
  await expect.poll(async () => peakChannel(page, 135, 160, 67, 1)).toBeLessThan(sharpFar * 0.6);
  // The near card sits on the focal plane and stays essentially sharp.
  const nearPeak = await peakChannel(page, 48, 72, 67, 0);
  expect(nearPeak).toBeGreaterThan(200);

  // Select the far card and hit focus-on-selection: its peak comes back.
  await page.getByLabel("Selection tool", { exact: true }).click();
  await page.getByLabel("Select layer Card 2", { exact: true }).click();
  await expect
    .poll(async () => {
      const c = await comp(page);
      const sel = c.layers["2"];
      return sel ? "ready" : "no";
    })
    .toBe("ready");
  await expect(page.getByLabel("Focus on selection", { exact: true })).toBeVisible();
  await page.getByLabel("Focus on selection", { exact: true }).click();
  await expect
    .poll(async () => (await comp(page)).camera?.focus ?? 0)
    .toBeCloseTo(600, -1);
  await expect
    .poll(async () => peakChannel(page, 135, 160, 67, 1))
    .toBeGreaterThan(sharpFar * 0.75);

  // One undo reverts the focus pull.
  await page.keyboard.press("Control+z");
  await expect
    .poll(async () => (await comp(page)).camera?.focus ?? 600)
    .toBe(0);
});

test("one-click looks grade real pixels and undo restores them", async ({ page }) => {
  await page.getByLabel("Selection tool", { exact: true }).click();
  await page.getByLabel("Select layer Card 1", { exact: true }).click();
  await expect(page.getByLabel("Quick grade")).toBeVisible();

  const before = await page
    .getByLabel("Rendered composition")
    .evaluate((node: HTMLCanvasElement) =>
      Array.from(node.getContext("2d")!.getImageData(60, 67, 1, 1).data),
    );
  // Orange card: red dominates before Noir.
  expect(before[0]).toBeGreaterThan(before[2] + 40);

  await page.getByRole("button", { name: "Noir", exact: true }).click();
  await expect
    .poll(async () => (await comp(page)).layers["1"].effects[0]?.params.saturation?.Float ?? 1)
    .toBe(0);
  // Saturation 0: the card renders gray — channels converge.
  await expect
    .poll(async () => {
      const px = await page
        .getByLabel("Rendered composition")
        .evaluate((node: HTMLCanvasElement) =>
          Array.from(node.getContext("2d")!.getImageData(60, 67, 1, 1).data),
        );
      return Math.abs(px[0] - px[2]);
    })
    .toBeLessThan(6);

  // Teal & Orange re-colors, and the whole look is one undo.
  await page.getByRole("button", { name: "Teal & Orange", exact: true }).click();
  await expect
    .poll(async () => (await comp(page)).layers["1"].effects[0]?.params.saturation?.Float ?? 0)
    .toBeCloseTo(0.18, 1);
  await page.keyboard.press("Control+z");
  await expect
    .poll(async () => (await comp(page)).layers["1"].effects[0]?.params.saturation?.Float ?? -1)
    .toBe(0);
  await page.keyboard.press("Control+z");
  await expect
    .poll(async () => (await comp(page)).layers["1"].effects.length)
    .toBe(0);
});
