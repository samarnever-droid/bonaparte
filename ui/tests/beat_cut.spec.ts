/**
 * Beat Cut ⚡ — the one-click music-video edit. Drop a video + a click
 * track, Detect beats from the clip menu, then Cut video to beat: the
 * timeline slices the footage into beat-length jump cuts with downbeat
 * pops. Everything is verified against the engine via /api/state.
 * Needs ffmpeg on PATH; skips without it.
 */
import { test, expect, type APIRequestContext } from "@playwright/test";
import { execFileSync } from "child_process";
import { readFileSync, existsSync, mkdirSync } from "fs";
import path from "path";

const H = { "X-Bonaparte-Client": "editor" };
const FIXTURES = path.join("test-results", "beat-fixtures");

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
    name: "Beat Cut",
    comps: {
      "1": {
        id: 1,
        name: "Scene",
        width: 320,
        height: 180,
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

/** Generate a video + a 120 BPM click track once; null when ffmpeg is missing. */
function ensureFixtures(): { video: string; clicks: string } | null {
  try {
    execFileSync("ffmpeg", ["-version"], { stdio: "ignore" });
  } catch {
    return null;
  }
  mkdirSync(FIXTURES, { recursive: true });
  const video = path.join(FIXTURES, "take.webm");
  const clicks = path.join(FIXTURES, "clicks.wav");
  if (!existsSync(video))
    execFileSync(
      "ffmpeg",
      [
        "-hide_banner", "-loglevel", "error",
        "-f", "lavfi", "-i", "testsrc2=size=320x180:rate=30:duration=3",
        "-c:v", "libvpx-vp9", "-b:v", "300k", "-y", video,
      ],
      { stdio: "ignore", timeout: 60000 },
    );
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
  return { video, clicks };
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

test("detect beats and cut the video to them", async ({ page, request }) => {
  const fx = ensureFixtures();
  test.skip(!fx, "ffmpeg not available");
  await api(request, "open_project", { json: JSON.stringify(projectFixture()) });
  page.on("pageerror", (e) => console.log("PAGEERROR:", e.message));
  await page.goto("/");
  await expect(page.locator(".loading-screen")).toHaveCount(0);
  await dropFile(page, fx!.video, "video/webm", "take.webm");
  await dropFile(page, fx!.clicks, "audio/wav", "clicks.wav");

  // Engine has the click-track asset with an audio payload.
  let audioId = 0;
  await expect
    .poll(async () => {
      const project = await rustState(request);
      const entry = Object.entries<any>(project.media ?? {}).find(
        ([, m]: any) => m.audio,
      );
      audioId = entry ? Number(entry[0]) : 0;
      return audioId;
    })
    .toBeGreaterThan(0);

  // Right-click the audio clip → Detect beats.
  const clip = page.getByLabel("Audio clip clicks.wav");
  await expect(clip).toBeVisible();
  await clip.click({ button: "right" });
  const menu = page.getByRole("menu");
  await expect(menu.getByText("Detect beats")).toBeVisible();
  await menu.getByText("Detect beats").click();

  // The grid lands on the asset and beat markers paint the ruler.
  await expect
    .poll(async () => {
      const project = await rustState(request);
      const audio = project.media[String(audioId)]?.audio;
      return audio?.beat_grid?.length ?? 0;
    })
    .toBeGreaterThanOrEqual(14);
  await expect
    .poll(async () => {
      const project = await rustState(request);
      const markers: { id: string }[] = project.comps["1"].audio?.markers ?? [];
      return markers.filter((m) => m.id.startsWith("beat-")).length;
    })
    .toBeGreaterThanOrEqual(6);
  // Toast confirms the tempo.
  await expect(page.locator(".toast")).toContainText("BPM", { timeout: 5000 });

  // Cut video to beat — the jaw-dropper.
  await clip.click({ button: "right" });
  await page.getByRole("menu").getByText("Cut video to beat").click();

  // Segments appear as real footage layers with differing source offsets.
  await expect
    .poll(async () => {
      const project = await rustState(request);
      const layers: any[] = Object.values<any>(project.comps["1"].layers ?? {});
      return layers.filter((l) => l.name?.startsWith("Beat ")).length;
    })
    .toBeGreaterThanOrEqual(4);
  const project = await rustState(request);
  const segments = Object.values<any>(project.comps["1"].layers).filter((l) =>
    l.name?.startsWith("Beat "),
  );
  const offsets = segments.map((l) => l.kind?.Footage?.source_start ?? 0);
  expect(new Set(offsets).size).toBeGreaterThan(1); // actual jump cuts
  expect(offsets.some((o) => o > 0)).toBeTruthy();

  // And the engine still renders the cut timeline (raw PNG bytes).
  const response = await request.post("/api/export_png", {
    headers: H,
    data: { compId: 1, time: 300_000 },
  });
  expect(response.ok()).toBeTruthy();
  const png = await response.body();
  expect(png[0]).toBe(0x89);
});
