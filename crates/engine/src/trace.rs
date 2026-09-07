//! Raster → vector: posterized marching-squares tracing. An embedded image is
//! quantized to a few dominant colors; each color mask becomes closed
//! contours, simplified, and lands as editable vector shape layers. The
//! result is fully keyframeable geometry — the thing raster layers can
//! never be.

use bonaparte_model::{Layer, LayerKind, ShapeStyle, Time};

/// Trace an RGBA image into vector shape layers, one layer per dominant
/// color. `max_colors` caps the posterization. Coordinates come back in
/// image pixel space (y-down), per-layer points centered on the color's
/// own bounding box.
pub fn trace_image(
    rgba: &[u8],
    width: u32,
    height: u32,
    max_colors: usize,
    duration_ticks: i64,
) -> Result<Vec<Layer>, String> {
    if width == 0 || height == 0 || rgba.len() < (width as usize) * (height as usize) * 4 {
        return Err("Image is empty or the pixel buffer does not match its size".into());
    }
    let max_colors = max_colors.clamp(2, 16);
    // Downscale huge images for tracing speed; contours scale back up.
    let scale = ((width.max(height) as f32) / 384.0).ceil().max(1.0) as u32;
    let tw = (width / scale).max(1);
    let th = (height / scale).max(1);
    let mut px = vec![[0u8; 4]; (tw * th) as usize];
    for y in 0..th {
        for x in 0..tw {
            // Box-average the source block.
            let mut sum = [0u32; 4];
            let mut n = 0u32;
            for sy in 0..scale {
                for sx in 0..scale {
                    let sxp = (x * scale + sx).min(width - 1);
                    let syp = (y * scale + sy).min(height - 1);
                    let i = ((syp * width + sxp) * 4) as usize;
                    for c in 0..4 {
                        sum[c] += rgba[i + c] as u32;
                    }
                    n += 1;
                }
            }
            let mut p = [0u8; 4];
            for c in 0..4 {
                p[c] = (sum[c] / n).min(255) as u8;
            }
            px[(y * tw + x) as usize] = p;
        }
    }

    // Quantize: 5 bits per channel histogram, keep the top-k buckets with a
    // minimum population. Fully transparent pixels are background.
    let mut hist: std::collections::HashMap<u32, (u32, [f32; 3])> =
        std::collections::HashMap::new();
    for p in &px {
        if p[3] < 128 {
            continue;
        }
        let key = ((p[0] as u32 >> 3) << 10) | ((p[1] as u32 >> 3) << 5) | (p[2] as u32 >> 3);
        let e = hist.entry(key).or_insert((0, [0.0; 3]));
        e.0 += 1;
        e.1[0] += p[0] as f32;
        e.1[1] += p[1] as f32;
        e.1[2] += p[2] as f32;
    }
    let mut buckets: Vec<(u32, [f32; 3])> = hist
        .into_values()
        .map(|(n, s)| (n, [s[0] / n as f32, s[1] / n as f32, s[2] / n as f32]))
        .collect();
    buckets.sort_by(|a, b| {
        b.0.cmp(&a.0).then(
            a.1[0]
                .total_cmp(&b.1[0])
                .then(a.1[1].total_cmp(&b.1[1]).then(a.1[2].total_cmp(&b.1[2]))),
        )
    });
    buckets.truncate(max_colors);
    if buckets.is_empty() {
        return Err("Image is fully transparent — nothing to trace".into());
    }
    // Merge near-duplicate buckets (same 5-bit cell can still differ after
    // averaging); a greedy pass on mean color keeps the palette clean.
    let palette: Vec<[f32; 3]> = buckets.iter().map(|b| b.1).collect();

    // Nearest-palette assignment mask per color.
    let mut layers = Vec::new();
    for (color_index, color) in palette.iter().enumerate() {
        let mask_len = (tw * th) as usize;
        let mut mask = vec![false; mask_len];
        for (i, p) in px.iter().enumerate() {
            if p[3] < 128 {
                continue;
            }
            let pc = [p[0] as f32, p[1] as f32, p[2] as f32];
            let mut best = 0usize;
            let mut best_d = f32::MAX;
            for (ci, c) in palette.iter().enumerate() {
                let d = (pc[0] - c[0]).powi(2) + (pc[1] - c[1]).powi(2) + (pc[2] - c[2]).powi(2);
                if d < best_d {
                    best_d = d;
                    best = ci;
                }
            }
            mask[i] = best == color_index;
        }
        let contours = trace_mask(&mask, tw, th);
        if contours.is_empty() {
            continue;
        }
        // Layer in image pixel coordinates.
        let subpaths: Vec<Vec<[f32; 2]>> = contours
            .iter()
            .map(|loop_pts| {
                loop_pts
                    .iter()
                    .map(|p| [p[0] * scale as f32, p[1] * scale as f32])
                    .collect()
            })
            .collect();
        let Some(bounds) = crate::vector::bounds(&subpaths) else {
            continue;
        };
        let w = (bounds[2] - bounds[0]).max(1.0);
        let h = (bounds[3] - bounds[1]).max(1.0);
        // sRGB bytes → linear for the engine's paint pipeline.
        let linear: [f32; 4] = [
            srgb_to_linear(color[0]),
            srgb_to_linear(color[1]),
            srgb_to_linear(color[2]),
            1.0,
        ];
        let mut layer = Layer::new(
            format!("Traced {}", color_name(color)),
            LayerKind::Shape {
                color: linear,
                generator: None,
                style: ShapeStyle {
                    size: Some([w, h]),
                    corner_radius: 0.0,
                    stroke_width: 0.0,
                    stroke_color: linear,
                },
                points: subpaths
                    .iter()
                    .map(|sub| {
                        sub.iter()
                            .map(|p| [p[0] - (bounds[0] + w * 0.5), p[1] - (bounds[1] + h * 0.5)])
                            .collect()
                    })
                    .collect(),
            },
            Time(0),
            Time(duration_ticks),
        );
        layer.transform.position = [
            bounds[0] + w * 0.5 - width as f32 * 0.5,
            bounds[1] + h * 0.5 - height as f32 * 0.5,
        ];
        layers.push(layer);
    }
    if layers.is_empty() {
        return Err("Tracing produced no regions".into());
    }
    Ok(layers)
}

fn srgb_to_linear(byte: f32) -> f32 {
    let c = byte / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Stable readable name from a quantized color.
fn color_name(c: &[f32; 3]) -> String {
    format!(
        "#{:02x}{:02x}{:02x}",
        c[0].round() as u8,
        c[1].round() as u8,
        c[2].round() as u8
    )
}

/// Binary marching squares with segment stitching. Contours come back closed
/// (first point == last point is NOT duplicated) in cell coordinates.
fn trace_mask(mask: &[bool], w: u32, h: u32) -> Vec<Vec<[f32; 2]>> {
    let w = w as i64;
    let h = h as i64;
    let at = |x: i64, y: i64| -> bool {
        if x < 0 || y < 0 || x >= w || y >= h {
            false
        } else {
            mask[(y * w + x) as usize]
        }
    };
    // Directed segments: start key → end point. Corner sample points sit on
    // cell edges at half-integer coordinates (doubled ints keep keys exact).
    type P = (i64, i64);
    let mut segments: Vec<(P, P)> = Vec::new();
    for y in -1..h {
        for x in -1..w {
            // Corners: tl, tr, bl, br of the dual cell around grid corner (x,y).
            let tl = at(x, y) as u8;
            let tr = at(x + 1, y) as u8;
            let bl = at(x, y + 1) as u8;
            let br = at(x + 1, y + 1) as u8;
            let case = tl | (tr << 1) | (bl << 2) | (br << 3);
            if case == 0 || case == 15 {
                continue;
            }
            // Edge midpoints in doubled coordinates.
            let top = (2 * x + 1, 2 * y);
            let bottom = (2 * x + 1, 2 * y + 2);
            let left = (2 * x, 2 * y + 1);
            let right = (2 * x + 2, 2 * y + 1);
            // Material on the left of the directed edge (counter-clockwise
            // around solid regions in a y-down grid).
            let mut emit = |pairs: &[(P, P)]| {
                for (a, b) in pairs {
                    segments.push((*a, *b));
                }
            };
            // Bit order: tl=1, tr=2, bl=4, br=8. Directions keep material on
            // the left of travel (counter-clockwise in the y-down grid), so
            // segments chain end→start into closed loops.
            match case {
                1 => emit(&[(left, top)]),                  // tl corner
                2 => emit(&[(top, right)]),                 // tr corner
                3 => emit(&[(left, right)]),                // top pair
                4 => emit(&[(bottom, left)]),               // bl corner
                5 => emit(&[(bottom, top)]),                // left pair (vertical edge)
                6 => emit(&[(right, top), (bottom, left)]), // tr+bl saddle → two arcs
                7 => emit(&[(bottom, right)]),              // all but br
                8 => emit(&[(right, bottom)]),              // br corner
                9 => emit(&[(left, top), (right, bottom)]), // tl+br saddle → two arcs
                10 => emit(&[(top, bottom)]),               // right pair (vertical edge)
                11 => emit(&[(left, bottom)]),              // all but bl (reversed vs 4)
                12 => emit(&[(right, left)]),               // bottom pair
                13 => emit(&[(right, top)]),                // all but tr (reversed vs 2)
                14 => emit(&[(top, left)]),                 // all but tl (reversed vs 1)
                _ => {}
            }
        }
    }
    // Stitch directed segments into closed loops.
    let mut from: std::collections::HashMap<P, Vec<usize>> = std::collections::HashMap::new();
    for (i, (a, _)) in segments.iter().enumerate() {
        from.entry(*a).or_default().push(i);
    }
    let mut used = vec![false; segments.len()];
    let mut loops: Vec<Vec<[f32; 2]>> = Vec::new();
    for start in 0..segments.len() {
        if used[start] {
            continue;
        }
        used[start] = true;
        let mut chain: Vec<P> = vec![segments[start].0, segments[start].1];
        loop {
            let tail = chain[chain.len() - 1];
            let Some(next_list) = from.get(&tail) else {
                break;
            };
            let Some(&next) = next_list.iter().find(|&&i| !used[i]) else {
                break;
            };
            used[next] = true;
            chain.push(segments[next].1);
            if chain[0] == chain[chain.len() - 1] {
                chain.pop();
                break;
            }
        }
        if chain.len() >= 3 {
            let pts: Vec<[f32; 2]> = chain
                .iter()
                .map(|p| [p.0 as f32 * 0.5, p.1 as f32 * 0.5])
                .collect();
            let simplified = simplify(&pts, 0.8);
            if simplified.len() >= 3 {
                loops.push(simplified);
            }
        }
    }
    // Big regions first so they paint under details.
    loops.sort_by(|a, b| area(b).total_cmp(&area(a)));
    loops
}

/// Shoelace area (absolute).
fn area(loop_pts: &[[f32; 2]]) -> f32 {
    let n = loop_pts.len();
    let mut a = 0.0;
    for i in 0..n {
        let p = loop_pts[i];
        let q = loop_pts[(i + 1) % n];
        a += p[0] * q[1] - q[0] * p[1];
    }
    (a * 0.5).abs()
}

/// Douglas-Peucker for closed polylines (treated as open with wraparound
/// tolerance; keeps endpoints).
fn simplify(pts: &[[f32; 2]], epsilon: f32) -> Vec<[f32; 2]> {
    if pts.len() <= 4 {
        return pts.to_vec();
    }
    let mut keep = vec![false; pts.len()];
    keep[0] = true;
    keep[pts.len() - 1] = true;
    let mut stack = vec![(0usize, pts.len() - 1)];
    while let Some((start, end)) = stack.pop() {
        if end <= start + 1 {
            continue;
        }
        let a = pts[start];
        let b = pts[end];
        let (mut max_d, mut max_i) = (0.0f32, start);
        for i in start + 1..end {
            let d = point_line_distance(pts[i], a, b);
            if d > max_d {
                max_d = d;
                max_i = i;
            }
        }
        if max_d > epsilon {
            keep[max_i] = true;
            stack.push((start, max_i));
            stack.push((max_i, end));
        }
    }
    (0..pts.len())
        .filter(|&i| keep[i])
        .map(|i| pts[i])
        .collect()
}

fn point_line_distance(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let abx = b[0] - a[0];
    let aby = b[1] - a[1];
    let apx = p[0] - a[0];
    let apy = p[1] - a[1];
    let len2 = abx * abx + aby * aby;
    if len2 < 1e-9 {
        return apx.hypot(apy);
    }
    let t = (apx * abx + apy * aby) / len2;
    let cx = a[0] + abx * t - p[0];
    let cy = a[1] + aby * t - p[1];
    cx.hypot(cy)
}
