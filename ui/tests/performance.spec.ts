import { test, expect, type Page, type APIRequestContext } from "@playwright/test";
import { readFile } from "node:fs/promises";
const headers = { "X-Bonaparte-Client": "editor" };
async function command(request: APIRequestContext, name: string, data: unknown = {}) {
  const response = await request.post(`/api/${name}`, { headers, data });
  expect(response.ok(), await response.text()).toBeTruthy();
  return response.json();
}
function documentFixture() {
  const layer = {
    id: 1,
    name: "Source",
    kind: {
      Shape: {
        color: [0.25, 0.6, 0.3, 1],
        generator: null,
        style: { size: [140, 100], corner_radius: 8, stroke_width: 0, stroke_color: [1, 1, 1, 1] },
      },
    },
    start: 0,
    duration: 240000,
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
  return {
    name: "Performance test",
    comps: {
      "1": {
        id: 1,
        name: "Preview composition",
        width: 320,
        height: 180,
        fps: { num: 30, den: 1 },
        duration: 240000,
        background: [0.01, 0.02, 0.01, 1],
        layer_order: [1],
        layers: { "1": layer },
      },
    },
    media: {},
    next_comp: 2,
    next_layer: 2,
    next_media: 1,
  };
}
const errors = new WeakMap<Page, string[]>();
test.beforeEach(async ({ page, request }) => {
  const found: string[] = [];
  errors.set(page, found);
  page.on("pageerror", (e) => found.push(e.message));
  await command(request, "open_project", { json: JSON.stringify(documentFixture()) });
  await page.goto("/");
  await expect(page.locator(".loading-screen")).toHaveCount(0);
  await expect(page.getByLabel("Rendered composition")).toHaveAttribute("width", "320");
  await page.getByLabel("Playhead timecode").fill("00:00:00:00");
  await page.getByLabel("Playhead timecode").press("Tab");
  await expect(page.getByLabel("Rendered composition")).toHaveAttribute("data-preview-time", "0");
});
test.afterEach(async ({ page }) => {
  expect(errors.get(page)).toEqual([]);
  await expect(page.locator(".render-error")).toHaveCount(0);
});

test("resolution, frame reuse, and cache clearing are preview-only controls", async ({
  page,
  request,
}) => {
  const before = await command(request, "state");
  const canvas = page.getByLabel("Rendered composition");
  await page.getByLabel("Preview resolution").selectOption("4");
  await expect(canvas).toHaveAttribute("width", "80");
  await page.getByLabel("Preview resolution").selectOption("1");
  await expect(canvas).toHaveAttribute("width", "320");
  await page.getByLabel("Preview resolution").selectOption("4");
  await expect(canvas).toHaveAttribute("data-cache-hit", "true");
  expect(await command(request, "state")).toEqual(before);
  await page.getByLabel("Preview performance").click();
  await expect(page.getByRole("dialog")).toContainText("Actual execution", { ignoreCase: true });
  await expect(page.getByLabel("Frame cache memory")).toBeVisible();
  await page.getByRole("button", { name: "Clear caches", exact: true }).click();
  await expect(canvas).toHaveAttribute("data-cache-hit", "false");
  expect(await command(request, "state")).toEqual(before);
  await page.getByLabel("Dismiss performance panel").click();
});

test("half-resolution canvas gestures use original coordinates and Auto restores full quality", async ({
  page,
  request,
}) => {
  const canvas = page.getByLabel("Rendered composition");
  await page.getByLabel("Preview resolution").selectOption("2");
  await expect(canvas).toHaveAttribute("width", "160");
  const box = (await canvas.boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(
    box.x + box.width / 2 + box.width / 8,
    box.y + box.height / 2 + box.height / 9,
    { steps: 8 },
  );
  await page.mouse.up();
  await expect
    .poll(
      async () =>
        (await command(request, "state")).project.comps["1"].layers["1"].transform.position,
    )
    .toEqual([40, 20]);
  await page.getByLabel("Preview resolution").selectOption("auto");
  await expect(canvas).toHaveAttribute("width", "320");
  await page.getByRole("button", { name: "Play", exact: true }).click();
  await expect(canvas).toHaveAttribute("width", "160");
  await page.getByRole("button", { name: "Pause playback", exact: true }).click();
  await expect(canvas).toHaveAttribute("width", "320");
  expect((await command(request, "state")).project.comps["1"].width).toBe(320);
});

test("the engine badge reports real GPU execution and explicit whole-frame fallback", async ({
  page,
  request,
}) => {
  const status = await command(request, "preview_status");
  test.skip(
    !status.gpu.available,
    "No graphics API adapter is available; required GPU CI exercises this test on llvmpipe.",
  );
  await page.getByLabel("Preview renderer").selectOption("gpu");
  await expect(page.getByLabel("Rendered composition")).toHaveAttribute(
    "data-renderer",
    status.gpu.software ? "gpu-software" : "gpu",
  );
  await expect
    .poll(async () => (await command(request, "preview_status")).gpu.submittedFrames)
    .toBeGreaterThan(0);
  await page.locator(".sidebar").getByRole("button", { name: "Effects", exact: true }).click();
  await page
    .locator(".sidebar")
    .getByRole("button", { name: "Gaussian Blur", exact: true })
    .click();
  await expect(page.getByLabel("Rendered composition")).toHaveAttribute("data-renderer", "cpu");
  await page.getByLabel("Preview performance").click();
  await expect(page.getByRole("dialog")).toContainText("Gaussian Blur");
  await page.getByLabel("Dismiss performance panel").click();
  await page.getByLabel("Remove Gaussian Blur", { exact: true }).click();
  await expect(page.getByLabel("Rendered composition")).toHaveAttribute(
    "data-renderer",
    status.gpu.software ? "gpu-software" : "gpu",
  );
});

test("quarter preview does not downsample downloaded PNG exports", async ({ page }, info) => {
  await page.getByLabel("Preview resolution").selectOption("4");
  await expect(page.getByLabel("Rendered composition")).toHaveAttribute("width", "80");
  await page.getByRole("button", { name: "Export", exact: true }).click();
  await page.getByRole("button", { name: /PNG image/ }).click();
  const download = page.waitForEvent("download");
  await page.getByRole("button", { name: "Export PNG", exact: true }).click();
  const path = info.outputPath("full-resolution.png");
  await (await download).saveAs(path);
  const bytes = await readFile(path);
  expect(bytes.readUInt32BE(16)).toBe(320);
  expect(bytes.readUInt32BE(20)).toBe(180);
});

test("late real renderer responses cannot overwrite a newer resolution choice", async ({
  page,
}) => {
  let delayed = false;
  // Delay a genuine Rust response; no renderer or pixels are mocked.
  await page.route("**/api/preview_frame", async (route) => {
    const response = await route.fetch();
    if (!delayed && route.request().postDataJSON().divisor === 2) {
      delayed = true;
      await new Promise((resolve) => setTimeout(resolve, 250));
    }
    await route.fulfill({ response });
  });
  await page.getByLabel("Preview resolution").selectOption("2");
  await page.getByLabel("Preview resolution").selectOption("4");
  await expect(page.getByLabel("Rendered composition")).toHaveAttribute("width", "80");
  await page.waitForTimeout(350);
  await expect(page.getByLabel("Rendered composition")).toHaveAttribute("width", "80");
});
