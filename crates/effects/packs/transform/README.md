# Transform Effect (builtin.transform)

Post-processing 2D spatial translation, scale, and rotation within an effect stack.

## Parameters

| Param | Range | Default | What it does |
|---|---|---|---|
| Offset X | -1000–1000 px | 0.0 | Horizontal pixel shift |
| Offset Y | -1000–1000 px | 0.0 | Vertical pixel shift |
| Scale X | 0.01–10.0 | 1.0 | Horizontal scale multiplier |
| Scale Y | 0.01–10.0 | 1.0 | Vertical scale multiplier |
| Rotation | -360–360° | 0.0 | Rotation angle in degrees |

## Try this

Whip pan glitch: keyframe Offset X from -500 to 0 with high velocity easing to simulate fast camera whip-in.

## Shader contract

Standard 5-binding contract (`u_texture`, `u_sampler`, `u_resolution`, `u_time`, `params`).
