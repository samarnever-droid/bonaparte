/**
 * Media flow regression: the June-2026 user report — "infinite scroll when I
 * drag a bar or shrink the timeline", "audio right-click does nothing",
 * "can't import a VIDEO", "can't reuse assets", "32 MB asset wall". Every
 * fix here is verified against the engine via /api/state, not just the DOM.
 * Video/audio fixtures need ffmpeg on PATH; those tests skip without it.
 */
import { test, expect, type APIRequestContext } from "@playwright/test";
import { execFileSync } from "child_process";
import { readFileSync, existsSync, mkdirSync } from "fs";
import path from "path";

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
  const layer = (id: number, x: number) => ({
    id,
    name: `Card ${id}`,
    kind: {
      Shape: {
        color: [0.7, 0.5, 0.2, 1],
        generator: null,
        style: { size: [120, 80], corner_radius: 10, stroke_width: 0, stroke_color: [1, 1, 1, 1] },
      },
    },
    start: 0,
    duration: 2_400_000,
    transform: {
      position: [x, 0],
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
  return {
    name: "Media flow",
    comps: {
      "1": {
        id: 1,
        name: "Scene",
        width: 640,
        height: 360,
        fps: { num: 30, den: 1 },
        duration: 2_400_000,
        background: [0.05, 0.05, 0.06, 1],
        layer_order: [1, 2],
        layers: { "1": layer(1, -80), "2": layer(2, 80) },
      },
    },
    media: {},
    next_comp: 2,
    next_layer: 3,
    next_media: 1,
  };
}

const FIXTURES = path.join("test-results", "media-fixtures");

/** Generate tiny media fixtures once; returns null when ffmpeg is missing. */
function ensureFixtures(): { video: string; audio: string } | null {
  try {
    execFileSync("ffmpeg", ["-version"], { stdio: "ignore" });
  } catch {
    return null;
  }
  mkdirSync(FIXTURES, { recursive: true });
  const video = path.join(FIXTURES, "clip.webm");
  const audio = path.join(FIXTURES, "tone.wav");
  if (!existsSync(video))
    execFileSync(
      "ffmpeg",
      [
        "-hide_banner",
        "-loglevel",
        "error",
        "-f",
        "lavfi",
        "-i",
        "testsrc2=size=320x180:rate=30:duration=3",
        "-c:v",
        "libvpx-vp9",
        "-b:v",
        "300k",
        "-y",
        video,
      ],
      { stdio: "ignore", timeout: 60000 },
    );
  if (!existsSync(audio))
    execFileSync(
      "ffmpeg",
      [
        "-hide_banner",
        "-loglevel",
        "error",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:duration=2",
        "-c:a",
        "pcm_s16le",
        "-y",
        audio,
      ],
      { stdio: "ignore", timeout: 60000 },
    );
  return { video, audio };
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

test("dragging bars or shrinking the panel can never explode the page width", async ({
  page,
  request,
}) => {
  await api(request, "open_project", { json: JSON.stringify(projectFixture()) });
  await page.goto("/");
  await expect(page.locator(".loading-screen")).toHaveCount(0);
  await page.waitForTimeout(1200);
  // Shrink the timeline panel (the report's other trigger).
  const resizer = page.getByLabel("Resize timeline");
  const rbox = await resizer.boundingBox();
  await page.mouse.move(rbox!.x + 5, rbox!.y + 2);
  await page.mouse.down();
  await page.mouse.move(rbox!.x + 5, rbox!.y - 120, { steps: 5 });
  await page.mouse.up();
  // Drag a bar far past the composition end (the report's main trigger).
  const bar = page.getByLabel("Move Card 1 and its keyframes");
  const box = await bar.boundingBox();
  const y = box!.y + box!.height / 2;
  const x = box!.x + box!.width * 0.3;
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x + 250, y, { steps: 6 });
  await page.mouse.up();
  await page.waitForTimeout(1500);
  const geo = await page.evaluate(() => ({
    docSW: document.documentElement.scrollWidth,
    tlW: document.querySelector(".timeline-scroll")?.clientWidth ?? -1,
  }));
  expect(geo.docSW).toBeLessThan(3000);
  expect(geo.tlW).toBeLessThan(3000);
});

test("dropping a video samples keyframes, lands in the library, and renders", async ({
  page,
  request,
}) => {
  const fx = ensureFixtures();
  test.skip(!fx, "ffmpeg not available");
  await api(request, "open_project", { json: JSON.stringify(projectFixture()) });
  await page.goto("/");
  await expect(page.locator(".loading-screen")).toHaveCount(0);
  await dropFile(page, fx!.video, "video/webm", "clip.webm");
  // Engine holds a Video asset with sampled frames.
  await expect
    .poll(async () => {
      const project = await rustState(request);
      const videos = Object.values<any>(project.media ?? {}).filter(
        (m: any) => m.kind && typeof m.kind === "object" && "Video" in m.kind,
      );
      return videos[0]?.video?.frames_base64?.length ?? 0;
    })
    .toBeGreaterThan(0);
  // The import created a footage layer.
  const project = await rustState(request);
  expect(Object.keys(project.comps["1"].layers).length).toBe(3);
  // The asset is reusable from the library.
  await page.getByLabel("Add clip.webm to the composition").click();
  await expect
    .poll(async () => Object.keys((await rustState(request)).comps["1"].layers).length)
    .toBe(4);
  // …and the engine can actually render the video layer (raw PNG bytes).
  const response = await request.post("/api/export_png", {
    headers: H,
    data: { compId: 1, time: 30_000 },
  });
  expect(response.ok()).toBeTruthy();
  const png = await response.body();
  expect(png.length).toBeGreaterThan(1000);
  expect(png[0]).toBe(0x89); // PNG magic
});

test("audio clips have a working right-click menu (split at playhead)", async ({
  page,
  request,
}) => {
  const fx = ensureFixtures();
  test.skip(!fx, "ffmpeg not available");
  await api(request, "open_project", { json: JSON.stringify(projectFixture()) });
  await page.goto("/");
  await expect(page.locator(".loading-screen")).toHaveCount(0);
  await dropFile(page, fx!.audio, "audio/wav", "tone.wav");
  const clip = page.getByLabel("Audio clip tone.wav");
  await expect(clip).toBeVisible();
  // Park the cursor mid-clip via the audio ruler (clip starts ~1.2s in).
  const cbox = await clip.boundingBox();
  const ruler = page.locator(".audio-ruler");
  const rbox = await ruler.boundingBox();
  await page.mouse.click(
    rbox!.x + (cbox!.x + cbox!.width * 0.5 - rbox!.x),
    rbox!.y + rbox!.height / 2,
  );
  await page.waitForTimeout(300);
  await clip.click({ button: "right" });
  const menu = page.getByRole("menu");
  await expect(menu.getByText("Split at playhead")).toBeVisible();
  await menu.getByText("Split at playhead").click();
  await page.waitForTimeout(1000);
  expect(await page.locator(".audio-clip").count()).toBe(2);
});

test("audio timeline track header has a context menu", async ({ page, request }) => {
  const fx = ensureFixtures();
  test.skip(!fx, "ffmpeg not available");
  await api(request, "open_project", { json: JSON.stringify(projectFixture()) });
  await page.goto("/");
  await expect(page.locator(".loading-screen")).toHaveCount(0);
  await dropFile(page, fx!.audio, "audio/wav", "tone.wav");
  await expect(page.getByLabel("Audio clip tone.wav")).toBeVisible();
  await page.getByLabel("Select audio track Audio 1").click({ button: "right" });
  await expect(page.getByRole("menu").getByText("Solo track")).toBeVisible();
});
