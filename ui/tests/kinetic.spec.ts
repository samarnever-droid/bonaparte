/**
 * Kinetic ⚡ — the lyric-video subsystem, end to end through the real UI:
 * drop a click track, Detect beats, open the Lyric video dialog, type
 * lines, Generate → word layers with keyframe tracks land in the engine,
 * the sound-design lane appears, and the comp still renders.
 * Needs ffmpeg on PATH; skips without it.
 */
import { test, expect, type APIRequestContext } from "@playwright/test";
import { execFileSync } from "child_process";
import { readFileSync, existsSync, mkdirSync } from "fs";
import path from "path";

const H = { "X-Bonaparte-Client": "editor" };
const FIXTURES = path.join("test-results", "kinetic-fixtures");

async function api(request: APIRequestContext, name: string, data: unknown = {}) {
  const response = await request.post(`/api/${name}`, { headers: H, data });
  expect(response.ok(), await response.text()).toBeTruthy();
  return response.json();
}
async function rustState(request: APIRequestContext) {
  return (await api(request, "state")).project;
}

function projectFixture() {
  return {
    name: "Kinetic",
    comps: {
      "1": {
        id: 1,
        name: "Scene",
        width: 640,
        height: 360,
        fps: { num: 30, den: 1 },
        duration: 2_400_000,
        background: [0.05, 0.05, 0.06, 1],
        layer_order: [],
        layers: {},
      },
    },
    media: {},
    next_comp: 2,
    next_layer: 1,
    next_media: 1,
    audio: { tracks: [], markers: [], gain_db: 0, muted: false, bpm: 120, beat_offset: 0 },
  };
}

function ensureFixture(): string | null {
  try {
    execFileSync("ffmpeg", ["-version"], { stdio: "ignore" });
  } catch {
    return null;
  }
  mkdirSync(FIXTURES, { recursive: true });
  const clicks = path.join(FIXTURES, "clicks.wav");
  if (!existsSync(clicks))
    execFileSync(
      "ffmpeg",
      [
        "-hide_banner", "-loglevel", "error",
        "-f", "lavfi",
        "-i",
        "aevalsrc=if(lt(mod(t\\,0.5)\\,0.04)\\,sin(880*2*PI*t)*0.9\\,0.01*sin(440*2*PI*t)):d=8:s=48000",
        "-c:a", "pcm_s16le", "-y", clicks,
      ],
      { stdio: "ignore", timeout: 60000 },
    );
  return clicks;
}

function dropFile(page: import("@playwright/test").Page, file: string, type: string, name: string) {
  const b64 = readFileSync(file).toString("base64");
  return page.evaluate(
    ({ b64, type, name }) => {
      const bytes = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
      const dt = new DataTransfer();
      dt.items.add(new File([bytes], name, { type }));
      window.dispatchEvent(new DragEvent("drop", { dataTransfer: dt }));
    },
    { b64, type, name },
  );
}

test("typed lyrics become a beat-synced animated sequence", async ({ page, request }) => {
  const clicks = ensureFixture();
  test.skip(!clicks, "ffmpeg not available");
  await api(request, "open_project", { json: JSON.stringify(projectFixture()) });
  await page.goto("/");
  await expect(page.locator(".loading-screen")).toHaveCount(0);
  await dropFile(page, clicks!, "audio/wav", "clicks.wav");

  // Detect beats from the clip menu.
  let audioId = 0;
  await expect
    .poll(async () => {
      const project = await rustState(request);
      const entry = Object.entries<any>(project.media ?? {}).find(([, m]: any) => m.audio);
      audioId = entry ? Number(entry[0]) : 0;
      return audioId;
    })
    .toBeGreaterThan(0);
  const clip = page.getByLabel("Audio clip clicks.wav");
  await expect(clip).toBeVisible();
  await clip.click({ button: "right" });
  await page.getByRole("menu").getByText("Detect beats").click();
  await expect
    .poll(async () => (await rustState(request)).media[String(audioId)]?.audio?.beat_grid?.length ?? 0)
    .toBeGreaterThanOrEqual(14);

  // Open the Lyric video dialog and generate.
  await clip.click({ button: "right" });
  await page.getByRole("menu").getByText("Lyric video ⚡").click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();
  await expect(dialog).toContainText("Type the words");
  await dialog.getByLabel("Lyric lines").fill("we own the night\nhold the beat down");
  await dialog.getByLabel("Motion style").selectOption("pop");
  await dialog.getByText("Generate ⚡").click();

  // Word layers with scale keyframe tracks land in the engine.
  await expect
    .poll(async () => {
      const project = await rustState(request);
      const layers: any[] = Object.values<any>(project.comps["1"].layers ?? {});
      return layers.filter((l) => l.name?.startsWith("Lyric: ")).length;
    })
    .toBe(8); // 4 + 4 words
  const project = await rustState(request);
  const lyricLayers = Object.values<any>(project.comps["1"].layers).filter((l) =>
    l.name?.startsWith("Lyric: "),
  );
  for (const layer of lyricLayers) {
    expect(layer.tracks?.Scale?.keys?.length).toBe(3);
    expect(layer.kind?.Text?.text?.length).toBeGreaterThan(0);
    expect(layer.duration).toBeGreaterThan(0);
  }
  // One undo removes the whole sequence.
  const layersBefore = Object.keys(project.comps["1"].layers).length;

  // Sound design: the impact lane with one clip per downbeat.
  await expect
    .poll(async () => {
      const state = await rustState(request);
      const tracks: any[] = state.comps["1"].audio?.tracks ?? [];
      const lane = tracks.find((t) => t.name?.includes("Sound design"));
      return lane?.clips?.length ?? 0;
    })
    .toBeGreaterThanOrEqual(2);
  const fresh = await rustState(request);
  const synthAsset = Object.values<any>(fresh.media).find((m) =>
    m.name?.includes("Impact"),
  );
  expect(synthAsset?.audio?.frames).toBeGreaterThan(10000);

  // And the engine renders the lyric timeline (raw PNG bytes).
  const response = await request.post("/api/export_png", {
    headers: H,
    data: { compId: 1, time: 300_000 },
  });
  expect(response.ok()).toBeTruthy();
  const png = await response.body();
  expect(png[0]).toBe(0x89);

  // Undo twice: sound design gone, then the lyric layers gone.
  await page.keyboard.press("Control+z");
  await expect
    .poll(async () => {
      const state = await rustState(request);
      const tracks: any[] = state.comps["1"].audio?.tracks ?? [];
      return tracks.filter((t) => t.name?.includes("Sound design")).length;
    })
    .toBe(0);
  await page.keyboard.press("Control+z");
  await expect
    .poll(async () => Object.keys((await rustState(request)).comps["1"].layers ?? {}).length)
    .toBe(layersBefore - 8);
});
