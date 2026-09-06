import { test, expect, type Page, type APIRequestContext } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const headers = { "X-Bonaparte-Client": "editor" };
const solid = () => ({
  id: 1,
  name: "Source",
  kind: { Solid: { color: [0.1, 0.1, 0.1, 1] } },
  start: 0,
  duration: 480000,
  transform: { position: [0, 0], scale: [100, 100], rotation: 0, opacity: 1, anchor_point: [0, 0] },
  tracks: {},
  effects: [],
  visible: true,
  locked: false,
  parent: null,
  blend_mode: "Normal",
});
function project(
  layers: ReturnType<typeof solid>[] = [],
  width = 640,
  height = 360,
  duration = 480000,
) {
  return {
    name: "Foundation test",
    comps: {
      "1": {
        id: 1,
        name: "Test composition",
        width,
        height,
        fps: { num: 30, den: 1 },
        duration,
        background: [0, 0, 0, 1],
        layer_order: layers.map((l) => l.id),
        layers: Object.fromEntries(layers.map((l) => [String(l.id), l])),
      },
    },
    media: {},
    next_comp: 2,
    next_layer: layers.length + 1,
    next_media: 1,
  };
}
async function api(request: APIRequestContext, name: string, data: unknown = {}) {
  const response = await request.post(`/api/${name}`, { headers, data });
  expect(response.ok(), await response.text()).toBeTruthy();
  return response.json();
}
async function boot(page: Page, request: APIRequestContext, doc = project()) {
  // Only the isolated 5183/4318 test session is reset. This is fixture setup, not a mock renderer.
  await api(request, "open_project", { json: JSON.stringify(doc) });
  await page.goto("/");
  await expect(page.locator(".loading-screen")).toHaveCount(0);
  await expect(page.getByLabel("Rendered composition")).toHaveAttribute(
    "width",
    String(doc.comps["1"].width),
  );
  await seek(page, "00:00:00:00");
}
async function seek(page: Page, timecode: string) {
  const field = page.getByLabel("Playhead timecode");
  await field.fill(timecode);
  await field.press("Tab");
  await expect(field).toHaveValue(timecode);
}
async function edit(page: Page, label: string, value: string) {
  const input = page.getByLabel(label, { exact: true });
  await input.fill(value);
  await input.press("Tab");
}
async function state(page: Page) {
  return api(page.request, "state");
}
async function source(page: Page) {
  return (await state(page)).project.comps["1"].layers["1"];
}
async function pixel(page: Page) {
  return page
    .getByLabel("Rendered composition")
    .evaluate((node: HTMLCanvasElement) =>
      Array.from(node.getContext("2d")!.getImageData(0, 0, 1, 1).data),
    );
}
async function canvasChecksum(page: Page) {
  return page.getByLabel("Rendered composition").evaluate((node: HTMLCanvasElement) => {
    const bytes = node.getContext("2d")!.getImageData(0, 0, node.width, node.height).data;
    let hash = 2166136261;
    for (const byte of bytes) hash = Math.imul(hash ^ byte, 16777619);
    return hash >>> 0;
  });
}
const errors = new WeakMap<Page, string[]>();
test.beforeEach(async ({ page }) => {
  const list: string[] = [];
  errors.set(page, list);
  page.on("pageerror", (error) => list.push(error.message));
  page.on("dialog", (dialog) => dialog.accept());
});
test.afterEach(async ({ page }) => {
  expect(errors.get(page)).toEqual([]);
  await expect(page.locator(".render-error")).toHaveCount(0);
});

test("composition creation and background are one undoable transaction", async ({
  page,
  request,
}) => {
  await boot(page, request);
  await page.getByRole("button", { name: "File", exact: true }).click();
  await page.getByRole("button", { name: "New composition…", exact: true }).click();
  await page.getByLabel("Composition name", { exact: true }).fill("Portrait study");
  await page.getByLabel("Width", { exact: true }).fill("320");
  await page.getByLabel("Height", { exact: true }).fill("180");
  await page.getByLabel("Duration", { exact: true }).fill("1");
  await edit(page, "Composition background hex", "#FF0000");
  await page.getByRole("button", { name: "Create composition", exact: true }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(page.getByLabel("Rendered composition")).toHaveAttribute("width", "320");
  await expect.poll(() => pixel(page)).toEqual([255, 0, 0, 255]);
  const created = await state(page);
  expect(created.history).toHaveLength(1);
  expect(created.project.comps["2"].background).toEqual([1, 0, 0, 1]);
  await page.getByRole("button", { name: "Undo", exact: true }).click();
  await expect.poll(async () => Object.keys((await state(page)).project.comps).length).toBe(1);
  await page.getByRole("button", { name: "Redo", exact: true }).click();
  await expect.poll(async () => Object.keys((await state(page)).project.comps).length).toBe(2);
  await page.getByLabel("Active composition").selectOption("2");
  await page.getByLabel("Composition settings", { exact: true }).click();
  await page.getByLabel("Width", { exact: true }).fill("480");
  await page.getByRole("button", { name: "Apply settings" }).click();
  await expect(page.getByLabel("Rendered composition")).toHaveAttribute("width", "480");
});

test("text, shape content, canvas gestures, lock and duplicate update Rust state", async ({
  page,
  request,
}) => {
  await boot(page, request);
  await page.getByLabel("Add text layer").click();
  await expect(page.getByLabel("Text content")).toBeVisible();
  const original = await canvasChecksum(page);
  await edit(page, "Text content", "Motion matters.");
  await edit(page, "Layer name", "Hero title");
  await expect.poll(async () => (await source(page)).kind.Text.text).toBe("Motion matters.");
  await expect.poll(() => canvasChecksum(page)).not.toBe(original);
  await page.getByLabel("Add rectangle layer").click();
  await edit(page, "Shape width", "220");
  await edit(page, "Shape height", "120");
  await expect
    .poll(async () => (await state(page)).project.comps["1"].layers["2"].kind.Shape.style.size)
    .toEqual([220, 120]);
  const box = (await page.getByLabel("Rendered composition").boundingBox())!;
  const history = (await state(page)).history.length;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(
    box.x + box.width / 2 + (40 * box.width) / 640,
    box.y + box.height / 2 + (20 * box.height) / 360,
    { steps: 8 },
  );
  await page.mouse.up();
  await expect
    .poll(async () => (await state(page)).project.comps["1"].layers["2"].transform.position)
    .toEqual([40, 20]);
  expect((await state(page)).history).toHaveLength(history + 1);
  await page.getByRole("button", { name: "Undo", exact: true }).click();
  await expect(page.getByLabel("Position X", { exact: true })).toHaveValue("0");
  await page.getByRole("button", { name: "Redo", exact: true }).click();
  await expect(page.getByLabel("Position X", { exact: true })).toHaveValue("40");
  await page.getByLabel("Toggle layer lock").click();
  await expect(page.getByLabel("Position X", { exact: true })).toBeDisabled();
  await expect(page.getByLabel("Duplicate layer", { exact: true })).toBeDisabled();
  await page.getByLabel("Toggle layer lock").click();
  await page.getByLabel("Duplicate layer", { exact: true }).click();
  await expect(page.getByLabel("Layer name", { exact: true })).toHaveValue("Rectangle copy");
  await expect.poll(async () => (await state(page)).project.comps["1"].layer_order.length).toBe(3);
});

test("color grading changes real pixels, supports bypass, order, and parameter keys", async ({
  page,
  request,
}) => {
  await boot(page, request, project([solid()], 160, 90));
  const original = await pixel(page);
  await page.getByRole("button", { name: "Color", exact: true }).click();
  await page.getByRole("button", { name: "Add Color Grade", exact: true }).click();
  await edit(page, "Exposure", "1");
  await expect.poll(async () => (await pixel(page))[0]).toBeGreaterThan(original[0]);
  const graded = await pixel(page);
  await expect(
    page.getByRole("img", { name: "RGB histogram of the rendered frame" }),
  ).toBeVisible();
  await page.getByLabel("Enable Color Grade", { exact: true }).uncheck();
  await expect.poll(() => pixel(page)).toEqual(original);
  await page.getByLabel("Enable Color Grade", { exact: true }).check();
  await expect.poll(() => pixel(page)).toEqual(graded);
  await page.getByLabel("Bypass all effects", { exact: true }).click();
  await expect.poll(() => pixel(page)).toEqual(original);
  await page.getByLabel("Bypass all effects", { exact: true }).click();
  await page.getByLabel("Exposure slider", { exact: true }).focus();
  await page.keyboard.press("ArrowRight");
  await expect(page.getByLabel("Playhead timecode")).toHaveValue("00:00:00:00");
  await expect
    .poll(async () => (await source(page)).effects[0].params.exposure.Float)
    .toBeCloseTo(1.05);
  await page.locator(".sidebar").getByRole("button", { name: "Effects", exact: true }).click();
  await page.locator(".sidebar").getByRole("button", { name: "Vignette", exact: true }).click();
  await page.getByLabel("Move Vignette up", { exact: true }).click();
  await expect
    .poll(async () => (await source(page)).effects.map((e: { effect_id: string }) => e.effect_id))
    .toEqual(["builtin.vignette", "builtin.color_grade"]);
  await page.getByRole("button", { name: "Undo", exact: true }).click();
  await expect
    .poll(async () => (await source(page)).effects[0].effect_id)
    .toBe("builtin.color_grade");
  await page.getByLabel("Keyframe Exposure", { exact: true }).click();
  await seek(page, "00:00:00:15");
  await edit(page, "Exposure", "2");
  await expect
    .poll(async () => (await source(page)).effects[0].tracks.exposure.keys.length)
    .toBe(2);
  await seek(page, "00:00:00:07");
  await expect
    .poll(async () => Number(await page.getByLabel("Exposure", { exact: true }).inputValue()))
    .toBeGreaterThan(1.05);
  await expect
    .poll(async () => Number(await page.getByLabel("Exposure", { exact: true }).inputValue()))
    .toBeLessThan(2);
});

test("image import, project file round trip, invalid open, PNG and MP4 downloads", async ({
  page,
  request,
}, info) => {
  await boot(page, request, project([], 160, 90, 12000));
  const image = fileURLToPath(new URL("./fixtures/pixels.png", import.meta.url));
  const chooser = page.waitForEvent("filechooser");
  await page.getByRole("button", { name: /Bring your ideas in/ }).click();
  await (await chooser).setFiles(image);
  await expect(page.getByLabel("Layer name", { exact: true })).toHaveValue("pixels.png");
  await expect.poll(async () => (await state(page)).project.media["1"].embedded.width).toBe(4);
  const saved = page.waitForEvent("download");
  await page.getByRole("button", { name: "Save", exact: true }).click();
  const savedPath = info.outputPath("image-project.bonaparte");
  await (await saved).saveAs(savedPath);
  const file = JSON.parse(await readFile(savedPath, "utf8"));
  expect(file.version).toBe(4);
  expect(file.project.media["1"].embedded.rgba_base64).toBeTruthy();
  await edit(page, "Project name", "Changed project");
  await page.getByRole("button", { name: "File", exact: true }).click();
  const open = page.waitForEvent("filechooser");
  await page.getByRole("button", { name: /Open project…/ }).click();
  await (await open).setFiles(savedPath);
  await expect(page.getByLabel("Project name", { exact: true })).toHaveValue("Foundation test");
  expect((await state(page)).project.media).toEqual(file.project.media);
  const before = await state(page);
  await page.getByRole("button", { name: "File", exact: true }).click();
  const invalid = page.waitForEvent("filechooser");
  await page.getByRole("button", { name: /Open project…/ }).click();
  await (
    await invalid
  ).setFiles({
    name: "future.bonaparte",
    mimeType: "application/json",
    buffer: Buffer.from(
      JSON.stringify({ format: "bonaparte", version: 99, project: file.project }),
    ),
  });
  await expect(page.getByRole("alert")).toContainText("Unsupported project format/version");
  expect(await state(page)).toEqual(before);
  await page.getByRole("button", { name: "Export", exact: true }).click();
  await page.getByRole("button", { name: /PNG image/ }).click();
  const pngDownload = page.waitForEvent("download");
  await page.getByRole("button", { name: "Export PNG", exact: true }).click();
  const pngPath = info.outputPath("frame.png");
  await (await pngDownload).saveAs(pngPath);
  const png = await readFile(pngPath);
  expect(png.subarray(0, 8)).toEqual(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]));
  expect(png.readUInt32BE(16)).toBe(160);
  expect(png.readUInt32BE(20)).toBe(90);
  const catalog = await api(request, "catalog");
  if (catalog.ffmpeg) {
    await page.getByRole("button", { name: "Export", exact: true }).click();
    await page.getByRole("button", { name: /MP4 video/ }).click();
    const mp4Download = page.waitForEvent("download");
    await page.getByRole("button", { name: "Export MP4", exact: true }).click();
    const mp4Path = info.outputPath("composition.mp4");
    await (await mp4Download).saveAs(mp4Path);
    const probe = JSON.parse(
      execFileSync(
        "ffprobe",
        ["-v", "error", "-show_entries", "stream=nb_frames,width,height", "-of", "json", mp4Path],
        { encoding: "utf8" },
      ),
    );
    expect(probe.streams[0]).toMatchObject({ width: 160, height: 90, nb_frames: "3" });
  }
});

test("motion recipes, key seeking and dragging, easing editor and playback", async ({
  page,
  request,
}) => {
  const l = solid();
  l.name = "Animated source";
  l.duration = 240000;
  await boot(page, request, project([l], 320, 180));
  await page.getByRole("button", { name: "Animate", exact: true }).click();
  await page.getByRole("button", { name: /Rise & reveal/ }).click();
  await expect.poll(async () => (await source(page)).tracks.Position.keys.length).toBe(2);
  expect((await state(page)).history).toHaveLength(1);
  await page.getByLabel("Expand Animated source", { exact: true }).click();
  await page.getByLabel("Position keyframe at 1.00 seconds", { exact: true }).click();
  await expect(page.getByLabel("Playhead timecode")).toHaveValue("00:00:01:00");
  await page.getByLabel("Graph Position on Animated source", { exact: true }).click();
  await expect(page.getByRole("img", { name: "Cubic Bézier easing curve" })).toBeVisible();
  await page.getByRole("button", { name: "Smooth", exact: true }).click();
  await expect
    .poll(async () => (await source(page)).tracks.Position.keys[0].easing.Bezier.p1[0])
    .toBeCloseTo(0.42);
  await page.getByLabel("First easing handle").focus();
  await page.keyboard.press("ArrowRight");
  await expect
    .poll(async () => (await source(page)).tracks.Position.keys[0].easing.Bezier.p1[0])
    .toBeCloseTo(0.44);
  await page.getByLabel("Close graph editor", { exact: true }).click();
  const key = page.getByLabel("Position keyframe at 1.00 seconds", { exact: true });
  const keyBox = (await key.boundingBox())!;
  const ruler = (await page.getByLabel("Timeline ruler").boundingBox())!;
  await page.mouse.move(keyBox.x + keyBox.width / 2, keyBox.y + keyBox.height / 2);
  await page.mouse.down();
  await page.mouse.move(
    keyBox.x + keyBox.width / 2 + ruler.width / 8,
    keyBox.y + keyBox.height / 2,
    { steps: 6 },
  );
  await expect(key).toHaveAttribute("style", /left:\s*37\.5%/);
  expect((await source(page)).tracks.Position.keys[1].time).toBe(120000);
  await page.mouse.up();
  await expect.poll(async () => (await source(page)).tracks.Position.keys[1].time).toBe(180000);
  await page.getByRole("button", { name: "Undo", exact: true }).click();
  await expect.poll(async () => (await source(page)).tracks.Position.keys[1].time).toBe(120000);
  await page.getByRole("button", { name: "Play", exact: true }).click();
  const time = await page.getByLabel("Playhead timecode").inputValue();
  await expect.poll(() => page.getByLabel("Playhead timecode").inputValue()).not.toBe(time);
  await page.getByRole("button", { name: "Pause playback", exact: true }).click();
  const stopped = await page.getByLabel("Playhead timecode").inputValue();
  await page.waitForTimeout(150);
  await expect(page.getByLabel("Playhead timecode")).toHaveValue(stopped);
});

test("layer bars trim and retime transform keys in one undoable gesture", async ({
  page,
  request,
}) => {
  const layer = solid();
  layer.name = "Retimed source";
  layer.duration = 240000;
  await boot(page, request, project([layer], 320, 180));
  await page.getByRole("button", { name: "Animate", exact: true }).click();
  await page.getByRole("button", { name: /Rise & reveal/ }).click();
  await expect.poll(async () => (await source(page)).tracks.Position.keys.length).toBe(2);
  const ruler = (await page.getByLabel("Timeline ruler").boundingBox())!;
  const trim = (await page.getByTitle("Trim out point", { exact: true }).boundingBox())!;
  await page.mouse.move(trim.x + trim.width / 2, trim.y + trim.height / 2);
  await page.mouse.down();
  await page.mouse.move(trim.x + trim.width / 2 - ruler.width / 8, trim.y + trim.height / 2, {
    steps: 5,
  });
  await page.mouse.up();
  await expect.poll(async () => (await source(page)).duration).toBe(180000);
  expect((await source(page)).tracks.Position.keys.map((k: { time: number }) => k.time)).toEqual([
    0, 120000,
  ]);
  const bar = (await page
    .getByLabel("Move Retimed source and its keyframes", { exact: true })
    .boundingBox())!;
  const history = (await state(page)).history.length;
  await page.mouse.move(bar.x + bar.width / 2, bar.y + bar.height / 2);
  await page.mouse.down();
  await page.mouse.move(bar.x + bar.width / 2 + ruler.width / 8, bar.y + bar.height / 2, {
    steps: 5,
  });
  await page.mouse.up();
  await expect.poll(async () => (await source(page)).start).toBe(60000);
  expect((await source(page)).tracks.Position.keys.map((k: { time: number }) => k.time)).toEqual([
    60000, 180000,
  ]);
  expect((await source(page)).duration).toBe(180000);
  expect((await state(page)).history).toHaveLength(history + 1);
  await page.getByRole("button", { name: "Undo", exact: true }).click();
  await expect.poll(async () => (await source(page)).start).toBe(0);
  expect((await source(page)).tracks.Position.keys[0].time).toBe(0);
});

test("editable Orbit example uses the same renderer in all three workspaces", async ({
  page,
  request,
}, info) => {
  await api(request, "load_example");
  await page.goto("/");
  await expect(page.locator(".loading-screen")).toHaveCount(0);
  await expect(page.getByLabel("Rendered composition")).toHaveAttribute("width", "960");
  await expect(page.getByLabel("Layer name", { exact: true })).toHaveValue("Orbital form");
  await page.screenshot({ path: info.outputPath("design.png") });
  await page.getByRole("button", { name: "Color", exact: true }).click();
  await expect(
    page.getByRole("img", { name: "RGB histogram of the rendered frame" }),
  ).toBeVisible();
  await page.screenshot({ path: info.outputPath("color.png") });
  await page.getByRole("button", { name: "Animate", exact: true }).click();
  await expect(page.getByRole("button", { name: /Rise & reveal/ })).toBeVisible();
  await page.getByLabel("Select layer Orbital form", { exact: true }).click();
  await page.getByLabel("Expand Orbital form", { exact: true }).click();
  await page.getByLabel("Graph Rotation on Orbital form", { exact: true }).click();
  await page.screenshot({ path: info.outputPath("animate.png") });
  await page.getByLabel("Close graph editor", { exact: true }).click();
  await page.getByRole("button", { name: "Design", exact: true }).click();
  await page.getByLabel("Preview resolution").selectOption("2");
  await expect(page.getByLabel("Rendered composition")).toHaveAttribute("width", "480");
  await page.getByLabel("Preview performance").click();
  await page.screenshot({ path: info.outputPath("performance.png") });
});
