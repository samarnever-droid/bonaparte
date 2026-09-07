/**
 * 3D camera + depth (the diorama feature) and quick grade in the main editor.
 * Everything lands through the real bridge as undoable ops and is verified
 * against real rendered pixels.
 */
import { test, expect, type Page, type APIRequestContext } from "@playwright/test";

const headers = { "X-Bonaparte-Client": "editor" };

async function api(request: APIRequestContext, name: string, data: unknown = {}) {
  const response = await request.post(`/api/${name}`, { headers, data });
  expect(response.ok(), await response.text()).toBeTruthy();
  return response.json();
}

const card = (id: number, name: string) => ({
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

function projectFixture() {
  return {
    name: "3D and quick grade",
    comps: {
      "1": {
        id: 1,
        name: "Diorama",
        width: 240,
        height: 135,
        fps: { num: 30, den: 1 },
        duration: 240000,
        background: [0, 0, 0, 1],
        layer_order: [1],
        layers: { "1": card(1, "Card") },
      },
    },
    media: {},
    next_comp: 2,
    next_layer: 2,
    next_media: 1,
  };
}

async function comp(page: Page) {
  return (await api(page.request, "state")).project.comps["1"];
}

/** Read one canvas pixel (RGBA 0-255). */
function pixelAt(page: Page, x: number, y: number) {
  return page.getByLabel("Rendered composition").evaluate(
    (node: HTMLCanvasElement, [px, py]: number[]) =>
      Array.from(node.getContext("2d")!.getImageData(px, py, 1, 1).data),
    [x, y],
  );
}

function checksum(page: Page) {
  return page.getByLabel("Rendered composition").evaluate((node: HTMLCanvasElement) => {
    const bytes = node.getContext("2d")!.getImageData(0, 0, node.width, node.height).data;
    let hash = 2166136261;
    for (const byte of bytes) hash = Math.imul(hash ^ byte, 16777619);
    return hash >>> 0;
  });
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

test("the 3D orbit tool drives the real camera and one undo restores it", async ({ page }) => {
  const before = await checksum(page);
  await page.getByLabel("3D orbit tool", { exact: true }).click();
  const canvas = page.getByLabel("Rendered composition");
  const box = await canvas.boundingBox();
  // Drag across the canvas: rotates the camera and enables perspective.
  await page.mouse.move(box!.x + box!.width / 2, box!.y + box!.height / 2);
  await page.mouse.down();
  await page.mouse.move(box!.x + box!.width / 2 + 90, box!.y + box!.height / 2 - 30, { steps: 8 });
  await page.mouse.up();
  await expect
    .poll(async () => (await comp(page)).camera?.fov ?? 0)
    .toBeGreaterThan(0);
  expect((await comp(page)).camera.position).not.toEqual([0, 0]);
  await expect.poll(async () => checksum(page)).not.toBe(before);

  // One undo removes the whole camera drag.
  await page.keyboard.press("Control+z");
  await expect
    .poll(async () => (await comp(page)).camera?.fov ?? 0)
    .toBe(0);
  await expect.poll(async () => checksum(page)).toBe(before);
});

test("depth scales layers with true perspective in the rendered pixels", async ({ page }) => {
  // Deterministic camera straight on: fov 500 at the comp center.
  await api(page.request, "apply", {
    op: { type: "setCamera", comp: 1, position: [0, 0], z: 0, fov: 500 },
  });
  await page.reload();
  await expect(page.locator(".loading-screen")).toHaveCount(0);
  // The card spans 80..160 horizontally; (155, 67) is inside.
  await expect.poll(async () => (await pixelAt(page, 155, 67))[0]).toBeGreaterThan(200);

  // Select the card and push it 250px away through the Depth control:
  // scale 500/750 = 2/3 -> spans 93..147.
  await page.getByLabel("Selection tool", { exact: true }).click();
  await page.getByLabel("Rendered composition").click({ position: { x: 120, y: 67 } });
  const depth = page.getByLabel("Depth", { exact: true });
  await depth.fill("250");
  await depth.press("Tab");
  await expect
    .poll(async () => (await pixelAt(page, 155, 67))[0])
    .toBeLessThan(60, "the card shrinks away from the camera");
  // The center stays put.
  await expect.poll(async () => (await pixelAt(page, 120, 67))[0]).toBeGreaterThan(200);

  // Bring it 250px closer instead: scale 2x -> spans 40..200.
  await depth.fill("-250");
  await depth.press("Tab");
  await expect
    .poll(async () => (await pixelAt(page, 155, 67))[0])
    .toBeGreaterThan(200);
  await expect
    .poll(async () => (await pixelAt(page, 50, 67))[0])
    .toBeGreaterThan(200, "the card grows past its original edge");
});

test("the turntable orbits the camera while the timeline plays", async ({ page }) => {
  await api(page.request, "apply", {
    op: { type: "setCamera", comp: 1, position: [0, 0], z: 0, fov: 400 },
  });
  await api(page.request, "apply", {
    op: { type: "setValue", comp: 1, layer: 1, property: "Z", value: { Scalar: -200 } },
  });
  await page.reload();
  await expect(page.locator(".loading-screen")).toHaveCount(0);
  await page.getByLabel("Toggle turntable", { exact: true }).click();
  await expect
    .poll(async () => (await comp(page)).turntable?.enabled ?? false)
    .toBe(true);

  // Press play and sample the frame hash at two times — the orbit must move
  // real pixels.
  await page.keyboard.press("Space");
  await page.waitForTimeout(900);
  const first = await checksum(page);
  await page.waitForTimeout(1100);
  const second = await checksum(page);
  expect(first).not.toBe(second);
  await page.keyboard.press("Space");

  // Toggling off stops the orbit (a disabled turntable is omitted from state).
  await page.getByLabel("Toggle turntable", { exact: true }).click();
  await expect
    .poll(async () => (await comp(page)).turntable?.enabled ?? false)
    .toBe(false);
});

test("depth is keyframable from the properties panel", async ({ page }) => {
  await page.getByLabel("Selection tool", { exact: true }).click();
  await page.getByLabel("Rendered composition").click({ position: { x: 120, y: 67 } });
  await page.getByLabel("Keyframe Depth", { exact: true }).click();
  await expect
    .poll(async () => (await comp(page)).layers["1"].tracks.Z?.keys.length ?? 0)
    .toBe(1);
  const field = page.getByLabel("Playhead timecode");
  await field.fill("00:00:01:00");
  await field.press("Tab");
  const depth = page.getByLabel("Depth", { exact: true });
  // Editing an animated property commits a keyframe at the playhead itself.
  await depth.fill("-180");
  await depth.press("Tab");
  await expect
    .poll(async () => (await comp(page)).layers["1"].tracks.Z?.keys.length ?? 0)
    .toBe(2);
  // The -180 key sits at the 1s playhead (the boot key lives at 1.2s).
  const keys = (await comp(page)).layers["1"].tracks.Z.keys;
  expect(keys.find((k) => k.time === 120000)?.value).toEqual({ Scalar: -180 });
});

test("selection follows depth: clicking an overlap picks the nearer card", async ({ page }) => {
  // Second card composed ON TOP (higher in layer_order) but pushed FAR back;
  // first card below but NEAR. With the camera on, the near card paints last
  // and must win the click even though it sits lower in the stack.
  await api(page.request, "apply", {
    op: {
      type: "addLayer",
      comp: 1,
      layer: card(2, "Far"),
    },
  });
  await api(page.request, "apply", {
    op: { type: "reorderLayer", comp: 1, layer: 2, newIndex: 1 },
  });
  await api(page.request, "apply", {
    op: {
      type: "setCamera",
      comp: 1,
      position: [0, 0],
      z: 0,
      fov: 500,
      focus: 0,
      dof: 0,
    },
  });
  await api(page.request, "apply", {
    op: { type: "setValue", comp: 1, layer: 2, property: "Z", value: { Scalar: 500 } },
  });
  await page.reload();
  await expect(page.locator(".loading-screen")).toHaveCount(0);
  // The far card scaled to half: it spans 100..140 around center; the near
  // card is full size 80..160. Click the overlap on the right side.
  await page.getByLabel("Selection tool", { exact: true }).click();
  await page.getByLabel("Rendered composition").click({ position: { x: 135, y: 67 } });
  const inspector = page.locator(".selected-name input");
  await expect(inspector).toHaveValue("Card", { timeout: 5000 });
  // In legacy order (camera off) the same click hits "Far" (top of stack).
  await api(page.request, "apply", {
    op: { type: "setCamera", comp: 1, position: [0, 0], z: 0, fov: 0, focus: 0, dof: 0 },
  });
  // Let the live reconciliation merge the camera change into the UI.
  await expect
    .poll(async () => (await comp(page)).camera?.fov ?? 0)
    .toBe(0);
  await page.waitForTimeout(1600);
  await page.getByLabel("Rendered composition").click({ position: { x: 135, y: 67 } });
  await expect(inspector).toHaveValue("Far", { timeout: 5000 });
});

test("parenting moves a child in depth: it shrinks with the parent", async ({ page }) => {
  await api(page.request, "apply", {
    op: { type: "addLayer", comp: 1, layer: card(2, "Child") },
  });
  await api(page.request, "apply", {
    op: { type: "setLayerParent", comp: 1, layer: 2, parent: 1 },
  });
  await api(page.request, "apply", {
    op: {
      type: "setCamera",
      comp: 1,
      position: [0, 0],
      z: 0,
      fov: 500,
      focus: 0,
      dof: 0,
    },
  });
  await page.reload();
  await expect(page.locator(".loading-screen")).toHaveCount(0);
  const full = await pixelAt(page, 155, 67);
  expect(full[0]).toBeGreaterThan(200);
  // Push the PARENT back 600px: the child (compounded depth 600) halves.
  await page.getByLabel("Selection tool", { exact: true }).click();
  await page.getByLabel("Select layer Card", { exact: true }).click();
  const depth = page.getByLabel("Depth", { exact: true });
  await depth.fill("600");
  await depth.press("Tab");
  await expect
    .poll(async () => (await pixelAt(page, 155, 67))[0])
    .toBeLessThan(60, "the child shrank with its parent's depth");
});

test("quick grade lives in the main editor and grades real pixels", async ({ page }) => {
  await page.getByLabel("Selection tool", { exact: true }).click();
  await page.getByLabel("Rendered composition").click({ position: { x: 120, y: 67 } });
  await expect(page.getByLabel("Quick grade")).toBeVisible();
  await page.getByRole("button", { name: "Add quick grade" }).click();
  const exposure = page.getByLabel("Exposure", { exact: true });
  await expect(exposure).toBeVisible();
  const before = await pixelAt(page, 120, 67);
  await exposure.fill("1");
  await exposure.press("Tab");
  await expect
    .poll(async () => (await comp(page)).layers["1"].effects[0]?.params.exposure?.Float ?? 0)
    .toBeCloseTo(1);
  // Green has headroom (red clips at 255).
  await expect
    .poll(async () => (await pixelAt(page, 120, 67))[1])
    .toBeGreaterThan(before[1] + 30, "+1EV brightens the layer");

  // The palette button hands off to the full Color workspace.
  await page.getByLabel("Open full grading", { exact: true }).click();
  await expect(page.getByRole("button", { name: "Color", exact: true })).toHaveClass(/active/);
});
