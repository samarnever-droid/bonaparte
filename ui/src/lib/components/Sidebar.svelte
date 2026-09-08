<script lang="ts">
  import Icon from "./Icon.svelte";
  import AudioLibrary from "./AudioLibrary.svelte";
  import {
    editor,
    activeComp,
    selectedLayer,
    selectComp,
    addLayer,
    addEffect,
    importAnyFile,
    applyOp,
    newLayer,
    addMediaToComp,
  } from "../store.svelte";
  import { formatFps, timeToSecs } from "../model";
  import { create3dScene } from "../three-d";
  import { PRESETS, applyPreset } from "../presets";
  let search = $state("");
  let showHistory = $state(false);
  const comp = $derived(activeComp());
  const comps = $derived(
    Object.values(editor.project?.comps ?? {}).filter((c) =>
      c.name.toLowerCase().includes(search.toLowerCase()),
    ),
  );
  const assets = $derived(
    Object.values(editor.project?.media ?? {}).filter((a) =>
      a.name.toLowerCase().includes(search.toLowerCase()),
    ),
  );
  const effects = $derived(
    editor.effects.filter(
      (e) =>
        e.inputs.length && `${e.name} ${e.category}`.toLowerCase().includes(search.toLowerCase()),
    ),
  );
  const categories = $derived([...new Set(effects.map((e) => e.category || "Other"))]);
  const effectIcon = (category: string) =>
    category.startsWith("Color")
      ? "palette"
      : category.startsWith("Blur")
        ? "circle"
        : category.startsWith("Light")
          ? "sparkles"
          : "sliders";
  async function addPrecomp(id: number, name: string) {
    if (!comp) return;
    if (
      await applyOp({
        type: "addLayer",
        comp: comp.id,
        layer: newLayer({ PreComp: { comp: id } }, name, comp.duration),
      })
    )
      editor.selected = (editor.project?.next_layer ?? 1) - 1;
  }
</script>

<aside class="panel sidebar" aria-label="Project and effects library">
  <div class="panel-tabs">
    {#each ["project", "effects", "motion", ...(editor.audioProtocol ? ["audio"] : [])] as tab}
      <button
        class:active={editor.sidebar === tab}
        onclick={() => {
          editor.sidebar = tab as typeof editor.sidebar;
          search = "";
        }}
        >{tab === "project"
          ? "Project"
          : tab === "effects"
            ? "Effects"
            : tab === "audio"
              ? "Audio"
              : "Motion"}</button
      >
    {/each}
  </div>
  <div class="search">
    <Icon name="search" size={13} /><input
      aria-label="Search library"
      placeholder={editor.sidebar === "effects"
        ? "Find an effect…"
        : editor.sidebar === "motion"
          ? "Find a preset…"
          : "Search project…"}
      bind:value={search}
    /><kbd>⌕</kbd>
  </div>
  <div class="panel-scroll">
    {#if editor.sidebar === "project"}
      <div class="section-title upper">
        <Icon name="down" size={10} />Compositions<span class="spacer"></span><span class="count"
          >{Object.keys(editor.project?.comps ?? {}).length}</span
        ><button
          class="icon-button small"
          aria-label="New composition"
          title="New composition"
          onclick={() => (editor.dialog = { kind: "composition", compId: null })}
          ><Icon name="plus" size={13} /></button
        >
      </div>
      <div class="comp-list">
        {#each comps as item (item.id)}
          <div class="comp-item" class:active={comp?.id === item.id}>
            <button
              class="comp-main"
              onclick={() => selectComp(item.id)}
              aria-label={`Open composition ${item.name}`}
            >
              <span class="comp-icon"><Icon name="film" size={19} /></span>
              <span class="comp-info"
                ><strong class="truncate">{item.name}</strong><small class="mono"
                  >{item.width} × {item.height}<span>·</span>{timeToSecs(item.duration).toFixed(
                    0,
                  )}s</small
                ></span
              >
            </button>
            {#if comp?.id !== item.id}<button
                class="icon-button small add-comp"
                title="Add as a nested composition layer"
                aria-label={`Add ${item.name} as a layer`}
                onclick={() => void addPrecomp(item.id, item.name)}
                ><Icon name="plus" size={13} /></button
              >{:else}<span class="active-indicator"></span>{/if}
          </div>
        {/each}
        {#if !comps.length}<p class="search-empty">No compositions found.</p>{/if}
      </div>
      {#if comp}<div class="comp-details">
          <span>{formatFps(comp.fps)}</span><span>{comp.layer_order.length} layers</span><button
            title="Composition settings"
            aria-label="Composition settings"
            onclick={() => (editor.dialog = { kind: "composition", compId: comp.id })}
            ><Icon name="settings" size={12} /></button
          >
        </div>{/if}
      <div class="section-title upper">
        <Icon name="down" size={10} />Assets<span class="spacer"></span><span class="count"
          >{Object.keys(editor.project?.media ?? {}).length}</span
        >
      </div>
      {#each assets as asset (asset.id)}
        <div class="asset-item" class:reusable={!!comp}>
          <Icon
            name={asset.audio
              ? "wave"
              : typeof asset.kind === "object" && "Video" in asset.kind
                ? "film"
                : "image"}
            size={16}
          /><button
            class="asset-add"
            aria-label={`Add ${asset.name} to the composition`}
            title="Add to composition"
            disabled={!comp}
            onclick={() => void addMediaToComp(asset.id)}><Icon name="plus" size={12} /></button
          ><button
            class="asset-name truncate"
            aria-label={`Reuse ${asset.name}`}
            title="Click to add to the composition"
            disabled={!comp}
            onclick={() => void addMediaToComp(asset.id)}>{asset.name}</button
          ><span class="spacer"></span><small class="mono"
            >{asset.embedded?.width ?? asset.video?.width ?? ""}</small
          >
        </div>
      {/each}
      <button class="import-zone" onclick={() => void importAnyFile()} disabled={!comp}>
        <span class="import-icon"><Icon name="upload" size={16} /></span>
        <span
          ><strong>Bring your ideas in</strong><small
            >Drop a video, image, audio, SVG, or 3D model, or <em>browse files</em></small
          ></span
        >
        <span class="formats mono">MP4 · PNG · JPG · WEBP · SVG · OBJ · WAV</span>
      </button>
      <div class="create-section">
        <div class="section-title upper">Start with a layer</div>
        <div class="create-grid">
          {#each [{ id: "text", name: "Text", icon: "type" }, { id: "rectangle", name: "Shape", icon: "square" }, { id: "circle", name: "Ellipse", icon: "circle" }, { id: "adjustment", name: "Adjustment", icon: "adjust" }, { id: "3d", name: "3D Scene", icon: "cube" }] as item}
            <button
              onclick={() =>
                item.id === "3d"
                  ? create3dScene()
                  : void addLayer(item.id as "text" | "rectangle" | "circle" | "adjustment")}
              disabled={!comp}
              ><Icon name={item.icon} size={16} /><span>{item.name}</span><Icon
                name="plus"
                size={11}
              /></button
            >
          {/each}
        </div>
      </div>
      <button class="motion-link" onclick={() => (editor.sidebar = "motion")}
        ><span class="motion-icon"><Icon name="graph" size={18} /></span><span
          ><strong>A head start on motion</strong><small>Explore animation presets</small></span
        ><Icon name="right" size={12} /></button
      >
    {:else if editor.sidebar === "effects"}
      <div class="library-intro">
        <span class="badge green"><Icon name="sliders" size={11} />{effects.length} available</span>
        <p>Small changes.<br /><strong>Entirely different possibilities.</strong></p>
      </div>
      {#each categories as category}
        <div class="section-title upper"><Icon name="down" size={10} />{category}</div>
        <div class="effect-list">
          {#each effects.filter((e) => (e.category || "Other") === category) as effect (effect.id)}
            <button
              class="effect-item"
              onclick={() => void addEffect(effect.id)}
              disabled={!comp || selectedLayer()?.locked}
              title={`Add ${effect.name} to ${selectedLayer()?.name ?? "a new adjustment layer"}`}
            >
              <span class="effect-icon"><Icon name={effectIcon(category)} size={15} /></span><span
                >{effect.name}</span
              ><span class="spacer"></span><Icon name="plus" size={12} />
            </button>
          {/each}
        </div>
      {/each}
      {#if !effects.length}<div class="empty">No matching effects.</div>{/if}
      <div class="library-note">
        <Icon name="info" size={13} /><span
          >Controls come from plugin manifests. Effects are non-destructive and fully undoable.</span
        >
      </div>
    {:else if editor.sidebar === "audio"}
      <AudioLibrary {search} />
    {:else}
      <div class="library-intro">
        <span class="badge green"><Icon name="keyframe" size={11} />Animation presets</span>
        <p>Find your rhythm.<br /><strong>Then make it your own.</strong></p>
      </div>
      <div class="preset-list">
        {#each PRESETS.filter((p) => p.name
            .toLowerCase()
            .includes(search.toLowerCase())) as preset (preset.id)}
          <button
            class="preset-card"
            onclick={() => void applyPreset(preset.id)}
            disabled={!selectedLayer() || selectedLayer()?.locked}
            title="Apply at the playhead; existing keys at these times will be replaced"
          >
            <div class="preset-art">
              <Icon name={preset.icon} size={26} /><svg
                viewBox="0 0 90 30"
                width="90"
                height="30"
                aria-hidden="true"
                ><path
                  d="M2 28C38 28 25 3 88 3"
                  stroke="currentColor"
                  stroke-width="1"
                  fill="none"
                /><circle cx="2" cy="28" r="2" fill="currentColor" /><circle
                  cx="88"
                  cy="3"
                  r="2"
                  fill="currentColor"
                /></svg
              ><span class="mono">{preset.duration.toFixed(1)}s</span>
            </div>
            <div class="preset-copy">
              <strong>{preset.name}</strong><span>{preset.description}</span><Icon
                name="plus"
                size={13}
              />
            </div>
          </button>
        {/each}
      </div>
      <div class="library-note">
        <Icon name="info" size={13} /><span
          >{selectedLayer()
            ? "Apply at the playhead. Every keyframe stays editable in the timeline."
            : "Select a layer to add an animation."}</span
        >
      </div>
    {/if}
  </div>
  <div class="sidebar-bottom">
    <button class="history-toggle" onclick={() => (showHistory = !showHistory)}
      ><Icon name="history" size={13} /><span>History</span><span class="count"
        >{editor.history.length}</span
      ><span class="spacer"></span><Icon name={showHistory ? "down" : "up"} size={11} /></button
    >
    {#if showHistory}<div class="history-list">
        {#each editor.history.slice(-8).reverse() as entry, i}<div class:latest={i === 0}>
            <span class="history-point"></span><span class="truncate">{entry}</span>
          </div>{:else}<p>No edits yet. Your next move starts here.</p>{/each}
      </div>{/if}
    <div class="local-note">
      <Icon name="layers" size={12} /><span>Editable. Portable. Yours.</span><span class="spacer"
      ></span><span class="mono">v0.5.1</span>
    </div>
  </div>
</aside>

<style>
  .sidebar {
    border-right: 1px solid var(--border);
  }
  .panel-tabs {
    gap: 23px;
  }
  .search {
    margin: 14px 12px 0;
    padding: 7px 9px;
    background: #1a1b1a;
    border: 1px solid var(--border);
    border-radius: 4px;
    display: flex;
    gap: 7px;
    align-items: center;
    color: var(--text-muted);
  }
  .search input {
    background: none;
    outline: none;
    border: 0;
    width: 100%;
    font-size: 10px;
  }
  .search input::placeholder {
    color: #7a7c78;
  }
  .search kbd {
    font: 12px monospace;
    color: #6c6f6b;
  }
  .comp-list {
    padding: 0 8px;
  }
  .comp-item {
    display: flex;
    align-items: center;
    border: 1px solid transparent;
    border-radius: 4px;
    margin-bottom: 3px;
    padding-right: 6px;
  }
  .comp-item.active {
    background: #c3cac010;
    border-color: #797c7650;
  }
  .comp-main {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    min-width: 0;
    padding: 10px 8px;
    text-align: left;
  }
  .comp-icon {
    width: 31px;
    height: 34px;
    display: grid;
    place-items: center;
    color: #afacb3;
    background: #333234;
    border: 1px solid #4d4a4f;
    border-radius: 3px;
  }
  .comp-item.active .comp-icon {
    color: #cac7cf;
  }
  .comp-info {
    display: flex;
    flex-direction: column;
    min-width: 0;
    gap: 5px;
  }
  .comp-info strong {
    font-size: 11px;
    font-weight: 500;
  }
  .comp-info small {
    font-size: 8px;
    color: var(--text-dim);
  }
  .comp-info small span {
    padding: 0 6px;
  }
  .active-indicator {
    height: 4px;
    width: 4px;
    flex-shrink: 0;
    background: var(--accent);
    border-radius: 50%;
    margin: 0 6px;
  }
  .add-comp {
    opacity: 0;
  }
  .comp-item:hover .add-comp {
    opacity: 1;
  }
  .comp-details {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 7px 18px 14px;
    margin: 0 8px;
    border-bottom: 1px solid var(--border-subtle);
    font-size: 9px;
    color: #939591;
  }
  .comp-details button {
    margin-left: auto;
    color: #949792;
  }
  .asset-item.reusable {
    cursor: pointer;
  }
  .asset-add {
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    border-radius: 4px;
    color: #a1a49e;
    opacity: 0;
    transition:
      opacity 0.12s ease,
      background-color 0.12s ease;
  }
  .asset-item.reusable:hover .asset-add,
  .asset-item.reusable:focus-within .asset-add {
    opacity: 1;
  }
  .asset-add:hover {
    background: #ffffff14;
    color: #e6e8e4;
  }
  .asset-name {
    text-align: left;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }
  .asset-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 9px 18px;
    font-size: 10px;
    color: #a7aaa5;
  }
  .asset-item small {
    font-size: 8px;
    color: #7b7d79;
  }
  .import-zone {
    display: flex;
    align-items: center;
    justify-content: center;
    flex-direction: column;
    text-align: center;
    border: 1px dashed #464744;
    border-radius: 5px;
    margin: 4px 14px 14px;
    padding: 18px 8px 13px;
    width: calc(100% - 28px);
    background: #1d1e1d;
    gap: 10px;
  }
  .import-zone:hover {
    border-color: #979b94;
    background: #262826;
  }
  .import-icon {
    color: #a9ada7;
    border: 1px solid #40423f;
    background: #282a27;
    display: grid;
    place-items: center;
    border-radius: 5px;
    height: 30px;
    width: 30px;
  }
  .import-zone strong {
    display: block;
    font-size: 10px;
    font-weight: 500;
    margin-bottom: 5px;
  }
  .import-zone small {
    color: #80837e;
    font-size: 9px;
  }
  .import-zone em {
    font-style: normal;
    color: #c0c6bd;
  }
  .formats {
    font-size: 7px;
    letter-spacing: 1px;
    color: #646762;
  }
  .create-section {
    border-top: 1px solid var(--border-subtle);
    margin-top: 8px;
  }
  .create-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
    padding: 0 14px;
  }
  .create-grid button {
    display: flex;
    align-items: center;
    gap: 8px;
    text-align: left;
    padding: 11px 8px;
    border: 1px solid #3a3b38;
    border-radius: 4px;
    font-size: 9px;
    color: #adafab;
    background: #282927;
  }
  .create-grid button :global(svg:first-child) {
    color: #c5c8c3;
  }
  .create-grid button :global(svg:last-child) {
    margin-left: auto;
    color: #767a73;
  }
  .create-grid button:hover {
    border-color: #797e75;
  }
  .motion-link {
    display: flex;
    align-items: center;
    gap: 9px;
    margin: 19px 14px 15px;
    padding: 12px 8px;
    border: 1px solid #40423f;
    border-radius: 4px;
    width: calc(100% - 28px);
    text-align: left;
  }
  .motion-link:hover {
    background: #2d2e2b;
  }
  .motion-icon {
    color: #aeb3ab;
  }
  .motion-link strong {
    display: block;
    font-size: 9px;
    font-weight: 500;
    margin-bottom: 5px;
  }
  .motion-link small {
    color: #848781;
    font-size: 8px;
  }
  .motion-link > :global(svg) {
    margin-left: auto;
    color: #80847c;
  }
  .sidebar-bottom {
    border-top: 1px solid var(--border);
    flex-shrink: 0;
  }
  .history-toggle {
    width: 100%;
    display: flex;
    gap: 7px;
    align-items: center;
    padding: 12px 15px;
    font-size: 10px;
    color: #9c9e9a;
  }
  .local-note {
    border-top: 1px solid #2e302d;
    padding: 10px 14px;
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 8px;
    color: #727470;
  }
  .local-note .mono {
    font-size: 8px;
  }
  .history-list {
    max-height: 150px;
    overflow-y: auto;
    padding: 0 14px 10px;
    font-size: 9px;
    color: #888c86;
  }
  .history-list > div {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-bottom: 9px;
  }
  .history-list .latest {
    color: var(--accent);
  }
  .history-point {
    height: 4px;
    width: 4px;
    border-radius: 50%;
    background: currentColor;
    flex-shrink: 0;
  }
  .library-intro {
    padding: 20px 15px 4px;
  }
  .library-intro p {
    line-height: 1.8;
    font-size: 12px;
    color: #8f938c;
    margin: 14px 0 0;
  }
  .library-intro strong {
    color: #cccfca;
    font-weight: 400;
  }
  .effect-list {
    padding: 0 10px;
  }
  .effect-item {
    width: 100%;
    display: flex;
    gap: 9px;
    align-items: center;
    text-align: left;
    padding: 9px 5px;
    font-size: 10px;
    color: #c3c5c2;
    border-radius: 4px;
  }
  .effect-item:hover {
    background: #373935;
  }
  .effect-icon {
    height: 26px;
    width: 26px;
    display: grid;
    place-items: center;
    background: #2f312e;
    border: 1px solid #4a4d49;
    color: #babdb7;
    border-radius: 4px;
  }
  .effect-item > :global(svg) {
    color: #767a73;
  }
  .library-note {
    padding: 22px 15px;
    display: flex;
    gap: 8px;
    color: #848881;
    font-size: 9px;
    line-height: 1.7;
  }
  .library-note :global(svg) {
    margin-top: 2px;
  }
  .search-empty {
    padding: 15px;
    color: var(--text-dim);
    font-size: 10px;
  }
  .preset-list {
    padding: 15px 14px;
    display: grid;
    gap: 12px;
  }
  .preset-card {
    border: 1px solid #474945;
    border-radius: 5px;
    overflow: hidden;
    text-align: left;
  }
  .preset-card:hover {
    border-color: #abafa7;
  }
  .preset-art {
    background: radial-gradient(ellipse at 10% 10%, #474b45, #282a27);
    height: 70px;
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 14px;
    color: #bbc0b7;
    position: relative;
  }
  .preset-art > span {
    position: absolute;
    bottom: 8px;
    right: 9px;
    font-size: 8px;
    color: #a9ada6;
  }
  .preset-copy {
    padding: 10px;
    position: relative;
  }
  .preset-copy strong {
    display: block;
    font-size: 10px;
    font-weight: 500;
    margin-bottom: 4px;
  }
  .preset-copy span {
    font-size: 8px;
    color: #8a8d87;
  }
  .preset-copy > :global(svg) {
    position: absolute;
    right: 10px;
    top: 13px;
    color: var(--accent);
  }
</style>
