# Directional Blur (builtin.directional_blur)

Simulates linear camera or subject motion blur along an arbitrary angle vector.

## Parameters

| Param | Range | Default | What it does |
|---|---|---|---|
| Length | 0-200 px | 20.0 | Blur streak length in pixels |
| Angle | -180 deg to 180 deg | 0.0 | Angle direction of the motion blur |

## Try this

Whip pan or velocity streaks: pair with rapid position translation, aligning Angle to the movement direction and linking Length to layer velocity.

## Shader contract

Standard 5-binding contract (u_texture, u_sampler, u_resolution, u_time, params).
