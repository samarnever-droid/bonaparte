//! Wavefront .obj import: vertices/faces become per-object wireframe vector
//! layers (unique triangle edges, stroke-only), so any mesh renders
//! immediately in the 3D diorama — depth-sorted, animatable, turntable-ready.

use bonaparte_model::{Layer, LayerKind, ShapeStyle, Time};

/// Parse an OBJ document into one wireframe layer per `o`/`g` group (or a
/// single layer for group-less files). Returns the layers positioned in
/// model space; the caller fits them into a comp and assigns depth.
pub fn obj_layers(obj: &str, duration_ticks: i64) -> Result<Vec<Layer>, String> {
    let mut vertices: Vec<[f32; 3]> = Vec::new();
    // (group_name, edge_list) in insertion order
    let mut groups: Vec<(String, Vec<[usize; 2]>)> = Vec::new();
    let mut current = String::from("Model");
    let mut seen = std::collections::HashSet::new();
    let mut faces_total = 0usize;

    for raw in obj.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let keyword = parts.next().unwrap_or("");
        match keyword {
            "o" | "g" => {
                let name = parts.collect::<Vec<_>>().join(" ");
                if !name.is_empty() && name != current {
                    current = name;
                    groups.push((current.clone(), Vec::new()));
                    seen = std::collections::HashSet::new();
                }
            }
            "v" => {
                let coords: Vec<f32> = parts.filter_map(|t| t.parse().ok()).collect();
                if coords.len() >= 3 {
                    vertices.push([coords[0], coords[1], coords[2]]);
                }
            }
            "f" => {
                let indices: Vec<usize> = parts
                    .filter_map(|t| {
                        t.split('/')
                            .next()
                            .and_then(|v| v.parse::<i64>().ok())
                            .map(|v| {
                                if v > 0 {
                                    (v - 1) as usize
                                } else {
                                    (vertices.len() as i64 + v).max(0) as usize
                                }
                            })
                    })
                    .collect();
                if indices.len() < 2 {
                    continue;
                }
                if groups.is_empty() {
                    groups.push((current.clone(), Vec::new()));
                }
                faces_total += 1;
                // Fan-triangulate, keep unique edges.
                for w in indices.windows(2) {
                    let edge = [w[0], w[1]];
                    let key = (edge[0].min(edge[1]), edge[0].max(edge[1]));
                    if seen.insert(key) {
                        if let Some(last) = groups.last_mut() {
                            last.1.push(edge);
                        }
                    }
                }
                let first = indices[0];
                let last_index = *indices.last().unwrap();
                let key = (first.min(last_index), first.max(last_index));
                if seen.insert(key) {
                    if let Some(last) = groups.last_mut() {
                        last.1.push([first, last_index]);
                    }
                }
            }
            _ => {}
        }
    }

    if vertices.is_empty() || faces_total == 0 {
        return Err("No vertices/faces found in the OBJ".into());
    }

    // Global bounds for fit + depth.
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for v in &vertices {
        for axis in 0..3 {
            min[axis] = min[axis].min(v[axis]);
            max[axis] = max[axis].max(v[axis]);
        }
    }
    let center = [
        (min[0] + max[0]) * 0.5,
        (min[1] + max[1]) * 0.5,
        (min[2] + max[2]) * 0.5,
    ];
    let span = (max[0] - min[0]).max((max[1] - min[1]).max(1.0));

    // Per-object depth from its own z-center, normalized to the model height.
    let height = (max[1] - min[1]).max(1e-3);

    let palette: [[f32; 4]; 6] = [
        [0.62, 0.76, 0.50, 1.0],
        [0.55, 0.70, 0.85, 1.0],
        [0.90, 0.63, 0.42, 1.0],
        [0.72, 0.55, 0.86, 1.0],
        [0.86, 0.84, 0.55, 1.0],
        [0.60, 0.85, 0.78, 1.0],
    ];

    let mut layers = Vec::new();
    for (index, (name, edges)) in groups.iter().enumerate() {
        if edges.is_empty() {
            continue;
        }
        // Object-local z-center → depth offset (-0.5..0.5 of model height).
        let mut z_min = f32::MAX;
        let mut z_max = f32::MIN;
        let mut x_min = f32::MAX;
        let mut x_max = f32::MIN;
        let mut y_min = f32::MAX;
        let mut y_max = f32::MIN;
        for edge in edges {
            for vi in edge {
                if let Some(v) = vertices.get(*vi) {
                    z_min = z_min.min(v[2]);
                    z_max = z_max.max(v[2]);
                    x_min = x_min.min(v[0]);
                    x_max = x_max.max(v[0]);
                    y_min = y_min.min(v[1]);
                    y_max = y_max.max(v[1]);
                }
            }
        }
        let z_center = (z_min + z_max) * 0.5;
        let depth = ((z_center - center[2]) / height * 240.0).clamp(-480.0, 480.0);
        // Layer-local stroke-only subpaths in px (centered on the object bbox).
        let subpaths: Vec<Vec<[f32; 2]>> = edges
            .iter()
            .filter_map(|edge| {
                let a = vertices.get(edge[0])?;
                let b = vertices.get(edge[1])?;
                Some(vec![
                    [
                        a[0] - (x_min + x_max) * 0.5,
                        -(a[1] - (y_min + y_max) * 0.5),
                    ],
                    [
                        b[0] - (x_min + x_max) * 0.5,
                        -(b[1] - (y_min + y_max) * 0.5),
                    ],
                ])
            })
            .collect();
        let w = (x_max - x_min).max(1.0);
        let h = (y_max - y_min).max(1.0);
        let mut layer = Layer::new(
            if groups.len() > 1 {
                format!("Wireframe {name}")
            } else {
                "Wireframe".to_string()
            },
            LayerKind::Shape {
                // Fill is transparent: pure wireframe.
                color: [0.0, 0.0, 0.0, 0.0],
                generator: None,
                style: ShapeStyle {
                    size: Some([w, h]),
                    corner_radius: 0.0,
                    stroke_width: 1.5,
                    stroke_color: palette[index % palette.len()],
                },
                points: subpaths,
            },
            Time(0),
            Time(duration_ticks),
        );
        layer.transform.position = [
            (x_min + x_max) * 0.5 - center[0],
            -((y_min + y_max) * 0.5 - center[1]),
        ];
        layer.transform.z = depth;
        layers.push(layer);
    }
    let _ = span;
    if layers.is_empty() {
        return Err("OBJ groups contain no drawable edges".into());
    }
    Ok(layers)
}
