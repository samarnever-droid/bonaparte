/**
 * Asset previews and vault round-trips: imported media shows its actual
 * pixels in the shelf, right-click sends any asset to the vault (embedded
 * files are re-materialized as real PNG/WAV), and the vault dialog previews
 * what it holds — with one-click audition for audio.
 */
import { test, expect, type APIRequestContext } from "@playwright/test";
import { writeFile, readFile, rm } from "node:fs/promises";

const H = { "X-Bonaparte-Client": "editor" };

async function api(request: APIRequestContext, name: string, data: unknown = {}) {
  const response = await request.post(`/api/${name}`, { headers: H, data });
  const body = await response.text();
  expect(response.ok(), `${name}: ${body.slice(0, 300)}`).toBeTruthy();
  return JSON.parse(body);
}

function projectFixture() {
  return {
    name: "Shelf",
    comps: {
      "1": {
        id: 1,
        name: "Composition 01",
        width: 640,
        height: 360,
        fps: { num: 30, den: 1 },
        duration: 3_600_000,
        background: [0, 0, 0, 1],
        layer_order: [],
        layers: {},
      },
    },
    media: {},
    next_comp: 2,
    next_layer: 1,
    next_media: 1,
  };
}

function redImageRgba(): string {
  const w = 32,
    h = 16,
    bytes = Buffer.alloc(w * h * 4);
  for (let i = 0; i < w * h; i++) {
    bytes[i * 4] = 220;
    bytes[i * 4 + 1] = Math.floor((i % w) * 6);
    bytes[i * 4 + 3] = 255;
  }
  return bytes.toString("base64");
}

function tinyWav(): string {
  const rate = 44100,
    n = Math.floor(rate * 0.06),
    data = Buffer.alloc(n * 2);
  for (let i = 0; i < n; i++) data.writeInt16LE(Math.round(Math.sin(i / 9) * 12000), i * 2);
  const head = Buffer.alloc(44);
  head.write("RIFF", 0);
  head.writeUInt32LE(36 + data.length, 4);
  head.write("WAVE", 8);
  head.write("fmt ", 12);
  head.writeUInt32LE(16, 16);
  head.writeUInt16LE(1, 20);
  head.writeUInt16LE(1, 22);
  head.writeUInt32LE(rate, 24);
  head.writeUInt32LE(rate * 2, 28);
  head.writeUInt16LE(2, 32);
  head.writeUInt16LE(16, 34);
  head.write("data", 36);
  head.writeUInt32LE(data.length, 40);
  return Buffer.concat([head, data]).toString("base64");
}

test("assets show real previews, ride to the vault, and audition there", async ({
  page,
  request,
}) => {
  test.setTimeout(120_000);
  await api(request, "open_project", { json: JSON.stringify(projectFixture()) });
  await api(request, "import_image", {
    name: "crimson.png",
    width: 32,
    height: 16,
    rgbaBase64: redImageRgba(),
    compId: 1,
  });
  await api(request, "import_audio", {
    compId: 1,
    name: "tiny.wav",
    dataBase64: tinyWav(),
    startFrame: 0,
  });

  await page.goto("/");
  await expect(page.locator(".loading-screen")).toHaveCount(0);
  // Opening a project with audio lands the sidebar on its Audio tab —
  // the asset shelf lives under Project.
  await page.getByRole("button", { name: "Project", exact: true }).click();

  // The image row carries a real pixel thumbnail; the audio row an icon.
  const imageRow = page.locator(".asset-item", { hasText: "crimson.png" });
  const imgThumb = imageRow.locator("img.asset-thumb");
  await expect(imgThumb).toBeVisible({ timeout: 10_000 });
  await expect(imgThumb).toHaveAttribute("src", /^data:image\/png/);
  await expect
    .poll(
      async () => {
        const src = (await imgThumb.getAttribute("src")) ?? "";
        return src.length;
      },
      { timeout: 10_000 },
    )
    .toBeGreaterThan(200);
  const audioRow = page.locator(".asset-item", { hasText: "tiny.wav" });
  await expect(audioRow.locator("span.asset-thumb")).toBeVisible();

  // Right-click → Send to vault: an embedded image becomes a real PNG file.
  await imageRow.click({ button: "right" });
  await page.getByRole("menuitem", { name: "Send to vault" }).click();
  const listAfterImage = await api(request, "vault_list");
  const images = (listAfterImage.folders ?? []).find((f: any) => f.name === "images");
  expect((images?.entries ?? []).some((e: any) => e.file === "crimson.png")).toBeTruthy();

  // Audio travels too, sniffed to a .wav name.
  await audioRow.click({ button: "right" });
  await page.getByRole("menuitem", { name: "Send to vault" }).click();
  await expect
    .poll(async () => {
      const l = await api(request, "vault_list");
      const folder = (l.folders ?? []).find((f: any) => f.name === "audio");
      return (folder?.entries ?? []).some((e: any) => e.file === "tiny.wav");
    })
    .toBe(true);

  // The vault dialog shows the picture and offers the speaker.
  await page.getByLabel("Open the Vault").click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByText("crimson.png")).toBeVisible();
  await expect(
    dialog.locator(".vault-entry").filter({ hasText: "crimson.png" }).locator("img"),
  ).toBeVisible({ timeout: 15_000 });
  const audition = dialog.getByLabel("Audition tiny.wav");
  await expect(audition).toBeVisible();
  await audition.click(); // plays through the browser; icon flips to pause
  await expect(dialog.getByLabel("Audition tiny.wav")).toBeVisible();

  // Close and reopen the dialog — the preview is re-fetched, not stale.
  await dialog.getByRole("button", { name: "Close", exact: true }).click();
  await page.getByLabel("Open the Vault").click();
  await expect(dialog.getByText("crimson.png")).toBeVisible();
});

test("vault copies path-based files natively and refuses ghosts", async ({ request }) => {
  const src = `/tmp/bp-shelf-src-${process.pid}.dat`;
  await writeFile(src, "clip-bytes-through-the-native-copy");
  const saved = await api(request, "vault_save_path", {
    folder: "video",
    name: "clip-from-path.dat",
    path: src,
  });
  expect(saved.saved).toBe("clip-from-path.dat");
  const listing = await api(request, "vault_list");
  const video = (listing.folders ?? []).find((f: any) => f.name === "video");
  expect((video?.entries ?? []).some((e: any) => e.file === "clip-from-path.dat")).toBeTruthy();
  const read = await api(request, "vault_read", { folder: "video", name: "clip-from-path.dat" });
  expect(Buffer.from(read.dataBase64, "base64").toString()).toBe(
    "clip-bytes-through-the-native-copy",
  );

  const response = await request.post("/api/vault_save_path", {
    headers: H,
    data: { folder: "video", name: "ghost.dat", path: "/nope/nowhere.dat" },
  });
  expect(response.ok()).toBe(false);
  expect(await response.text()).toContain("Cannot read");
  await rm(src, { force: true });
});
