# Chromatic Aberration (builtin.chromatic_aberration)

Separates RGB color channels along an angular vector, replicating optical prism dispersion, vintage lens fringing, and stylized glitch aesthetics.

## Parameters

| Param | Range | Default | What it does |
|---|---|---|---|
| Amount | 0-50 px | 5.0 | Pixel separation distance between color channels |
| Angle | -180 deg to 180 deg | 0.0 | Direction angle of the chromatic separation |

## Try this

Impact glitch or lens distortion: combine with an energetic camera shake track, keyframing Amount from 20 down to 0 over 6 frames on beat hits.

## Shader contract

Standard 5-binding contract (u_texture, u_sampler, u_resolution, u_time, params).
