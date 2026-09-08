<script lang="ts">
  import Icon from "./Icon.svelte";
  import PerformancePanel from "./PerformancePanel.svelte";
  import {
    editor,
    activeComp,
    selectedLayer,
    applyOp,
    saveProject,
    openProject,
    undoOp,
    redoOp,
    addLayer,
    duplicateSelected,
    deleteSelected,
    setWorkspace,
  } from "../store.svelte";
  let menu = $state<"file" | "layer" | null>(null);
  const comp = $derived(activeComp());
  function run(action: () => unknown) {
    menu = null;
    action();
  }
</script>

<header class="topbar">
  <div class="brand">
    <svg width="23" height="25" viewBox="0 0 23 25" fill="none" aria-label="Bonaparte"
      ><path
        d="M3 2h10c8 0 9 10 2 11 9 1 7 10-1 10H3V2Z"
        stroke="currentColor"
        stroke-width="2.6"
      /><path d="M8 7v11m0-6h5" stroke="currentColor" stroke-width="2.4" /></svg
    >
    <span>bonaparte<span class="brand-dot">.</span></span>
  </div>
  <span class="header-divider"></span>
  <Icon name="folder" size={14} class="dim" />
  <input
    class="project-name"
    aria-label="Project name"
    value={editor.project?.name ?? "Untitled project"}
    onchange={(e) => void applyOp({ type: "renameProject", name: e.currentTarget.value })}
  />
  <span class="save-state"
    ><i class:dirty={editor.dirty}></i>{editor.dirty ? "Unsaved changes" : "Project ready"}</span
  >
  <div class="spacer"></div>
  <div class="row history-actions">
    <button
      class="icon-button"
      title="Undo (Ctrl/⌘ Z)"
      aria-label="Undo"
      disabled={!editor.canUndo || editor.pending > 0}
      onclick={() => void undoOp()}><Icon name="undo" size={16} /></button
    >
    <button
      class="icon-button"
      title="Redo (Ctrl/⌘ Shift Z)"
      aria-label="Redo"
      disabled={!editor.canRedo || editor.pending > 0}
      onclick={() => void redoOp()}><Icon name="redo" size={16} /></button
    >
    <span class="divider"></span>
    <button class="btn ghost" onclick={() => void saveProject()} title="Save project (Ctrl/⌘ S)"
      ><Icon name="save" size={14} />Save</button
    >
    <button
      class="btn primary export-button"
      onclick={() => (editor.dialog = { kind: "export" })}
      disabled={!comp}
      ><Icon name="download" size={14} />Export<Icon name="right" size={12} /></button
    >
  </div>
</header>

<nav class="workflow-bar" aria-label="Editor workspace">
  <div class="menus">
    <div class="menu-anchor">
      <button
        class="menu-trigger"
        class:active={menu === "file"}
        onclick={() => (menu = menu === "file" ? null : "file")}
        >File<Icon name="down" size={11} /></button
      >
      {#if menu === "file"}
        <div class="menu">
          <button onclick={() => run(() => (editor.dialog = { kind: "new-project" }))}
            ><Icon name="plus" />New project</button
          >
          <button onclick={() => run(openProject)}
            ><Icon name="folder" />Open project…<kbd>⌘ O</kbd></button
          >
          <button onclick={() => run(saveProject)}
            ><Icon name="save" />Save project<kbd>⌘ S</kbd></button
          >
          <button onclick={() => run(() => saveProject(true))}
            ><Icon name="save" />Save project as…<kbd>⇧ ⌘ S</kbd></button
          >
          <hr />
          <button onclick={() => run(() => (editor.dialog = { kind: "composition", compId: null }))}
            ><Icon name="film" />New composition…</button
          >
          <button
            disabled={!comp}
            onclick={() => run(() => (editor.dialog = { kind: "composition", compId: comp!.id }))}
            ><Icon name="settings" />Composition settings…</button
          >
          <hr />
          <button onclick={() => run(() => (editor.dialog = { kind: "export" }))}
            ><Icon name="download" />Export…</button
          >
        </div>
      {/if}
    </div>
    <div class="menu-anchor">
      <button
        class="menu-trigger"
        class:active={menu === "layer"}
        onclick={() => (menu = menu === "layer" ? null : "layer")}
        >Layer<Icon name="down" size={11} /></button
      >
      {#if menu === "layer"}
        <div class="menu">
          <button onclick={() => run(() => addLayer("text"))}><Icon name="type" />Text layer</button
          >
          <button onclick={() => run(() => addLayer("rectangle"))}
            ><Icon name="square" />Rectangle</button
          >
          <button onclick={() => run(() => addLayer("circle"))}
            ><Icon name="circle" />Ellipse</button
          >
          <button onclick={() => run(() => addLayer("solid"))}
            ><Icon name="square" />Solid color</button
          >
          <button onclick={() => run(() => addLayer("adjustment"))}
            ><Icon name="adjust" />Adjustment layer</button
          >
          <hr />
          <button
            disabled={!selectedLayer() || selectedLayer()?.locked}
            onclick={() => run(duplicateSelected)}
            ><Icon name="copy" />Duplicate<kbd>⌘ D</kbd></button
          >
          <button
            disabled={!selectedLayer() || selectedLayer()?.locked}
            onclick={() => run(deleteSelected)}><Icon name="trash" />Delete<kbd>⌫</kbd></button
          >
        </div>
      {/if}
    </div>
    <button class="menu-trigger" onclick={() => (editor.dialog = { kind: "shortcuts" })}
      >Help</button
    >
  </div>
  <div class="workspace-tabs">
    {#each [{ name: "Design", icon: "layers" }, { name: "Color", icon: "palette" }, { name: "Animate", icon: "graph" }, ...(editor.audioProtocol ? [{ name: "Audio", icon: "headphones" }] : [])] as item}
      <button
        class:active={editor.workspace === item.name}
        onclick={() => setWorkspace(item.name as typeof editor.workspace)}
        ><Icon name={item.icon} size={13} />{item.name}</button
      >
    {/each}
  </div>
  <PerformancePanel />
</nav>
{#if menu}<button class="menu-overlay" aria-label="Close menu" onclick={() => (menu = null)}
  ></button>{/if}

<style>
  .topbar {
    height: 52px;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 17px;
    background: #212221;
    border-bottom: 1px solid #111211;
  }
  .brand {
    display: flex;
    gap: 9px;
    align-items: center;
    color: #e6e7e5;
    font-size: 21px;
    font-weight: 650;
    letter-spacing: -0.8px;
    min-width: 182px;
  }
  .brand svg {
    color: var(--accent);
  }
  .brand-dot {
    color: var(--accent);
  }
  .header-divider {
    height: 21px;
    width: 1px;
    background: var(--border);
    margin: 0 8px 0 1px;
  }
  .project-name {
    max-width: 260px;
    width: 230px;
    border: 0;
    background: transparent;
    outline: 0;
    font-size: 11px;
    color: #c5c6c4;
    text-overflow: ellipsis;
    padding: 4px;
    border-radius: 3px;
  }
  .project-name:focus {
    background: #353734;
  }
  .save-state {
    font-size: 9px;
    color: #848683;
    display: flex;
    gap: 6px;
    align-items: center;
    white-space: nowrap;
  }
  .save-state i {
    width: 4px;
    height: 4px;
    background: #a7ada4;
    border-radius: 50%;
  }
  .save-state i.dirty {
    background: #b2ada2;
  }
  .history-actions {
    gap: 2px;
  }
  .export-button {
    margin-left: 10px;
    gap: 9px;
    padding: 7px 11px;
  }
  .workflow-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    background: #252524;
    border-bottom: 1px solid var(--border);
    height: 38px;
    padding: 0 14px;
    position: relative;
    z-index: 61;
  }
  .menus {
    display: flex;
    gap: 2px;
    width: 220px;
  }
  .menu-anchor {
    position: relative;
  }
  .menu-trigger {
    height: 25px;
    padding: 0 8px;
    display: flex;
    align-items: center;
    gap: 7px;
    color: var(--text-dim);
    font-size: 10px;
    border-radius: 3px;
  }
  .menu-trigger:hover,
  .menu-trigger.active {
    background: #363735;
    color: var(--text);
  }
  .workspace-tabs {
    display: flex;
    gap: 3px;
    height: 100%;
    align-items: center;
  }
  .workspace-tabs button {
    display: flex;
    gap: 7px;
    align-items: center;
    height: 26px;
    min-width: 88px;
    justify-content: center;
    font-size: 10px;
    color: #91928f;
    border-radius: 4px;
  }
  .workspace-tabs button.active {
    color: var(--accent);
    background: #393b37;
    box-shadow: inset 0 0 0 1px #50534e;
  }
  @media (max-width: 1150px) {
    .save-state {
      display: none;
    }
    .brand {
      min-width: 168px;
    }
    .project-name {
      width: 200px;
    }
  }
  @media (max-width: 900px) {
    .brand {
      min-width: 140px;
      font-size: 18px;
    }
    .project-name {
      width: 150px;
    }
    .workspace-tabs {
      margin-left: auto;
    }
    .menus {
      width: 170px;
    }
    .header-divider {
      display: none;
    }
  }
</style>
