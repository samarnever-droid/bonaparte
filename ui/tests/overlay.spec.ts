/**
 * Chrome layering: overlapping transient surfaces (top menus, the floating
 * performance panel, the group-selection pill) must never swallow each
 * other's clicks — the bug class that makes an editor feel broken.
 */
import { test, expect, type APIRequestContext } from "@playwright/test";

test.describe.configure({ mode: "serial" });
test.use({ viewport: { width: 1100, height: 620 } });

const card = (id: number) => ({
  id,
  name: `Card ${id}`,
  kind: {
    Shape: {
      color: [0.3 + id * 0.2, 0.5, 0.4, 1],
      generator: null,
      style: { size: [80, 60], corner_radius: 6, stroke_width: 0, stroke_color: [1, 1, 1, 1] },
    },
  },
  start: 0,
  duration: 240000,
  transform: {
    position: [id * 40 - 60, 0],
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
});
const boot = async (request: APIRequestContext, page: import("@playwright/test").Page) => {
  // Deterministic document: two layers, so Ctrl+A selects exactly two.
  const headers = { "X-Bonaparte-Client": "editor" };
  const r = await request.post("/api/open_project", {
    headers,
    data: {
      json: JSON.stringify({
        name: "Overlay",
        comps: {
          "1": {
            id: 1,
            name: "Scene",
            width: 320,
            height: 180,
            fps: { num: 30, den: 1 },
            duration: 240000,
            background: [0, 0, 0, 1],
            layer_order: [1, 2],
            layers: { "1": card(1), "2": card(2) },
          },
        },
        media: {},
        next_comp: 2,
        next_layer: 3,
        next_media: 1,
      }),
    },
  });
  expect(r.ok(), await r.text()).toBeTruthy();
  await page.goto("/");
  await expect(page.locator(".loading-screen")).toHaveCount(0);
};

test("the File menu stays clickable above the open performance panel", async ({
  page,
  request,
}) => {
  await boot(request, page);
  await page.getByLabel("Preview performance").click();
  await expect(page.getByText("ACTUAL EXECUTION")).toBeVisible();
  // Menu opens ON TOP of the panel's click-away sheet and its items hit.
  await page.getByRole("button", { name: "File", exact: true }).click();
  const script = page.getByRole("button", { name: /Run script/ });
  await expect(script).toBeVisible();
  await script.click();
  await expect(page.locator(".script-editor")).toBeVisible();
  await page.keyboard.press("Escape");
});

test("Escape closes a top menu before it touches the selection", async ({ page, request }) => {
  await boot(request, page);
  await page.keyboard.press("Control+a");
  await expect(page.getByText("2 selected")).toBeVisible();
  await page.getByRole("button", { name: "File", exact: true }).click();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("button", { name: "File", exact: true })).not.toHaveClass(/active/);
  // The menu ate the Escape: the selection (and its pill) survive.
  await expect(page.getByText("2 selected")).toBeVisible();
});

test("the group pill floats above the canvas, never over its caption", async ({
  page,
  request,
}) => {
  await boot(request, page);
  await page.keyboard.press("Control+a");
  const pill = page.getByRole("group", { name: "Group tools" });
  await expect(pill).toBeVisible();
  const canvas = page.getByLabel("Rendered composition");
  const p = (await pill.boundingBox())!;
  const c = (await canvas.boundingBox())!;
  // Sits in the upper band of the canvas column — clear of the caption
  // strip that lives under the frame (the old bottom position hit it).
  expect(p.y + p.height / 2).toBeLessThan(c.y + c.height / 2);
});
