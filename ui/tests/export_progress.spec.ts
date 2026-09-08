/**
 * Export overhaul: real telemetry from the Rust runtime (parallel pipeline),
 * a progress bar with ETA, and the runner game that rides the bar.
 */
import { test, expect, type Page, type APIRequestContext } from "@playwright/test";

const headers = { "X-Bonaparte-Client": "editor" };

async function api(request: APIRequestContext, name: string, data: unknown = {}) {
  const response = await request.post(`/api/${name}`, { headers, data });
  expect(response.ok(), await response.text()).toBeTruthy();
  return response.json();
}

const card = (id: number, name: string, hue: number, size: [number, number]) => ({
  id,
  name,
  kind: {
    Shape: {
      color: [hue / 360, 0.5, 0.4, 1],
      generator: null,
      style: { size, corner_radius: 6, stroke_width: 0, stroke_color: [1, 1, 1, 1] },
    },
  },
  start: 0,
  duration: 360000,
  transform: {
    position: [id * 8 - 12, 0],
    scale: [100, 100],
    rotation: 0,
    opacity: 1,
    anchor_point: [0, 0],
    z: id * 30,
  },
  tracks: {},
  effects: [],
  visible: true,
  locked: false,
  parent: null,
  blend_mode: "Normal",
});

function projectFixture(durationTicks: number, width = 160, height = 90) {
  const size: [number, number] = [Math.round(width * 0.45), Math.round(height * 0.55)];
  return {
    name: "Export progress",
    comps: {
      "1": {
        id: 1,
        name: "Scene",
        width,
        height,
        fps: { num: 30, den: 1 },
        duration: durationTicks,
        background: [0, 0, 0, 1],
        layer_order: [1, 2, 3, 4],
        layers: {
          "1": card(1, "One", 10, size),
          "2": card(2, "Two", 90, size),
          "3": card(3, "Three", 180, size),
          "4": card(4, "Four", 260, size),
        },
      },
    },
    media: {},
    next_comp: 2,
    next_layer: 5,
    next_media: 1,
  };
}

const errors = new WeakMap<Page, string[]>();
test.beforeEach(async ({ page, request }) => {
  const found: string[] = [];
  errors.set(page, found);
  page.on("pageerror", (e) => found.push(e.message));
  await api(request, "open_project", { json: JSON.stringify(projectFixture(360000)) });
  await page.goto("/");
  await expect(page.locator(".loading-screen")).toHaveCount(0);
  await expect(page.getByLabel("Rendered composition")).toHaveAttribute("width", "160");
});
test.afterEach(async ({ page }) => {
  expect(errors.get(page)).toEqual([]);
  await expect(page.locator(".render-error")).toHaveCount(0);
});

test("MP4 export shows a live meter with ETA and the runner game", async ({ page, request }) => {
  // 720p × 60 frames renders long enough for several live samples.
  await api(request, "open_project", {
    json: JSON.stringify(projectFixture(240000, 1280, 720)),
  });
  await page.reload();
  await expect(page.locator(".loading-screen")).toHaveCount(0);
  await expect(page.getByLabel("Rendered composition")).toHaveAttribute("width", "1280");
  await page.locator(".export-button").click();
  await expect(page.getByRole("dialog")).toBeVisible();
  await page.getByRole("button", { name: "Export MP4" }).click();
  const meter = page.getByRole("progressbar", { name: "Export progress" });
  await expect(meter).toBeVisible();
  const game = page.locator("canvas[aria-label^='Export runner']");
  await expect(game).toBeVisible();
  // Percent climbs (or the export is already this fast — then it completes).
  await expect
    .poll(async () => Number((await meter.getAttribute("aria-valuenow")) ?? 0), {
      timeout: 30_000,
    })
    .toBeGreaterThan(0);
  // Completion: success toast + dialog closes.
  await expect(page.getByText("MP4 exported from the Rust runtime.")).toBeVisible({
    timeout: 30_000,
  });
  await expect(meter).toHaveCount(0);
  // The runtime reported every frame through the telemetry slot.
  const progress = await api(page.request, "export_progress");
  expect(progress.framesDone).toBe(60);
  expect(progress.totalFrames).toBe(60);
  expect(progress.active).toBe(false);
});

test("cancel export stops the runtime and says so", async ({ page, request }) => {
  // A long comp gives the cancel button time to land mid-render.
  await api(request, "open_project", {
    json: JSON.stringify(projectFixture(2_400_000)),
  });

  await page.reload();
  await expect(page.locator(".loading-screen")).toHaveCount(0);
  await page.locator(".export-button").click();
  await page.getByRole("button", { name: "Export MP4" }).click();
  const meter = page.getByRole("progressbar", { name: "Export progress" });
  await expect(meter).toBeVisible();
  await page.getByRole("button", { name: "Cancel export" }).click();
  await expect(page.getByText("Export canceled.")).toBeVisible({ timeout: 10_000 });
  await expect(meter).toHaveCount(0);
  // Nothing partial ships: the runtime slot is free again.
  await expect
    .poll(async () => (await api(page.request, "export_progress")).active)
    .toBe(false);
});
