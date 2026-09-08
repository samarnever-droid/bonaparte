//! Vector path rasterization: even-odd fill across subpaths plus stroked
//! outlines. SVG imports, flattened glyph outlines and 3D wireframes all
//! land here as `LayerKind::Shape { points }`.

use bonaparte_effects::CpuFrame;
use bonaparte_model::ShapeStyle;

/// One pixel-space subpath (already scaled/translated to raster coords).
type Subpath = Vec<[f32; 2]>;

/// One flattened edge with cached extents for the active-edge scanline.
struct Edge {
    ax: f32,
    ay: f32,
    bx: f32,
    by: f32,
}

/// Rasterize vector subpaths into a `width × height` frame in layer-local
/// pixel space (origin at the top-left of the frame; callers pre-offset the
/// points so the layer's natural bbox maps into the frame).
///
/// Fill uses `color` (even-odd across subpaths); when `style.stroke_width`
/// is positive every subpath outline is stroked with `style.stroke_color`.
/// Edges get a 1px linear feather via a 2×2 supersample.
///
/// Complexity: fill work is O(rows × active-edges) via an active-edge table
/// and pixels outside the path bbox are skipped entirely; stroke distance
/// tests reject whole subpaths by bounding box before measuring.
pub fn rasterize_paths(
    width: u32,
    height: u32,
    color: [f32; 4],
    style: &ShapeStyle,
    subpaths: &[Subpath],
) -> CpuFrame {
    let mut out = CpuFrame::new(width, height);
    if subpaths.is_empty() || width == 0 || height == 0 {
        return out;
    }
    let stroke = style.stroke_width;
    let do_fill = color[3] > 0.0;
    let do_stroke = stroke > 0.0 && style.stroke_color[3] > 0.0;
    if !do_fill && !do_stroke {
        return out;
    }
    let half = stroke * 0.5;
    let pad = half + 1.0;

    // Path bbox (fills) and per-subpath bboxes (stroke rejection).
    let Some([min_x, min_y, max_x, max_y]) = bounds(subpaths) else {
        return out;
    };
    let sub_boxes: Vec<[f32; 4]> = subpaths
        .iter()
        .filter_map(|sub| bounds(std::slice::from_ref(sub)))
        .collect();

    // Flat edges for the fill scanline (subpaths are closed polygons).
    let mut edges: Vec<Edge> = Vec::new();
    if do_fill {
        for sub in subpaths {
            if sub.len() < 3 {
                continue;
            }
            for i in 0..sub.len() {
                let a = sub[i];
                let b = sub[(i + 1) % sub.len()];
                if a[1] != b[1] {
                    edges.push(Edge {
                        ax: a[0],
                        ay: a[1],
                        bx: b[0],
                        by: b[1],
                    });
                }
            }
        }
        // Scanline visits edges in y-order.
        edges.sort_by(|a, b| a.ay.min(a.by).total_cmp(&b.ay.min(b.by)));
    }

    // Raster window: intersect the frame with the padded path bbox.
    let x0 = (min_x - pad).floor().max(0.0) as u32;
    let x1 = ((max_x + pad).ceil().min(width as f32)).max(0.0) as u32;
    let y0 = (min_y - pad).floor().max(0.0) as u32;
    let y1 = ((max_y + pad).ceil().min(height as f32)).max(0.0) as u32;

    const OFFSETS: [[f32; 2]; 4] = [[0.25, 0.25], [0.75, 0.25], [0.25, 0.75], [0.75, 0.75]];
    // sRGB-encoded paint, computed once.
    let fill_rgb = [
        bonaparte_effects::grading::linear_to_srgb(color[0]),
        bonaparte_effects::grading::linear_to_srgb(color[1]),
        bonaparte_effects::grading::linear_to_srgb(color[2]),
    ];
    let stroke_rgb = [
        bonaparte_effects::grading::linear_to_srgb(style.stroke_color[0]),
        bonaparte_effects::grading::linear_to_srgb(style.stroke_color[1]),
        bonaparte_effects::grading::linear_to_srgb(style.stroke_color[2]),
    ];
    let stroke_alpha = style.stroke_color[3];

    let mut active: Vec<usize> = Vec::new();
    let mut next_edge = 0usize;
    let mut crossings: Vec<f32> = Vec::new();

    for py in y0..y1 {
        // The 2x2 subsample grid visits two sample rows (py+0.25, py+0.75).
        // Manage the active-edge table once per ROW against that whole span
        // — advancing per sample would rewind: samples restart at +0.25 in
        // every pixel while the table only moves forward, so any edge whose
        // span begins between the two sample rows went missing for the rest
        // of the row (half-covered scanlines through polygon vertices).
        let sy_lo = py as f32 + 0.25;
        let sy_hi = py as f32 + 0.75;
        if do_fill {
            while next_edge < edges.len() && edges[next_edge].ay.min(edges[next_edge].by) <= sy_hi {
                active.push(next_edge);
                next_edge += 1;
            }
            active.retain(|&e| {
                let e = &edges[e];
                e.ay.max(e.by) > sy_lo
            });
        }
        for px_x in x0..x1 {
            let mut coverage_fill = 0u32;
            let mut coverage_stroke = 0u32;
            for off in OFFSETS {
                let sx = px_x as f32 + off[0];
                let sy = py as f32 + off[1];
                if do_fill {
                    crossings.clear();
                    for &e in &active {
                        let e = &edges[e];
                        if (e.ay > sy) != (e.by > sy) {
                            let t = (sy - e.ay) / (e.by - e.ay);
                            crossings.push(e.ax + t * (e.bx - e.ax));
                        }
                    }
                    crossings.sort_by(|a, b| a.total_cmp(b));
                    // Even-odd: inside if an odd number of crossings lie right of sx.
                    let mut inside = false;
                    for c in crossings.iter().rev() {
                        if *c > sx {
                            inside = !inside;
                        } else {
                            break;
                        }
                    }
                    if inside {
                        coverage_fill += 1;
                    }
                }
                if do_stroke {
                    for (si, box4) in sub_boxes.iter().enumerate() {
                        if sx < box4[0] - half
                            || sx > box4[2] + half
                            || sy < box4[1] - half
                            || sy > box4[3] + half
                        {
                            continue;
                        }
                        if outline_distance_sub(sx, sy, &subpaths[si]) <= half {
                            coverage_stroke += 1;
                            break;
                        }
                    }
                }
            }
            if coverage_fill == 0 && coverage_stroke == 0 {
                continue;
            }
            let idx = (py as usize * width as usize + px_x as usize) * 4;
            let mut r = 0.0f32;
            let mut g = 0.0;
            let mut b = 0.0;
            let mut a = 0.0f32;
            if coverage_fill > 0 {
                let cf = coverage_fill as f32 / 4.0;
                r += cf * fill_rgb[0];
                g += cf * fill_rgb[1];
                b += cf * fill_rgb[2];
                a += cf * color[3];
            }
            if coverage_stroke > 0 {
                let cs = coverage_stroke as f32 / 4.0;
                // Stroke over fill.
                r = r * (1.0 - stroke_alpha) + cs * stroke_rgb[0] * stroke_alpha;
                g = g * (1.0 - stroke_alpha) + cs * stroke_rgb[1] * stroke_alpha;
                b = b * (1.0 - stroke_alpha) + cs * stroke_rgb[2] * stroke_alpha;
                a = (a * (1.0 - stroke_alpha) + cs * stroke_alpha).min(1.0);
            }
            out.rgba[idx] = (r * 255.0).round().clamp(0.0, 255.0) as u8;
            out.rgba[idx + 1] = (g * 255.0).round().clamp(0.0, 255.0) as u8;
            out.rgba[idx + 2] = (b * 255.0).round().clamp(0.0, 255.0) as u8;
            out.rgba[idx + 3] = (a * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

/// Distance from a point to a segment.
fn segment_distance(px: f32, py: f32, a: [f32; 2], b: [f32; 2]) -> f32 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len2 = dx * dx + dy * dy;
    let t = if len2 <= f32::EPSILON {
        0.0
    } else {
        (((px - a[0]) * dx + (py - a[1]) * dy) / len2).clamp(0.0, 1.0)
    };
    (px - (a[0] + t * dx)).hypot(py - (a[1] + t * dy))
}

/// Nearest distance from a point to one subpath's segments (closed when it
/// forms a polygon, open for polylines).
fn outline_distance_sub(x: f32, y: f32, sub: &[[f32; 2]]) -> f32 {
    let n = sub.len();
    if n < 2 {
        return f32::MAX;
    }
    let closed = n > 2;
    let last = if closed { n } else { n - 1 };
    let mut best = f32::MAX;
    for i in 0..last {
        let a = sub[i];
        let b = sub[(i + 1) % n];
        let d = segment_distance(x, y, a, b);
        if d < best {
            best = d;
        }
    }
    best
}

/// Bounding box of all subpath points.
pub fn bounds(subpaths: &[Subpath]) -> Option<[f32; 4]> {
    let mut min = [f32::MAX; 2];
    let mut max = [f32::MIN; 2];
    let mut any = false;
    for sub in subpaths {
        for p in sub {
            any = true;
            min[0] = min[0].min(p[0]);
            min[1] = min[1].min(p[1]);
            max[0] = max[0].max(p[0]);
            max[1] = max[1].max(p[1]);
        }
    }
    if any {
        Some([min[0], min[1], max[0], max[1]])
    } else {
        None
    }
}
