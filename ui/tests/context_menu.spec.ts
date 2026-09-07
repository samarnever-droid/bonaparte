/**
 * Right-click context menus: canvas, timeline layer rows/bars and keyframe
 * diamonds — every action lands through the real bridge as undoable ops.
 */
import { test, expect, type Page, type APIRequestContext } from "@playwright/test";

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

function projectFixture(order: number[] = [1, 2]) {
  return {
    name: "Context menus",
    comps: {
      "1": {
        id: 1,
        name: "Menu comp",
        width: 240,
        height: 135,
        fps: { num: 30, den: 1 },
        duration: 240000,
        background: [0, 0, 0, 1],
        layer_order: order,
        layers: { 1: shapeLayer(1, "Source"), 2: shapeLayer(2, "Understudy") },
      },
    },
    media: {},
    next_comp: 2,
    next_layer: 3,
    next_media: 1,
  };
}

async function state(page: Page) {
  return (await api(page.request, "state")).project.comps["1"];
}

const errors = new WeakMap<Page, string[]>();
test.beforeEach(async ({ page, request }) => {
  const found: string[] = [];
  errors.set(page, found);
  page.on("pageerror", (e) => found.push(e.message));
  await api(request, "open_project", { json: JSON.stringify(projectFixture([2, 1])) });
  await page.goto("/");
  await expect(page.locator(".loading-screen")).toHaveCount(0);
  await expect(page.getByLabel("Rendered composition")).toHaveAttribute("width", "240");
});
test.afterEach(async ({ page }) => {
  expect(errors.get(page)).toEqual([]);
  await expect(page.locator(".render-error")).toHaveCount(0);
});

test("right-clicking a layer on the canvas offers duplicate, rename, lock and delete", async ({
  page,
}) => {
  await page.getByLabel("Rendered composition").click({ button: "right" }); // center = the shape
  const menu = page.getByRole("menu");
  await expect(menu.getByRole("menuitem", { name: "Duplicate" })).toBeVisible();
  await expect(menu.getByRole("menuitem", { name: "Rename…" })).toBeVisible();
  await expect(menu.getByRole("menuitem", { name: "Delete" })).not.toBeDisabled();

  // Rename through the in-app dialog (no native prompt — desktop safe).
  await menu.getByRole("menuitem", { name: "Rename…" }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();
  await dialog.getByLabel("Layer name", { exact: true }).fill("Renamed from menu");
  await dialog.getByRole("button", { name: "Rename", exact: true }).click();
  await expect.poll(async () => (await state(page)).layers["1"].name).toBe("Renamed from menu");

  // Duplicate straight off the canvas.
  await page.getByLabel("Rendered composition").click({ button: "right" });
  await page.getByRole("menuitem", { name: "Duplicate" }).click();
  await expect
    .poll(async () => Object.values((await state(page)).layers).length)
    .toBe(3);
  const copy = Object.values((await state(page)).layers).find((l: { name: string }) =>
    l.name.endsWith("copy"),
  );
  expect(copy).toBeTruthy();

  // Delete it again; one undo brings it back.
  await page.getByLabel("Rendered composition").click({ button: "right" });
  await page.getByRole("menuitem", { name: "Delete", exact: true }).click();
  await expect
    .poll(async () => Object.values((await state(page)).layers).length)
    .toBe(2);
  await page.keyboard.press("Control+z");
  await expect
    .poll(async () => Object.values((await state(page)).layers).length)
    .toBe(3);
});

test("right-clicking empty canvas creates layers, Escape dismisses the menu", async ({ page }) => {
  await page
    .getByLabel("Rendered composition")
    .click({ button: "right", position: { x: 8, y: 6 } });
  await expect(page.getByRole("menuitem", { name: "New text layer" })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("menu")).toHaveCount(0);
  await page
    .getByLabel("Rendered composition")
    .click({ button: "right", position: { x: 8, y: 6 } });
  await page.getByRole("menuitem", { name: "New text layer" }).click();
  await expect
    .poll(async () => Object.values((await state(page)).layers).length)
    .toBe(3);
  const newest = Object.values((await state(page)).layers).find(
    (l: { kind: Record<string, unknown> }) => "Text" in l.kind,
  );
  expect(newest).toBeTruthy();
});

test("right-clicking the timeline bar locks, hides and reorders layers", async ({ page }) => {
  const bar = page.getByLabel("Move Source and its keyframes");
  await bar.click({ button: "right" });
  await page.getByRole("menuitem", { name: "Lock" }).click();
  await expect.poll(async () => (await state(page)).layers["1"].locked).toBe(true);
  // Locked layers only offer Unlock, and delete/duplicate are disabled.
  await bar.click({ button: "right" });
  await expect(page.getByRole("menuitem", { name: "Unlock" })).toBeVisible();
  await expect(page.getByRole("menuitem", { name: "Delete", exact: true })).toBeDisabled();
  await page.getByRole("menuitem", { name: "Unlock" }).click();
  await expect.poll(async () => (await state(page)).layers["1"].locked).toBe(false);

  await bar.click({ button: "right" });
  await page.getByRole("menuitem", { name: "Hide" }).click();
  await expect.poll(async () => (await state(page)).layers["1"].visible).toBe(false);

  // Understudy is the bottom layer; bring it forward above Source.
  expect((await state(page)).layer_order).toEqual([2, 1]);
  const understudy = page.getByLabel("Move Understudy and its keyframes");
  await understudy.click({ button: "right" });
  await page.getByRole("menuitem", { name: "Bring forward" }).click();
  await expect.poll(async () => (await state(page)).layer_order).toEqual([1, 2]);
});

test("right-clicking a keyframe copies and deletes it", async ({ page }) => {
  for (const [property, key] of [
    ["Position", { time: 0, value: { Vec2: [10, 20] } }],
    ["Scale", { time: 48000, value: { Vec2: [100, 100] } }],
  ] as const) {
    await api(page.request, "apply", {
      op: {
        type: "addKeyframe",
        comp: 1,
        layer: 1,
        property,
        key: { ...key, easing: "Linear" },
      },
    });
  }
  await page.reload();
  await expect(page.locator(".loading-screen")).toHaveCount(0);
  await page.getByLabel("Expand Source", { exact: true }).click();
  const diamond = page.getByLabel("Position keyframe at 0.00 seconds", { exact: true });
  await diamond.click({ button: "right" });
  await page.getByRole("menuitem", { name: "Delete keyframe" }).click();
  await expect
    .poll(async () => (await state(page)).layers["1"].tracks.Position.keys.length)
    .toBe(0);
  // One undo restores the keyframe with its value intact.
  await page.keyboard.press("Control+z");
  await expect
    .poll(async () => (await state(page)).layers["1"].tracks.Position.keys.length)
    .toBe(1);
  expect((await state(page)).layers["1"].tracks.Position.keys[0].value).toEqual({
    Vec2: [10, 20],
  });

  // Copy from the menu, then paste onto Scale at the playhead from the graph editor.
  await diamond.click({ button: "right" });
  await page.getByRole("menuitem", { name: "Copy keyframe" }).click();
  await page.getByLabel("Graph Scale on Source", { exact: true }).click();
  await page.getByRole("button", { name: "Paste at playhead" }).click();
  await expect
    .poll(async () => (await state(page)).layers["1"].tracks.Scale.keys.length)
    .toBe(2);
  // The copied Position value travels onto Scale (pasted at the playhead).
  const values = (await state(page)).layers["1"].tracks.Scale.keys.map((k) => k.value);
  expect(values).toContainEqual({ Vec2: [10, 20] });
});
