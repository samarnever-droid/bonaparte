# Drop Shadow (builtin.drop_shadow)

Renders a soft or crisp shadow beneath the alpha contours of a layer.

## Parameters

| Param | Range | Default | What it does |
|---|---|---|---|
| Offset X | -200–200 px | 10 | Horizontal shadow offset |
| Offset Y | -200–200 px | 10 | Vertical shadow offset |
| Radius | 0–100 px | 15 | Blur radius of the shadow |
| Color | RGBA | [0, 0, 0, 0.75] | Base shadow color |
| Opacity | 0–1 | 0.75 | Overall shadow opacity multiplier |

## Try this

Card lift effect: animate Offset Y from 2 to 20 and Radius from 5 to 30 on hover to give elements realistic tactile elevation.

## Shader contract

Standard 5-binding contract (`u_texture`, `u_sampler`, `u_resolution`, `u_time`, `params`).
