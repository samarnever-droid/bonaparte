# Glow (builtin.glow)

A soft luminous halo around bright areas of the layer beneath it.

## Parameters

| Param | Range | Default | What it does |
|---|---|---|---|
| Radius | 0–100 px | 12 | Size of the glow halo |
| Intensity | 0–4 | 1 | Halo brightness relative to source |
| Tint | RGBA | white | Multiplier applied to the halo |

## Try this

Neon text in 30 seconds: white text layer → add Glow → Radius 18,
Intensity 1.5 → keyframe Intensity 0 → 1.5 over 0.5s with the default
ease-in-out. Neon ignition.

## Shader contract

Standard contract (`u_texture`, `u_resolution`, `u_time` auto-injected;
params arrive in manifest order). See `glow.wgsl` header for details.
Multi-pass Gaussian version pending the GPU backend; manifest and params
are final and will not change.
