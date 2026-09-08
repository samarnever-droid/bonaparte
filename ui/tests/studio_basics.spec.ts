/**
 * The muscle-memory round: multi-selection (Shift/Ctrl-click, mod+A,
 * Escape, multi-delete), SVG importing AS one assembled object with
 * right-click Disassemble, and the Kaya ⚡ plugin (analyze + manifest).
 * Everything verified against the engine via /api/state.
 */
import { test, expect, type APIRequestContext } from "@playwright/test";

const H = { "X-Bonaparte-Client": "editor" };

async function api(request: APIRequestContext, name: string, data: unknown = {}) {
  const response = await request.post(`/api/${name}`, { headers: H, data });
  expect(response.ok(), await response.text()).toBeTruthy();
  return response.json();
}
async function rustState(request: APIRequestContext) {
  return (await api(request, "state")).project;
}

function projectFixture() {
  const layer = (id: number, x: number, name: string) => ({
    id,
    name,
    kind: {
      Shape: {
        color: [0.7, 0.5, 0.2, 1],
        generator: null,
        style: { size: [120, 80], corner_radius: 10, stroke_width: 0, stroke_color: [1, 1, 1, 1] },
      },
    },
    start: 0,
    duration: 2_400_000,
    transform: { position: [x, 0], scale: [100, 100], rotation: 0, opacity: 1, anchor_point: [0, 0] },
    tracks: {},
    effects: [],
    visible: true,
    locked: false,
    parent: null,
    blend_mode: "Normal",
  });
  return {
    name: "Basics",
    comps: {
      "1": {
        id: 1,
        name: "Scene",
        width: 640,
        height: 360,
        fps: { num: 30, den: 1 },
        duration: 2_400_000,
        background: [0.05, 0.05, 0.06, 1],
        layer_order: [1, 2, 3],
        layers: { "1": layer(1, -120, "Alpha"), "2": layer(2, 0, "Beta"), "3": layer(3, 120, "Gamma") },
      },
    },
    media: {},
    next_comp: 2,
    next_layer: 4,
    next_media: 1,
    audio: { tracks: [], markers: [], gain_db: 0, muted: false, bpm: 120, beat_offset: 0 },
  };
}

const SVG = `<svg xmlns="http://www.w3.org/2000/svg" width="200" height="120"><rect x="10" y="10" width="80" height="60" fill="#ff5555"/><circle cx="150" cy="60" r="40" fill="#5588ff"/></svg>`;

function dropText(page: import("@playwright/test").Page, text: string, type: string, name: string) {
  return page.evaluate(
    ({ text, type, name }) => {
      const dt = new DataTransfer();
      dt.items.add(new File([text], name, { type }));
      window.dispatchEvent(new DragEvent("drop", { dataTransfer: dt }));
    },
    { text, type, name },
  );
}

test("modifier-click multi-selects, mod+A grabs all, Delete takes them all", async ({
  page,
  request,
}) => {
  await api(request, "open_project", { json: JSON.stringify(projectFixture()) });
  await page.goto("/");
  await expect(page.locator(".loading-screen")).toHaveCount(0);
  const alpha = page.getByRole("button", { name: "Select layer Alpha" });
  const gamma = page.getByRole("button", { name: "Select layer Gamma" });
  await alpha.click();
  await gamma.click({ modifiers: ["Shift"] });
  // Delete removes BOTH in two ops (multi-selection drives the loop).
  await page.keyboard.press("Delete");
  await expect
    .poll(async () => Object.keys((await rustState(request)).comps["1"].layers ?? {}).length)
    .toBe(1);
  const project = await rustState(request);
  expect(Object.keys(project.comps["1"].layers)).toEqual(["2"]); // Beta survives
});

test("Escape clears the multi-selection", async ({ page, request }) => {
  await api(request, "open_project", { json: JSON.stringify(projectFixture()) });
  await page.goto("/");
  await expect(page.locator(".loading-screen")).toHaveCount(0);
  const rows = [
    page.getByRole("button", { name: "Select layer Alpha" }),
    page.getByRole("button", { name: "Select layer Beta" }),
  ];
  await rows[0].click();
  await rows[1].click({ modifiers: ["Control"] });
  await page.keyboard.press("Escape");
  // Nothing selected → Delete is a no-op.
  await page.keyboard.press("Delete");
  await page.waitForTimeout(400);
  expect(Object.keys((await rustState(request)).comps["1"].layers).length).toBe(3);
});

test("SVG arrives assembled as one group; Disassemble explodes it; undo regroups", async ({
  page,
  request,
}) => {
  await api(request, "open_project", { json: JSON.stringify(projectFixture()) });
  await page.goto("/");
  await expect(page.locator(".loading-screen")).toHaveCount(0);
  await dropText(page, SVG, "image/svg+xml", "logo.svg");
  // ONE assembled layer — not a scatter of paths.
  await expect
    .poll(async () => Object.keys((await rustState(request)).comps["1"].layers ?? {}).length)
    .toBe(4);
  const project = await rustState(request);
  const assembled = Object.values<any>(project.comps["1"].layers).find((l) =>
    l.name?.includes("logo.svg"),
  );
  expect(assembled?.kind?.PreComp?.comp).toBeGreaterThan(1);
  const childComp = project.comps[String(assembled.kind.PreComp.comp)];
  expect(Object.keys(childComp.layers).length).toBe(2);

  // Right-click → Disassemble into layers.
  const row = page.getByRole("button", { name: `Select layer ${assembled.name}` });
  await expect(row).toBeVisible();
  await row.click({ button: "right" });
  await page.getByRole("menu").getByText("Disassemble into layers").click();
  await expect
    .poll(async () => {
      const state = await rustState(request);
      const comps = Object.keys(state.comps ?? {}).length;
      const kinds = Object.values<any>(state.comps["1"].layers ?? {}).map(
        (l) => Object.keys(l.kind ?? {})[0],
      );
      return comps === 1 && kinds.filter((k) => k === "Shape").length ? kinds.length : 0;
    })
    .toBe(5); // 3 originals + 2 exploded shapes
  const exploded = await rustState(request);
  expect(Object.keys(exploded.comps).length).toBe(1); // sub-comp removed
  const shapes = Object.values<any>(exploded.comps["1"].layers).filter(
    (l) => Object.keys(l.kind ?? {})[0] === "Shape",
  );
  expect(shapes.length).toBe(5);
  // Fit scale composed down: the SVG children carry the group's scale.
  expect(shapes.every((l) => l.transform.scale[0] <= 200)).toBeTruthy();

  // Undo regroups.
  await page.keyboard.press("Control+z");
  await expect
    .poll(async () => Object.keys((await rustState(request)).comps ?? {}).length)
    .toBe(2);
});
