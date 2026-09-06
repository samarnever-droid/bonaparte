# Vignette (builtin.vignette)

Adds optical lens edge falloff, drawing the eye toward the center of the frame.

## Parameters

| Param | Range | Default | What it does |
|---|---|---|---|
| Radius | 0.0–1.5 | 0.75 | Inner radius of unaffected center area |
| Softness | 0.01–1.0 | 0.45 | Gradient falloff transition width |
| Intensity | 0.0–2.0 | 0.8 | Maximum darkening strength at borders |
| Color | RGBA | [0, 0, 0, 1] | Falloff color (black for classic lens shading, white for dream effect) |

## Try this

Vintage film look: combine Vignette (radius 0.6, softness 0.5) with Color Adjust (saturation 0.85, contrast 1.15).

## Shader contract

Standard 5-binding contract (`u_texture`, `u_sampler`, `u_resolution`, `u_time`, `params`).
