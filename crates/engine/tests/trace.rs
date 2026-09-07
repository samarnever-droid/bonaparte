//! Raster→vector gate: posterized tracing must recover clean closed contours,
//! keep holes, and land layers in image coordinates.

use bonaparte_engine::trace::trace_image;
use bonaparte_model::{LayerKind, TICKS_PER_SEC};

fn solid(width: u32, height: u32, color: [u8; 4]) -> Vec<u8> {
    (0..width * height).flat_map(|_| color).collect()
}

#[test]
fn solid_square_traces_to_one_rectangular_loop() {
    // 64×64 transparent, 16..48 red square.
    let mut rgba = solid(64, 64, [0, 0, 0, 0]);
    for y in 16..48 {
        for x in 16..48 {
            let i = ((y * 64 + x) * 4) as usize;
            rgba[i..i + 4].copy_from_slice(&[230, 40, 40, 255]);
        }
    }
    let layers = trace_image(&rgba, 64, 64, 4, 2 * TICKS_PER_SEC).expect("square");
    assert_eq!(layers.len(), 1);
    let LayerKind::Shape {
        points,
        color,
        style,
        ..
    } = &layers[0].kind
    else {
        panic!("shape layer");
    };
    assert_eq!(points.len(), 1, "one contour");
    assert!(
        points[0].len() >= 4,
        "closed quad-ish loop, got {}",
        points[0].len()
    );
    // Contour extent ≈ 32 px (16..48) after simplification.
    let xs: Vec<f32> = points[0].iter().map(|p| p[0]).collect();
    let w =
        xs.iter().cloned().fold(f32::MIN, f32::max) - xs.iter().cloned().fold(f32::MAX, f32::min);
    assert!((w - 32.0).abs() < 3.0, "square width ≈ 32, got {w}");
    assert!(
        color[0] > 0.7 && color[1] < 0.1,
        "red linearized, got {color:?}"
    );
    assert_eq!(style.stroke_width, 0.0, "fill only");
    // Centered on the square's own bbox: square center (32,32), image center (32,32).
    assert!(layers[0].transform.position[0].abs() < 0.5);
    assert!(
        layers[0].transform.position[1].abs() < 1.0,
        "half-pixel contour offset is fine"
    );
}

#[test]
fn square_with_hole_keeps_two_loops_for_even_odd_fill() {
    let mut rgba = solid(64, 64, [10, 10, 240, 255]);
    // Punch a transparent 16..48 hole → outer ring remains.
    for y in 16..48 {
        for x in 16..48 {
            let i = ((y * 64 + x) * 4) as usize;
            rgba[i + 3] = 0;
        }
    }
    let layers = trace_image(&rgba, 64, 64, 4, 2 * TICKS_PER_SEC).expect("ring");
    assert_eq!(layers.len(), 1);
    let LayerKind::Shape { points, .. } = &layers[0].kind else {
        panic!("shape layer");
    };
    assert_eq!(points.len(), 2, "outer boundary + hole boundary");
}

#[test]
fn two_color_posterization_yields_two_layers_with_distinct_fills() {
    let mut rgba = solid(48, 48, [250, 240, 220, 255]);
    for y in 8..40 {
        for x in 8..24 {
            let i = ((y * 48 + x) * 4) as usize;
            rgba[i..i + 4].copy_from_slice(&[20, 160, 90, 255]);
        }
    }
    let layers = trace_image(&rgba, 48, 48, 4, 2 * TICKS_PER_SEC).expect("poster");
    assert_eq!(layers.len(), 2, "background + green blob");
    // Largest first: the full background paints under the blob.
    let LayerKind::Shape { color: first, .. } = &layers[0].kind else {
        panic!("shape");
    };
    assert!(first[0] > 0.85, "cream background first, got {first:?}");
    // Layer positions: green blob center (16,24) vs image center (24,24).
    let LayerKind::Shape { points, .. } = &layers[1].kind else {
        panic!("shape");
    };
    assert!(!points.is_empty());
    let cx: f32 = points
        .iter()
        .map(|s| s.iter().map(|p| p[0]).sum::<f32>())
        .zip(points.iter().map(|s| s.len() as f32))
        .map(|(sum, n)| sum / n)
        .sum::<f32>()
        / points.len() as f32;
    assert!(cx.abs() < 2.0, "blob centered on its bbox, got {cx}");
}

#[test]
fn transparent_image_and_bad_sizes_are_rejected() {
    let rgba = solid(32, 32, [0, 0, 0, 0]);
    assert!(trace_image(&rgba, 32, 32, 4, 1000).is_err());
    assert!(trace_image(&rgba, 0, 32, 4, 1000).is_err());
    assert!(trace_image(&rgba[..10], 32, 32, 4, 1000).is_err());
}

#[test]
fn antialiased_circle_survives_downscale_tracing() {
    // 200×200 circle radius 70 at (100,100), hard edge (downscale averages it).
    let mut rgba = solid(200, 200, [255, 255, 255, 0]);
    for y in 0..200 {
        for x in 0..200 {
            let dx = x as f32 - 100.0;
            let dy = y as f32 - 100.0;
            if dx * dx + dy * dy <= 70.0 * 70.0 {
                let i = ((y * 200 + x) * 4) as usize;
                rgba[i..i + 4].copy_from_slice(&[240, 120, 30, 255]);
            }
        }
    }
    let layers = trace_image(&rgba, 200, 200, 4, 2 * TICKS_PER_SEC).expect("circle");
    assert!(!layers.is_empty());
    let LayerKind::Shape { points, .. } = &layers[0].kind else {
        panic!("shape");
    };
    let n = points[0].len();
    assert!(n >= 8 && n <= 400, "circle contour reasonably dense: {n}");
    // Radius check: extent ≈ 140 px.
    let xs: Vec<f32> = points[0].iter().map(|p| p[0]).collect();
    let w =
        xs.iter().cloned().fold(f32::MIN, f32::max) - xs.iter().cloned().fold(f32::MAX, f32::min);
    assert!((w - 140.0).abs() < 6.0, "diameter ≈ 140, got {w}");
}
