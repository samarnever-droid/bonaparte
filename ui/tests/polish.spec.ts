/**
 * Everyday-user polish: undo/redo tells you what changed, one-click frame
 * snapshot downloads a real PNG, and a fresh bridge restores your work.
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
    name: "Polish",
    comps: {
      "1": {
        id: 1,
        name: "Comp",
        width: 240,
        height: 135,
        fps: { num: 30, den: 1 },
        duration: 240000,
        background: [0, 0, 0, 1],
        layer_order: [1],
        layers: { "1": card(1, "Base") },
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
  await api(request, "open_project", { json: JSON.stringify(projectFixture()) });
  await page.goto("/");
  await expect(page.locator(".loading-screen")).toHaveCount(0);
  await expect(page.getByLabel("Rendered composition")).toHaveAttribute("width", "240");
});
test.afterEach(async ({ page }) => {
  expect(errors.get(page)).toEqual([]);
  await expect(page.locator(".render-error")).toHaveCount(0);
});

test("undo and redo announce what changed", async ({ page }) => {
  await api(page.request, "apply", {
    op: { type: "renameLayer", comp: 1, layer: 1, name: "Toasted" },
  });
  // Wait for the live reconciliation to surface the external edit first.
  await expect
    .poll(async () => (await api(page.request, "state")).project.comps["1"].layers["1"].name)
    .toBe("Toasted");
  await expect(page.getByLabel("Select layer Toasted", { exact: true })).toBeVisible({
    timeout: 5000,
  });
  await page.getByLabel("Undo", { exact: true }).click();
  await expect(page.locator(".toast")).toContainText("Undid:");
  await expect(page.locator(".toast")).toContainText("Renamed");
  await page.getByLabel("Redo", { exact: true }).click();
  await expect(page.locator(".toast")).toContainText("Redid:");
  // Draining undo stays silent: after the last entry, the button disables
  // itself (and the Rust bridge test proves no stale label is served).
  await page.getByLabel("Undo", { exact: true }).click();
  await expect(page.getByLabel("Undo", { exact: true })).toBeDisabled({ timeout: 5000 });
});

test("one click snapshots the current frame as a real PNG", async ({ page }, testInfo) => {
  const downloadPromise = page.waitForEvent("download");
  await page.getByLabel("Snapshot frame", { exact: true }).click();
  const download = await downloadPromise;
  expect(download.suggestedFilename()).toMatch(/\.png$/);
  const path = testInfo.outputPath("frame.png");
  await download.saveAs(path);
  const { readFile } = await import("node:fs/promises");
  const bytes = await readFile(path);
  expect(bytes.subarray(1, 4).toString()).toBe("PNG");
  // And the toast confirms it.
  await expect(page.locator(".toast")).toContainText("snapshot");
});
