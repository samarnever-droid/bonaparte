<script lang="ts">
  import { onMount } from "svelte";
  import { command } from "../bridge";
  import { editor } from "../store.svelte";
  import type { AudioClip } from "../audio/model";
  let { clip, color = "#81B8C6" }: { clip: AudioClip; color?: string } = $props();
  let canvas = $state<HTMLCanvasElement | null>(null),
    width = $state(200),
    height = $state(52);
  let peaks = $state<number[][]>([]),
    channels = $state(2),
    error = $state(false);
  let version = 0;
  const source = $derived(editor.project?.media[String(clip.media)]?.audio);
  $effect(() => {
    const s = source,
      start = clip.source_offset,
      end = clip.source_offset + (clip.duration_frames - 1) * clip.rate,
      points = Math.max(8, Math.min(2048, Math.ceil(width)));
    if (!s) return;
    const token = ++version;
    error = false;
    if (editor.timelineGesture) return;
    const timer = setTimeout(() => {
      void command<{ peaks: number[][]; channels: number }>("audio_waveform", {
        mediaId: clip.media,
        startFrame: Math.max(0, Math.floor(Math.min(start, end))),
        endFrame: Math.min(s.frames, Math.ceil(Math.max(start, end)) + 1),
        points,
      })
        .then((result) => {
          if (token === version) {
            peaks = clip.rate < 0 ? [...result.peaks].reverse() : result.peaks;
            channels = result.channels;
          }
        })
        .catch(() => {
          if (token === version) error = true;
        });
    }, 100);
    return () => clearTimeout(timer);
  });
  $effect(() => {
    if (!canvas) return;
    const scale = window.devicePixelRatio || 1;
    canvas.width = Math.ceil(width * scale);
    canvas.height = Math.ceil(height * scale);
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.scale(scale, scale);
    ctx.clearRect(0, 0, width, height);
    ctx.strokeStyle = color;
    ctx.globalAlpha = 0.85;
    ctx.lineWidth = Math.max(0.7, width / Math.max(1, peaks.length));
    const rows = channels === 1 ? 1 : 2;
    const rowHeight = height / rows;
    for (let channel = 0; channel < rows; channel++) {
      const mid = rowHeight * (channel + 0.5);
      ctx.beginPath();
      ctx.moveTo(0, mid);
      ctx.lineTo(width, mid);
      ctx.globalAlpha = 0.25;
      ctx.stroke();
      ctx.globalAlpha = 0.9;
      ctx.beginPath();
      for (let i = 0; i < peaks.length; i++) {
        const x = ((i + 0.5) * width) / peaks.length;
        ctx.moveTo(x, mid - peaks[i][channel * 2 + 1] * rowHeight * 0.42);
        ctx.lineTo(x, mid - peaks[i][channel * 2] * rowHeight * 0.42);
      }
      ctx.stroke();
    }
  });
  onMount(() => {
    if (!canvas) return;
    const ro = new ResizeObserver((entries) => {
      width = Math.max(1, entries[0].contentRect.width);
      height = Math.max(1, entries[0].contentRect.height);
    });
    ro.observe(canvas);
    return () => ro.disconnect();
  });
</script>

<canvas
  bind:this={canvas}
  aria-label={`${channels === 1 ? "Mono" : "Stereo"} waveform for ${clip.name}`}
  title={error
    ? "Waveform unavailable"
    : `${channels === 1 ? "Mono" : "Stereo"} source waveform · gain is shown separately`}
></canvas>

<style>
  canvas {
    display: block;
    position: absolute;
    inset: 20px 0 3px;
    width: 100%;
    height: calc(100% - 23px);
    pointer-events: none;
  }
</style>
