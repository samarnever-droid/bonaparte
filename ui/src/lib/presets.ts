/// First-party animation recipes. Timing is relative to the playhead, and
/// scale/position recipes adapt to the layer's values instead of fixed coordinates.
import { activeComp, selectedLayer, editor, applyOp, notify } from "./store.svelte";
import {
  evaluate,
  snapToFrame,
  TICKS_PER_SEC,
  type Property,
  type PropValue,
  type Op,
  type Easing,
} from "./model";
interface Recipe {
  property: Property;
  mode: "absolute" | "offset" | "multiply";
  keys: [number, number | [number, number]][];
}
export const PRESETS: {
  id: string;
  name: string;
  description: string;
  icon: string;
  duration: number;
  easing: Easing;
  tracks: Recipe[];
}[] = [
  {
    id: "fade",
    name: "Fade in",
    description: "A quiet entrance",
    icon: "adjust",
    duration: 0.8,
    easing: "Linear",
    tracks: [
      {
        property: "Opacity",
        mode: "absolute",
        keys: [
          [0, 0],
          [1, 1],
        ],
      },
    ],
  },
  {
    id: "rise",
    name: "Rise & reveal",
    description: "A smooth upward entrance",
    icon: "up",
    duration: 1,
    easing: { Bezier: { p1: [0.16, 1], p2: [0.3, 1] } },
    tracks: [
      {
        property: "Position",
        mode: "offset",
        keys: [
          [0, [0, 80]],
          [1, [0, 0]],
        ],
      },
      {
        property: "Opacity",
        mode: "absolute",
        keys: [
          [0, 0],
          [0.7, 1],
        ],
      },
    ],
  },
  {
    id: "pop",
    name: "Pop in",
    description: "A little overshoot",
    icon: "maximize",
    duration: 0.8,
    easing: { Bezier: { p1: [0.2, 1.4], p2: [0.4, 1] } },
    tracks: [
      {
        property: "Scale",
        mode: "multiply",
        keys: [
          [0, [0.45, 0.45]],
          [1, [1, 1]],
        ],
      },
      {
        property: "Opacity",
        mode: "absolute",
        keys: [
          [0, 0],
          [0.3, 1],
        ],
      },
    ],
  },
  {
    id: "spin",
    name: "Slow rotation",
    description: "One full, linear revolution",
    icon: "rotate",
    duration: 4,
    easing: "Linear",
    tracks: [
      {
        property: "Rotation",
        mode: "offset",
        keys: [
          [0, 0],
          [1, 360],
        ],
      },
    ],
  },
];
export async function applyPreset(id: string) {
  const comp = activeComp(),
    layer = selectedLayer(),
    preset = PRESETS.find((p) => p.id === id);
  if (!comp || !layer || !preset) {
    notify("Select a layer to apply an animation.");
    return;
  }
  if (layer.locked) {
    notify("Unlock this layer before animating it.", true);
    return;
  }
  const start = editor.currentTime;
  const duration = Math.min(preset.duration * TICKS_PER_SEC, comp.duration - start);
  if (duration < (comp.fps.den * TICKS_PER_SEC) / comp.fps.num) {
    notify("Move the playhead earlier to leave room for animation.", true);
    return;
  }
  const ops: Op[] = [];
  for (const recipe of preset.tracks) {
    const base = evaluate(layer, recipe.property, start);
    for (const [fraction, raw] of recipe.keys) {
      let value: PropValue;
      if (Array.isArray(raw) && "Vec2" in base)
        value = {
          Vec2: raw.map((v, i) =>
            recipe.mode === "offset"
              ? base.Vec2[i] + v
              : recipe.mode === "multiply"
                ? base.Vec2[i] * v
                : v,
          ) as [number, number],
        };
      else if (typeof raw === "number" && "Scalar" in base)
        value = {
          Scalar:
            recipe.mode === "offset"
              ? base.Scalar + raw
              : recipe.mode === "multiply"
                ? base.Scalar * raw
                : raw,
        };
      else continue;
      ops.push({
        type: "addKeyframe",
        comp: comp.id,
        layer: layer.id,
        property: recipe.property,
        key: {
          time: snapToFrame(start + fraction * duration, comp.fps),
          value,
          easing: preset.easing,
        },
      });
    }
  }
  if (await applyOp({ type: "batch", label: `Applied ${preset.name} to ${layer.name}`, ops }))
    notify(`${preset.name} applied at the playhead. One undo restores the previous animation.`);
}
