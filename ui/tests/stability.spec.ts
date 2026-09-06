import { test, expect, type Page, type APIRequestContext } from "@playwright/test";
const h = { "X-Bonaparte-Client": "editor" };
async function api(request: APIRequestContext, name: string, data: unknown = {}) {
  const r = await request.post("/api/" + name, { headers: h, data });
  expect(r.ok(), await r.text()).toBeTruthy();
  return r.json();
}
function fixture(text = false) {
  return {
    name: "Stability regression",
    comps: {
      "1": {
        id: 1,
        name: "Main",
        width: 480,
        height: 320,
        fps: { num: 30, den: 1 },
        duration: 1200000,
        background: [0, 0, 0, 1],
        layer_order: [1],
        layers: {
          "1": {
            id: 1,
            name: "Subject",
            kind: text
              ? {
                  Text: {
                    text: "BEFORE",
                    size: 32,
                    style: { color: [1, 1, 1, 1], bold: true, tracking: 0 },
                  },
                }
              : {
                  Shape: {
                    color: [1, 0, 0, 1],
                    generator: null,
                    style: {
                      size: [120, 80],
                      corner_radius: 0,
                      stroke_width: 0,
                      stroke_color: [1, 1, 1, 1],
                    },
                  },
                },
            start: 0,
            duration: 1200000,
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
          },
        },
      },
    },
    media: {},
    next_comp: 2,
    next_layer: 2,
    next_media: 1,
  };
}
async function boot(page: Page, request: APIRequestContext, p = fixture()) {
  await api(request, "open_project", { json: JSON.stringify(p) });
  await page.goto("/");
  await expect(page.locator(".loading-screen")).toHaveCount(0);
  await page.getByLabel("Select layer Subject", { exact: true }).click();
  await page.getByLabel("Preview resolution").selectOption("1");
  await expect(page.getByLabel("Rendered composition")).toHaveAttribute("width", "480");
}
const subject = async (request: APIRequestContext) =>
  (await api(request, "state")).project.comps["1"].layers["1"];
async function checksum(page: Page) {
  return page.getByLabel("Rendered composition").evaluate((node: HTMLCanvasElement) => {
    let hash = 2166136261;
    for (const byte of node.getContext("2d")!.getImageData(0, 0, node.width, node.height).data)
      hash = Math.imul(hash ^ byte, 16777619);
    return hash >>> 0;
  });
}
const errors = new WeakMap<Page, string[]>();
test.beforeEach(({ page }) => {
  const list: string[] = [];
  errors.set(page, list);
  page.on("pageerror", (e) => list.push(e.message));
});
test.afterEach(async ({ page }) => {
  expect(errors.get(page)).toEqual([]);
  await expect(page.locator(".render-error")).toHaveCount(0);
});

test("text and position update while the input remains focused", async ({ page, request }) => {
  await boot(page, request, fixture(true));
  const before = await checksum(page);
  const input = page.getByLabel("Text content", { exact: true });
  await input.fill("LIVE TEXT");
  await expect.poll(() => checksum(page), { timeout: 3000 }).not.toBe(before);
  await expect(input).toBeFocused();
  await expect
    .poll(async () => (await subject(request)).kind.Text.text, { timeout: 3000 })
    .toBe("LIVE TEXT");
  const position = page.getByLabel("Position X", { exact: true });
  const old = await checksum(page);
  await position.fill("70");
  await expect.poll(() => checksum(page), { timeout: 3000 }).not.toBe(old);
  await expect(position).toBeFocused();
  await expect.poll(async () => (await subject(request)).transform.position[0]).toBe(70);
});

test("color and alpha apply on input without a blur click", async ({ page, request }) => {
  await boot(page, request);
  const before = await checksum(page);
  const hex = page.getByLabel("Shape fill hex", { exact: true });
  await hex.fill("#00FF00");
  await expect.poll(() => checksum(page), { timeout: 3000 }).not.toBe(before);
  await expect(hex).toBeFocused();
  await expect.poll(async () => (await subject(request)).kind.Shape.color).toEqual([0, 1, 0, 1]);
  const old = await checksum(page);
  const alpha = page.getByLabel("Shape fill alpha", { exact: true });
  await alpha.fill("35");
  await expect.poll(() => checksum(page), { timeout: 3000 }).not.toBe(old);
  await expect(alpha).toBeFocused();
  await expect.poll(async () => (await subject(request)).kind.Shape.color[3]).toBeCloseTo(0.35, 5);
});

test("rapid visibility toggles use the latest state rather than the captured old value", async ({
  page,
  request,
}) => {
  await boot(page, request);
  await page.route("**/api/apply", async (route) => {
    const response = await route.fetch();
    await new Promise((r) => setTimeout(r, 180));
    await route.fulfill({ response });
  });
  const toggle = page.getByLabel("Toggle layer visibility", { exact: true });
  await toggle.click();
  await toggle.click();
  await expect.poll(async () => (await subject(request)).visible, { timeout: 4000 }).toBe(true);
  await expect(toggle).toHaveAttribute("aria-pressed", "true");
});

test("rotation handle follows the rotated edge and uses the anchor as pivot", async ({
  page,
  request,
}) => {
  const p = fixture();
  p.comps["1"].layers["1"].transform.rotation = 90;
  p.comps["1"].layers["1"].transform.anchor_point = [30, 10];
  await boot(page, request, p);
  const canvas = page.getByLabel("Rendered composition"),
    box = (await canvas.boundingBox())!;
  const css = box.width / 480;
  const pivot = { x: box.x + 240 * css, y: box.y + 160 * css };
  const handle = page.getByLabel("Rotation handle", { exact: true });
  const hb = (await handle.boundingBox())!;
  const start = { x: hb.x + hb.width / 2, y: hb.y + hb.height / 2 };
  // At 90 degrees the top edge points right; the handle must sit outside that edge.
  const edge = await page
    .getByLabel("Layer transform overlay")
    .locator("polygon")
    .getAttribute("points");
  const pts = edge!.split(" ").map((s) => s.split(",").map(Number));
  const topX = (pts[0][0] + pts[1][0]) / 2;
  expect((start.x - box.x) / css).toBeGreaterThan(topX + 5);
  const vx = start.x - pivot.x,
    vy = start.y - pivot.y;
  await page.mouse.move(start.x, start.y);
  await page.mouse.down();
  for (let i = 1; i <= 12; i++) {
    const a = (i * Math.PI) / 24;
    await page.mouse.move(
      pivot.x + vx * Math.cos(a) - vy * Math.sin(a),
      pivot.y + vx * Math.sin(a) + vy * Math.cos(a),
    );
  }
  await page.mouse.up();
  await expect.poll(async () => (await subject(request)).transform.rotation).toBeCloseTo(180, 0);
});

test("timeline fits on composition change and permits time beyond the old viewport", async ({
  page,
  request,
}) => {
  const p = fixture();
  (p.comps as any)["2"] = {
    ...structuredClone(p.comps["1"]),
    id: 2,
    name: "Long timeline",
    duration: 120000 * 90,
    layers: {},
    layer_order: [],
  };
  p.next_comp = 3;
  await boot(page, request, p);
  await page.getByLabel("Timeline zoom", { exact: true }).fill("8");
  await page.getByLabel("Active composition").selectOption("2");
  await expect(page.getByLabel("Timeline zoom", { exact: true })).toHaveValue("1");
  await expect(page.getByLabel("Timeline ruler", { exact: true })).toHaveAttribute(
    "aria-valuemax",
    String(120000 * 90),
  );
  const time = page.getByLabel("Playhead timecode");
  await time.fill("00:01:15:00");
  await time.press("Enter");
  await expect(time).toHaveValue("00:01:15:00");
});

test("a visibility edit during post-transform refinement cannot leave a stuck proxy", async ({
  page,
  request,
}) => {
  await boot(page, request);
  await page.getByLabel("Preview resolution").selectOption("auto");
  await page.waitForTimeout(200);
  await page.route("**/api/preview_frame", async (route) => {
    const response = await route.fetch();
    await new Promise((r) => setTimeout(r, 350));
    await route.fulfill({ response });
  });
  const handle = page.getByLabel("Rendered composition");
  const b = (await handle.boundingBox())!;
  await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2);
  await page.mouse.down();
  await page.mouse.move(b.x + b.width / 2 + 25, b.y + b.height / 2 + 15);
  await page.mouse.up();
  await page.getByLabel("Toggle layer visibility", { exact: true }).click();
  await expect.poll(async () => (await subject(request)).visible).toBe(false);
  await expect(page.locator(".interaction-planes")).toHaveAttribute(
    "data-interaction-active",
    "false",
    { timeout: 5000 },
  );
  await expect(page.getByLabel("Rendered composition")).toHaveCSS("opacity", "1");
});

test("focused typing persists incrementally but remains one undoable edit", async ({
  page,
  request,
}) => {
  await boot(page, request, fixture(true));
  const input = page.getByLabel("Text content", { exact: true });
  await input.fill("LIVE");
  await expect.poll(async () => (await subject(request)).kind.Text.text).toBe("LIVE");
  await expect(input).toBeFocused();
  await input.press("End");
  await input.pressSequentially(" EDIT", { delay: 30 });
  await expect.poll(async () => (await subject(request)).kind.Text.text).toBe("LIVE EDIT");
  expect((await api(request, "state")).history).toHaveLength(1);
  await page.getByRole("button", { name: "Undo", exact: true }).click();
  await expect(input).toHaveValue("BEFORE");
  await page.getByRole("button", { name: "Redo", exact: true }).click();
  await expect(input).toHaveValue("LIVE EDIT");
});

test("effect values update without blur and incomplete numeric prefixes do not change content", async ({
  page,
  request,
}) => {
  const p = fixture();
  (p.comps["1"].layers["1"].kind as any).Shape.color = [0.08, 0.1, 0.12, 1];
  (p.comps["1"].layers["1"].effects as any) = [
    { id: "grade", effect_id: "builtin.color_grade", enabled: true, params: {}, tracks: {} },
  ];
  await boot(page, request, p);
  await page.getByRole("button", { name: "Color", exact: true }).click();
  const before = await checksum(page),
    exposure = page.getByLabel("Exposure", { exact: true });
  await exposure.fill("1.5");
  await expect.poll(() => checksum(page)).not.toBe(before);
  await expect(exposure).toBeFocused();
  await expect
    .poll(async () => (await subject(request)).effects[0].params.exposure?.Float)
    .toBe(1.5);
  await page.getByRole("button", { name: "Design", exact: true }).click();
  const value = page.getByLabel("Position X", { exact: true });
  await value.fill("");
  await page.waitForTimeout(180);
  expect((await subject(request)).transform.position[0]).toBe(0);
  await value.fill("-35");
  await expect.poll(async () => (await subject(request)).transform.position[0]).toBe(-35);
});

test("scaling uses the anchor pivot and Full quality stays Full during the gesture", async ({
  page,
  request,
}) => {
  const p = fixture();
  p.comps["1"].layers["1"].transform.anchor_point = [30, 10];
  await boot(page, request, p);
  const canvas = page.getByLabel("Rendered composition");
  const b = (await canvas.boundingBox())!,
    css = b.width / 480;
  const h = page.getByLabel("Scale handle 3"),
    hb = (await h.boundingBox())!;
  const x = hb.x + hb.width / 2,
    y = hb.y + hb.height / 2;
  const px = b.x + 240 * css,
    py = b.y + 160 * css;
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(px + (x - px) * 2, py + (y - py) * 2, { steps: 6 });
  await expect(canvas).toHaveAttribute("width", "480");
  await expect(canvas).toHaveCSS("opacity", "1");
  await page.mouse.up();
  await expect.poll(async () => (await subject(request)).transform.scale[0]).toBeCloseTo(200, 0);
  await expect.poll(async () => (await subject(request)).transform.scale[1]).toBeCloseTo(200, 0);
  expect((await subject(request)).transform.position).toEqual([0, 0]);
});

test("timeline duration can be extended and far timecode seeks create usable range", async ({
  page,
  request,
}) => {
  await boot(page, request);
  await page.getByLabel("Extend composition by 10 seconds", { exact: true }).click();
  await expect
    .poll(async () => (await api(request, "state")).project.comps["1"].duration)
    .toBe(20 * 120000);
  const time = page.getByLabel("Playhead timecode");
  await time.fill("00:01:05:00");
  await time.press("Enter");
  await expect
    .poll(async () => (await api(request, "state")).project.comps["1"].duration)
    .toBeGreaterThan(65 * 120000);
  await expect(time).toHaveValue("00:01:05:00");
});

test("rotation remains correct beneath a non-uniformly scaled parent", async ({
  page,
  request,
}) => {
  const p: any = fixture();
  const subject = p.comps["1"].layers["1"];
  subject.transform.anchor_point = [20, -10];
  subject.transform.rotation = 15;
  subject.parent = 2;
  const parent = structuredClone(subject);
  parent.id = 2;
  parent.name = "Parent";
  parent.parent = null;
  parent.visible = false;
  parent.transform = {
    position: [0, 0],
    scale: [170, 65],
    rotation: 25,
    opacity: 1,
    anchor_point: [0, 0],
  };
  p.comps["1"].layers["2"] = parent;
  p.comps["1"].layer_order = [2, 1];
  p.next_layer = 3;
  await boot(page, request, p);
  const b = (await page.getByLabel("Rendered composition").boundingBox())!,
    scale = b.width / 480;
  const h = (await page.getByLabel("Rotation handle", { exact: true }).boundingBox())!;
  const x = h.x + h.width / 2,
    y = h.y + h.height / 2,
    cx = b.x + 240 * scale,
    cy = b.y + 160 * scale;
  const c = Math.cos((25 * Math.PI) / 180),
    s = Math.sin((25 * Math.PI) / 180);
  const dx = (x - cx) / scale,
    dy = (y - cy) / scale,
    lx = (c * dx + s * dy) / 1.7,
    ly = (-s * dx + c * dy) / 0.65;
  await page.mouse.move(x, y);
  await page.mouse.down();
  for (let i = 1; i <= 16; i++) {
    const a = (i * Math.PI) / 32,
      rx = lx * Math.cos(a) - ly * Math.sin(a),
      ry = lx * Math.sin(a) + ly * Math.cos(a);
    await page.mouse.move(
      cx + (1.7 * c * rx - 0.65 * s * ry) * scale,
      cy + (1.7 * s * rx + 0.65 * c * ry) * scale,
    );
  }
  await page.mouse.up();
  await expect
    .poll(
      async () => (await api(request, "state")).project.comps["1"].layers["1"].transform.rotation,
    )
    .toBeCloseTo(105, 0);
});

test("late edit acknowledgements do not replace newer focused text", async ({ page, request }) => {
  await boot(page, request, fixture(true));
  await page.route("**/api/apply", async (route) => {
    const response = await route.fetch();
    await new Promise((r) => setTimeout(r, 220));
    await route.fulfill({ response });
  });
  const input = page.getByLabel("Text content", { exact: true });
  await input.fill("A");
  await page.waitForTimeout(170);
  await input.press("End");
  await input.pressSequentially("BCDEF", { delay: 40 });
  await expect.poll(async () => (await subject(request)).kind.Text.text).toBe("ABCDEF");
  await expect(input).toHaveValue("ABCDEF");
  await expect(input).toBeFocused();
  await page.getByRole("button", { name: "Undo", exact: true }).click();
  await expect(input).toHaveValue("BEFORE");
});

test("native color-picker input and shape dimensions update without change events", async ({
  page,
  request,
}) => {
  await boot(page, request);
  await page.getByLabel("Shape fill color", { exact: true }).evaluate((input: HTMLInputElement) => {
    input.value = "#2233ff";
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await expect.poll(async () => (await subject(request)).kind.Shape.color[2]).toBe(1);
  await expect.poll(async () => (await subject(request)).kind.Shape.color[0]).toBeLessThan(0.05);
  const width = page.getByLabel("Shape width", { exact: true });
  await width.fill("190");
  await expect(width).toBeFocused();
  await expect.poll(async () => (await subject(request)).kind.Shape.style.size[0]).toBe(190);
});

test("blank projects start with an editable thirty-second timeline", async ({ page, request }) => {
  await boot(page, request);
  await page.getByLabel("Timeline zoom", { exact: true }).fill("8");
  await page.getByRole("button", { name: "File", exact: true }).click();
  await page.getByRole("button", { name: "New project", exact: true }).click();
  await page.getByRole("button", { name: /Blank project/ }).click();
  await expect
    .poll(async () => (await api(request, "state")).project.comps["1"].duration)
    .toBe(30 * 120000);
  await expect(page.getByLabel("Timeline zoom", { exact: true })).toHaveValue("1");
  await expect(page.getByLabel("Timeline duration seconds", { exact: true })).toHaveValue("30");
});
