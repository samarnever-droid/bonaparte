/**
 * Footage lifecycle through the real UI: link a clip by path, watch it play
 * live, lose it, relink it, and get the proxy quietly generated. Everything
 * runs against the studio bridge with a real FFmpeg-generated file — the
 * suite skips entirely when the machine has no FFmpeg.
 */
import { test, expect, type Page, type APIRequestContext } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, renameSync, rmSync } from "node:fs";

const headers = { "X-Bonaparte-Client": "editor" };

async function command(request: APIRequestContext, name: string, data: unknown = {}) {
  const response = await request.post(`/api/${name}`, { headers, data });
  const body = await response.text();
  expect(response.ok(), body).toBeTruthy();
  return JSON.parse(body);
}

function haveFfmpeg() {
  try {
    execFileSync("ffmpeg", ["-version"], { stdio: "ignore" });
    execFileSync("ffprobe", ["-version"], { stdio: "ignore" });
    return true;
  } catch {
    return false;
  }
}

const clipDir = `/tmp/bonaparte-footage-e2e-${process.pid}`;
const clipPath = `${clipDir}/hero.mp4`;
const movedPath = `${clipDir}/moved.mp4`;

test.beforeAll(() => {
  test.skip(!haveFfmpeg(), "no FFmpeg on this machine");
  mkdirSync(clipDir, { recursive: true });
  execFileSync("ffmpeg", [
    "-y",
    "-v",
    "error",
    "-f",
    "lavfi",
    "-i",
    "testsrc2=size=320x180:rate=24:duration=2",
    "-c:v",
    "libx264",
    "-pix_fmt",
    "yuv420p",
    clipPath,
  ]);
});

test.afterAll(() => {
  if (existsSync(clipDir)) rmSync(clipDir, { recursive: true, force: true });
});

test("link a clip by path → live badge, proxy arrives, relink revives an offline asset", async ({
  page,
  request,
}) => {
  test.setTimeout(120_000);
  // A pristine session keeps assertions independent of earlier suites.
  await request.post("/api/new_project", { headers, data: {} });
  await page.goto("/");
  await expect(page.locator(".sidebar")).toBeVisible();

  // The entry point sits under the drop zone, only on machines that can read
  // files — never an upload button pretending to be a linker.
  const linkButton = page.getByRole("button", { name: /Link a clip by path/ });
  await expect(linkButton).toBeVisible();
  await linkButton.click();

  const input = page.getByLabel("Clip path");
  await expect(input).toBeVisible();
  await input.fill(clipPath);
  await page.getByRole("button", { name: /Link footage/ }).click();

  // The library row shows the live chip first (proxy is still cooking).
  const chip = page.locator(".footage-chip").first();
  await expect(chip).toBeVisible({ timeout: 20_000 });
  await expect(chip).toHaveText(/live|proxy/);

  // The background proxy lands on its own — poll for the flip.
  await expect
    .poll(
      async () => {
        const rows = await command(request, "media_status");
        return rows.some((r: { proxy: boolean; online: boolean }) => r.proxy && r.online);
      },
      { timeout: 60_000, message: "proxy should appear without any user action" },
    )
    .toBe(true);
  await expect
    .poll(async () => {
      // accept() of the proxy patch refreshes the client chip too
      const state = await command(request, "state");
      return Object.values(state.project.media).some(
        (asset: any) => asset.footage?.proxy_path != null,
      );
    })
    .toBe(true);

  // Full-framerate playback answers: raw frame bytes flow through the live
  // decode seam (source quality; the proxy only ever serves the preview).
  const grab = async (time: number) => {
    const response = await request.post("/api/render_frame_raw", {
      headers,
      data: { compId: 1, time, bypassEffects: false, sourceQuality: true },
    });
    expect(response.ok(), await response.text()).toBeTruthy();
    return (await response.body()).length;
  };
  expect(await grab(10_000)).toBeGreaterThan(8);
  expect(await grab(150_000)).toBeGreaterThan(8);

  // Offline: the file moves on disk — after a reload the row confesses.
  renameSync(clipPath, movedPath);
  await page.reload();
  await expect(page.locator(".footage-chip").first()).toHaveText("offline", {
    timeout: 20_000,
  });

  // Relink from the row, back to online.
  await page.getByRole("button", { name: /^Relink/ }).first().click();
  await page.getByLabel("Clip path").fill(movedPath);
  await page.getByRole("button", { name: /Relink/ }).last().click();
  await expect
    .poll(
      async () => {
        const rows = await command(request, "media_status");
        return rows.every((r: { online: boolean }) => r.online);
      },
      { timeout: 20_000 },
    )
    .toBe(true);
  await expect(page.locator(".footage-chip").first()).toHaveText(/live|proxy/);

  // Undo the import: footage is history now, the document forgets the file.
  const before = await command(request, "state");
  for (let guard = 0; guard < 12; guard++) {
    await command(request, "undo");
    const state = await command(request, "state");
    if (Object.keys(state.project.media).length === 0) break;
    expect(guard).toBeLessThan(11);
  }
  const cleared = await command(request, "state");
  const leftovers = Object.values(cleared.project.media).filter(
    (asset: any) => asset.footage != null,
  );
  expect(leftovers).toHaveLength(0);
  expect(before).toBeTruthy();
});

test("turbo export control reaches the dialog and the runtime honours draft mode", async ({
  page,
  request,
}) => {
  test.setTimeout(120_000);
  await request.post("/api/new_project", { headers, data: {} });
  await page.goto("/");
  await page.getByRole("button", { name: /^export/i }).first().click();
  const turbo = page.locator(".export-turbo");
  await expect(turbo).toBeVisible({ timeout: 10_000 });
  await expect(turbo).toContainText("Turbo pass");

  // The runtime side of the same switch: a draft export of the demo comp
  // finishes with a real file, at draft settings, via the bridge route the
  // editor uses.
  const started = Date.now();
  const reply = await request.post("/api/export_video", {
    headers,
    data: { compId: 1, time: 0, bypassEffects: false, draft: true },
  });
  expect(reply.ok()).toBeTruthy();
  const bytes = await reply.body();
  expect(bytes.length).toBeGreaterThan(1000);
  const elapsedMs = Date.now() - started;
  // A draft of a 6-second ident must feel instant even on tiny CI boxes.
  expect(elapsedMs).toBeLessThan(90_000);
});
