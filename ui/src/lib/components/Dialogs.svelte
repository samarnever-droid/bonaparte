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

  /** "—" under a second, otherwise "12s" / "1m 03s". */
  function etaLabel(seconds: number): string {
    if (!seconds || seconds < 1) return "—";
    if (seconds < 60) return `${Math.ceil(seconds)}s`;
    return `${Math.floor(seconds / 60)}m ${String(Math.round(seconds % 60)).padStart(2, "0")}s`;
  }
  import { formatFps, timeToSecs, secsToTime, ticksPerFrame, type Color } from "../model";
  const comp = $derived(activeComp());
  let element = $state<HTMLDialogElement | null>(null);
  let name = $state("Composition 01"),
    width = $state(1920),
    height = $state(1080),
    fps = $state("30/1"),
    seconds = $state(30),
    background = $state<Color>([0, 0, 0, 1]);
  let format = $state<"png" | "mp4">("mp4");
  let bitDepth = $state<8 | 16>(8);
  let outputSpace = $state<"srgb" | "display-p3" | "rec2020" | "linear">("srgb");
  let submitting = $state(false);
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
                max="8192"
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
                max="8192"
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
            The current renderer supports a maximum of 16 megapixels.
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
              {#each spaces as space}<option value={space.id}
                  >{space.label} — {space.note}</option
                >{/each}</select
            ></label
          >
        </div>{/if}
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
            ? "CRF 18 · veryfast"
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
          <span>{p.canceled
              ? "canceling…"
              : p.stage === 2
                ? "packing file…"
                : `ETA ${etaLabel(p.etaSec)}`}</span>
          <span>elapsed {etaLabel(p.elapsedSec)}</span>
          <button
            class="export-cancel"
            onclick={() => void cancelExport()}>Cancel export</button
          >
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
          onclick={() =>
            void exportFile(format, { bitDepth, outputSpace })}
          ><Icon name="download" size={14} />{editor.exporting
            ? "Exporting…"
            : `Export ${format.toUpperCase()}`}</button
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
    {:else if editor.dialog?.kind === "shortcuts"}
      <h2 id="dialog-title">Keep your flow.</h2>
      <p class="dialog-subtitle">A few shortcuts between an idea and a frame.</p>
      <div class="shortcut-list">
        {#each [["Space", "Play / pause"], ["← / →", "Step one frame"], ["Shift + ← / →", "Step ten frames"], ["Home / End", "First / last frame"], ["V / H", "Selection / hand tool"], ["G", "Composition guides"], ["K", "Keyframe graph property or position"], ["1 / 2 / 3", "Design / Color / Animate workspace"], ["Ctrl/⌘ + S", "Save project"], ["Ctrl/⌘ + O", "Open project"], ["Ctrl/⌘ + D", "Duplicate selected layer"], ["Ctrl/⌘ + Z", "Undo"], ["Ctrl/⌘ + Shift + Z", "Redo"]] as shortcut}<div
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
    flex-wrap: wrap;
    gap: 8px;
    font-size: 11px;
    color: var(--text-3, #8b9284);
    margin-top: 8px;
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
</style>
