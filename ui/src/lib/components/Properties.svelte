<script lang="ts">
  import Icon from "./Icon.svelte";
  import ColorField from "./ColorField.svelte";
  import EffectsPanel from "./EffectsPanel.svelte";
  import {
    editor,
    selectedLayer,
    activeComp,
    applyOp,
    setProperty,
    toggleKeyframeAtPlayhead,
    clone,
    selectComp,
    notify,
  } from "../store.svelte";
  import {
    evaluate,
    findKeyframeAtTime,
    timeToSecs,
    secsToTime,
    formatFps,
    BLEND_MODES,
    type Property,
    type LayerKind,
    type Op,
    type PropValue,
  } from "../model";
  import { layerIcon, layerColor, layerType } from "../geometry";
  const layer = $derived(selectedLayer());
  const comp = $derived(activeComp());
  let linkedScale = $state(true);
  const props: { key: Property; label: string; axes: string[]; step: number; unit?: string }[] = [
    { key: "Position", label: "Position", axes: ["X", "Y"], step: 1 },
    { key: "Scale", label: "Scale", axes: ["X", "Y"], step: 1, unit: "%" },
    { key: "Rotation", label: "Rotation", axes: [""], step: 1, unit: "°" },
    { key: "Opacity", label: "Opacity", axes: [""], step: 1, unit: "%" },
    { key: "AnchorPoint", label: "Anchor", axes: ["X", "Y"], step: 1 },
  ];
  function numbers(property: Property): number[] {
    if (!layer) return [0, 0];
    const value = evaluate(layer, property, editor.currentTime);
    return "Vec2" in value
      ? value.Vec2
      : [property === "Opacity" ? value.Scalar * 100 : value.Scalar];
  }
  async function changeNumber(property: Property, index: number, raw: string) {
    if (!layer) return;
    const values = numbers(property),
      next = Number(raw);
    if (!Number.isFinite(next)) return;
    if (property === "Scale" && linkedScale) {
      const other = index === 0 ? 1 : 0;
      values[other] = values[index] !== 0 ? (values[other] * next) / values[index] : next;
    }
    values[index] = next;
    const value: PropValue =
      values.length === 2
        ? { Vec2: [values[0], values[1]] }
        : {
            Scalar:
              property === "Opacity" ? Math.max(0, Math.min(100, values[0])) / 100 : values[0],
          };
    await setProperty(layer.id, property, value);
  }
  async function content(update: (kind: LayerKind) => void) {
    const id = layer?.id,
      cid = comp?.id;
    if (id === undefined || cid === undefined) return;
    await applyOp((project) => {
      const l = project.comps[String(cid)]?.layers[String(id)];
      if (!l || l.locked) return null;
      const kind = clone(l.kind);
      update(kind);
      return { type: "setLayerContent", comp: cid, layer: id, kind };
    });
  }
  async function resetTransform() {
    const l = layer,
      c = comp;
    if (!l || !c) return;
    const values: Record<Property, PropValue> = {
      Position: { Vec2: [0, 0] },
      Scale: { Vec2: [100, 100] },
      Rotation: { Scalar: 0 },
      Opacity: { Scalar: 1 },
      AnchorPoint: { Vec2: [0, 0] },
    };
    const ops: Op[] = [];
    for (const property of Object.keys(values) as Property[]) {
      for (const key of l.tracks[property]?.keys ?? [])
        ops.push({ type: "removeKeyframe", comp: c.id, layer: l.id, property, time: key.time });
      ops.push({ type: "setValue", comp: c.id, layer: l.id, property, value: values[property] });
    }
    if (ops.length > 256) {
      notify(
        "This layer has more than 251 transform keys. Reset is limited to one 256-operation transaction; no changes were made.",
        true,
      );
      return;
    }
    await applyOp({ type: "batch", label: `Reset ${l.name} transform and animation`, ops });
  }
  function timing(start: number, duration: number) {
    if (comp && layer)
      void applyOp({
        type: "setLayerTime",
        comp: comp.id,
        layer: layer.id,
        start: secsToTime(start),
        duration: secsToTime(duration),
      });
  }
</script>

<aside class="panel inspector" aria-label="Layer inspector">
  <div class="panel-heading">
    Inspector<span class="spacer"></span><Icon name="sliders" size={13} class="dim" />
  </div>
  {#if layer && comp}
    <div class="selected-header">
      <span class="layer-kind-icon" style={`color:${layerColor(layer)}`}
        ><Icon name={layerIcon(layer)} size={18} /></span
      >
      <div class="selected-name">
        <input
          aria-label="Layer name"
          value={layer.name}
          disabled={layer.locked}
          onchange={(e) =>
            void applyOp({
              type: "renameLayer",
              comp: comp.id,
              layer: layer.id,
              name: e.currentTarget.value,
            })}
        /><span>{layerType(layer)}</span>
      </div>
      <button
        class="icon-button small"
        title={layer.visible ? "Hide layer" : "Show layer"}
        aria-label="Toggle layer visibility"
        onclick={() =>
          void applyOp({
            type: "setLayerVisible",
            comp: comp.id,
            layer: layer.id,
            visible: !layer.visible,
          })}><Icon name={layer.visible ? "eye" : "eye-off"} size={14} /></button
      >
      <button
        class="icon-button small"
        class:active={layer.locked}
        title={layer.locked ? "Unlock layer" : "Lock layer"}
        aria-label="Toggle layer lock"
        onclick={() =>
          void applyOp({
            type: "setLayerLocked",
            comp: comp.id,
            layer: layer.id,
            locked: !layer.locked,
          })}><Icon name={layer.locked ? "lock" : "unlock"} size={12} /></button
      >
    </div>
    <div class="panel-tabs inspector-tabs">
      <button
        class:active={editor.inspector === "properties"}
        onclick={() => (editor.inspector = "properties")}>Properties</button
      ><button
        class:active={editor.inspector === "effects"}
        onclick={() => (editor.inspector = "effects")}
        >Effects<span class="count">{layer.effects.length}</span></button
      >
    </div>
    <div class="panel-scroll">
      {#if editor.inspector === "effects"}
        <EffectsPanel />
      {:else}
        {#if layer.locked}<div class="locked-note">
            <Icon name="lock" size={12} />Unlock this layer to make changes.
          </div>{/if}
        {#if "Text" in layer.kind}
          <section class="inspector-section">
            <div class="section-bar">
              <Icon name="down" size={10} /><span>Typography</span><span class="spacer"></span><span
                class="section-meta">TEXT</span
              >
            </div>
            <textarea
              class="field text-content"
              aria-label="Text content"
              value={layer.kind.Text.text}
              disabled={layer.locked}
              spellcheck={false}
              maxlength="16384"
              onchange={(e) => {
                const value = e.currentTarget.value;
                void content((k) => {
                  if ("Text" in k) k.Text.text = value;
                });
              }}></textarea>
            <div class="font-row">
              <span class="font-family">DejaVu Sans<Icon name="down" size={10} /></span><button
                class="weight"
                class:active={layer.kind.Text.style.bold}
                disabled={layer.locked}
                title="Toggle bold"
                aria-label="Bold text"
                onclick={() =>
                  void content((k) => {
                    if ("Text" in k) k.Text.style.bold = !k.Text.style.bold;
                  })}>B</button
              >
            </div>
            <div class="form-row">
              <label for="font-size">Size</label>
              <div class="number-field">
                <input
                  id="font-size"
                  type="number"
                  value={layer.kind.Text.size}
                  min="1"
                  max="2048"
                  disabled={layer.locked}
                  onchange={(e) => {
                    const v = Number(e.currentTarget.value);
                    void content((k) => {
                      if ("Text" in k) k.Text.size = v;
                    });
                  }}
                /><span>px</span>
              </div>
              <label for="tracking">Tracking</label>
              <div class="number-field">
                <input
                  id="tracking"
                  type="number"
                  step=".5"
                  value={layer.kind.Text.style.tracking}
                  disabled={layer.locked}
                  onchange={(e) => {
                    const v = Number(e.currentTarget.value);
                    void content((k) => {
                      if ("Text" in k) k.Text.style.tracking = v;
                    });
                  }}
                />
              </div>
            </div>
            <div class="form-row">
              <span class="control-label">Text fill</span><ColorField
                label="Text fill"
                value={layer.kind.Text.style.color}
                disabled={layer.locked}
                onchange={(color) =>
                  void content((k) => {
                    if ("Text" in k) k.Text.style.color = color;
                  })}
              />
            </div>
          </section>
        {:else if "Shape" in layer.kind}
          <section class="inspector-section">
            <div class="section-bar">
              <Icon name="down" size={10} /><span>Shape</span><span class="spacer"></span><span
                class="section-meta">VECTOR</span
              >
            </div>
            <div class="form-row">
              <label for="shape-geometry">Geometry</label><select
                id="shape-geometry"
                class="field geometry-select"
                value={layer.kind.Shape.generator ? "circle" : "rectangle"}
                disabled={layer.locked}
                onchange={(e) => {
                  const v = e.currentTarget.value;
                  void content((k) => {
                    if ("Shape" in k) k.Shape.generator = v === "circle" ? "builtin.circle" : null;
                  });
                }}
                ><option value="rectangle">Rectangle</option><option value="circle">Ellipse</option
                ></select
              >
            </div>
            <div class="form-row">
              <span class="control-label">Size</span>{#each [0, 1] as axis}<div
                  class="number-field"
                >
                  <span>{axis === 0 ? "W" : "H"}</span><input
                    aria-label={`Shape ${axis === 0 ? "width" : "height"}`}
                    type="number"
                    min="1"
                    max="8192"
                    disabled={layer.locked}
                    value={(layer.kind.Shape.style.size ?? [comp.width, comp.height])[axis]}
                    onchange={(e) => {
                      const v = Number(e.currentTarget.value);
                      void content((k) => {
                        if ("Shape" in k) {
                          k.Shape.style.size ??= [comp.width, comp.height];
                          k.Shape.style.size[axis] = v;
                        }
                      });
                    }}
                  />
                </div>{/each}
            </div>
            <div class="form-row">
              <span class="control-label">Fill</span><ColorField
                label="Shape fill"
                value={layer.kind.Shape.color}
                disabled={layer.locked}
                onchange={(color) =>
                  void content((k) => {
                    if ("Shape" in k) k.Shape.color = color;
                  })}
              />
            </div>
            {#if !layer.kind.Shape.generator}<div class="form-row">
                <label for="corner-radius">Corner radius</label>
                <div class="number-field">
                  <input
                    id="corner-radius"
                    type="number"
                    min="0"
                    value={layer.kind.Shape.style.corner_radius}
                    disabled={layer.locked}
                    onchange={(e) => {
                      const v = Number(e.currentTarget.value);
                      void content((k) => {
                        if ("Shape" in k) k.Shape.style.corner_radius = v;
                      });
                    }}
                  /><span>px</span>
                </div>
              </div>{/if}
            <div class="form-row">
              <label for="stroke-width">Stroke</label>
              <div class="number-field">
                <input
                  id="stroke-width"
                  type="number"
                  min="0"
                  max="100"
                  value={layer.kind.Shape.style.stroke_width}
                  disabled={layer.locked}
                  onchange={(e) => {
                    const v = Number(e.currentTarget.value);
                    void content((k) => {
                      if ("Shape" in k) k.Shape.style.stroke_width = v;
                    });
                  }}
                /><span>px</span>
              </div>
            </div>
            {#if layer.kind.Shape.style.stroke_width > 0}<div class="form-row">
                <span class="control-label">Stroke color</span><ColorField
                  label="Stroke"
                  value={layer.kind.Shape.style.stroke_color}
                  disabled={layer.locked}
                  onchange={(color) =>
                    void content((k) => {
                      if ("Shape" in k) k.Shape.style.stroke_color = color;
                    })}
                />
              </div>{/if}
          </section>
        {:else if "Solid" in layer.kind}
          <section class="inspector-section">
            <div class="section-bar"><Icon name="down" size={10} />Solid color</div>
            <div class="form-row">
              <span class="control-label">Fill</span><ColorField
                label="Solid fill"
                value={layer.kind.Solid.color}
                disabled={layer.locked}
                onchange={(color) =>
                  void content((k) => {
                    if ("Solid" in k) k.Solid.color = color;
                  })}
              />
            </div>
          </section>
        {:else if "PreComp" in layer.kind}
          <section class="inspector-section">
            <div class="section-bar"><Icon name="film" size={12} />Nested composition</div>
            <button
              class="nested-comp"
              onclick={() => {
                if (layer && "PreComp" in layer.kind) selectComp(layer.kind.PreComp.comp);
              }}
              ><span
                >{editor.project?.comps[String(layer.kind.PreComp.comp)]?.name ??
                  "Composition"}</span
              ><Icon name="right" size={13} /></button
            >
            <p class="hint">Open the source composition to edit its layers.</p>
          </section>
        {:else if "Adjustment" in layer.kind}
          <section class="inspector-section adjustment-note">
            <Icon name="adjust" size={24} />
            <p>
              One layer. A whole new look.<small
                >Effects on this layer process everything below it. Opacity controls their strength.</small
              >
            </p>
            <button class="btn full" onclick={() => (editor.inspector = "effects")}
              ><Icon name="sliders" size={13} />Open effect controls</button
            >
          </section>
        {/if}
        <section class="inspector-section">
          <div class="section-bar">
            <Icon name="down" size={10} /><span>Transform</span><span class="spacer"></span><button
              class="icon-button small"
              disabled={layer.locked}
              title="Reset transform and remove its keyframes"
              aria-label="Reset transform"
              onclick={() => void resetTransform()}><Icon name="rotate" size={12} /></button
            >
          </div>
          {#each props as prop (prop.key)}
            {@const values = numbers(prop.key)}
            <div
              class="transform-row"
              title={prop.key === "Position"
                ? "Pixels relative to composition center (or parent origin)"
                : prop.label}
            >
              <button
                class="key-button"
                class:animated={!!layer.tracks[prop.key]?.keys.length}
                class:keyed={!!findKeyframeAtTime(
                  layer.tracks[prop.key],
                  editor.currentTime,
                  comp.fps,
                )}
                disabled={layer.locked}
                aria-label={`Keyframe ${prop.label}`}
                title="Toggle a keyframe at the playhead"
                onclick={() => void toggleKeyframeAtPlayhead(layer.id, prop.key)}
                ><Icon name="keyframe" size={10} /></button
              >
              <span class="property-label">{prop.label}</span>
              <div class="property-inputs">
                {#each prop.axes as axis, index}<div class="number-field">
                    <span>{axis}</span><input
                      type="number"
                      aria-label={`${prop.label}${axis ? " " + axis : ""}`}
                      value={Number((values[index] ?? 0).toFixed(2))}
                      step={prop.step}
                      disabled={layer.locked}
                      onchange={(e) => void changeNumber(prop.key, index, e.currentTarget.value)}
                    />{#if prop.unit}<span>{prop.unit}</span>{/if}
                  </div>{/each}
              </div>
              {#if prop.key === "Scale"}<button
                  class="link-scale"
                  class:active={linkedScale}
                  aria-label="Link scale dimensions"
                  title={linkedScale ? "Unlink scale dimensions" : "Link scale dimensions"}
                  onclick={() => (linkedScale = !linkedScale)}
                  ><Icon name="link" size={10} /></button
                >{:else}<span class="link-space"></span>{/if}
            </div>
          {/each}
        </section>
        <section class="inspector-section">
          <div class="section-bar"><Icon name="down" size={10} /><span>Compositing</span></div>
          {#if !("Adjustment" in layer.kind)}<div class="form-row">
              <label for="blend-mode">Blend mode</label><select
                id="blend-mode"
                class="field geometry-select"
                value={layer.blend_mode}
                disabled={layer.locked}
                onchange={(e) =>
                  void applyOp({
                    type: "setLayerBlendMode",
                    comp: comp.id,
                    layer: layer.id,
                    blendMode: e.currentTarget.value as typeof layer.blend_mode,
                  })}
                >{#each BLEND_MODES as mode}<option>{mode}</option>{/each}</select
              >
            </div>{/if}
          <div class="form-row">
            <label for="parent-layer">Parent</label><select
              id="parent-layer"
              class="field geometry-select"
              value={layer.parent ?? ""}
              disabled={layer.locked}
              onchange={(e) =>
                void applyOp({
                  type: "setLayerParent",
                  comp: comp.id,
                  layer: layer.id,
                  parent: e.currentTarget.value === "" ? null : Number(e.currentTarget.value),
                })}
              ><option value="">None</option
              >{#each Object.values(comp.layers).filter((l) => l.id !== layer.id) as parent}<option
                  value={parent.id}>{parent.name}</option
                >{/each}</select
            >
          </div>
        </section>
        <section class="inspector-section">
          <div class="section-bar">
            <Icon name="down" size={10} /><span>Timing</span><span class="spacer"></span><span
              class="section-meta">SECONDS</span
            >
          </div>
          <div class="form-row">
            <label for="layer-start">In point</label>
            <div class="number-field">
              <input
                id="layer-start"
                type="number"
                step=".1"
                value={timeToSecs(layer.start)}
                disabled={layer.locked}
                onchange={(e) => timing(Number(e.currentTarget.value), timeToSecs(layer.duration))}
              /><span>s</span>
            </div>
            <label for="layer-duration">Duration</label>
            <div class="number-field">
              <input
                id="layer-duration"
                type="number"
                min="0"
                step=".1"
                value={timeToSecs(layer.duration)}
                disabled={layer.locked}
                onchange={(e) => timing(timeToSecs(layer.start), Number(e.currentTarget.value))}
              /><span>s</span>
            </div>
          </div>
        </section>
        <p class="inspector-footnote">
          <Icon name="keyframe" size={10} />Diamonds turn values into motion.
        </p>
      {/if}
    </div>
  {:else}
    <div class="panel-scroll">
      {#if editor.workspace === "Color"}<EffectsPanel />{:else if comp}
        <div class="comp-summary">
          <span class="large-comp-icon"><Icon name="film" size={26} /></span><span class="upper dim"
            >Composition</span
          >
          <h2>{comp.name}</h2>
          <div class="summary-grid">
            <span>Dimensions</span><strong class="mono">{comp.width} × {comp.height}</strong><span
              >Frame rate</span
            ><strong class="mono">{formatFps(comp.fps)}</strong><span>Duration</span><strong
              class="mono">{timeToSecs(comp.duration)} seconds</strong
            ><span>Layers</span><strong class="mono">{comp.layer_order.length}</strong>
          </div>
          <button
            class="btn full"
            onclick={() => (editor.dialog = { kind: "composition", compId: comp.id })}
            ><Icon name="settings" size={13} />Composition settings</button
          >
        </div>
        <div class="selection-hint">
          <Icon name="pointer" size={22} /><strong>Make it yours</strong>
          <p>
            Select a layer on the canvas or timeline to edit its properties, effects, and animation.
          </p>
        </div>
      {:else}<div class="empty">Create a composition to get started.</div>{/if}
    </div>
  {/if}
</aside>

<style>
  .inspector {
    border-left: 1px solid var(--border);
  }
  .selected-header {
    padding: 14px 12px;
    display: flex;
    align-items: center;
    gap: 7px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .layer-kind-icon {
    width: 31px;
    height: 34px;
    display: grid;
    place-items: center;
    background: #2a2b2a;
    border: 1px solid var(--border);
    border-radius: 4px;
  }
  .selected-name {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
    flex: 1;
  }
  .selected-name input {
    width: 100%;
    background: none;
    border: 0;
    outline: 0;
    font-size: 11px;
    font-weight: 500;
    text-overflow: ellipsis;
  }
  .selected-name input:focus {
    background: #373935;
  }
  .selected-name > span {
    font-size: 8px;
    color: var(--text-muted);
  }
  .inspector-tabs {
    height: 37px;
    gap: 22px;
    font-size: 10px;
  }
  .inspector-tabs button {
    font-size: 10px;
  }
  .inspector-section {
    padding: 0 14px 13px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .section-bar {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 10px;
    font-weight: 500;
    height: 41px;
    color: #c4c5c3;
  }
  .section-meta {
    color: #757773;
    font: 7px monospace;
    letter-spacing: 1px;
  }
  .text-content {
    font-size: 11px;
    min-height: 63px;
    background: #1b1c1b;
    padding: 8px;
    line-height: 1.5;
  }
  .font-row {
    display: flex;
    gap: 5px;
    margin-top: 9px;
  }
  .font-family {
    display: flex;
    align-items: center;
    flex: 1;
    justify-content: space-between;
    background: #1d1e1d;
    border: 1px solid var(--border);
    padding: 6px 8px;
    color: #a8aaa6;
    font-size: 10px;
    border-radius: 3px;
  }
  .font-family :global(svg) {
    opacity: 0.3;
  }
  .weight {
    border: 1px solid var(--border);
    width: 28px;
    background: #2b2c2a;
    border-radius: 3px;
    font-size: 12px;
    font-weight: 700;
    color: #90938e;
  }
  .weight.active {
    background: #40433f;
    color: var(--accent);
    border-color: #666a62;
  }
  .form-row > .number-field {
    max-width: 100px;
  }
  .geometry-select {
    width: 157px;
    flex: none;
    font-size: 10px !important;
  }
  .form-row > label,
  .form-row > .control-label {
    font-size: 9px;
  }
  .transform-row {
    display: flex;
    align-items: center;
    gap: 3px;
    margin: 8px -7px 8px -5px;
  }
  .property-label {
    font-size: 9px;
    color: #989b97;
    width: 48px;
    flex-shrink: 0;
  }
  .property-inputs {
    display: flex;
    gap: 5px;
    flex: 1;
    min-width: 0;
  }
  .transform-row .key-button {
    width: 16px;
    flex-shrink: 0;
  }
  .property-inputs .number-field {
    padding: 6px 5px;
  }
  .link-scale,
  .link-space {
    width: 15px;
    flex-shrink: 0;
    color: #6b6e69;
    display: grid;
    place-items: center;
  }
  .link-scale.active {
    color: #c5c8c1;
  }
  .inspector-footnote {
    display: flex;
    gap: 7px;
    align-items: center;
    justify-content: center;
    margin: 18px 0;
    color: #727670;
    font-size: 8px;
  }
  .nested-comp {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    padding: 9px;
    border: 1px solid #5a575d;
    background: #302f32;
    color: #c1bec4;
    border-radius: 3px;
    font-size: 10px;
  }
  .hint {
    font-size: 8px;
    line-height: 1.6;
    color: #848782;
    margin: 9px 0 0;
  }
  .adjustment-note {
    padding-top: 16px;
  }
  .adjustment-note > :global(svg) {
    color: #b4b0a7;
  }
  .adjustment-note p {
    font-size: 11px;
    color: #c9cac7;
    line-height: 1.8;
    margin: 9px 0 14px;
  }
  .adjustment-note small {
    display: block;
    font-size: 9px;
    color: #8e918c;
  }
  .locked-note {
    display: flex;
    gap: 7px;
    align-items: center;
    color: #b5b1a8;
    font-size: 9px;
    padding: 12px 14px;
    background: #57554d18;
  }
  .comp-summary {
    padding: 22px 16px;
  }
  .large-comp-icon {
    width: 47px;
    height: 50px;
    display: grid;
    place-items: center;
    background: #343336;
    color: #b6b1ba;
    border: 1px solid #545056;
    border-radius: 5px;
    margin-bottom: 19px;
  }
  .comp-summary h2 {
    font-size: 17px;
    font-weight: 500;
    margin: 7px 0 21px;
    color: #d7d8d5;
    letter-spacing: -0.3px;
  }
  .summary-grid {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 15px 8px;
    margin-bottom: 24px;
    font-size: 9px;
    color: #92958f;
  }
  .summary-grid strong {
    font-size: 9px;
    color: #b9bcb7;
    font-weight: 400;
  }
  .selection-hint {
    padding: 23px 29px;
    border-top: 1px solid var(--border-subtle);
    text-align: center;
    color: #737770;
  }
  .selection-hint > :global(svg) {
    margin: 0 auto 12px;
  }
  .selection-hint strong {
    display: block;
    font-size: 11px;
    font-weight: 500;
    color: #bdc0ba;
  }
  .selection-hint p {
    font-size: 10px;
    line-height: 1.9;
    margin: 10px 0;
  }
</style>
