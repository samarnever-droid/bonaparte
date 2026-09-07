//! Import gate: SVG geometry lands as editable vector shape layers and OBJ
//! meshes as depth-positioned wireframes. No mercy for lossy imports.

use bonaparte_engine::import_obj::obj_layers;
use bonaparte_engine::import_svg::svg_layers;
use bonaparte_model::{LayerKind, TICKS_PER_SEC};

const DUR: i64 = 2 * TICKS_PER_SEC;

fn shape_points(layer: &bonaparte_model::Layer) -> &Vec<Vec<[f32; 2]>> {
    match &layer.kind {
        LayerKind::Shape { points, .. } => points,
        other => panic!("expected shape layer, got {other:?}"),
    }
}

#[test]
fn svg_rect_becomes_four_corner_subpath() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100">
        <rect x="20" y="10" width="60" height="40" fill="#ff0000"/>
    </svg>"##;
    let (w, h, layers) = svg_layers(svg, DUR).expect("rect import");
    assert_eq!((w, h), (200.0, 100.0));
    assert_eq!(layers.len(), 1);
    let layer = &layers[0];
    assert_eq!(layer.start.0, 0);
    assert_eq!(layer.duration.0, DUR);
    let points = shape_points(layer);
    assert_eq!(points.len(), 1, "one closed subpath");
    assert_eq!(points[0].len(), 4, "rect = 4 corners");
    // Bounding box of the subpath ≈ 60×40.
    let xs: Vec<f32> = points[0].iter().map(|p| p[0]).collect();
    let ys: Vec<f32> = points[0].iter().map(|p| p[1]).collect();
    let bw =
        xs.iter().cloned().fold(f32::MIN, f32::max) - xs.iter().cloned().fold(f32::MAX, f32::min);
    let bh =
        ys.iter().cloned().fold(f32::MIN, f32::max) - ys.iter().cloned().fold(f32::MAX, f32::min);
    assert!((bw - 60.0).abs() < 0.5, "width {bw}");
    assert!((bh - 40.0).abs() < 0.5, "height {bh}");
    // Centered on its own bbox.
    let cx = (xs.iter().sum::<f32>()) / 4.0;
    let cy = (ys.iter().sum::<f32>()) / 4.0;
    assert!(
        cx.abs() < 0.5 && cy.abs() < 0.5,
        "centered, got ({cx},{cy})"
    );
    match &layer.kind {
        LayerKind::Shape { color, .. } => {
            assert!(color[0] > 0.9 && color[1] < 0.1, "red fill, got {color:?}");
        }
        _ => unreachable!(),
    }
}

#[test]
fn svg_circle_and_ellipse_get_bezier_rings() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
        <circle cx="50" cy="50" r="20"/>
        <ellipse cx="50" cy="50" rx="30" ry="10" fill="none" stroke="#00ff00" stroke-width="4"/>
    </svg>"##;
    let (_, _, layers) = svg_layers(svg, DUR).expect("circle import");
    assert_eq!(layers.len(), 2);
    let circle = shape_points(&layers[0]);
    assert!(circle[0].len() >= 16, "circle ring is dense enough");
    // Ellipse: stroke-only → stroke color green, width 4.
    match &layers[1].kind {
        LayerKind::Shape { style, .. } => {
            assert!(style.stroke_color[1] > 0.9, "green stroke");
            assert!((style.stroke_width - 4.0).abs() < 0.01);
        }
        _ => unreachable!(),
    }
}

#[test]
fn svg_path_curves_and_arcs_flatten() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="120" height="120">
        <path d="M10 80 C 40 10, 65 10, 95 80 Z" fill="blue"/>
        <path d="M20 20 A 30 30 0 0 1 80 20" stroke="black" fill="none"/>
    </svg>"##;
    let (_, _, layers) = svg_layers(svg, DUR).expect("path import");
    assert_eq!(layers.len(), 2);
    let cubic = shape_points(&layers[0]);
    assert!(cubic[0].len() >= 12, "cubic flattens to many points");
    let arc = shape_points(&layers[1]);
    assert!(arc[0].len() >= 8, "arc flattens to enough points");
    // Both endpoints survive, bbox-center-relative (center (50,5), half-circle
    // over the top): (20,20) → (-30, 15), (80,20) → (30, 15).
    let first = arc[0][0];
    let last = *arc[0].last().unwrap();
    assert!(
        (first[0] + 30.0).abs() < 1.0 && (first[1] - 15.0).abs() < 1.0,
        "arc start {first:?}"
    );
    assert!(
        (last[0] - 30.0).abs() < 1.0 && (last[1] - 15.0).abs() < 1.0,
        "arc end {last:?}"
    );
    // Sweep 1 bulges over the top → topmost point well above endpoints.
    let top = arc[0].iter().map(|p| p[1]).fold(f32::MAX, f32::min);
    assert!(top < -10.0, "arc apex {top}");
}

#[test]
fn svg_group_transform_and_fill_inherit() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="300" height="200">
        <g fill="#ff8800" transform="translate(50, 25)">
            <rect x="0" y="0" width="40" height="20"/>
        </g>
    </svg>"##;
    let (w, h, layers) = svg_layers(svg, DUR).expect("group import");
    assert_eq!((w, h), (300.0, 200.0));
    assert_eq!(layers.len(), 1);
    // Position = shape center (70, 35) minus doc center (150, 100), y-down.
    assert!(
        (layers[0].transform.position[0] - (70.0 - 150.0)).abs() < 1.0,
        "x {:?}",
        layers[0].transform.position
    );
    assert!(
        (layers[0].transform.position[1] - (35.0 - 100.0)).abs() < 1.0,
        "y {:?}",
        layers[0].transform.position
    );
    match &layers[0].kind {
        LayerKind::Shape { color, .. } => {
            // #ff8800 → linear space: g = srgb_to_linear(136/255) ≈ 0.251.
            assert!(
                color[0] > 0.9 && color[1] > 0.2 && color[1] < 0.3,
                "orange fill {color:?}"
            );
        }
        _ => unreachable!(),
    }
}

#[test]
fn svg_scale_transform_scales_geometry() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">
        <g transform="scale(2)">
            <rect x="10" y="10" width="20" height="10"/>
        </g>
    </svg>"##;
    let (_, _, layers) = svg_layers(svg, DUR).expect("scale import");
    let points = shape_points(&layers[0]);
    let xs: Vec<f32> = points[0].iter().map(|p| p[0]).collect();
    let bw =
        xs.iter().cloned().fold(f32::MIN, f32::max) - xs.iter().cloned().fold(f32::MAX, f32::min);
    assert!((bw - 40.0).abs() < 0.5, "20×2 = 40 wide, got {bw}");
}

#[test]
fn svg_viewbox_only_document() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 400 300">
        <rect x="10" y="10" width="50" height="50"/>
    </svg>"##;
    let (w, h, _) = svg_layers(svg, DUR).expect("viewbox import");
    assert_eq!((w, h), (400.0, 300.0));
}

#[test]
fn svg_no_size_is_rejected() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg"><rect width="5" height="5"/></svg>"##;
    assert!(svg_layers(svg, DUR).is_err(), "sizeless SVG must error");
}

#[test]
fn svg_polygon_and_polyline() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
        <polygon points="10,10 90,10 50,90"/>
        <polyline points="0,0 20,30 40,10" fill="none" stroke="red"/>
    </svg>"##;
    let (_, _, layers) = svg_layers(svg, DUR).expect("poly import");
    assert_eq!(layers.len(), 2);
    assert_eq!(shape_points(&layers[0])[0].len(), 3, "triangle corners");
    // Y axis stays y-down like SVG/canvas: apex (y=90) → positive local y.
    let ys: Vec<f32> = shape_points(&layers[0])[0].iter().map(|p| p[1]).collect();
    assert!(
        ys.iter().cloned().fold(f32::MIN, f32::max) > 0.0,
        "apex below center"
    );
}

#[test]
fn obj_cube_single_group_twelve_edges() {
    let obj = r##"
# cube
v 0 0 0
v 1 0 0
v 1 1 0
v 0 1 0
v 0 0 1
v 1 0 1
v 1 1 1
v 0 1 1
o Cube
f 1 2 3 4
f 5 6 7 8
f 1 2 6 5
f 2 3 7 6
f 3 4 8 7
f 4 1 5 8
"##;
    let layers = obj_layers(obj, DUR).expect("cube");
    assert_eq!(layers.len(), 1);
    let points = shape_points(&layers[0]);
    assert_eq!(
        points.len(),
        12,
        "cube has 12 unique edges, got {}",
        points.len()
    );
    // Pure wireframe: transparent fill, visible stroke.
    match &layers[0].kind {
        LayerKind::Shape { color, style, .. } => {
            assert!(color[3] < 0.01, "no fill");
            assert!(style.stroke_color[3] > 0.9);
        }
        _ => unreachable!(),
    }
    // Centered model → z ≈ 0.
    assert!(
        layers[0].transform.z.abs() < 1.0,
        "centered cube on scene plane"
    );
}

#[test]
fn obj_groups_get_depth_and_palette() {
    let obj = r##"
v 0 0 -1
v 1 0 -1
v 1 1 -1
v 0 1 -1
v 0 0 1
v 1 0 1
v 1 1 1
v 0 1 1
o Back
f 1 2 3 4
o Front
f 5 6 7 8
"##;
    let layers = obj_layers(obj, DUR).expect("two groups");
    assert_eq!(layers.len(), 2);
    let z_back = layers[0].transform.z;
    let z_front = layers[1].transform.z;
    assert!(z_back < 0.0, "back group pushed away, got {z_back}");
    assert!(z_front > 0.0, "front group pulled near, got {z_front}");
    match (&layers[0].kind, &layers[1].kind) {
        (LayerKind::Shape { style: s0, .. }, LayerKind::Shape { style: s1, .. }) => assert_ne!(
            s0.stroke_color, s1.stroke_color,
            "groups get distinct colors"
        ),
        _ => unreachable!(),
    }
}

#[test]
fn obj_face_indices_negative_and_slash_forms() {
    let obj = r##"
v 0 0 0
v 2 0 0
v 2 2 0
v 0 2 0
f 1/1/1 2/2/2 3/3/3
f -4 -3 -2
"##;
    let layers = obj_layers(obj, DUR).expect("slash/negative indices");
    assert_eq!(layers.len(), 1);
    // 4 verts: -4 -3 -2 == 1 2 3, so both faces are the same triangle
    // → 3 unique edges, and no out-of-bounds index panic.
    assert_eq!(shape_points(&layers[0]).len(), 3);
}

#[test]
fn obj_without_faces_rejected() {
    let obj = "v 0 0 0\nv 1 0 0\nv 0 1 0\n";
    assert!(obj_layers(obj, DUR).is_err());
}
