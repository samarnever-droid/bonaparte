<script lang="ts">
  import Icon from "./Icon.svelte";
  import { contextMenu, closeContextMenu } from "../store.svelte";

  // Rough menu footprint for first-paint clamping; refined after measure.
  const ITEM_H = 30,
    PAD = 14;
  let left = $state(0),
    top = $state(0),
    menuEl = $state<HTMLElement | null>(null);

  $effect(() => {
    const open = contextMenu.current;
    if (!open) return;
    const width = menuEl?.offsetWidth ?? 190,
      height = menuEl?.offsetHeight ?? open.items.length * ITEM_H + PAD;
    left = Math.max(6, Math.min(open.x, window.innerWidth - width - 6));
    top = Math.max(6, Math.min(open.y, window.innerHeight - height - 6));
  });

  function keydown(event: KeyboardEvent) {
    if (!contextMenu.current) return;
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      closeContextMenu();
    }
  }
</script>

<svelte:window onkeydown={keydown} onresize={closeContextMenu} />

{#if contextMenu.current}
  <button
    class="ctx-overlay"
    aria-label="Close context menu"
    onclick={closeContextMenu}
    oncontextmenu={(e) => {
      e.preventDefault();
      closeContextMenu();
    }}></button>
  <div
    class="ctx-menu"
    role="menu"
    tabindex="-1"
    bind:this={menuEl}
    style={`left:${left}px;top:${top}px`}
    oncontextmenu={(e) => e.preventDefault()}>
    {#each contextMenu.current.items as item, i}
      {#if item.separator}
        <hr />
      {:else}
        <button
          role="menuitem"
          class:danger={item.danger}
          disabled={item.disabled}
          onclick={() => {
            closeContextMenu();
            item.run?.();
          }}>
          {#if item.icon}<Icon name={item.icon} size={12} />{:else}<span class="icon-gap"></span>{/if}
          <span class="ctx-label">{item.label}</span>
          {#if item.hint}<kbd>{item.hint}</kbd>{/if}
        </button>
      {/if}
    {/each}
  </div>
{/if}

<style>
  .ctx-overlay {
    position: fixed;
    inset: 0;
    z-index: 90;
    background: transparent;
    border: 0;
    cursor: default;
  }
  .ctx-menu {
    position: fixed;
    z-index: 91;
    min-width: 172px;
    display: flex;
    flex-direction: column;
    padding: 5px;
    background: #262826;
    border: 1px solid #3a3d3a;
    border-radius: 9px;
    box-shadow: 0 14px 38px #0009;
  }
  .ctx-menu hr {
    border: 0;
    border-top: 1px solid #383b38;
    margin: 4px 6px;
  }
  .ctx-menu button {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 9px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--text-1, #e4ead9);
    font: inherit;
    font-size: 12px;
    text-align: left;
    cursor: pointer;
  }
  .ctx-menu button:hover:not(:disabled),
  .ctx-menu button:focus-visible {
    background: #343832;
    outline: none;
  }
  .ctx-menu button:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .ctx-menu button.danger {
    color: #e39a90;
  }
  .ctx-menu button.danger:hover:not(:disabled) {
    background: #3d2e2b;
  }
  .ctx-label {
    flex: 1;
  }
  .icon-gap {
    width: 12px;
  }
  kbd {
    font: inherit;
    font-size: 10px;
    color: var(--text-3, #8b9284);
  }
</style>
