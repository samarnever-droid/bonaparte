/**
 * AI-friendliness end to end: edits made OUTSIDE the browser (the way an AI
 * agent works — raw bridge calls) appear in the open editor without a
 * reload, and rejected ops explain themselves.
 */
import { test, expect, type Page, type APIRequestContext } from "@playwright/test";

const headers = { "X-Bonaparte-Client": "editor" };

async function api(request: APIRequestContext, name: string, data: unknown = {}) {
  const response = await request.post(`/api/${name}`, { headers, data });
  return response;
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
    name: "AI friendly",
    comps: {
      "1": {
        id: 1,
        name: "Live comp",
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

test("an AI adding a layer through the bridge updates the open editor live", async ({ page }) => {
  // The agent's view: plain bridge calls, no browser involved.
  const response = await api(page.request, "apply", {
    op: {
      type: "addLayer",
      comp: 1,
      layer: card(2, "From the AI"),
    },
  });
  expect(response.ok()).toBeTruthy();
  // No reload, no user action: the layer surfaces in the timeline.
  await expect(page.getByLabel("Select layer From the AI", { exact: true })).toBeVisible({
    timeout: 5000,
  });
  // And it renders (next_layer was assigned by the model).
  const state = await (await api(page.request, "state")).json();
  expect(Object.values(state.project.comps["1"].layers).length).toBe(2);

  // The editor's undo (user side) reverts the AI's edit: one entry, not spam.
  await page.getByLabel("Undo", { exact: true }).click();
  await expect
    .poll(async () => {
      const after = await (await api(page.request, "state")).json();
      return Object.values(after.project.comps["1"].layers).length;
    }, { timeout: 5000 })
    .toBe(1);
});

test("an AI renaming and animating a layer shows up live and stays one undo", async ({ page }) => {
  const batch = {
    type: "batch",
    label: "AI: title pass",
    ops: [
      { type: "renameLayer", comp: 1, layer: 1, name: "Renamed by AI" },
      {
        type: "addKeyframe",
        comp: 1,
        layer: 1,
        property: "Position",
        key: { time: 0, value: { Vec2: [0, -40] }, easing: "Linear" },
      },
      {
        type: "addKeyframe",
        comp: 1,
        layer: 1,
        property: "Position",
        key: { time: 240000, value: { Vec2: [0, 40] }, easing: "Linear" },
      },
    ],
  };
  expect((await api(page.request, "apply", { op: batch })).ok()).toBeTruthy();
  // Timeline reflects the new name without a reload.
  await expect(page.getByLabel("Select layer Renamed by AI", { exact: true })).toBeVisible({
    timeout: 5000,
  });
  // Expand reveals the animated property with both keys.
  await page.getByLabel("Expand Renamed by AI", { exact: true }).click();
  await expect(
    page.getByLabel("Position keyframe at 0.00 seconds", { exact: true }),
  ).toBeVisible();
  // One user undo reverts the whole AI batch.
  await page.getByLabel("Undo", { exact: true }).click();
  await expect
    .poll(
      async () =>
        (await (await api(page.request, "state")).json()).project.comps["1"].layers["1"].name,
      { timeout: 5000 },
    )
    .toBe("Base");
});

test("rejected AI ops answer with a corrective hint", async ({ page }) => {
  const response = await api(page.request, "apply", {
    op: {
      type: "setValue",
      comp: 1,
      layer: 1,
      property: "Hue",
      value: { Scalar: 1 },
    },
  });
  expect(response.status()).toBe(400);
  const body = await response.json();
  const message = String(body.error ?? "");
  expect(message).toContain("Position|Scale|Rotation");
  expect(message).toContain("/api/describe");
});
