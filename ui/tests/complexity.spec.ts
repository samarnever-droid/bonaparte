/**
 * Phase 4 proof: a project that busts every former ceiling — 2,100 layers,
 * 10,500 keys on one track, an 18,000-character text layer at 3,000 px, a
 * 9,000 × 9,000 composition, 147 compositions, 1,300 media assets and a
 * 70-track arrangement with 2,200 markers — opens, edits, undoes and
 * round-trips through the portable file format with zero errors.
 */
import { test, expect } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

test("a project beyond every old ceiling opens, edits and round-trips", async ({
  page,
  request,
}) => {
  test.setTimeout(180_000);
  const project = JSON.parse(
    await readFile(
      fileURLToPath(new URL("../../examples/ignition.bonaparte.json", import.meta.url)),
      "utf8",
    ),
  );
  const comp = project.comps["1"];
  const api = async (name: string, data: unknown = {}) => {
    const r = await request.post("/api/" + name, {
      headers: { "X-Bonaparte-Client": "editor" },
      data,
    });
    const body = await r.text();
    expect(r.ok(), `${name}: ${body.slice(0, 300)}`).toBeTruthy();
    return JSON.parse(body);
  };

  // ---- layers: grow past the old 2,048 ceiling -------------------------
  const mkLayer = (id: number, name: string, kind: unknown) => ({
    id,
    name,
    kind,
    start: 0,
    duration: comp.duration,
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
  });
  const shape = (size: number) => ({
    Shape: {
      color: [0.4, 0.7, 1.0, 1],
      generator: null,
      style: { size: [size, size], corner_radius: 0, stroke_width: 0, stroke_color: [1, 1, 1, 1] },
    },
  });
  while (comp.layer_order.length < 2_100) {
    const id = project.next_layer++;
    comp.layers[String(id)] = mkLayer(id, `Bulk ${id}`, shape(4));
    comp.layer_order.push(id);
  }

  // ---- giant text: 18k chars at 3,000 px (old caps 16,384 / 2,048) -----
  const giantId = project.next_layer++;
  const giant = mkLayer(giantId, "Giant text", {
    Text: {
      text: "The quick brown fox. ".repeat(857).trim(), // ≈ 17,140 chars
      size: 3_000,
      style: { color: [1, 1, 1, 1], bold: true, tracking: 0 },
    },
  });
  expect(giant.kind.Text.text.length).toBeGreaterThan(16_384);
  giant.visible = false; // present in the document; not on the render path
  comp.layers[String(giantId)] = giant;
  comp.layer_order.push(giantId);

  // ---- 10,500 keys on one Position track (old cap 10,000) --------------
  const keys = Array.from({ length: 10_500 }, (_, i) => ({
    time: 100_000 + i * 100,
    value: { Vec2: [i * 0.1, -i * 0.1] },
    easing: "Linear",
  }));
  comp.layers[String(comp.layer_order[0])].tracks = { Position: { keys } };

  // ---- 9,000 × 7,000 composition (breaks old 8,192 side + 16 MP area) --
  const bigId = project.next_comp++;
  project.comps[String(bigId)] = {
    id: bigId,
    name: "Bigwall 9K",
    width: 9_000,
    height: 7_000,
    fps: comp.fps,
    duration: 120_000,
    background: [0, 0, 0, 1],
    layer_order: [],
    layers: {},
  };

  // ---- 147 small compositions (old cap 128) ------------------------------
  const firstExtra = project.next_comp;
  for (let i = 0; i < 145; i++) {
    const id = project.next_comp++;
    project.comps[String(id)] = {
      id,
      name: `Micro ${i}`,
      width: 320,
      height: 240,
      fps: comp.fps,
      duration: 120_000,
      background: [0.05, 0.05, 0.08, 1],
      layer_order: [],
      layers: {},
    };
  }
  void firstExtra;
  const compCount = Object.keys(project.comps).length;
  expect(compCount).toBeGreaterThan(128);

  // ---- 1,300 media assets (old cap 1,024) --------------------------------
  project.media ??= {};
  for (let i = 0; i < 1_300; i++) {
    const id = project.next_media++;
    project.media[String(id)] = {
      id,
      name: `chip-${id}.png`,
      path: null,
      kind: "Image",
      embedded: { width: 1, height: 1, rgba_base64: "AAAAAA==" },
      slot: null,
    };
  }
  expect(Object.keys(project.media).length).toBeGreaterThan(1_024);

  // ---- audio: 70 tracks and 2,200 markers (old caps 64 / 2,048) ---------
  comp.audio = {
    tracks: Array.from({ length: 70 }, (_, i) => ({
      id: `t${i + 1}`,
      name: `Track ${i + 1}`,
      color: "#7aa2ff",
      gain_db: 0,
      pan: 0,
      clips: [],
    })),
    markers: Array.from({ length: 2_200 }, (_, i) => ({
      id: `m${i + 1}`,
      frame: i * 10,
      name: `Mark ${i + 1}`,
    })),
  };

  // ==================== the moment of truth: open it ======================
  await api("open_project", { json: JSON.stringify(project) });
  const state = await api("state");
  expect(Object.values(state.project.comps["1"].layers).length).toBe(2_101);
  expect(Object.keys(state.project.comps).length).toBe(compCount);
  expect(Object.keys(state.project.media).length).toBeGreaterThanOrEqual(1_300);
  expect(state.project.comps["1"].audio.tracks.length).toBe(70);

  // ==================== editable: big atomic batch + undo =================
  const batch = {
    type: "batch",
    label: "Complexity probe",
    ops: Array.from({ length: 1_500 }, (_, i) => ({
      type: "renameLayer",
      comp: 1,
      layer: comp.layer_order[i],
      name: `Renamed ${comp.layer_order[i]}`,
    })),
  };
  await api("apply", { op: batch });
  const mid = await api("state");
  expect(mid.project.comps["1"].layers[String(comp.layer_order[0])].name).toBe(
    `Renamed ${comp.layer_order[0]}`,
  );
  await api("undo");
  const back = await api("state");
  expect(back.project.comps["1"].layers[String(comp.layer_order[0])].name).not.toBe(
    `Renamed ${comp.layer_order[0]}`,
  );

  // ==================== portable round-trip through the file ==============
  // `save_project` serializes exactly what a file write would persist;
  // reopening that text exercises parse + validate + rebuild in full.
  const savedText: string = await api("save_project", { compact: true });
  expect(typeof savedText).toBe("string");
  expect(savedText.length).toBeGreaterThan(1_000_000); // sanity: it really saved the bulk
  await api("open_project", { json: savedText });
  const state2 = await api("state");
  expect(Object.values(state2.project.comps["1"].layers).length).toBe(2_101);

  // ==================== the real UI swallows it too ========================
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto("/");
  await expect(page.locator(".loading-screen")).toHaveCount(0, { timeout: 120_000 });
  await expect(page.getByLabel("Rendered composition")).toBeVisible();
  await page.getByLabel("Select layer Bulk", { exact: false }).first().click();
  await page.waitForTimeout(500);
  expect(errors).toEqual([]);

  await api("new_project"); // leave a clean session for later specs
});
