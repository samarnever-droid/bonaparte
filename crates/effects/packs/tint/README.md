# Tint (builtin.tint)

Remaps dark and light luminance values to two customizable colors, producing classic duotone, sepia, or cyber-aesthetic styling.

## Parameters

| Param | Range | Default | What it does |
|---|---|---|---|
| Map Black To | Color RGBA | [0, 0, 0, 1] | Shadow luminance color |
| Map White To | Color RGBA | [1, 1, 1, 1] | Highlight luminance color |
| Amount | 0.0-1.0 | 1.0 | Blend intensity between original and tinted image |

## Try this

Spotify duotone look: set Black to deep navy (#051923) and White to vibrant neon green (#00FF66), then keyframe Amount from 0.0 to 1.0 for a graphic reveal.

## Shader contract

Standard 5-binding contract (u_texture, u_sampler, u_resolution, u_time, params).
