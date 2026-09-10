<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "./Icon.svelte";
  import ColorField from "./ColorField.svelte";
  import {
    editor,
    activeComp,
    applyOp,
    selectComp,
    newProject,
    exportFile,
    cancelExport,
    saveProject,
    hasAudio,
  } from "../store.svelte";
  import ExportRunner from "./ExportRunner.svelte";
  import { generateKinetic } from "../kinetic";
  import {
    kayaKeys,
    saveKayaKeys,
    kayaAnalyze,
    kayaTranscribe,
    narratorSpeak,
  } from "../kaya-client";
  import {
    vaultList,
    vaultSave,
    vaultRead,
    vaultFolderFor,
    type VaultFolder,
  } from "../vault-client";
  import { importAnyFile, importFootagePath, accept, notify } from "../store.svelte";
  import { command } from "../bridge";
  import type { Snapshot, SnapshotPatch } from "../model";

  /** "—" under a second, otherwise "12s" / "1m 03s". */
  function etaLabel(seconds: number): string {
    if (!seconds || seconds < 1) return "—";
    if (seconds < 60) return `${Math.ceil(seconds)}s`;
    return `${Math.floor(seconds / 60)}m ${String(Math.round(seconds % 60)).padStart(2, "0")}s`;
  }
  import { formatFps, timeToSecs, secsToTime, ticksPerFrame, type Color } from "../model";
  const comp = $derived(activeComp());
  let scriptTools = $state(null as string[] | null);
  let scriptText = $state('[\n  { "tool": "project.info", "args": {} }\n]');
  let scriptBusy = $state(false);
  let scriptError = $state("");
  let scriptResults = $state(
    null as { index: number; tool: string; ok: boolean; result: unknown }[] | null,
  );
  $effect(() => {
    if (editor.dialog?.kind === "script" && !scriptTools) void fetchScriptTools();
  });
  async function fetchScriptTools() {
    try {
      const doc = await command<{ mcp_tools?: string[] }>("describe");
      scriptTools = doc.mcp_tools ?? [];
    } catch {
      scriptTools = [];
    }
  }
  function addScriptStep(tool: string) {
    const step = `{ "tool": "${tool}", "args": {} }`;
    const t = scriptText.trim();
    if (!t || t === "[]") scriptText = `[\n  ${step}\n]`;
    else if (t.endsWith("]")) {
      const body = t.slice(0, -1).trimEnd();
      scriptText = body.endsWith("[") ? `[\n  ${step}\n]` : `${body},\n  ${step}\n]`;
    } else scriptText = `${t}\n${step}`;
  }
  async function runScript() {
    scriptError = "";
    let parsed: unknown;
    try {
      parsed = JSON.parse(scriptText);
    } catch (error) {
      scriptError = `Fix the JSON first: ${String(error)}`;
      return;
    }
    const raw = parsed as { steps?: unknown };
    const steps = Array.isArray(parsed) ? parsed : raw?.steps;
    if (!Array.isArray(steps) || !steps.length) {
      scriptError = 'Expected [{ "tool": …, "args": … }, …] or { "steps": [ … ] }.';
      return;
    }
    scriptBusy = true;
    try {
      const reply = await command<{
        ran: number;
        failed: boolean;
        steps: { index: number; tool: string; ok: boolean; result: unknown }[];
      }>("script.run", { steps, stopOnFailure: true });
      scriptResults = reply.steps ?? [];
      if (reply.failed)
        scriptError = `The run stopped on a failure after ${reply.ran} step${reply.ran === 1 ? "" : "s"}.`;
      // The server owns the session: pull the editor up to date so every
      // scripted change shows up — and undoes — like a human edit.
      accept(await command<Snapshot | SnapshotPatch>("state"));
      if (!reply.failed) notify(`Script ran ${reply.ran} tool call${reply.ran === 1 ? "" : "s"}.`);
    } catch (error) {
      scriptError = String(error instanceof Error ? error.message : error);
    } finally {
      scriptBusy = false;
    }
  }
  let element = $state<HTMLDialogElement | null>(null);
  let name = $state("Composition 01"),
    width = $state(1920),
    height = $state(1080),
    fps = $state("30/1"),
    seconds = $state(30),
    background = $state<Color>([0, 0, 0, 1]);
  let format = $state<"png" | "mp4">("mp4");
  let turbo = $state(false);
  let footagePath = $state("");
  let bitDepth = $state<8 | 16>(8);
  let outputSpace = $state<"srgb" | "display-p3" | "rec2020" | "linear">("srgb");
  let submitting = $state(false);
  let lyricText = $state("We light up the sky\nHold the beat down\nNever coming down");
  let lyricStyle = $state<"pop" | "rise" | "wave">("pop");
  let lyricSync = $state(true);
  let lyricFoley = $state(true);
  let lyricBusy = $state(false);
  let kayaOpenAi = $state("");
  let kayaSarvam = $state("");
  let kayaEleven = $state("");
  let kayaNarration = $state("Hi, I am the Bonaparte narrator.");
  let kayaProvider = $state<"sarvam" | "elevenlabs">("sarvam");
  let kayaStart = $state(0);
  let kayaBusy = $state(false);
  let vaultFolders = $state([] as VaultFolder[]);
  let vaultRoot = $state("");
  let vaultBusy = $state(false);
  let vaultUpload = $state<HTMLInputElement | null>(null);

  async function openVault() {
    vaultBusy = true;
    try {
      const listing = await vaultList();
      vaultFolders = listing.folders;
      vaultRoot = listing.root;
      void loadVaultPreviews(listing.folders);
    } finally {
      vaultBusy = false;
    }
  }

  /* Visual previews + audition — the shelf should look like a shelf. */
  const VAULT_IMG = ["png", "jpg", "jpeg", "webp", "gif", "svg"];
  const VAULT_VID = ["mp4", "m4v", "webm", "mov", "mkv"];
  const VAULT_AUD = ["wav", "mp3", "flac", "ogg", "oga", "m4a", "aac", "aif", "aiff"];
  let vaultPreviews = $state<Record<string, string>>({});
  let auditionKey = $state<string | null>(null);
  let auditionPlaying = $state(false);
  let auditionEl: HTMLAudioElement | null = $state(null);
  const vaultExt = (name: string) => name.split(".").pop()?.toLowerCase() ?? "";
  const vaultPreviewKey = (folder: string, file: string) => folder + "/" + file;
  async function vaultFileUrl(folder: string, file: string): Promise<string | null> {
    const key = vaultPreviewKey(folder, file);
    if (vaultPreviews[key]) return vaultPreviews[key];
    const f = await vaultRead(folder, file);
    const url = URL.createObjectURL(f);
    vaultPreviews = { ...vaultPreviews, [key]: url };
    return url;
  }
  async function loadVaultPreviews(folders: VaultFolder[]) {
    for (const url of Object.values(vaultPreviews)) URL.revokeObjectURL(url);
    vaultPreviews = {};
    const queue = folders
      .flatMap((f) => f.entries.map((e) => ({ folder: f.name, entry: e })))
      .filter(
        ({ entry }) =>
          (VAULT_IMG.includes(vaultExt(entry.file)) && entry.bytes < 12_000_000) ||
          (VAULT_VID.includes(vaultExt(entry.file)) && entry.bytes < 40_000_000),
      )
      .slice(0, 48);
    await Promise.allSettled(
      Array.from({ length: 3 }, async () => {
        while (queue.length) {
          const item = queue.shift()!;
          try {
            await vaultFileUrl(item.folder, item.entry.file);
          } catch {
            /* preview is a courtesy; never block the shelf */
          }
        }
      }),
    );
  }
  async function auditionVaultAudio(folder: string, file: string) {
    const key = vaultPreviewKey(folder, file);
    if (auditionKey === key && auditionEl) {
      if (auditionEl.paused) void auditionEl.play();
      else auditionEl.pause();
      return;
    }
    try {
      const url = await vaultFileUrl(folder, file);
      if (!url || !auditionEl) return;
      auditionEl.src = url;
      auditionKey = key;
      void auditionEl.play();
    } catch (error) {
      notify(String(error), true);
    }
  }

  $effect(() => {
    if (editor.dialog?.kind === "vault") void openVault();
  });

  async function vaultPull(folder: string, file: string) {
    vaultBusy = true;
    try {
      const asFile = await vaultRead(folder, file);
      await importAnyFile(asFile);
      editor.dialog = null;
    } finally {
      vaultBusy = false;
    }
  }

  async function vaultUploadFiles(files: FileList | null) {
    if (!files?.length) return;
    vaultBusy = true;
    try {
      for (const file of Array.from(files)) {
        const folder = vaultFolderFor(file.name);
        await vaultSave(folder, file.name, file);
      }
      const listing = await vaultList();
      vaultFolders = listing.folders;
      vaultRoot = listing.root;
    } finally {
      vaultBusy = false;
    }
  }

  $effect(() => {
    if (editor.dialog?.kind !== "kaya") return;
    const keys = kayaKeys();
    kayaOpenAi = keys.openaiKey ?? "";
    kayaSarvam = keys.sarvamKey ?? "";
    kayaEleven = keys.elevenLabsKey ?? "";
  });
  const spaces = [
    { id: "srgb", label: "sRGB", note: "standard screens" },
    { id: "display-p3", label: "Display P3", note: "wide gamut, modern displays" },
    { id: "rec2020", label: "Rec. 2020", note: "ultra-wide gamut masters" },
    { id: "linear", label: "Linear", note: "no transfer curve, for compositing" },
  ] as const;
  const resolutions = [
    { name: "Full HD", width: 1920, height: 1080 },
    { name: "Vertical", width: 1080, height: 1920 },
    { name: "Square", width: 1080, height: 1080 },
    { name: "Compact preview", width: 960, height: 540 },
    { name: "4K UHD", width: 3840, height: 2160 },
  ];
  const resolution = $derived(
    resolutions.find((p) => p.width === width && p.height === height)?.name ?? "Custom",
  );
  $effect(() => {
    if (editor.dialog?.kind === "composition") {
      const c =
        editor.dialog.compId !== null ? editor.project?.comps[String(editor.dialog.compId)] : null;
      name = c?.name ?? "Composition 01";
      width = c?.width ?? 1920;
      height = c?.height ?? 1080;
      fps = c ? `${c.fps.num}/${c.fps.den}` : "30/1";
      seconds = c ? timeToSecs(c.duration) : 30;
      background = c ? [...c.background] : [0, 0, 0, 1];
    }
  });
  onMount(() => {
    element?.querySelector<HTMLElement>("input,button,select")?.focus();
  });
  function close() {
    if (!editor.exporting && !submitting) editor.dialog = null;
  }
  function keydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      close();
    }
    if (event.key === "Tab" && element) {
      const nodes = [
        ...element.querySelectorAll<HTMLElement>(
          'button:not(:disabled),input:not(:disabled),select:not(:disabled),textarea:not(:disabled),[tabindex="0"]',
        ),
      ];
      const first = nodes[0],
        last = nodes.at(-1);
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last?.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first?.focus();
      }
    }
  }
  async function submit() {
    if (editor.dialog?.kind !== "composition") return;
    submitting = true;
    const [num, den] = fps.split("/").map(Number),
      id = editor.dialog.compId;
    let created: number | null = null;
    const properties = {
      name,
      width,
      height,
      fps: { num, den },
      duration: secsToTime(seconds),
      background,
    };
    const ok = await applyOp((project) => {
      if (id !== null) return { type: "setCompProps", comp: id, ...properties };
      created = project.next_comp;
      return {
        type: "batch",
        label: `Created composition ${name}`,
        ops: [
          {
            type: "createComp",
            name,
            width,
            height,
            fps: { num, den },
            duration: properties.duration,
          },
          { type: "setCompProps", comp: created, ...properties },
        ],
      };
    });
    if (ok) {
      if (created !== null) selectComp(created);
      editor.dialog = null;
    }
    submitting = false;
  }
</script>

<svelte:window onkeydown={keydown} />
<div
  class="modal-layer"
  role="presentation"
  onclick={(e) => {
    if (e.target === e.currentTarget) close();
  }}
>
  <dialog bind:this={element} open aria-modal="true" aria-labelledby="dialog-title" class="dialog">
    <div class="dialog-top">
      <div class="dialog-emblem">
        <Icon
          name={editor.dialog?.kind === "composition"
            ? "film"
            : editor.dialog?.kind === "export"
              ? "download"
              : editor.dialog?.kind === "shortcuts"
                ? "bolt"
                : editor.dialog?.kind === "lyrics"
                  ? "bolt"
                  : editor.dialog?.kind === "kaya"
                    ? "wave"
                    : editor.dialog?.kind === "vault"
                      ? "layers"
                      : "plus"}
          size={20}
        />
      </div>
      <span class="spacer"></span><button
        class="icon-button"
        aria-label="Close dialog"
        disabled={editor.exporting || submitting}
        onclick={close}><Icon name="x" size={17} /></button
      >
    </div>
    {#if editor.dialog?.kind === "composition"}
      <h2 id="dialog-title">
        {editor.dialog.compId === null ? "A space for your next idea." : "Composition settings"}
      </h2>
      <p class="dialog-subtitle">Set the stage. Everything else starts here.</p>
      <form
        onsubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
      >
        <label class="form-label" for="comp-name">Composition name</label><input
          id="comp-name"
          class="field"
          bind:value={name}
          required
          maxlength="256"
        />
        <div class="modal-row">
          <div>
            <label class="form-label" for="comp-preset">Canvas preset</label><select
              id="comp-preset"
              class="field"
              value={resolution}
              onchange={(e) => {
                const preset = resolutions.find((p) => p.name === e.currentTarget.value);
                if (preset) {
                  width = preset.width;
                  height = preset.height;
                }
              }}
              ><option value="Custom">Custom</option>{#each resolutions as preset}<option
                  >{preset.name}</option
                >{/each}</select
            >
          </div>
          <div class="aspect-label">
            <span>Aspect ratio</span><strong class="mono">{(width / height).toFixed(2)} : 1</strong>
          </div>
        </div>
        <div class="modal-row">
          <div>
            <label class="form-label" for="comp-width">Width</label>
            <div class="field-unit">
              <input
                id="comp-width"
                type="number"
                min="1"
                max="16384"
                step="1"
                bind:value={width}
                required
              /><span>px</span>
            </div>
          </div>
          <span class="dimension-cross">×</span>
          <div>
            <label class="form-label" for="comp-height">Height</label>
            <div class="field-unit">
              <input
                id="comp-height"
                type="number"
                min="1"
                max="16384"
                step="1"
                bind:value={height}
                required
              /><span>px</span>
            </div>
          </div>
        </div>
        <div class="modal-row">
          <div>
            <label class="form-label" for="comp-fps">Frame rate</label><select
              id="comp-fps"
              class="field"
              bind:value={fps}
              ><option value="24000/1001">23.976 fps</option><option value="24/1">24 fps</option
              ><option value="25/1">25 fps</option><option value="30000/1001">29.97 fps</option
              ><option value="30/1">30 fps</option><option value="48/1">48 fps</option><option
                value="50/1">50 fps</option
              ><option value="60000/1001">59.94 fps</option><option value="60/1">60 fps</option
              >{#if !["24000/1001", "24/1", "25/1", "30000/1001", "30/1", "48/1", "50/1", "60000/1001", "60/1"].includes(fps)}<option
                  value={fps}>{fps} fps</option
                >{/if}</select
            >
          </div>
          <div>
            <label class="form-label" for="comp-duration">Duration</label>
            <div class="field-unit">
              <input
                id="comp-duration"
                type="number"
                min=".01"
                max="86400"
                step=".01"
                bind:value={seconds}
                required
              /><span>seconds</span>
            </div>
          </div>
        </div>
        <div class="background-row">
          <span class="form-label">Background</span><ColorField
            label="Composition background"
            value={background}
            onchange={(color) => (background = color)}
          />
        </div>
        <p class="modal-note">
          <Icon name="info" size={12} /><span
            >Dimensions resize the canvas, not existing layers. The renderer currently uses 8-bit
            sRGB output.</span
          >
        </p>
        <div class="dialog-actions">
          <button type="button" class="btn ghost" onclick={close}>Cancel</button><button
            class="btn primary"
            type="submit"
            disabled={submitting || width * height > 16777216}
            >{submitting
              ? "Applying…"
              : editor.dialog.compId === null
                ? "Create composition"
                : "Apply settings"}<Icon name="right" size={13} /></button
          >
        </div>
        {#if width * height > 16777216}<p class="validation-message">
            The current renderer supports a maximum of 64 megapixels.
          </p>{/if}
      </form>
    {:else if editor.dialog?.kind === "export"}
      <h2 id="dialog-title">Give it a life outside the canvas.</h2>
      <p class="dialog-subtitle">Export your composition from the Rust renderer.</p>
      <div class="export-comp">
        <Icon name="film" size={22} />
        <div>
          <strong>{comp?.name}</strong><span class="mono"
            >{comp?.width} × {comp?.height} · {comp ? formatFps(comp.fps) : ""} · {comp
              ? timeToSecs(comp.duration).toFixed(2)
              : 0}s</span
          >
        </div>
      </div>
      <div class="export-options">
        <button
          class:chosen={format === "mp4"}
          onclick={() => (format = "mp4")}
          disabled={editor.exporting}
          ><Icon name="film" size={19} /><strong>MP4 video</strong><span
            >Full composition · H.264</span
          ><span class="radio"></span></button
        ><button
          class:chosen={format === "png"}
          onclick={() => (format = "png")}
          disabled={editor.exporting}
          ><Icon name="image" size={19} /><strong>PNG image</strong><span
            >Current frame · lossless RGBA</span
          ><span class="radio"></span></button
        >
      </div>
      {#if format === "png"}<div class="export-deep">
          <label
            >Bit depth<select bind:value={bitDepth} disabled={editor.exporting}>
              <option value={8}>8-bit — classic, small files</option>
              <option value={16}>16-bit — deep color, no banding</option>
            </select></label
          ><label
            >Color space<select bind:value={outputSpace} disabled={editor.exporting}>
              {#each spaces as space}<option value={space.id}>{space.label} — {space.note}</option
                >{/each}</select
            ></label
          >
        </div>{/if}
      {#if format === "mp4"}<label class="export-turbo">
          <input type="checkbox" bind:checked={turbo} disabled={editor.exporting} /><Icon
            name="sparkles"
            size={13}
          /><span
            ><strong>⚡ Turbo pass</strong><small
              >Draft quality for review exports; unchanged frames render once and static spans ship
              instantly.</small
            ></span
          ></label
        >{/if}
      <div class="export-facts">
        <span>Render engine</span><strong>Rust CPU reference</strong><span
          >{format === "mp4" ? "Frame range" : "Frame"}</span
        ><strong class="mono"
          >{format === "mp4"
            ? `0–${comp ? Math.ceil(comp.duration / ticksPerFrame(comp.fps)) - 1 : 0}`
            : comp
              ? Math.floor(editor.currentTime / ticksPerFrame(comp.fps))
              : 0}</strong
        ><span>{format === "mp4" ? "Encoding quality" : "Color format"}</span><strong
          >{format === "mp4"
            ? turbo
              ? "CRF 30 · ultrafast — draft"
              : "CRF 18 · veryfast"
            : `${bitDepth}-bit ${spaces.find((space) => space.id === outputSpace)?.label} + alpha`}</strong
        >
      </div>
      {#if format === "mp4"}<p class="modal-note">
          <Icon name="info" size={12} /><span
            >{editor.audioProtocol && hasAudio()
              ? "Project audio is mixed at 48 kHz stereo and encoded as AAC at 256 kbps. Mute/solo, automation and fades are included."
              : "This composition has no audio clips; the export will be silent."} Transparency is flattened
            to black. Rendering time depends on your layers and effects.</span
          >
        </p>{/if}
      {#if format === "mp4" && !editor.ffmpeg}<p class="validation-message">
          FFmpeg is not available. Install it on PATH for MP4 export, or export a PNG.
        </p>{/if}
      {#if format === "mp4" && comp && (comp.width % 2 || comp.height % 2)}<p
          class="validation-message"
        >
          H.264 needs even canvas dimensions. Adjust composition settings or export PNG.
        </p>{/if}
      {#if editor.exporting && editor.exportProgress}
        {@const p = editor.exportProgress}
        <ExportRunner progress={p} />
        <div
          class="export-meter"
          role="progressbar"
          aria-label="Export progress"
          aria-valuemin="0"
          aria-valuemax="100"
          aria-valuenow={Math.round(Math.min(100, p.percent))}
        >
          <div
            class="export-meter-fill"
            class:packing={p.stage === 2}
            style={`width:${Math.min(100, p.percent)}%`}
          ></div>
          <span class="export-meter-num mono">{Math.floor(Math.min(100, p.percent))}%</span>
        </div>
        <div class="export-live mono">
          <span>{p.framesDone}/{p.totalFrames} frames</span>
          <span>{p.fps > 0 ? `${p.fps.toFixed(1)} fps` : "warming up…"}</span>
          <span
            >{p.canceled
              ? "canceling…"
              : p.stage === 2
                ? "packing file…"
                : `ETA ${etaLabel(p.etaSec)}`}</span
          >
          <span>elapsed {etaLabel(p.elapsedSec)}</span>
          <button class="export-cancel" onclick={() => void cancelExport()}>Cancel export</button>
        </div>
        <p class="modal-note">
          <Icon name="info" size={12} /><span
            >Frames render on every CPU core in parallel and stream straight into the encoder. Your
            source project remains editable after export.</span
          >
        </p>
      {:else if editor.exporting}<div class="export-progress">
          <span class="spinner"></span><span
            >Rendering…<small
              >Your source project remains editable after export. Keep this window open.</small
            ></span
          >
        </div>{/if}
      <div class="dialog-actions">
        <button class="btn ghost" disabled={editor.exporting} onclick={close}>Cancel</button><button
          class="btn primary"
          disabled={editor.exporting ||
            !comp ||
            (format === "mp4" && (!editor.ffmpeg || !!(comp.width % 2) || !!(comp.height % 2)))}
          onclick={() => void exportFile(format, { bitDepth, outputSpace, draft: turbo })}
          ><Icon name="download" size={14} />{editor.exporting
            ? "Exporting…"
            : `Export ${format.toUpperCase()}`}</button
        >
      </div>
    {:else if editor.dialog?.kind === "footage"}
      <h2 id="dialog-title">
        {editor.dialog.mediaId == null
          ? "Point at a clip on this machine."
          : "Find the clip again."}
      </h2>
      <p class="dialog-subtitle">
        {editor.dialog.mediaId == null
          ? "No copying, no baking: the project keeps a reference and plays the file at full framerate, then quietly builds a half-resolution proxy for smooth scrubbing."
          : "This asset went offline — its file moved or the disk changed. Pick the new location and every layer keeps its trims."}
      </p>
      <form
        class="dialog-form"
        onsubmit={(e) => {
          e.preventDefault();
          void importFootagePath(
            footagePath,
            editor.dialog?.kind === "footage" ? editor.dialog.mediaId : null,
          );
          footagePath = "";
        }}
      >
        <input
          class="field"
          aria-label="Clip path"
          placeholder="/media/hero-shot.mp4"
          spellcheck="false"
          bind:value={footagePath}
        />
        <div class="dialog-actions">
          <button type="button" class="btn ghost" onclick={close}>Cancel</button><button
            type="submit"
            class="btn primary"
            disabled={!footagePath.trim()}
            >{editor.dialog.mediaId == null ? "Link footage" : "Relink"}<Icon
              name="right"
              size={13}
            /></button
          >
        </div>
      </form>
    {:else if editor.dialog?.kind === "script"}
      <h2 id="dialog-title">Speak the editor's own language.</h2>
      <p class="dialog-subtitle">
        A script is a list of tool calls — the exact surface an AI client drives over MCP, nothing
        more and nothing less. Every step validates, commits through `Op`s, and lands in the undo
        history.
      </p>
      {#if scriptTools && scriptTools.length}<div class="script-tools">
          <small>Tools — click to append a step</small>
          <div class="script-chips">
            {#each scriptTools as tool (tool)}<button
                class="tool-chip mono"
                title="Append a {tool} step"
                onclick={() => addScriptStep(tool)}>{tool}</button
              >{/each}
          </div>
        </div>{/if}
      <textarea
        class="field script-editor"
        rows="9"
        spellcheck="false"
        aria-label="Script JSON"
        bind:value={scriptText}
        onkeydown={(e) => {
          if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
            e.preventDefault();
            void runScript();
          }
        }}></textarea>
      {#if scriptError}<p class="validation-message">{scriptError}</p>{/if}
      {#if scriptResults}<div class="script-results">
          {#each scriptResults as step (step.index)}
            <div class="script-step" class:bad={!step.ok}>
              <span class="step-mark"><Icon name={step.ok ? "check" : "x"} size={11} /></span><span
                class="mono step-tool">{step.index + 1}. {step.tool}</span
              >{#if !step.ok}<pre class="step-err">{typeof step.result === "string"
                    ? step.result
                    : JSON.stringify(step.result)}</pre>{/if}
            </div>
          {/each}
        </div>{/if}
      <div class="dialog-actions">
        <button type="button" class="btn ghost" onclick={close}>Close</button><button
          type="button"
          class="btn primary"
          disabled={scriptBusy}
          onclick={() => void runScript()}
          ><Icon name="play" size={12} />{scriptBusy ? "Running…" : "Run script"}<kbd>⌘ ↵</kbd
          ></button
        >
      </div>
    {:else if editor.dialog?.kind === "rename-layer"}
      <h2 id="dialog-title">Rename this layer.</h2>
      <p class="dialog-subtitle">A clear name keeps the timeline readable.</p>
      <form
        class="rename-form"
        onsubmit={(e) => {
          e.preventDefault();
          const dialog = editor.dialog;
          if (dialog?.kind !== "rename-layer") return;
          const name = dialog.name.trim();
          if (name)
            void applyOp({
              type: "renameLayer",
              comp: dialog.compId,
              layer: dialog.layerId,
              name,
            });
          close();
        }}
      >
        <input
          class="field"
          aria-label="Layer name"
          maxlength="120"
          value={editor.dialog.kind === "rename-layer" ? editor.dialog.name : ""}
          oninput={(e) => {
            if (editor.dialog?.kind === "rename-layer") editor.dialog.name = e.currentTarget.value;
          }}
        />
        <div class="dialog-actions">
          <button type="button" class="btn ghost" onclick={close}>Cancel</button><button
            type="submit"
            class="btn primary">Rename<Icon name="right" size={13} /></button
          >
        </div>
      </form>
    {:else if editor.dialog?.kind === "lyrics"}
      <h2 id="dialog-title">Type the words. The beat does the rest. ⚡</h2>
      <p class="dialog-subtitle">
        Kinetic lays every word on the beat grid — motion, timing, even the impacts, synthesized.
      </p>
      <label class="form-label" for="lyric-lines">Lyric lines</label>
      <textarea
        id="lyric-lines"
        class="field"
        rows="5"
        aria-label="Lyric lines"
        placeholder="One line per row"
        bind:value={lyricText}></textarea>
      <label class="form-label" for="lyric-style">Motion style</label>
      <select id="lyric-style" class="field" aria-label="Motion style" bind:value={lyricStyle}>
        <option value="pop">Pop — words punch in on the beat</option>
        <option value="rise">Rise — words glide up into place</option>
        <option value="wave">Wave — words ride the line</option>
      </select>
      <label class="form-label checkbox-row">
        <input type="checkbox" bind:checked={lyricSync} />
        <span>Sync phrases to the beat grid</span>
      </label>
      <label class="form-label checkbox-row">
        <input type="checkbox" bind:checked={lyricFoley} />
        <span>Auto sound design — synthesized impacts on downbeats</span>
      </label>
      <div class="dialog-actions">
        <button type="button" class="btn ghost" onclick={close}>Cancel</button>
        <button
          type="button"
          class="btn primary"
          disabled={lyricBusy}
          onclick={() => {
            if (editor.dialog?.kind !== "lyrics") return;
            lyricBusy = true;
            void generateKinetic({
              assetId: editor.dialog.assetId,
              lines: lyricText.split("\n"),
              style: lyricStyle,
              sync: lyricSync,
              foley: lyricFoley,
            }).then(() => {
              lyricBusy = false;
              close();
            });
          }}>Generate ⚡<Icon name="bolt" size={13} /></button
        >
      </div>
    {:else if editor.dialog?.kind === "kaya"}
      {@const assetId = editor.dialog.assetId}
      <h2 id="dialog-title">Kaya ⚡ hears the timeline.</h2>
      <p class="dialog-subtitle">
        Word-level transcription, scene sense, and a narrator — your keys stay in this browser,
        never in the project.
      </p>
      <div class="dialog-actions" style="justify-content:flex-start;gap:8px;margin:0 0 12px">
        <button
          type="button"
          class="btn ghost"
          disabled={kayaBusy}
          onclick={() => void kayaAnalyze(assetId).then(() => close())}
          >Analyze rhythm & silence<Icon name="graph" size={13} /></button
        >
      </div>
      <label class="form-label" for="kaya-openai">OpenAI key (Whisper transcription)</label>
      <input
        id="kaya-openai"
        class="field"
        type="password"
        placeholder="sk-…"
        bind:value={kayaOpenAi}
      />
      <div class="dialog-actions" style="justify-content:flex-start;gap:8px;margin:8px 0 12px">
        <button
          type="button"
          class="btn ghost"
          disabled={kayaBusy || !kayaOpenAi}
          title={kayaOpenAi ? "" : "Paste an OpenAI key first"}
          onclick={() => {
            saveKayaKeys({ ...kayaKeys(), openaiKey: kayaOpenAi });
            kayaBusy = true;
            void kayaTranscribe(assetId, kayaOpenAi).then(() => {
              kayaBusy = false;
              close();
            });
          }}>Transcribe word-by-word<Icon name="wave" size={13} /></button
        >
      </div>
      <label class="form-label" for="kaya-narration">Narrator</label>
      <textarea
        id="kaya-narration"
        class="field"
        rows="3"
        aria-label="Narration text"
        bind:value={kayaNarration}></textarea>
      <div style="display:flex;gap:8px;margin-top:8px">
        <select
          class="field"
          aria-label="Narrator provider"
          style="flex:1"
          bind:value={kayaProvider}
        >
          <option value="sarvam">Sarvam AI · Bulbul</option>
          <option value="elevenlabs">ElevenLabs</option>
        </select>
        <input
          class="field"
          type="number"
          min="0"
          step="0.1"
          aria-label="Start at seconds"
          style="width:110px"
          bind:value={kayaStart}
        />
      </div>
      <div class="dialog-actions" style="margin-top:10px">
        <button type="button" class="btn ghost" onclick={close}>Close</button>
        <button
          type="button"
          class="btn primary"
          disabled={kayaBusy}
          onclick={() => {
            saveKayaKeys({
              openaiKey: kayaOpenAi,
              sarvamKey: kayaSarvam,
              elevenLabsKey: kayaEleven,
            });
            if (kayaProvider === "sarvam" && kayaSarvam)
              saveKayaKeys({ ...kayaKeys(), sarvamKey: kayaSarvam });
            if (kayaProvider === "elevenlabs" && kayaEleven)
              saveKayaKeys({ ...kayaKeys(), elevenLabsKey: kayaEleven });
            kayaBusy = true;
            void narratorSpeak({
              text: kayaNarration,
              startSecs: Number(kayaStart) || 0,
              provider: kayaProvider,
            }).then(() => {
              kayaBusy = false;
              close();
            });
          }}>Speak ⚡<Icon name="bolt" size={13} /></button
        >
      </div>
    {:else if editor.dialog?.kind === "vault"}
      <h2 id="dialog-title">The Vault — one shelf for every project.</h2>
      <p class="dialog-subtitle">
        Logos, brand art, audio, Lottie — dumped here once, available everywhere. {vaultRoot
          ? `Living at ${vaultRoot}`
          : ""}
      </p>
      <div class="dialog-actions" style="justify-content:flex-start;gap:8px;margin:0 0 10px">
        <button
          type="button"
          class="btn ghost"
          disabled={vaultBusy}
          onclick={() => vaultUpload?.click()}>Upload to vault<Icon name="plus" size={13} /></button
        >
        <input
          bind:this={vaultUpload}
          type="file"
          multiple
          hidden
          aria-label="Upload files to the vault"
          onchange={(e) => void vaultUploadFiles(e.currentTarget.files)}
        />
      </div>
      <div style="max-height:46vh;overflow:auto;display:flex;flex-direction:column;gap:12px">
        {#each vaultFolders as folder (folder.name)}
          {#if folder.entries.length}
            <div>
              <div class="form-label" style="margin-bottom:4px">
                {folder.name} · {folder.entries.length}
              </div>
              {#each folder.entries as entry (entry.file)}
                {@const ext = vaultExt(entry.file)}
                {@const pkey = vaultPreviewKey(folder.name, entry.file)}
                <div class="vault-entry">
                  <button
                    type="button"
                    class="vault-thumb"
                    title="Add {entry.file} to this composition"
                    aria-label="Add {entry.file} to the composition"
                    disabled={vaultBusy}
                    onclick={() => void vaultPull(folder.name, entry.file)}
                  >
                    {#if vaultPreviews[pkey] && VAULT_IMG.includes(ext)}
                      <img src={vaultPreviews[pkey]} alt="" />
                    {:else if vaultPreviews[pkey] && VAULT_VID.includes(ext)}
                      <video src={vaultPreviews[pkey]} preload="metadata" muted playsinline></video>
                    {:else}
                      <Icon
                        name={VAULT_IMG.includes(ext)
                          ? "image"
                          : VAULT_VID.includes(ext)
                            ? "film"
                            : VAULT_AUD.includes(ext)
                              ? "wave"
                              : "folder"}
                        size={14}
                      />
                    {/if}
                  </button>
                  <button
                    type="button"
                    class="btn ghost vault-name"
                    style="display:flex;justify-content:space-between;width:100%;margin:0"
                    disabled={vaultBusy}
                    title="Add to this composition"
                    onclick={() => void vaultPull(folder.name, entry.file)}
                    ><span class="truncate">{entry.file}</span><span
                      >{entry.bytes > 1_048_576
                        ? `${(entry.bytes / 1_048_576).toFixed(1)} MB`
                        : `${Math.max(1, Math.round(entry.bytes / 1024))} KB`}</span
                    ></button
                  >
                  {#if VAULT_AUD.includes(ext)}
                    <button
                      type="button"
                      class="btn ghost vault-audition"
                      aria-label={"Audition " + entry.file}
                      title={auditionKey === pkey && auditionPlaying ? "Pause" : "Audition"}
                      onclick={() => void auditionVaultAudio(folder.name, entry.file)}
                      ><Icon
                        name={auditionKey === pkey && auditionPlaying ? "pause" : "play"}
                        size={11}
                      /></button
                    >
                  {/if}
                </div>
              {/each}
            </div>
          {/if}
        {/each}
        {#if vaultFolders.every((f) => !f.entries.length)}
          <p class="modal-note">
            <Icon name="info" size={12} /><span
              >The vault is empty — upload a logo, a track, a Lottie. Files land in clean folders
              and show up in every project.</span
            >
          </p>
        {/if}
      </div>
      <audio
        bind:this={auditionEl}
        onplay={() => (auditionPlaying = true)}
        onpause={() => (auditionPlaying = false)}
        onended={() => (auditionPlaying = false)}
      ></audio>
      <div class="dialog-actions">
        <button type="button" class="btn ghost" onclick={close}>Close</button>
      </div>
    {:else if editor.dialog?.kind === "shortcuts"}
      <h2 id="dialog-title">Keep your flow.</h2>
      <p class="dialog-subtitle">A few shortcuts between an idea and a frame.</p>
      <div class="shortcut-list">
        {#each [["Space", "Play / pause"], ["← / →", "Step one frame"], ["Shift + ← / →", "Step ten frames"], ["Home / End", "First / last frame"], ["Ctrl/⌘ + [ / ]", "Move layer up / down"], ["V / H", "Selection / hand tool"], ["G", "Composition guides"], ["K", "Keyframe graph property or position"], ["1 / 2 / 3", "Design / Color / Animate workspace"], ["Ctrl/⌘ + S", "Save project"], ["Ctrl/⌘ + O", "Open project"], ["Ctrl/⌘ + D", "Duplicate selected layer"], ["Ctrl/⌘ + Z", "Undo"], ["Ctrl/⌘ + Shift + Z", "Redo"], ["Right-click", "Context menus everywhere — timeline, audio, assets"]] as shortcut}<div
          >
            <span>{shortcut[1]}</span><kbd>{shortcut[0]}</kbd>
          </div>{/each}
      </div>
      <p class="modal-note">
        <Icon name="info" size={12} /><span
          >Hold Shift when dragging a canvas layer to constrain movement, scale proportionally, or
          snap rotation to 15°.</span
        >
      </p>
      <div class="dialog-actions">
        <button class="btn primary" onclick={close}
          >Back to creating<Icon name="right" size={13} /></button
        >
      </div>
    {:else}
      <h2 id="dialog-title">A fresh start.</h2>
      <p class="dialog-subtitle">Choose a blank canvas or explore an editable example.</p>
      {#if editor.dirty}<p class="validation-message">
          Your project has unsaved changes. Save a file before starting a new project.
        </p>
        <button class="btn full" onclick={() => void saveProject()}
          ><Icon name="save" size={14} />Save current project</button
        >{/if}
      <div class="new-project-options">
        <button onclick={() => void newProject(false)}
          ><Icon name="plus" size={25} /><strong>Blank project</strong><span
            >1920 × 1080 · 30 fps · 30 seconds</span
          ><Icon name="right" size={15} /></button
        ><button onclick={() => void newProject(true)}
          ><Icon name="circle" size={25} /><strong>Orbit studio ident</strong><span
            >Editable shapes, typography, animation & grading</span
          ><Icon name="right" size={15} /></button
        >
      </div>
      <p class="modal-note">
        This creates a new editing session and clears undo history. Downloaded project files are not
        changed.
      </p>
    {/if}
  </dialog>
</div>

<style>
  .modal-layer {
    position: fixed;
    inset: 0;
    z-index: 150;
    background: #0b0c0bba;
    backdrop-filter: blur(7px);
    display: grid;
    place-items: center;
    padding: 22px;
  }
  .dialog {
    position: relative;
    inset: auto;
    display: block;
    background: #252724;
    color: var(--text);
    border: 1px solid #565a53;
    width: 460px;
    max-width: 100%;
    max-height: calc(100dvh - 44px);
    overflow-y: auto;
    border-radius: 10px;
    padding: 22px 27px;
    box-shadow: 0 30px 140px #0009;
    margin: 0;
  }
  .dialog-top {
    display: flex;
    align-items: center;
    margin-bottom: 18px;
  }
  .dialog-emblem {
    display: grid;
    place-items: center;
    width: 42px;
    height: 42px;
    border: 1px solid #595e56;
    background: #383b36;
    color: var(--accent);
    border-radius: 7px;
  }
  .dialog h2 {
    font-size: 22px;
    line-height: 1.3;
    letter-spacing: -0.7px;
    font-weight: 500;
    margin: 0 0 10px;
    color: #dee1dc;
  }
  .dialog-subtitle {
    font-size: 11px;
    color: #959a92;
    line-height: 1.8;
    margin: 0 0 23px;
  }
  .form-label {
    font-size: 10px;
    color: #acb0a8;
    display: block;
    margin: 12px 0 7px;
  }
  .field {
    font-size: 11px;
    padding: 9px;
    background: #1c1e1b;
  }
  .modal-row {
    display: flex;
    align-items: flex-end;
    gap: 13px;
  }
  .modal-row > div {
    flex: 1;
    min-width: 0;
  }
  .aspect-label {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px 0 4px 8px;
  }
  .aspect-label span {
    font-size: 9px;
    color: #828880;
  }
  .aspect-label strong {
    font-size: 11px;
    font-weight: 400;
    color: #c5c8c1;
  }
  .field-unit {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 9px;
    border: 1px solid var(--border);
    background: #1c1e1b;
    border-radius: 4px;
  }
  .field-unit input {
    background: none;
    border: 0;
    outline: 0;
    width: 100%;
    color: #d3d6d1;
    font: 11px monospace;
    min-width: 0;
  }
  .field-unit span {
    font-size: 8px;
    color: #878c84;
  }
  .dimension-cross {
    margin-bottom: 8px;
    color: #777c73;
    font-size: 14px;
  }
  .background-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-top: 15px;
  }
  .background-row .form-label {
    margin: 0;
  }
  .modal-note {
    display: flex;
    gap: 7px;
    font-size: 9px;
    color: #878b84;
    line-height: 1.8;
    margin: 21px 0 0;
  }
  .modal-note :global(svg) {
    margin-top: 2px;
    flex-shrink: 0;
  }
  .dialog-actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 7px;
    border-top: 1px solid #42463f;
    padding-top: 18px;
    margin-top: 22px;
  }
  .dialog-actions .btn {
    padding: 9px 13px;
    font-size: 10px;
  }
  .validation-message {
    font-size: 10px;
    color: #c2bcb3;
    background: #726c6312;
    border: 1px solid #5f5d56;
    padding: 11px;
    border-radius: 4px;
    line-height: 1.7;
    margin: 14px 0;
  }
  .export-comp {
    display: flex;
    gap: 13px;
    align-items: center;
    padding: 14px;
    border: 1px solid #4d514a;
    border-radius: 5px;
    background: #20221f;
    color: #afb2ac;
  }
  .export-comp > div {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .export-comp strong {
    font-size: 12px;
    font-weight: 500;
    color: #d0d3cd;
  }
  .export-comp span {
    font-size: 9px;
    color: #90948c;
  }
  .export-options {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
    margin: 18px 0;
  }
  .export-options button {
    display: flex;
    flex-direction: column;
    gap: 10px;
    align-items: flex-start;
    text-align: left;
    border: 1px solid #4d514a;
    padding: 17px 13px;
    border-radius: 5px;
    color: #93978f;
    position: relative;
  }
  .export-options button.chosen {
    border-color: #aaafa5;
    background: #6a6e6520;
  }
  .export-options strong {
    font-size: 11px;
    font-weight: 500;
    color: #c6c9c3;
  }
  .export-options span {
    font-size: 8px;
    color: #888c84;
  }
  .radio {
    position: absolute;
    top: 14px;
    right: 13px;
    width: 11px;
    height: 11px;
    border: 1px solid #757971;
    border-radius: 50%;
  }
  .chosen .radio {
    background: var(--accent);
    box-shadow: inset 0 0 0 3px #393c36;
    border-color: var(--accent);
  }
  .export-deep {
    display: grid;
    gap: 8px;
    margin-bottom: 14px;
  }
  .export-deep label {
    display: grid;
    gap: 4px;
    font-size: 11px;
    color: #8a9583;
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }
  .export-deep select {
    background: #1d2318;
    color: #e6ecdd;
    border: 1px solid #3a4232;
    border-radius: 6px;
    padding: 6px 8px;
    font: inherit;
    font-size: 13px;
  }

  .export-facts {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 12px;
    font-size: 10px;
    color: #949890;
    padding: 8px 0;
  }
  .export-facts strong {
    font-size: 10px;
    font-weight: 400;
    color: #c4c7c0;
  }
  .export-meter {
    position: relative;
    height: 30px;
    background: #191b18;
    border: 1px solid var(--border);
    border-radius: 9px;
    overflow: hidden;
    margin-top: 10px;
  }
  .export-meter-fill {
    position: absolute;
    inset: 0 auto 0 0;
    background: linear-gradient(90deg, #5c7d45, #9dc37f);
    transition: width 0.18s linear;
  }
  .export-meter-fill.packing {
    background: linear-gradient(90deg, #4c7d8d, #6db3d9);
  }
  .export-meter-num {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    font-size: 12px;
    color: #f2f5ec;
    text-shadow: 0 1px 2px rgba(0, 0, 0, 0.7);
  }
  .export-live {
    display: flex;
    align-items: center;
    /* nowrap + shrinkable status spans keep the cancel button pinned to the
       right edge even as the live numbers change width on every telemetry
       tick — a button that reflows every 200 ms is a button you can't click. */
    flex-wrap: nowrap;
    gap: 8px;
    font-size: 11px;
    color: var(--text-3, #8b9284);
    margin-top: 8px;
  }
  .export-live span {
    flex: 0 1 auto;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .export-live span::before {
    content: "";
    display: inline-block;
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: var(--text-3, #8b9284);
    margin-right: 6px;
    vertical-align: 2px;
  }
  .export-cancel {
    flex: 0 0 auto;
    margin-left: auto;
    font-size: 11px;
    padding: 4px 10px;
    border-radius: 7px;
    border: 1px solid var(--border);
    background: #222422;
    color: var(--text-2, #c9c4b4);
    cursor: pointer;
  }
  .export-cancel:hover {
    color: #e26d5a;
    border-color: #e26d5a;
  }
  .export-progress {
    margin-top: 20px;
    padding: 14px;
    border: 1px solid #61665e;
    background: #393c37;
    border-radius: 5px;
    display: flex;
    gap: 12px;
    font-size: 11px;
    color: #ced2cb;
  }
  .export-progress small {
    display: block;
    font-size: 8px;
    line-height: 1.7;
    margin-top: 5px;
    color: #a3a7a0;
  }
  .spinner {
    border: 2px solid #5a5f56;
    border-top-color: var(--accent);
    height: 17px;
    width: 17px;
    border-radius: 50%;
    animation: spin 1s linear infinite;
    flex-shrink: 0;
    margin-top: 2px;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .shortcut-list {
    display: grid;
    gap: 0;
  }
  .shortcut-list > div {
    display: flex;
    justify-content: space-between;
    gap: 10px;
    padding: 10px 0;
    border-bottom: 1px solid #3b3f39;
    font-size: 10px;
    color: #adb1a9;
  }
  .shortcut-list kbd {
    font: 9px monospace;
    color: #cbcfc8;
    background: #3a3e38;
    border: 1px solid #525750;
    border-radius: 3px;
    padding: 3px 5px;
    white-space: nowrap;
  }
  .new-project-options {
    display: grid;
    gap: 12px;
    margin-top: 20px;
  }
  .new-project-options button {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 10px;
    border: 1px solid #5a5e56;
    padding: 20px;
    border-radius: 6px;
    text-align: left;
    color: #b2b6ae;
  }
  .new-project-options button:hover {
    background: #3a3d38;
    border-color: #a0a59b;
  }
  .new-project-options strong {
    font-weight: 500;
    font-size: 14px;
    color: #d5d8d3;
  }
  .new-project-options span {
    font-size: 9px;
    color: #939890;
  }
  .new-project-options button :global(svg:last-child) {
    position: absolute;
    right: 20px;
    top: calc(50% - 7px);
  }
  .export-turbo {
    display: flex;
    gap: 9px;
    align-items: flex-start;
    margin: 10px 0 2px;
    padding: 9px 11px;
    border: 1px solid #3a3c39;
    border-radius: 6px;
    background: #22242166;
    cursor: pointer;
    color: #a9ada7;
  }
  .export-turbo:hover {
    border-color: #4c6146;
  }
  .export-turbo strong {
    display: block;
    color: #e8eae6;
    font-size: 12px;
  }
  .export-turbo small {
    display: block;
    font-size: 11px;
    color: #8d928b;
    margin-top: 1px;
  }
  .export-turbo input {
    margin-top: 2px;
    accent-color: var(--accent);
  }
  .script-tools {
    margin: 4px 0 8px;
  }
  .script-tools small {
    color: #8d928b;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }
  .script-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-top: 5px;
  }
  .tool-chip {
    font-size: 10px;
    padding: 2px 8px;
    border-radius: 999px;
    border: 1px solid #3a3c39;
    background: #22242180;
    color: #a9ada7;
    cursor: pointer;
  }
  .tool-chip:hover {
    color: var(--accent);
    border-color: #4c6146;
  }
  .script-editor {
    font:
      12px/1.55 ui-monospace,
      SFMono-Regular,
      Menlo,
      monospace;
    background: #1a1b1a;
    color: #e8eae6;
    resize: vertical;
    min-height: 130px;
    tab-size: 2;
  }
  .script-results {
    margin-top: 9px;
    border: 1px solid #33352f;
    border-radius: 6px;
    overflow: hidden;
  }
  .script-step {
    display: flex;
    align-items: baseline;
    gap: 7px;
    padding: 6px 10px;
    border-bottom: 1px solid #2a2c29;
    font-size: 11px;
  }
  .script-step:last-child {
    border-bottom: none;
  }
  .script-step .step-mark {
    color: var(--accent);
    display: inline-flex;
  }
  .script-step.bad .step-mark {
    color: var(--warning);
  }
  .step-tool {
    color: #c9ccc6;
  }
  .step-err {
    flex: 1;
    margin: 0;
    color: #d8b98a;
    font-size: 10px;
    white-space: pre-wrap;
    word-break: break-word;
  }

  /* Vault shelf rows: thumb · name · audition */
  .vault-entry {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 3px 0;
  }
  .vault-thumb {
    width: 56px;
    height: 34px;
    flex: 0 0 auto;
    border: 0;
    border-radius: 5px;
    padding: 0;
    overflow: hidden;
    background: rgba(255, 255, 255, 0.05);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    color: inherit;
  }
  .vault-thumb img,
  .vault-thumb video {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .vault-entry .vault-name {
    flex: 1 1 auto;
    min-width: 0;
  }
  .vault-audition {
    flex: 0 0 auto;
    padding: 6px 9px;
  }
</style>
