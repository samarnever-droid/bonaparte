/**
 * The five 3D killer features for beginners, through the real bridge:
 *   1. Scene arrangements (diorama/parallax/carousel/corridor)
 *   2. Arrange in depth with perspective scale compensation
 *   3. New 3D scene starter (spins on first play)
 *   4. Cinematic focus on a layer (compound depth + DoF)
 *   5. Framing presets (Wide / Medium / Close-up)
 * Every action must be ONE undo step.
 */
import { test, expect, type Page, type APIRequestContext } from "@playwright/test";

const headers = { "X-Bonaparte-Client": "editor" };

async function api(request: APIRequestContext, name: string, data: unknown = {}) {
  const response = await request.post(`/api/${name}`, { headers, data });
  expect(response.ok(), await response.text()).toBeTruthy();
  return response.json();
}

const card = (id: number, name: string, position: [number, number]) => ({
  id,
  name,
  kind: {
    Shape: {
      color: [0.9, 0.5, 0.1, 1],
      generator: null,
      style: { size: [90, 70], corner_radius: 6, stroke_width: 0, stroke_color: [1, 1, 1, 1] },
    },
  },
  start: 0,
  duration: 240000,
  transform: {
    position,
    scale: [100, 100],
    rotation: 0,
    opacity: 1,
    anchor_point: [0, 0],
    z: 0,
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
    name: "Killer 3D",
    comps: {
      "1": {
        id: 1,
        name: "Scene",
        width: 240,
        height: 135,
        fps: { num: 30, den: 1 },
        duration: 240000,
        background: [0, 0, 0, 1],
        layer_order: [1, 2, 3],
        layers: {
          "1": card(1, "Top", [0, -30]),
          "2": card(2, "Middle", [10, 0]),
          "3": card(3, "Bottom", [-10, 40]),
        },
      },
    },
    media: {},
    next_comp: 2,
    next_layer: 4,
    next_media: 1,
  };
}

async function comp(page: Page) {
  return (await api(page.request, "state")).project.comps["1"];
}

async function layerZ(page: Page, id: number): Promise<number> {
  const c = await comp(page);
  return c.layers[String(id)]?.transform?.z ?? 0;
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
});
test.afterEach(async ({ page }) => {
  expect(errors.get(page)).toEqual([]);
  await expect(page.locator(".render-error")).toHaveCount(0);
});

test("arrangements distribute layers in depth and the carousel spins", async ({ page }) => {
  await page.getByLabel("3D scene arrangements").click();
  await page.getByRole("menuitem", { name: "Carousel", exact: true }).click();
  await expect.poll(async () => (await comp(page)).camera?.fov ?? 0).toBe(620);
  const zs = await Promise.all([layerZ(page, 1), layerZ(page, 2), layerZ(page, 3)]);
  expect(new Set(zs).size).toBe(3, "each layer gets its own depth");
  await expect.poll(async () => (await comp(page)).turntable?.enabled).toBe(true);
  // Diorama re-arranges over it and turns the spin off.
  await page.getByLabel("3D scene arrangements").click();
  await page.getByRole("menuitem", { name: "Diorama", exact: true }).click();
  await expect.poll(async () => (await comp(page)).camera?.dof ?? 0).toBeGreaterThan(0);
  await expect.poll(async () => (await comp(page)).turntable?.enabled ?? false).toBe(false);
  // One undo returns to the carousel arrangement.
  await page.getByLabel("Undo", { exact: true }).click();
  await expect.poll(async () => (await comp(page)).turntable?.enabled).toBe(true);
});

test("arrange in depth spreads Z with scale compensation in one undo", async ({ page }) => {
  await page.getByLabel("Select layer Bottom").click({ button: "right" });
  await page.getByRole("menuitem", { name: "Arrange layers in depth" }).click();
  await expect.poll(async () => layerZ(page, 1)).not.toBe(0);
  const c = await comp(page);
  const zs = [1, 2, 3].map((id) => c.layers[String(id)].transform.z);
  const sorted = [...zs].sort((a, b) => a - b);
  expect(zs).toEqual(sorted.map((z) => z), "distinct, monotonic depths");
  // Scale compensation: far layers are scaled up so they keep screen size.
  const far = c.layers[String(3)];
  expect(far.transform.scale[0]).toBeGreaterThan(100);
  // Undo restores all three depths at once.
  await page.getByLabel("Undo", { exact: true }).click();
  const restored = await comp(page);
  expect([1, 2, 3].map((id) => restored.layers[String(id)].transform.z ?? 0)).toEqual([0, 0, 0]);
});

test("framing presets move the camera in one click", async ({ page }) => {
  await page.getByLabel("New 3D scene").click();
  await expect.poll(async () => (await comp(page)).camera?.fov ?? 0).toBe(520);
  await page.getByRole("button", { name: "Close-up", exact: true }).click();
  await expect.poll(async () => (await comp(page)).camera?.fov ?? 0).toBe(330);
  await page.getByRole("button", { name: "Wide", exact: true }).click();
  await expect.poll(async () => (await comp(page)).camera?.fov ?? 0).toBe(780);
  await expect.poll(async () => (await comp(page)).camera?.z ?? 0).toBe(260);
});

test("cinematic focus aims the focal plane at the selected layer", async ({ page }) => {
  await page.getByLabel("New 3D scene").click();
  await expect.poll(async () => (await comp(page)).camera?.fov ?? 0).toBe(520);
  // Select the deepest starter card and focus on it.
  await page.getByLabel("Select layer Back drop").click({ button: "right" });
  await page.getByRole("menuitem", { name: "Cinematic focus on this" }).click();
  await expect
    .poll(async () => (await comp(page)).camera?.focus ?? 0)
    .toBe(400);
  await expect.poll(async () => (await comp(page)).camera?.dof ?? 0).toBeGreaterThan(0.5);
});

test("the 3D scene starter renders real pixels and undo empties it", async ({ page }) => {
  await page.getByLabel("New 3D scene").click();
  await expect.poll(async () => (await comp(page)).layer_order.length).toBe(6);
  await expect.poll(async () => (await comp(page)).turntable?.enabled).toBe(true);
  const canvas = page.getByLabel("Rendered composition");
  await expect.poll(async () =>
    canvas.evaluate((node: HTMLCanvasElement) => {
      const d = node.getContext("2d")!.getImageData(0, 0, node.width, node.height).data;
      let colored = 0;
      for (let i = 0; i < d.length; i += 4) {
        if (Math.abs(d[i] - d[i + 1]) > 24 || Math.abs(d[i + 1] - d[i + 2]) > 24) colored++;
      }
      return colored;
    }),
  ).toBeGreaterThan(50, "starter cards paint real colored pixels");
  await page.getByLabel("Undo", { exact: true }).click();
  await expect.poll(async () => (await comp(page)).layer_order.length).toBe(3);
});
