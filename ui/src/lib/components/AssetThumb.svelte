<script lang="ts">
  /** A media asset's face: real pixels when the document carries them,
   *  a type icon until (or unless) they decode. */
  import { ensureThumb } from "../media/thumbs";
  import Icon from "./Icon.svelte";
  import type { MediaAsset } from "../model";

  let { asset }: { asset: MediaAsset } = $props();
  let src = $state<string | null>(null);

  $effect(() => {
    const current = asset;
    src = null;
    void ensureThumb(current).then((url) => {
      if (url) src = url;
    });
  });

  const fallback = $derived(
    asset.audio
      ? "wave"
      : typeof asset.kind === "object" && "Video" in asset.kind
        ? "film"
        : "image",
  );
</script>

{#if src}
  <img class="asset-thumb" {src} alt="" />
{:else}
  <span class="asset-thumb icon"><Icon name={fallback} size={14} /></span>
{/if}

<style>
  .asset-thumb {
    width: 40px;
    height: 24px;
    border-radius: 4px;
    object-fit: cover;
    background: rgba(255, 255, 255, 0.06);
    flex: 0 0 auto;
  }
  .asset-thumb.icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    opacity: 0.75;
  }
</style>
