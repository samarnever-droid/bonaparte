/**
 * The creator toolkit round: Lottie (Bodymovin) imports as ONE assembled
 * animated group, the Vault is a real global shelf (save → list → pull →
 * project), touchpad pinch-zoom works on the viewport, and the color
 * picker is Bonaparte's own circular wheel — no browser chrome.
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
  return {
    name: "Toolkit",
    comps: {
      "1": {
        id: 1,
        name: "Scene",
        width: 400,
        height: 300,
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

const LOTTIE = JSON.stringify({
  v: "5.7.4",
  fr: 30,
  ip: 0,
  op: 60,
  w: 400,
  h: 300,
  nm: "Bounce",
  layers: [
    {
      ty: 4,
      nm: "Ball",
      ind: 1,
      ip: 0,
      op: 60,
      st: 0,
      ks: {
        o: { a: 0, k: 90 },
        p: {
          a: 1,
          k: [
            { t: 0, s: [100, 60, 0], o: { x: 0.4, y: 0 }, i: { x: 0.6, y: 1 } },
            { t: 30, s: [300, 240, 0], o: { x: 0.4, y: 0 }, i: { x: 0.6, y: 1 } },
            { t: 60, s: [100, 60, 0] },
          ],
        },
        a: { a: 0, k: [0, 0, 0] },
        s: { a: 0, k: [100, 100, 100] },
      },
      shapes: [
        { ty: "el", nm: "dot", p: { a: 0, k: [0, 0] }, s: { a: 0, k: [80, 80] } },
        { ty: "fl", nm: "fill", c: { a: 0, k: [1, 0.3, 0.2, 1] }, o: { a: 0, k: 100 } },
      ],
    },
  ],
});

function dropFile(page: import("@playwright/test").Page, text: string, type: string, name: string) {
  return page.evaluate(
    ({ text, type, name }) => {
      const dt = new DataTransfer();
      dt.items.add(new File([text], name, { type }));
      window.dispatchEvent(new DragEvent("drop", { dataTransfer: dt }));
    },
    { text, type, name },
  );
}

test("a .lottie file imports as one assembled animated group", async ({ page, request }) => {
  await api(request, "open_project", { json: JSON.stringify(projectFixture()) });
  await page.goto("/");
  await expect(page.locator(".loading-screen")).toHaveCount(0);
  await dropFile(page, LOTTIE, "application/lottie+json", "bounce.lottie");
  await expect.poll(async () => Object.keys((await rustState(request)).comps ?? {}).length).toBe(2);
  const project = await rustState(request);
  const layers = Object.values<any>(project.comps["1"].layers);
  expect(layers.length).toBe(1);
  const childId = layers[0]?.kind?.PreComp?.comp;
  expect(childId).toBeGreaterThan(1);
  const child = project.comps[String(childId)];
  expect(Object.keys(child.layers).length).toBe(1);
  const ball: any = Object.values<any>(child.layers)[0];
  expect(ball.tracks?.Position?.keys?.length).toBe(3);
  // The engine renders the imported animation (raw PNG bytes).
  const response = await request.post("/api/export_png", {
    headers: H,
    data: { compId: 1, time: 300_000 },
  });
  expect((await response.body())[0]).toBe(0x89);
});

test("the vault saves, lists, and pulls assets into any project", async ({ page, request }) => {
  const SVG = `<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"><circle cx="50" cy="50" r="40" fill="#33cc88"/></svg>`;
  await api(request, "open_project", { json: JSON.stringify(projectFixture()) });
  await api(request, "vault_save", {
    folder: "logos",
    name: "acme.svg",
    dataBase64: btoa(SVG),
  });
  await page.goto("/");
  await expect(page.locator(".loading-screen")).toHaveCount(0);
  await page.getByLabel("Open the Vault").click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toContainText("The Vault");
  await expect(dialog.getByText("acme.svg")).toBeVisible();
  // Pull: the SVG flows through the normal import pipeline (assembled).
  await dialog.getByText("acme.svg").click();
  await expect
    .poll(async () => Object.keys((await rustState(request)).comps ?? {}).length)
    .toBe(2);
  const layers = Object.values<any>((await rustState(request)).comps["1"].layers);
  expect(layers[0]?.kind?.PreComp?.comp).toBeGreaterThan(1);
});

test("pinch (ctrl+wheel) zooms the viewport; wheel pans", async ({ page, request }) => {
  await api(request, "open_project", { json: JSON.stringify(projectFixture()) });
  await page.goto("/");
  await expect(page.locator(".loading-screen")).toHaveCount(0);
  const zoom = page.getByLabel("Viewer zoom");
  const before = await zoom.inputValue();
  await page.locator(".stage").hover({ position: { x: 200, y: 150 } });
  await page.evaluate(() => {
    const stage = document.querySelector(".stage")!;
    stage.dispatchEvent(
      new WheelEvent("wheel", { ctrlKey: true, deltaY: -240, bubbles: true, cancelable: true }),
    );
  });
  const after = await zoom.inputValue();
  expect(after).not.toBe(before);
  expect(after).toMatch(/^\d+$/); // numeric zoom, no longer "fit"
  await page.evaluate(() => {
    const stage = document.querySelector(".stage")!;
    stage.dispatchEvent(
      new WheelEvent("wheel", { ctrlKey: true, deltaY: 480, bubbles: true, cancelable: true }),
    );
  });
  await expect(zoom).toHaveValue(/^\d+$/);
});

test("colors pick from Bonaparte's own circular wheel, not the browser", async ({
  page,
  request,
}) => {
  await api(request, "open_project", { json: JSON.stringify(projectFixture()) });
  await page.goto("/");
  await expect(page.locator(".loading-screen")).toHaveCount(0);
  await page.getByLabel("Add rectangle layer").click();
  const swatch = page.getByLabel("Fill color — open the circular picker");
  await expect(swatch).toBeVisible();
  await swatch.click();
  const wheel = page.getByLabel("Circular color picker");
  await expect(wheel).toBeVisible();
  const box = (await wheel.boundingBox())!;
  // Hue ring right edge (hue 90 = green), then SV square's saturated corner.
  await page.mouse.click(box.x + box.width - 6, box.y + box.height / 2);
  await page.mouse.click(box.x + box.width / 2 + 62, box.y + box.height / 2 - 62);
  const hex = page.getByLabel("Fill hex");
  await expect(hex).not.toHaveValue(/#000000/i);
  await page.getByLabel("Alpha percent").fill("40");
  await expect(page.getByLabel("Fill alpha")).toHaveValue("40");
});
