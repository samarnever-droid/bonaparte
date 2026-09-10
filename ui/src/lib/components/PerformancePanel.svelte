<script lang="ts">
  import { onDestroy, tick } from "svelte";
  import Icon from "./Icon.svelte";
  import { editor, refreshPreviewStatus, clearPreviewCache } from "../store.svelte";
  import { backendLabel, memoryLabel } from "../preview";
  let open = $state(false),
    busy = $state(false);
  let panel = $state<HTMLDialogElement | null>(null);
  const status = $derived(editor.previewStatus);
  const frame = $derived(editor.previewMetadata);
  const reason = $derived(frame?.fallbackReason ?? status?.gpu.reason);
  let timer: ReturnType<typeof setInterval> | null = null;
  async function toggle() {
    open = !open;
    if (timer) clearInterval(timer);
    if (open) {
      await refreshPreviewStatus();
      await tick();
      panel?.focus();
      timer = setInterval(() => {
        if (open) void refreshPreviewStatus();
      }, 2000);
    }
  }
  function close() {
    open = false;
    if (timer) clearInterval(timer);
    timer = null;
  }
  async function clear(retry = false) {
    busy = true;
    await clearPreviewCache(retry);
    busy = false;
  }
  onDestroy(() => {
    if (timer) clearInterval(timer);
  });
</script>

<svelte:window
  onkeydown={(e) => {
    if (open && e.key === "Escape") {
      e.preventDefault();
      close();
    }
  }}
/>
<div class="performance">
  <button
    class="engine-badge"
    aria-label="Preview performance"
    aria-expanded={open}
    aria-controls="preview-performance-panel"
    title={reason ?? "Actual preview backend, cache usage and renderer diagnostics"}
    onclick={() => void toggle()}
  >
    <span class="engine-dot" class:hardware={frame?.backend === "gpu"}></span>
    Rust compositor
    <span class="backend-chip"
      >{frame ? backendLabel(frame.backend) : editor.previewProtocol >= 3 ? "READY" : "CPU"}</span
    ><Icon name="down" size={10} />
  </button>
  {#if open}
    <button class="outside" aria-label="Close performance panel" onclick={close}></button>
    <dialog
      open
      bind:this={panel}
      tabindex="-1"
      id="preview-performance-panel"
      class="performance-dialog"
      aria-labelledby="performance-title"
    >
      <div class="heading">
        <Icon name="bolt" size={16} /><strong id="performance-title">Preview performance</strong
        ><span class="spacer"></span><button
          class="icon-button small"
          aria-label="Dismiss performance panel"
          onclick={close}><Icon name="x" size={13} /></button
        >
      </div>
      <p class="intro">
        Quality is a preview preference.<br />Your project and full-resolution exports stay
        unchanged.
      </p>
      {#if status}
        <div class="execution">
          <span class="eyebrow">ACTUAL EXECUTION</span>
          <div>
            <strong>{frame ? backendLabel(frame.backend) : "Waiting for a frame"}</strong><span
              class="spacer"
            ></span><span class="frame-kind">{frame?.cacheHit ? "FRAME CACHE" : "RENDERED"}</span>
          </div>
          {#if frame}<p>
              {frame.width} × {frame.height} preview pixels · {frame.cacheHit
                ? `${Math.round(frame.originalRenderMs)} ms originally`
                : `${frame.renderMs.toFixed(1)} ms render`}
            </p>{/if}
          <small
            >Requested: {editor.previewBackend === "auto"
              ? "Auto"
              : editor.previewBackend.toUpperCase()}</small
          >
        </div>
        <div class="device">
          <span class="eyebrow">GRAPHICS ADAPTER</span><strong
            >{status.gpu.name ?? "No GPU adapter available"}</strong
          ><span
            >{status.gpu.api ?? "CPU fallback"}{status.gpu.deviceType
              ? ` · ${status.gpu.deviceType}`
              : ""}{status.gpu.software ? " · software, not hardware" : ""}</span
          >
        </div>
        {#if reason}<p class="reason"><Icon name="info" size={12} /><span>{reason}</span></p>{/if}
        <div class="cache-row">
          <span>Rendered frames <small>{status.frames.entries} cached</small></span><strong
            >{memoryLabel(status.frames.bytes)}
            <small>/ {memoryLabel(status.frames.budgetBytes)}</small></strong
          >
        </div>
        <meter
          min="0"
          max={status.frames.budgetBytes}
          value={status.frames.bytes}
          aria-label="Frame cache memory"
        ></meter>
        <div class="cache-row">
          <span>Text & shapes <small>{status.sources.entries} sources</small></span><strong
            >{memoryLabel(status.sources.bytes)}
            <small>/ {memoryLabel(status.sources.budgetBytes)}</small></strong
          >
        </div>
        <meter
          min="0"
          max={status.sources.budgetBytes}
          value={status.sources.bytes}
          aria-label="Source cache memory"
        ></meter>
        {#if status.gpu.initialized}<div class="cache-row compact">
            <span>GPU textures & uploads</span><strong
              >{memoryLabel(status.gpu.pooledBytes + status.gpu.uploadBytes)}</strong
            >
          </div>{/if}
        <div class="counters">
          <span><strong>{status.frames.hits}</strong> cache hits</span><span
            ><strong>{status.cpuFrames}</strong> CPU renders</span
          ><span><strong>{status.gpuFrames}</strong> GPU renders</span>
        </div>
        <p class="note">
          Budgets count retained caches, not total process or driver memory. Unsupported GPU effects
          fall back to a complete CPU frame.
        </p>
        <div class="actions">
          <button class="btn" disabled={busy} onclick={() => void clear()}
            ><Icon name="trash" size={12} />Clear caches</button
          ><button
            class="btn ghost"
            disabled={busy || !status.gpu.compiled}
            onclick={() => void clear(true)}><Icon name="rotate" size={12} />Retry GPU</button
          >
        </div>
      {:else}<p class="note">
          This renderer uses the legacy preview protocol. Restart the Rust service to enable the new
          preview controls.
        </p>{/if}
    </dialog>
  {/if}
</div>

<style>
  .performance {
    position: relative;
  }
  .engine-badge {
    position: relative;
    z-index: 73;
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 9px;
    color: #969c91;
    padding: 7px 0 7px 8px;
  }
  .engine-dot {
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: #a1aa96;
  }
  .engine-dot.hardware {
    background: var(--accent);
    box-shadow: 0 0 8px #aed28c44;
  }
  .backend-chip {
    font: 7px monospace;
    border: 1px solid #52584d;
    padding: 2px 4px;
    border-radius: 3px;
    color: #aeb6a7;
  }
  .outside {
    position: fixed;
    /* Start below the top chrome (52 px header + 38 px workflow bar): the
       menus, workspace tabs and header buttons must stay clickable while
       this panel is open — a sheet that covers them swallows every click
       (and stalls automated actors entirely). Content below still dismisses. */
    top: 90px;
    right: 0;
    bottom: 0;
    left: 0;
    z-index: 70;
    cursor: default;
    background: transparent;
  }
  .performance-dialog {
    /* Above the promoted workflow bar (76) so the panel isn't clipped by
       the menu strip it hangs from. */
    position: absolute;
    display: block;
    right: 0;
    left: auto;
    top: calc(100% + 10px);
    z-index: 78;
    margin: 0;
    padding: 18px;
    width: 360px;
    max-width: calc(100vw - 30px);
    max-height: calc(100vh - 100px);
    overflow-y: auto;
    border: 1px solid #51574a;
    border-radius: 8px;
    background: #22251f;
    color: var(--text);
    box-shadow: 0 18px 70px #0008;
    outline: none;
  }
  .heading {
    display: flex;
    gap: 9px;
    align-items: center;
    color: #c0d9aa;
  }
  .heading strong {
    font-size: 12px;
    color: #d5dacc;
    font-weight: 500;
  }
  .intro {
    color: #969e8e;
    font-size: 10px;
    line-height: 1.7;
    margin: 12px 0 17px;
  }
  .execution {
    border: 1px solid #46503e;
    border-radius: 5px;
    background: #acc49108;
    padding: 12px;
  }
  .eyebrow {
    display: block;
    font: 7px monospace;
    letter-spacing: 1px;
    color: #87957b;
    margin-bottom: 8px;
  }
  .execution > div {
    display: flex;
    align-items: center;
  }
  .execution strong {
    font-size: 15px;
    font-weight: 500;
  }
  .frame-kind {
    color: #abc697;
    font: 7px monospace;
    border: 1px solid #52624a;
    border-radius: 3px;
    padding: 3px 4px;
  }
  .execution p {
    font: 9px monospace;
    color: #b2beaa;
    margin: 9px 0 5px;
  }
  .execution small {
    font-size: 8px;
    color: #89967f;
  }
  .device {
    margin: 17px 0 12px;
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .device .eyebrow {
    margin-bottom: 2px;
  }
  .device strong {
    font-size: 10px;
    font-weight: 500;
    overflow-wrap: anywhere;
    color: #c0c8b9;
  }
  .device > span:last-child {
    font-size: 8px;
    color: #8f9b84;
  }
  .reason {
    display: flex;
    gap: 7px;
    color: #b4b398;
    font-size: 9px;
    line-height: 1.7;
    padding: 10px;
    background: #b9a76609;
    border: 1px solid #635d403f;
    border-radius: 4px;
  }
  .reason :global(svg) {
    flex-shrink: 0;
    margin-top: 2px;
  }
  .cache-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 9px;
    margin: 16px 0 7px;
    color: #b5beac;
  }
  .cache-row strong {
    font: 9px monospace;
  }
  .cache-row small {
    font-size: 7px;
    color: #829275;
  }
  .cache-row > span small {
    margin-left: 5px;
  }
  .cache-row.compact {
    padding-top: 10px;
    border-top: 1px solid #3c4634;
  }
  meter {
    display: block;
    width: 100%;
    height: 5px;
    border: none;
    border-radius: 3px;
    background: #39432f;
  }
  meter::-webkit-meter-bar {
    border: none;
    background: #39432f;
    height: 5px;
    border-radius: 3px;
  }
  meter::-webkit-meter-optimum-value {
    background: #a1bd86;
  }
  .counters {
    display: flex;
    justify-content: space-between;
    margin-top: 17px;
    color: #87977b;
    font-size: 8px;
  }
  .counters strong {
    color: #bacbae;
    font: 10px monospace;
  }
  .note {
    font-size: 8px;
    line-height: 1.8;
    color: #8f9b85;
    margin: 15px 0;
  }
  .actions {
    display: flex;
    gap: 8px;
    padding-top: 13px;
    border-top: 1px solid #3f4a36;
  }
  .actions .btn {
    font-size: 9px;
  }
</style>
