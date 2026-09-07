<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import TopBar from "./lib/components/TopBar.svelte";
  import Sidebar from "./lib/components/Sidebar.svelte";
  import Viewport from "./lib/components/Viewport.svelte";
  import Properties from "./lib/components/Properties.svelte";
  import AudioInspector from "./lib/components/AudioInspector.svelte";
  import { deleteAudio, duplicateAudio, splitAudio } from "./lib/audio/actions";
  import Timeline from "./lib/components/Timeline.svelte";
  import Dialogs from "./lib/components/Dialogs.svelte";
  import ContextMenu from "./lib/components/ContextMenu.svelte";
  import Icon from "./lib/components/Icon.svelte";
  import {
    editor,
    init,
    activeComp,
    selectedLayer,
    undoOp,
    redoOp,
    saveProject,
    openProject,
    duplicateSelected,
    deleteSelected,
    play,
    pause,
    scrub,
    toggleKeyframeAtPlayhead,
    setWorkspace,
    importAnyFile,
    openProjectFile,
    notify,
  } from "./lib/store.svelte";
  import { ticksPerFrame } from "./lib/model";
  let timelineHeight = $state(252),
    draggingFiles = $state(false);
  let previousTimelineMode = "layers";
  $effect(() => {
    const mode = editor.timelineMode;
    if (mode !== previousTimelineMode) {
      timelineHeight = mode === "audio" ? Math.min(window.innerHeight * 0.55, 470) : 252;
      previousTimelineMode = mode;
    }
  });
  let dragDepth = 0,
    resizeOrigin = 0,
    resizeStart = 0;
  function resizeMove(e: PointerEvent) {
    timelineHeight = Math.max(
      180,
      Math.min(window.innerHeight * 0.65, resizeStart + resizeOrigin - e.clientY),
    );
  }
  function resizeEnd() {
    window.removeEventListener("pointermove", resizeMove);
    window.removeEventListener("pointerup", resizeEnd);
    window.removeEventListener("pointercancel", resizeEnd);
    document.body.style.cursor = "";
  }
  function resizeBegin(e: PointerEvent) {
    e.preventDefault();
    resizeOrigin = e.clientY;
    resizeStart = timelineHeight;
    document.body.style.cursor = "row-resize";
    window.addEventListener("pointermove", resizeMove);
    window.addEventListener("pointerup", resizeEnd);
    window.addEventListener("pointercancel", resizeEnd);
  }
  function keyboard(e: KeyboardEvent) {
    if (e.defaultPrevented || e.isComposing || editor.dialog || editor.loading) return;
    const target = e.target as HTMLElement | null;
    const typing = !!target?.closest('textarea,[contenteditable="true"],input,select');
    const mod = e.ctrlKey || e.metaKey,
      key = e.key.toLowerCase();
    if (mod && key === "s") {
      e.preventDefault();
      target?.blur();
      void saveProject();
      return;
    }
    if (mod && key === "o") {
      e.preventDefault();
      target?.blur();
      void openProject();
      return;
    }
    if (typing) return;
    if (mod && key === "z") {
      e.preventDefault();
      if (e.shiftKey) void redoOp();
      else void undoOp();
      return;
    }
    if (mod && key === "y") {
      e.preventDefault();
      void redoOp();
      return;
    }
    if (mod && key === "d") {
      e.preventDefault();
      if (editor.timelineMode === "audio") {
        if (e.shiftKey) void splitAudio();
        else void duplicateAudio();
      } else void duplicateSelected();
      return;
    }
    if (mod) return;
    if (e.code === "Space") {
      e.preventDefault();
      if (!e.repeat) {
        if (editor.playing || editor.audioStarting) pause();
        else void play();
      }
      return;
    }
    const comp = activeComp();
    if ((e.key === "ArrowLeft" || e.key === "ArrowRight") && comp) {
      e.preventDefault();
      pause();
      scrub(
        editor.currentTime +
          (e.key === "ArrowLeft" ? -1 : 1) * ticksPerFrame(comp.fps) * (e.shiftKey ? 10 : 1),
      );
      return;
    }
    if (e.key === "Home") {
      e.preventDefault();
      pause();
      scrub(0);
      return;
    }
    if (e.key === "End" && comp) {
      e.preventDefault();
      pause();
      scrub(comp.duration);
      return;
    }
    if (key === "k") {
      e.preventDefault();
      const layer = selectedLayer();
      if (layer) void toggleKeyframeAtPlayhead(layer.id, editor.graphProperty ?? "Position");
      return;
    }
    if (e.key === "Delete" || e.key === "Backspace") {
      e.preventDefault();
      if (editor.timelineMode === "audio") void deleteAudio();
      else void deleteSelected();
      return;
    }
    if (key === "v") editor.tool = "select";
    if (key === "h") editor.tool = "hand";
    if (key === "r") editor.tool = "rotate";
    if (key === "o") editor.tool = editor.tool === "orbit" ? "select" : "orbit";
    if (key === "g") editor.showGuides = !editor.showGuides;
    if (key === "1") setWorkspace("Design");
    if (key === "2") setWorkspace("Color");
    if (key === "3") setWorkspace("Animate");
    if (e.key === "?") editor.dialog = { kind: "shortcuts" };
  }
  function dragEnter(e: DragEvent) {
    if (e.dataTransfer?.types.includes("Files")) {
      e.preventDefault();
      dragDepth++;
      draggingFiles = true;
    }
  }
  function dragLeave() {
    dragDepth = Math.max(0, dragDepth - 1);
    if (!dragDepth) draggingFiles = false;
  }
  async function drop(e: DragEvent) {
    e.preventDefault();
    draggingFiles = false;
    dragDepth = 0;
    const file = e.dataTransfer?.files[0];
    if (!file) return;
    if (/\.(bonaparte|json)$/i.test(file.name)) {
      if (
        editor.dirty &&
        !confirm("Replace the current session? Save a project file first to keep your changes.")
      )
        return;
      await openProjectFile(file);
    } else await importAnyFile(file);
  }
  function beforeUnload(e: BeforeUnloadEvent) {
    if (editor.dirty) {
      e.preventDefault();
      e.returnValue = "";
    }
  }
  onMount(() => {
    void init();
    window.addEventListener("beforeunload", beforeUnload);
  });
  onDestroy(() => {
    pause();
    resizeEnd();
    window.removeEventListener("beforeunload", beforeUnload);
  });
</script>

<svelte:window
  onkeydown={keyboard}
  ondragenter={dragEnter}
  ondragover={(e) => {
    if (e.dataTransfer?.types.includes("Files")) e.preventDefault();
  }}
  ondragleave={dragLeave}
  ondrop={drop}
/>

<div class="shell" style={`--timeline-height:${timelineHeight}px`}>
  <TopBar />
  <main class="workspace">
    <Sidebar /><Viewport
    />{#if editor.timelineMode === "audio" && editor.audioProtocol}<AudioInspector
      />{:else}<Properties />{/if}
  </main>
  <div class="timeline-region">
    <button
      type="button"
      class="timeline-resizer"
      aria-label="Resize timeline"
      onpointerdown={resizeBegin}
      onkeydown={(e) => {
        if (e.key === "ArrowUp") {
          timelineHeight = Math.min(window.innerHeight * 0.65, timelineHeight + 20);
          e.stopPropagation();
        }
        if (e.key === "ArrowDown") {
          timelineHeight = Math.max(180, timelineHeight - 20);
          e.stopPropagation();
        }
      }}
    ></button><Timeline />
  </div>
  <footer class="statusbar">
    <span class="connection-dot" class:error={!!editor.renderError || !!editor.initError}
    ></span><span
      >{editor.renderError
        ? "Renderer needs attention"
        : editor.pending
          ? "Applying changes…"
          : editor.project
            ? "Rust engine connected"
            : "Connecting to engine…"}</span
    ><span class="status-separator">/</span><span class="mono"
      >{editor.frameMs > 0
        ? `${Math.round(editor.frameMs)} ms${editor.previewMetadata?.cacheHit ? " · cached" : " / frame"}`
        : "CPU reference"}</span
    ><span class="spacer"></span><span class="recovery-status"
      ><Icon name="check" size={9} />{editor.recovery}</span
    ><span class="status-separator">/</span><button
      onclick={() => (editor.dialog = { kind: "shortcuts" })}
      title="Keyboard shortcuts"><Icon name="help" size={10} /><span>Shortcuts</span></button
    >
  </footer>
</div>
{#if editor.loading || editor.initError}
  <div class="loading-screen">
    <div class="loader-brand">bonaparte<span>.</span></div>
    {#if editor.initError}<strong>The editor couldn’t connect.</strong>
      <p>{editor.initError}</p>
      <p class="loading-hint">
        For the browser version, run the Rust service and Vite together.<br />Use
        <code>npm run studio</code> from the ui directory.
      </p>
      <button class="btn primary" onclick={() => void init()}
        >Try again<Icon name="rotate" size={13} /></button
      >{:else}<div class="loading-track"><span></span></div>
      <p>Preparing your creative space…</p>{/if}
  </div>
{/if}
{#if editor.dialog}<Dialogs />{/if}
<ContextMenu />
{#if editor.toast}<div
    class="toast"
    class:error={editor.toast.error}
    role={editor.toast.error ? "alert" : "status"}
  >
    <span class="toast-symbol"><Icon name={editor.toast.error ? "info" : "check"} size={15} /></span
    ><span>{editor.toast.message}</span><button
      class="icon-button small"
      aria-label="Dismiss notification"
      onclick={() => (editor.toast = null)}><Icon name="x" size={12} /></button
    >
  </div>{/if}
{#if draggingFiles}<div class="drop-overlay">
    <Icon name="upload" size={34} /><strong>Bring it into the picture.</strong><span
      >Drop an image or a Bonaparte project.</span
    >
  </div>{/if}

<style>
  .shell {
    height: 100dvh;
    display: grid;
    grid-template-rows: 52px 38px minmax(160px, 1fr) var(--timeline-height) 24px;
    overflow: hidden;
    background: #1a1b1a;
  }
  .workspace {
    display: grid;
    grid-template-columns: 236px minmax(240px, 1fr) 304px;
    min-height: 0;
    min-width: 0;
    overflow: hidden;
  }
  .timeline-region {
    min-height: 0;
    position: relative;
    display: flex;
    flex-direction: column;
  }
  .timeline-region > :global(.timeline) {
    flex: 1;
  }
  .timeline-resizer {
    position: absolute;
    top: -4px;
    left: 0;
    right: 0;
    height: 7px;
    z-index: 45;
    cursor: row-resize;
    background: transparent;
    touch-action: none;
  }
  .timeline-resizer:hover {
    background: #cbd2c740;
  }
  .statusbar {
    height: 24px;
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 0 13px;
    background: #232421;
    border-top: 1px solid #393c37;
    font-size: 8px;
    color: #7f837b;
    letter-spacing: 0.1px;
  }
  .connection-dot {
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: #acb1a7;
  }
  .connection-dot.error {
    background: #aea49d;
  }
  .status-separator {
    color: #4a4e47;
    margin: 0 4px;
  }
  .statusbar button,
  .recovery-status {
    display: flex;
    gap: 5px;
    align-items: center;
    font-size: 8px;
  }
  .statusbar .mono {
    font-size: 7px;
  }
  .statusbar button:hover {
    color: var(--accent);
  }
  .loading-screen {
    position: fixed;
    inset: 0;
    z-index: 200;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 18px;
    background: #1d1f1d;
    color: #a3a89f;
    text-align: center;
    padding: 30px;
  }
  .loader-brand {
    font-size: 39px;
    color: #e1e4df;
    letter-spacing: -1.7px;
    font-weight: 600;
    margin-bottom: 5px;
  }
  .loader-brand span {
    color: var(--accent);
  }
  .loading-screen p {
    font-size: 11px;
    line-height: 1.8;
    max-width: 520px;
    margin: 0;
  }
  .loading-screen strong {
    font-size: 15px;
    font-weight: 400;
  }
  .loading-hint {
    color: #82867e;
  }
  .loading-track {
    width: 110px;
    height: 2px;
    background: #424640;
    overflow: hidden;
  }
  .loading-track span {
    display: block;
    height: 100%;
    width: 40%;
    background: var(--accent);
    animation: load 1.4s ease-in-out infinite;
  }
  @keyframes load {
    from {
      transform: translateX(-110%);
    }
    to {
      transform: translateX(360%);
    }
  }
  .toast {
    position: fixed;
    bottom: 39px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 220;
    display: flex;
    align-items: center;
    gap: 10px;
    background: #393c38;
    border: 1px solid #777b73;
    border-radius: 6px;
    box-shadow: 0 10px 40px #0006;
    color: #d4d8d1;
    font-size: 11px;
    line-height: 1.6;
    padding: 10px 11px;
    max-width: min(580px, 90vw);
  }
  .toast.error {
    border-color: #736d67;
    background: #34322f;
    color: #cbc6bf;
  }
  .toast-symbol {
    height: 23px;
    width: 23px;
    display: grid;
    place-items: center;
    flex-shrink: 0;
    color: var(--accent);
    background: #61665c30;
    border-radius: 50%;
  }
  .toast.error .toast-symbol {
    color: #beb7ad;
    background: #80756e20;
  }
  .toast > .icon-button {
    margin-left: 6px;
  }
  .drop-overlay {
    position: fixed;
    inset: 14px;
    z-index: 300;
    border: 2px dashed #c0c5bb;
    border-radius: 12px;
    background: #282a26ea;
    backdrop-filter: blur(4px);
    pointer-events: none;
    display: flex;
    flex-direction: column;
    justify-content: center;
    align-items: center;
    gap: 19px;
    color: #c3c8bf;
  }
  .drop-overlay strong {
    font-size: 25px;
    letter-spacing: -0.5px;
    font-weight: 500;
  }
  .drop-overlay span {
    font-size: 12px;
    color: #959a91;
  }
  @media (max-width: 1150px) {
    .workspace {
      grid-template-columns: 210px minmax(220px, 1fr) 282px;
    }
  }
  @media (max-width: 900px) {
    .workspace {
      grid-template-columns: 180px minmax(220px, 1fr) 260px;
    }
  }
  @media (max-width: 780px) {
    .workspace {
      grid-template-columns: minmax(220px, 1fr) 260px;
    }
    .workspace > :global(.sidebar) {
      display: none;
    }
    .recovery-status {
      display: none;
    }
  }
</style>
