# Invert (builtin.invert)

Inverts the red, green, and blue color channels (creating a photographic negative), with optional alpha channel inversion.

## Parameters

| Param | Range | Default | What it does |
|---|---|---|---|
| Amount | 0.0-1.0 | 1.0 | Inversion blend amount (0 = normal, 1 = inverted) |
| Invert Alpha | boolean | false | Invert transparency channel alongside RGB |

## Try this

Flash transition: keyframe Amount to 1.0 for a single frame or 2 frames on heavy impact moments to produce a punchy strobe effect.

## Shader contract

Standard 5-binding contract (u_texture, u_sampler, u_resolution, u_time, params).
