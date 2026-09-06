# Circle Shape Generator (`builtin.circle`)

First-party vector circle generator plugin for the Bonaparte microkernel compositor.

## Overview
- **Category**: Generator Plugin (`inputs = []`)
- **GPU Cost**: Light
- **Mathematical Model**: Analytical Signed Distance Function with 2px antialiased subpixel edge coverage and inner stroke blending.

## Parameters
- `color`: RGBA fill color
- `radius`: Normalized radius (0.0 to 1.0)
- `stroke_color`: RGBA stroke outline color
- `stroke_width`: Stroke thickness in pixels
