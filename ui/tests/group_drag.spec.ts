/**
 * Group manipulation: multi-selection is only real when 10+ layers ride
 * together — viewport drag, timeline drag, align/distribute, and atomic
 * batch delete/duplicate, each undone in a single step.
 */
import { test, expect, type APIRequestContext } from "@playwright/test";

const headers = { "X-Bonaparte-Client": "editor" };
async function api(request: APIRequestContext, name: string, data: unknown = {}) {
  const response = await request.post(`/api/${name}`, { headers, data });
  expect(response.ok(), await response.text()).toBeTruthy();
  return response.json();
}

const N = 12;
const card = (id: number) => ({
  id,
  name: `Card ${id}`,
  kind: {
    Shape: {
      color: [((id * 30) % 360) / 360, 0.55, 0.4, 1],
      generator: null,
      style: { size: [56, 40], corner_radius: 4, stroke_width: 0, stroke_color: [1, 1, 1, 1] },
    },
  },
  start: (id - 1) * 6000,
  duration: 120000,
  transform: {
    position: [-120 + id * 18 + (id % 3) * 7, -60 + (id % 4) * 14],
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
const fixture = () => ({
  name: "Group play",
  comps: {
    "1": {
      id: 1,
      name: "Scene",
      width: 480,
      height: 270,
      fps: { num: 30, den: 1 },
      duration: 240000,
      background: [0.06, 0.07, 0.06, 1],
      layer_order: Array.from({ length: N }, (_, i) => i + 1),
      layers: Object.fromEntries(Array.from({ length: N }, (_, i) => [String(i + 1), card(i + 1)])),
    },
  },
  media: {},
  next_comp: 2,
  next_layer: N + 1,
  next_media: 1,
});

const layersOf = (state: any) => Object.values(state.project.comps["1"].layers) as any[];

test.beforeEach(async ({ page, request }) => {
  await api(request, "open_project", { json: JSON.stringify(fixture()) });
  await page.goto("/");
  await expect(page.locator(".loading-screen")).toHaveCount(0);
  await expect(page.getByLabel("Rendered composition")).toHaveAttribute("width", "480");
});

test("select-all + a viewport drag moves all twelve layers as one gesture", async ({
  page,
  request,
}) => {
  const before = layersOf(await api(request, "state"));
  await page.keyboard.press("Control+a");
  await expect(page.getByText(`${N} selected`)).toBeVisible();

  // Grab the top-drawn card (12) at its own center and drag by 64×44 px.
  const box = (await page.getByLabel("Rendered composition").boundingBox())!;
  const target = before[N - 1]!.transform.position as [number, number];
  const px = box.x + box.width * (0.5 + target[0] / 480);
  const py = box.y + box.height * (0.5 + target[1] / 270);
  await page.mouse.move(px, py);
  await page.mouse.down();
  await page.mouse.move(px + 64, py + 44, { steps: 6 });
  await page.mouse.up();

  // Everyone rides the same world delta — that's what "group" means.
  const after = await expect
    .poll(async () => {
      const moved = layersOf(await api(request, "state"));
      const dx = moved[0]!.transform.position[0] - before[0]!.transform.position[0];
      const same = new Set(
        moved.map((l, i) => l.transform.position[0] - before[i]!.transform.position[0]),
      );
      return same.size === 1 && dx !== 0 ? dx : 0;
    })
    .toBeGreaterThan(0);
  void after;
  const moved = layersOf(await api(request, "state"));
  const dy = new Set(
    moved.map((l, i) => l.transform.position[1] - before[i]!.transform.position[1]),
  );
  expect(dy.size).toBe(1);
  expect([...dy][0]).toBeGreaterThan(0);

  // One undo step restores the whole group.
  await api(request, "undo", {});
  await expect
    .poll(async () => {
      const back = layersOf(await api(request, "state"));
      return back.every((l, i) => l.transform.position[0] === before[i]!.transform.position[0]);
    })
    .toBe(true);
});

test("align and distribute treat the group as one edit, each one undo step", async ({
  page,
  request,
}) => {
  await page.keyboard.press("Control+a");
  await page.getByLabel("Distribute horizontally", { exact: true }).click();
  await expect
    .poll(async () => {
      const xs = layersOf(await api(request, "state"))
        .map((l) => l.transform.position[0])
        .sort((a, b) => a - b);
      // Pixel-exact positions mean even spacing has gaps within 1 unit.
      const gaps = xs.slice(1).map((x, i) => x - xs[i]!);
      return gaps.every((g) => g > 0) && Math.max(...gaps) - Math.min(...gaps) <= 1.000001;
    })
    .toBe(true);
  await page.getByLabel("Undo", { exact: true }).click();
  await expect
    .poll(async () => {
      const xs = new Set(layersOf(await api(request, "state")).map((l) => l.transform.position[0]));
      return xs.size;
    })
    .toBeGreaterThan(1);

  await page.keyboard.press("Control+a");
  await page.getByLabel("Align left", { exact: true }).click();
  await expect
    .poll(async () => {
      const xs = new Set(layersOf(await api(request, "state")).map((l) => l.transform.position[0]));
      return xs.size;
    })
    .toBe(1);
  // Left edge at the frame ⇒ position.x = -compW/2 + sizeW/2 for uniform 56-wide cards.
  expect(layersOf(await api(request, "state"))[0]!.transform.position[0]).toBe(-212);
});

test("timeline drag rides every selected clip, keyframes and all", async ({ page, request }) => {
  const before = layersOf(await api(request, "state"));
  await page.keyboard.press("Control+a");
  const clip = page.getByLabel(`Move Card ${N} and its keyframes`);
  const box = (await clip.boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2 + 90, box.y + box.height / 2, { steps: 6 });
  await page.mouse.up();

  const deltas = await expect
    .poll(async () => {
      const moved = layersOf(await api(request, "state"));
      const set = new Set(moved.map((l, i) => l.start - before[i]!.start));
      return set.size === 1 && [...set][0]! > 0 ? ([...set][0] as number) : 0;
    })
    .toBeGreaterThan(0);
  void deltas;

  // Single undo: the whole arrangement snaps back together.
  await api(request, "undo", {});
  await expect
    .poll(async () => {
      const back = layersOf(await api(request, "state"));
      return back.every((l, i) => l.start === before[i]!.start);
    })
    .toBe(true);
});

test("group duplicate and group delete are each one undo step", async ({ page, request }) => {
  await page.keyboard.press("Control+a");
  await page.keyboard.press("Control+d");
  await expect.poll(async () => layersOf(await api(request, "state")).length).toBe(2 * N);
  // The fresh copies take the selection (the pill keeps counting).
  await expect(page.getByText(`${N} selected`)).toBeVisible();
  await page.getByLabel("Undo", { exact: true }).click();
  await expect.poll(async () => layersOf(await api(request, "state")).length).toBe(N);

  await page.keyboard.press("Control+a");
  await page.keyboard.press("Delete");
  await expect.poll(async () => layersOf(await api(request, "state")).length).toBe(0);
  await page.getByLabel("Undo", { exact: true }).click();
  await expect.poll(async () => layersOf(await api(request, "state")).length).toBe(N);
});
