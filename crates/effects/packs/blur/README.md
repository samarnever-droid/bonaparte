# Gaussian Blur (builtin.blur)

Smoothly softens the image by convolving pixels with a Gaussian distribution.

## Parameters

| Param | Range | Default | What it does |
|---|---|---|---|
| Radius | 0–100 px | 10 | Blur radius in pixels |
| Repeat Edge | boolean | true | Clamps UV sampling to edge to prevent border darkening |

## Try this

Cinematic depth of field: duplicate layer → apply Gaussian Blur with Radius 25 → animate mask or opacity to simulate camera defocus.

## Shader contract

Standard 5-binding contract (`u_texture`, `u_sampler`, `u_resolution`, `u_time`, `params`).
