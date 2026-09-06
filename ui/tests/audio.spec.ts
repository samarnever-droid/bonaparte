import { test, expect, type Page, type APIRequestContext } from "@playwright/test";
import { fileURLToPath } from "node:url";
import { readFile } from "node:fs/promises";
import { execFileSync } from "node:child_process";
const tone = fileURLToPath(new URL("./fixtures/tone.wav", import.meta.url));
async function api(request: APIRequestContext, name: string, data: unknown = {}) {
  const r = await request.post(`/api/${name}`, {
    headers: { "X-Bonaparte-Client": "editor" },
    data,
  });
  expect(r.ok(), await r.text()).toBeTruthy();
  return r.json();
}
function document() {
  return {
    name: "Audio test",
    comps: {
      "1": {
        id: 1,
        name: "Sound composition",
        width: 320,
        height: 180,
        fps: { num: 24, den: 1 },
        duration: 360000,
        background: [0.015, 0.03, 0.025, 1],
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
const errors = new WeakMap<Page, string[]>();
test.beforeEach(async ({ page, request }) => {
  const list: string[] = [];
  errors.set(page, list);
  page.on("pageerror", (e) => list.push(e.message));
  await api(request, "open_project", { json: JSON.stringify(document()) });
  await page.goto("/");
  await expect(page.locator(".loading-screen")).toHaveCount(0);
  await page.getByLabel("Playhead timecode").fill("00:00:00:00");
  await page.getByLabel("Playhead timecode").press("Tab");
  await page.getByLabel("Audio timeline tab").click();
});
test.afterEach(async ({ page }) => {
  expect(errors.get(page)).toEqual([]);
  await expect(page.locator(".render-error")).toHaveCount(0);
});
async function importTone(page: Page) {
  const choose = page.waitForEvent("filechooser");
  await page
    .locator(".audio-tools")
    .getByRole("button", { name: "Import audio", exact: true })
    .click();
  await (await choose).setFiles(tone);
  await expect(page.getByLabel("Audio clip tone.wav", { exact: true })).toBeVisible();
  await expect(page.getByLabel("Stereo waveform for tone.wav")).toBeVisible();
}
async function edit(page: Page, label: string, value: string) {
  const input = page.getByLabel(label, { exact: true });
  await input.fill(value);
  await input.press("Tab");
}
async function audio(request: APIRequestContext) {
  return (await api(request, "state")).project.comps["1"].audio;
}

test("real audio import, waveform, portable save, undo and redo", async ({
  page,
  request,
}, info) => {
  await importTone(page);
  const a = await audio(request);
  expect(a.tracks).toHaveLength(1);
  expect(a.tracks[0].clips[0].duration_frames).toBe(48000);
  const snapshot = await api(request, "state");
  expect(snapshot.project.media["1"].audio.original_sample_rate).toBe(44100);
  expect(snapshot.project.media["1"].audio.data_base64).toBe("");
  const downloading = page.waitForEvent("download");
  await page.getByRole("button", { name: "Save", exact: true }).click();
  const path = info.outputPath("audio.bonaparte");
  await (await downloading).saveAs(path);
  const saved = JSON.parse(await readFile(path, "utf8"));
  expect(saved.version).toBe(4);
  expect(Buffer.from(saved.project.media["1"].audio.data_base64, "base64")).toEqual(
    await readFile(tone),
  );
  await page.getByRole("button", { name: "Undo", exact: true }).click();
  await expect(page.getByLabel("Audio clip tone.wav", { exact: true })).toHaveCount(0);
  await page.getByRole("button", { name: "Redo", exact: true }).click();
  await expect(page.getByLabel("Audio clip tone.wav", { exact: true })).toBeVisible();
});

test("sample placement, gain/pan automation, fades and split are undoable", async ({
  page,
  request,
}) => {
  await importTone(page);
  await edit(page, "Audio clip start sample", "2401");
  await edit(page, "Audio playhead sample", "3402");
  await edit(page, "Audio clip gain dB", "-6");
  await edit(page, "Audio clip pan", "-0.5");
  await edit(page, "Audio fade in milliseconds", "80");
  await edit(page, "Audio fade out milliseconds", "120");
  await page.getByLabel("Toggle audio gain keyframe").click();
  await edit(page, "Audio clip gain dB", "-3");
  await page.getByLabel("Toggle audio pan keyframe").click();
  await edit(page, "Audio clip pan", "0.4");
  await expect
    .poll(async () => {
      const c = (await audio(request)).tracks[0].clips[0];
      return [
        c.start_frame,
        c.gain_points[0]?.frame,
        c.gain_points[0]?.value,
        c.pan_points[0]?.value,
        c.fade_in?.end,
        c.fade_out?.start,
      ];
    })
    .toEqual([2401, 1001, -3, 0.4, 3840, 42240]);
  await edit(page, "Audio playhead sample", "12345");
  await page.getByLabel("Split audio at playhead").click();
  await expect.poll(async () => (await audio(request)).tracks[0].clips.length).toBe(2);
  await page.getByRole("button", { name: "Undo", exact: true }).click();
  await expect.poll(async () => (await audio(request)).tracks[0].clips.length).toBe(1);
});

test("AudioWorklet consumes real PCM, drives the clock and obeys mute", async ({
  page,
  request,
}) => {
  await importTone(page);
  await page.getByRole("button", { name: "Play", exact: true }).click();
  await expect
    .poll(() =>
      page.evaluate(async () => {
        const { editor, audioTransport } = await import("/src/lib/store.svelte.ts");
        return { state: audioTransport.context?.state, peak: editor.audioMeter.peak[0] };
      }),
    )
    .toMatchObject({ state: "running" });
  await expect
    .poll(() =>
      page.evaluate(async () => {
        const { editor } = await import("/src/lib/store.svelte.ts");
        return editor.audioMeter.peak[0];
      }),
    )
    .toBeGreaterThan(0.05);
  await expect
    .poll(() =>
      page.evaluate(async () => {
        const { editor } = await import("/src/lib/store.svelte.ts");
        return editor.audioCursor;
      }),
    )
    .toBeGreaterThan(1000);
  await page.getByLabel("Mute audio track Audio 1").click();
  await expect(page.getByLabel("Mute audio track Audio 1")).toHaveAttribute("aria-pressed", "true");
  await expect
    .poll(() =>
      page.evaluate(async () => {
        const { editor } = await import("/src/lib/store.svelte.ts");
        return editor.audioMeter.peak[0];
      }),
    )
    .toBeLessThan(0.000001);
  await page.getByRole("button", { name: "Pause playback", exact: true }).click();
  const stopped = await page.evaluate(async () => {
    const { editor } = await import("/src/lib/store.svelte.ts");
    return editor.audioCursor;
  });
  await page.waitForTimeout(200);
  expect(
    await page.evaluate(async () => {
      const { editor } = await import("/src/lib/store.svelte.ts");
      return editor.audioCursor;
    }),
  ).toBe(stopped);
  expect((await audio(request)).tracks[0].muted).toBe(true);
});

test("source slip, reverse, track solo and tempo markers round-trip", async ({ page, request }) => {
  await importTone(page);
  await edit(page, "Audio clip duration samples", "24000");
  await edit(page, "Audio source offset samples", "12000");
  await page.getByLabel("Reverse audio clip").click();
  await expect.poll(async () => (await audio(request)).tracks[0].clips[0].rate).toBe(-1);
  await page.getByLabel("Solo audio track Audio 1").click();
  await edit(page, "Composition tempo", "96");
  await edit(page, "Audio playhead sample", "4801");
  await page.getByLabel("Add audio marker").click();
  await expect
    .poll(async () => ({
      solo: (await audio(request)).tracks[0].solo,
      bpm: (await audio(request)).bpm,
      frame: (await audio(request)).markers[0]?.frame,
    }))
    .toEqual({ solo: true, bpm: 96, frame: 4801 });
});

test("native WAV and MP4 contain the timeline mix, not a post-export soundtrack", async ({
  page,
}, info) => {
  await importTone(page);
  await edit(page, "Audio clip start sample", "4800");
  const wavDownload = page.waitForEvent("download");
  await page.getByRole("button", { name: "Export float WAV mix" }).click();
  const wavPath = info.outputPath("mix.wav");
  await (await wavDownload).saveAs(wavPath);
  const wav = await readFile(wavPath);
  expect(wav.subarray(0, 4).toString()).toBe("RIFF");
  expect(wav.readUInt32LE(24)).toBe(48000);
  expect(wav.readUInt16LE(20)).toBe(3);
  await page.getByRole("button", { name: "Export", exact: true }).click();
  await page.getByRole("button", { name: /MP4 video/ }).click();
  await expect(page.getByRole("dialog")).toContainText("AAC at 256 kbps");
  const mp4Download = page.waitForEvent("download");
  await page.getByRole("button", { name: "Export MP4", exact: true }).click();
  const mp4Path = info.outputPath("mixed.mp4");
  await (await mp4Download).saveAs(mp4Path);
  const probe = JSON.parse(
    execFileSync(
      "ffprobe",
      [
        "-v",
        "error",
        "-show_entries",
        "stream=codec_type,codec_name,channels,sample_rate,nb_frames",
        "-of",
        "json",
        mp4Path,
      ],
      { encoding: "utf8" },
    ),
  );
  expect(probe.streams.find((s: any) => s.codec_type === "video").nb_frames).toBe("72");
  expect(probe.streams.find((s: any) => s.codec_type === "audio")).toMatchObject({
    codec_name: "aac",
    sample_rate: "48000",
    channels: 2,
  });
});

test("malformed audio import preserves the document", async ({ page, request }) => {
  const before = await api(request, "state");
  const choosing = page.waitForEvent("filechooser");
  await page
    .locator(".audio-tools")
    .getByRole("button", { name: "Import audio", exact: true })
    .click();
  await (
    await choosing
  ).setFiles({
    name: "broken.wav",
    mimeType: "audio/wav",
    buffer: Buffer.from("not an audio source"),
  });
  await expect(page.getByRole("alert")).toContainText("Audio decoding failed");
  expect(await api(request, "state")).toEqual(before);
});

test("native EQ, compression, compensated limiter and bus routing stay editable", async ({
  page,
  request,
}) => {
  await importTone(page);
  await page.getByLabel("Create mix bus", { exact: true }).click();
  await edit(page, "Mix bus name", "Music bus");
  await edit(page, "Mix bus gain dB", "-3");
  await page.getByLabel("Select audio track Audio 1", { exact: true }).click();
  await page.getByLabel("Channel output bus", { exact: true }).selectOption({ label: "Music bus" });
  await page.getByLabel("Enable channel EQ", { exact: true }).click();
  await edit(page, "EQ high pass Hz", "80");
  await edit(page, "EQ mid frequency Hz", "2400");
  await edit(page, "EQ mid gain dB", "3");
  await page.getByLabel("Enable channel compressor", { exact: true }).click();
  await edit(page, "Compressor threshold", "-24");
  await edit(page, "Compressor ratio", "4");
  await expect(page.getByRole("img", { name: "Native EQ frequency response" })).toBeVisible();
  await expect
    .poll(async () => {
      const a = await audio(request);
      return {
        bus: a.buses[0].name,
        routed: a.tracks[0].output === a.buses[0].id,
        eq: a.tracks[0].processing.eq.mid_db,
        ratio: a.tracks[0].processing.compressor.ratio,
      };
    })
    .toEqual({ bus: "Music bus", routed: true, eq: 3, ratio: 4 });
  await page.getByLabel("Select master processing", { exact: true }).click();
  await page.getByLabel("Enable master limiter", { exact: true }).click();
  await edit(page, "Limiter ceiling dB", "-2");
  await expect
    .poll(async () => (await audio(request)).limiter)
    .toMatchObject({ enabled: true, ceiling_db: -2 });
  await page.getByRole("button", { name: "Undo", exact: true }).click();
  await expect.poll(async () => (await audio(request)).limiter.ceiling_db).toBe(-1);
});

test("processing edits replace prepared audio without resetting the running clock", async ({
  page,
}) => {
  await importTone(page);
  await page.getByLabel("Select master processing", { exact: true }).click();
  await page.getByRole("button", { name: "Play", exact: true }).click();
  const timeline = page.getByLabel("Audio timeline", { exact: true });
  await expect(timeline).toHaveAttribute("data-audio-status", "Audio clock locked");
  const before = Number(await timeline.getAttribute("data-playhead-sample"));
  await page.getByLabel("Enable channel EQ", { exact: true }).click();
  await edit(page, "EQ low gain dB", "-3");
  await expect(timeline).toHaveAttribute("data-audio-status", "Audio clock locked");
  await expect(page.getByRole("button", { name: "Pause playback", exact: true })).toBeVisible();
  const after = Number(await timeline.getAttribute("data-playhead-sample"));
  expect(after).toBeGreaterThan(before);
  expect(Number(await timeline.getAttribute("data-audio-underruns"))).toBe(0);
  await page.getByRole("button", { name: "Pause playback", exact: true }).click();
});
