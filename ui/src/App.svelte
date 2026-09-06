<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import {
    editor,
    init,
    undoOp,
    redoOp,
    play,
    pause,
    scrub,
    activeComp,
    selectedLayer,
    applyOp,
    toggleKeyframeAtPlayhead,
  } from "./lib/store.svelte";
  import { fpsAsNumber, ticksPerFrame, type Property } from "./lib/model";
  import TopBar from "./lib/components/TopBar.svelte";
  import PresetBrowser from "./lib/components/PresetBrowser.svelte";
  import Viewport from "./lib/components/Viewport.svelte";
  import Properties from "./lib/components/Properties.svelte";
  import Timeline from "./lib/components/Timeline.svelte";

  let showPresets = $state(false);

  function handleKeyDown(e: KeyboardEvent) {
    const target = e.target as HTMLElement | null;
    const isInput =
      target &&
      (target.tagName === "INPUT" ||
        target.tagName === "TEXTAREA" ||
        target.tagName === "SELECT" ||
        target.isContentEditable);

    // 1. Undo / Redo (Ctrl+Z / Ctrl+Shift+Z / Ctrl+Y)
    if (e.ctrlKey && e.key.toLowerCase() === "z") {
      e.preventDefault();
      if (e.shiftKey) void redoOp();
      else void undoOp();
      return;
    }
    if (e.ctrlKey && e.key.toLowerCase() === "y") {
      e.preventDefault();
      void redoOp();
      return;
    }

    // Do not intercept editing keys if user is typing inside an input element
    if (isInput) return;

    // 2. Play / Pause (Space)
    if (e.code === "Space" || e.key === " ") {
      e.preventDefault();
      if (editor.playing) pause();
      else play();
      return;
    }

    // 3. Toggle Keyframe at playhead (K)
    if (e.key.toLowerCase() === "k" && !e.ctrlKey && !e.altKey && !e.metaKey) {
      e.preventDefault();
      const comp = activeComp();
      const layer = selectedLayer();
      if (comp && layer) {
        const animatedTracks = Object.keys(layer.tracks) as Property[];
        if (animatedTracks.length > 0) {
          for (const prop of animatedTracks) {
            void toggleKeyframeAtPlayhead(layer.id, prop);
          }
        } else {
          // Default to Position track if layer has no animated tracks yet
          void toggleKeyframeAtPlayhead(layer.id, "Position");
        }
      }
      return;
    }

    // 4. Delete / Backspace (Remove selected layer)
    if ((e.key === "Delete" || e.key === "Backspace") && !e.ctrlKey && !e.altKey && !e.metaKey) {
      e.preventDefault();
      const comp = activeComp();
      const layer = selectedLayer();
      if (comp && layer) {
        const layerId = layer.id;
        editor.selected = null;
        void applyOp({
          type: "removeLayer",
          comp: comp.id,
          layer: layerId,
        });
      }
      return;
    }

    // 5. Arrow keys (Step 1 frame, or 10 frames with Shift)
    if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
      e.preventDefault();
      const comp = activeComp();
      if (comp) {
        const fps = fpsAsNumber(comp.fps) || 30;
        const isTicks = comp.duration > 1000;
        const frameStep = isTicks ? ticksPerFrame(comp.fps) : 1 / fps;
        const multiplier = e.shiftKey ? 10 : 1;
        const delta = (e.key === "ArrowRight" ? 1 : -1) * frameStep * multiplier;
        const nextTime = Math.min(comp.duration, Math.max(0, editor.currentTime + delta));
        scrub(nextTime);
      }
      return;
    }

    // 6. Home / End (Jump to start or end of timeline)
    if (e.key === "Home" && !e.ctrlKey && !e.altKey) {
      e.preventDefault();
      scrub(0);
      return;
    }
    if (e.key === "End" && !e.ctrlKey && !e.altKey) {
      e.preventDefault();
      const comp = activeComp();
      if (comp) scrub(comp.duration);
      return;
    }

    // 7. Toggle Preset Browser (P)
    if (e.key.toLowerCase() === "p" && !e.ctrlKey && !e.altKey && !e.metaKey) {
      e.preventDefault();
      showPresets = !showPresets;
      return;
    }
  }

  onMount(() => {
    void init();
    window.addEventListener("keydown", handleKeyDown);
  });

  onDestroy(() => {
    window.removeEventListener("keydown", handleKeyDown);
  });
</script>

<div class="grid h-screen w-screen grid-rows-[48px_1fr_240px] overflow-hidden bg-[var(--bg-base)] text-[var(--text)]">
  <!-- Top Navigation Bar -->
  <TopBar
    onTogglePresets={() => (showPresets = !showPresets)}
    presetsOpen={showPresets}
  />

  <!-- Main Workspace Row: Optional Preset Browser + Viewport + Properties -->
  <div class="grid min-h-0 {showPresets ? 'grid-cols-[280px_1fr_280px]' : 'grid-cols-[1fr_280px]'}">
    {#if showPresets}
      <PresetBrowser onClose={() => (showPresets = false)} />
    {/if}
    <Viewport />
    <Properties />
  </div>

  <!-- Timeline Panel -->
  <Timeline />
</div>

{#if !editor.project}
  <div class="fixed inset-0 z-50 grid place-items-center bg-[var(--bg-base)] text-sm text-[var(--text-dim)]">
    <div class="flex flex-col items-center gap-2">
      <div class="h-6 w-6 animate-spin rounded-full border-2 border-[var(--accent)] border-t-transparent"></div>
      <span>Loading Bonaparte project…</span>
    </div>
  </div>
{/if}
