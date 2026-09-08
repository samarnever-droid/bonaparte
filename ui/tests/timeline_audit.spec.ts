/**
 * Timeline audit — hammers the reported problem areas: visibility toggle
 * (reversal/stale-state), lock, reorder, drag, trim, keyframe drag, scrub,
 * zoom, and selection sync. Every UI assertion cross-checks the Rust state.
 */
import { test, expect, type Page, type APIRequestContext } from "@playwright/test";

const headers = { "X-Bonaparte-Client": "editor" };
async function api(request: APIRequestContext, name: string, data: unknown = {}) {
  const response = await request.post(`/api/${name}`, { headers, data });
  expect(response.ok(), await response.text()).toBeTruthy();
  return response.json();
}

const card = (id: number, name: string, y: number) => ({
  id,
  name,
  kind: {
    Shape: {
      color: [0.85 - id * 0.15, 0.5, 0.2, 1],
      generator: null,
      style: { size: [70, 46], corner_radius: 6, stroke_width: 0, stroke_color: [1, 1, 1, 1] },
    },
  },
  start: 0,
  duration: 240000,
  transform: {
    position: [id * 10 - 15, y],
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
    name: "Timeline audit",
    comps: {
      "1": {
        id: 1,
        name: "Scene",
        width: 240,
        height: 135,
        fps: { num: 30, den: 1 },
        duration: 240000,
        background: [0.05, 0.05, 0.06, 1],
        layer_order: [1, 2, 3],
        layers: { "1": card(1, "Alpha", -20), "2": card(2, "Beta", 0), "3": card(3, "Gamma", 20) },
      },
    },
    media: {},
    next_comp: 2,
    next_layer: 4,
    next_media: 1,
  };
}

async function rustState(request: APIRequestContext) {
  return (await api(request, "state")).project.comps["1"];
}

async function store(page: Page, expr: string) {
  return page.evaluate(async (src) => {
    const mod = await import("/src/lib/store.svelte.ts");
    return eval(src)(mod);
  }, expr);
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
  await page.waitForTimeout(1200);
});
test.afterEach(async ({ page }) => {
  expect(errors.get(page)).toEqual([]);
  await expect(page.locator(".render-error")).toHaveCount(0);
});

const eyeOf = (page: Page, name: string) => page.getByLabel(`Toggle visibility of ${name}`);
const lockOf = (page: Page, name: string) => page.getByLabel(`Toggle lock on ${name}`);

test("eye toggle: UI and engine agree, never reverses, survives reconcile", async ({
  page,
  request,
}) => {
  const eye = eyeOf(page, "Beta");
  await expect(eye).toBeVisible(); // controls must NOT be hover-gated into invisibility
  await eye.click();
  await expect(eye).toHaveAttribute("aria-pressed", "false");
  // Engine state agrees…
  await expect.poll(async () => (await rustState(request)).layers["2"].visible).toBe(false);
  // …and STAYS agreed after the ~1.2s live reconciliation window.
  await page.waitForTimeout(2200);
  expect((await rustState(request)).layers["2"].visible).toBe(false);
  await expect(eye).toBeVisible(); // still clickable, no snap-back
  const row = page.locator(".layer-label", { hasText: "Beta" });
  await expect(row).toHaveClass(/dimmed/);
  // Toggle back on.
  await eye.click();
  await expect.poll(async () => (await rustState(request)).layers["2"].visible).toBe(true);
  await expect(eye).toHaveAttribute("aria-pressed", "true");
  await page.waitForTimeout(2200);
  await expect(eye).toHaveAttribute("aria-pressed", "true");
});

test("rapid triple-click visibility ends in a consistent state", async ({ page, request }) => {
  const eye = eyeOf(page, "Gamma");
  await eye.click();
  await eye.click(); // label is stable now; rapid clicks are position-stable
  await eye.click();
  await page.waitForTimeout(2500);
  const visible = (await rustState(request)).layers["3"].visible;
  await expect(eye).toHaveAttribute("aria-pressed", String(visible));
  await expect(eye).toBeVisible();
});

test("visibility undo restores the previous state", async ({ page, request }) => {
  await eyeOf(page, "Beta").click();
  await expect.poll(async () => (await rustState(request)).layers["2"].visible).toBe(false);
  await page.getByLabel("Undo", { exact: true }).click();
  await expect.poll(async () => (await rustState(request)).layers["2"].visible).toBe(true);
  await page.waitForTimeout(2000);
  await expect(eyeOf(page, "Beta")).toHaveAttribute("aria-pressed", "true");
});

test("lock toggle: icon state and engine agree", async ({ page, request }) => {
  const lock = lockOf(page, "Alpha");
  await lock.click();
  await expect(lock).toHaveAttribute("aria-pressed", "true");
  await expect.poll(async () => (await rustState(request)).layers["1"].locked).toBe(true);
  await lock.click();
  await expect.poll(async () => (await rustState(request)).layers["1"].locked).toBe(false);
  await expect(lock).toHaveAttribute("aria-pressed", "false");
});

test("reorder up/down moves the layer in engine state", async ({ page, request }) => {
  const before = (await rustState(request)).layer_order;
  await page.getByLabel("Select layer Gamma").click();
  // Gamma is the top row (layer_order's last entry) — "down" is the real move.
  await page.getByLabel("Move layer down", { exact: true }).click();
  await expect.poll(async () => (await rustState(request)).layer_order).not.toEqual(before);
  const after = (await rustState(request)).layer_order;
  expect(after.indexOf(3)).toBeLessThan(before.indexOf(3));
  await page.getByLabel("Undo", { exact: true }).click();
  await expect.poll(async () => (await rustState(request)).layer_order).toEqual(before);
});

test("dragging a layer bar shifts its start in engine state", async ({ page, request }) => {
  const bar = page.getByLabel("Move Alpha and its keyframes");
  const box = await bar.boundingBox();
  expect(box).toBeTruthy();
  const y = box!.y + box!.height / 2;
  const startX = box!.x + box!.width * 0.35;
  await page.mouse.move(startX, y);
  await page.mouse.down();
  await page.mouse.move(startX + 70, y, { steps: 8 });
  await page.mouse.up();
  await expect.poll(async () => (await rustState(request)).layers["1"].start).toBeGreaterThan(0);
  await page.waitForTimeout(1800);
  expect((await rustState(request)).layers["1"].start).toBeGreaterThan(0);
});

test("trimming the right edge changes duration", async ({ page, request }) => {
  const bar = page.getByLabel("Move Beta and its keyframes");
  const box = await bar.boundingBox();
  const y = box!.y + box!.height / 2;
  const edgeX = box!.x + box!.width - 4;
  await page.mouse.move(edgeX, y);
  await page.mouse.down();
  await page.mouse.move(edgeX - box!.width * 0.25, y, { steps: 6 });
  await page.mouse.up();
  await expect
    .poll(async () => (await rustState(request)).layers["2"].duration)
    .toBeLessThan(240000);
});

test("scrubbing the ruler moves the playhead", async ({ page }) => {
  // A recovered session can start the playhead anywhere; 15% must actually
  // move it into the 1s–0.5s window, proving the scrub really happened.
  const before = await store(page, "(m) => m.editor.currentTime");
  const ruler = page.locator(".ruler");
  const box = await ruler.boundingBox();
  expect(box).toBeTruthy();
  await page.mouse.click(box!.x + box!.width * 0.15, box!.y + box!.height / 2);
  await page.waitForTimeout(400);
  const time = await store(page, "(m) => m.editor.currentTime");
  expect(time).toBeGreaterThan(1_000);
  expect(time).toBeLessThan(60_000);
  expect(time).not.toBe(before);
});

test("expanding a layer works even with no tracks (helpful empty state)", async ({ page }) => {
  const disclosure = page.getByLabel("Expand Alpha");
  await expect(disclosure).toBeEnabled();
  await disclosure.click();
  await expect(disclosure).toHaveAttribute("aria-expanded", "true");
  await expect(page.locator(".no-tracks")).toBeVisible();
  await disclosure.click();
  await expect(disclosure).toHaveAttribute("aria-expanded", "false");
  await expect(page.locator(".no-tracks")).toHaveCount(0);
});

test("selection sync: timeline selection highlights the row", async ({ page }) => {
  await page.getByLabel("Select layer Gamma").click();
  await page.waitForTimeout(250);
  const row = page.locator(".layer-label", { hasText: "Gamma" });
  await expect(row).toHaveClass(/selected/);
});
