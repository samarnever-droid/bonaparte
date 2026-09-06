# Color Adjust (builtin.color_adjust)

Adjust brightness, contrast, saturation, and hue shift on any layer or footage item.

## Parameters

| Param | Range | Default | What it does |
|---|---|---|---|
| Brightness | -1.0–1.0 | 0.0 | Linear luminance offset |
| Contrast | 0.0–3.0 | 1.0 | Contrast scaling around mid-gray (0.5) |
| Saturation | 0.0–3.0 | 1.0 | Colorfulness multiplier (0.0 = mono, 1.0 = normal) |
| Hue Shift | -180.0–180.0° | 0.0 | Color wheel rotation |

## Try this

Cyberpunk color grading: boost Contrast to 1.35, Saturation to 1.5, and rotate Hue Shift by -25° for stylized neon tones.

## Shader contract

Standard 5-binding contract (`u_texture`, `u_sampler`, `u_resolution`, `u_time`, `params`).
