//! Bridge import gate: SVG and OBJ land as editable layers through the same
//! atomic command path as images, survive save/open, and undo cleanly.

use base64::{engine::general_purpose::STANDARD, Engine};
use bonaparte_model::*;
use bonaparte_runtime::*;
use serde_json::json;

fn session() -> EditorSession {
    let mut p = Project::new("Import");
    p.create_comp("Main", 320, 180, FrameRate::FPS_30, Time(240_000));
    EditorSession::new(p).unwrap()
}

fn layer_count(s: &EditorSession) -> usize {
    s.project.comp(CompId(1)).unwrap().layer_order.len()
}

const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="160" height="90">
  <rect x="10" y="10" width="60" height="40" fill="#ff8800"/>
  <circle cx="120" cy="45" r="25" fill="none" stroke="#33ccff" stroke-width="3"/>
</svg>"##;

const OBJ: &str = r#"# little tetrahedron-ish wireframe
v 0 0 0
v 2 0 0
v 1 2 0
v 1 1 2
o Base
f 1 2 3
o Spike
f 1 2 4
f 2 3 4
f 3 1 4
"#;

#[test]
fn svg_import_creates_fitted_layers_and_renders() {
    let mut s = session();
    let state = s
        .command(
            "import_svg",
            json!({"name":"logo.svg","svg":SVG,"compId":1}),
        )
        .unwrap();
    let order = state["project"]["comps"]["1"]["layer_order"]
        .as_array()
        .unwrap();
    assert_eq!(order.len(), 1, "ONE assembled PreComp group");
    let group = state["project"]["comps"]["1"]["layers"][order[0].to_string()].clone();
    let child_id = group["kind"]["PreComp"]["comp"].as_u64().unwrap();
    let child = &state["project"]["comps"][&child_id.to_string()];
    assert_eq!(
        child["layer_order"].as_array().unwrap().len(),
        2,
        "rect + circle live inside the group"
    );
    // Render succeeds and produces real pixels.
    let frame = s
        .render_input(serde_json::from_value(json!({"compId":1,"time":0})).unwrap())
        .unwrap()
        .render()
        .unwrap();
    assert_eq!(frame.rgba.len(), 320 * 180 * 4);
    // The orange rect contributes non-background pixels.
    assert!(
        frame
            .rgba
            .chunks_exact(4)
            .any(|px| px[0] > 150 && px[2] < 100),
        "orange shape visible in render"
    );
}

#[test]
fn svg_import_fits_inside_comp_and_never_upscales() {
    let mut s = session();
    let big = SVG.replace("160", "6400").replace("90", "3600");
    s.command("import_svg", json!({"name":"big.svg","svg":big,"compId":1}))
        .unwrap();
    let comp = s.project.comp(CompId(1)).unwrap();
    // The fit scale lives on the assembled group; the drawing must fit.
    assert_eq!(comp.layer_order.len(), 1);
    let group = s.project.layer(CompId(1), comp.layer_order[0]).unwrap();
    let scale = group.transform.scale[0] / 100.0;
    assert!(scale <= 1.0, "no upscale, got {scale}");
    let child_id = match &group.kind {
        LayerKind::PreComp { comp: c } => *c,
        other => panic!("expected PreComp, got {other:?}"),
    };
    let child = s.project.comp(child_id).unwrap();
    let group_w = child.width as f32 * scale;
    assert!(
        group_w <= 320.0,
        "drawing stays inside the comp width: {group_w}"
    );
}

#[test]
fn obj_import_creates_depth_wireframes() {
    let mut s = session();
    let state = s
        .command(
            "import_obj",
            json!({"name":"mesh.obj","obj":OBJ,"compId":1}),
        )
        .unwrap();
    let order = state["project"]["comps"]["1"]["layer_order"]
        .as_array()
        .unwrap();
    assert_eq!(order.len(), 2, "Base + Spike groups");
    let comp = s.project.comp(CompId(1)).unwrap();
    let depths: Vec<f32> = comp
        .layer_order
        .iter()
        .map(|id| s.project.layer(CompId(1), *id).unwrap().transform.z)
        .collect();
    assert!(
        depths.iter().any(|z| *z != 0.0),
        "groups separate in depth, got {depths:?}"
    );
    let frame = s
        .render_input(serde_json::from_value(json!({"compId":1,"time":0})).unwrap())
        .unwrap()
        .render()
        .unwrap();
    assert_eq!(frame.rgba.len(), 320 * 180 * 4);
    assert!(
        frame
            .rgba
            .chunks_exact(4)
            .any(|px| px[3] == 255 && (px[0] < 250 || px[1] < 250)),
        "wireframe strokes visible in render"
    );
}

#[test]
fn malformed_imports_are_atomic() {
    let mut s = session();
    let before = s.command("save_project", json!({})).unwrap().to_string();
    assert!(s
        .command(
            "import_svg",
            json!({"name":"bad.svg","svg":"<svg></svg>","compId":1})
        )
        .is_err());
    assert!(s
        .command("import_svg", json!({"name":"sizeless.svg","svg":"<svg xmlns=\"x\"><rect width=\"5\" height=\"5\"/></svg>","compId":1}))
        .is_err());
    assert!(s
        .command(
            "import_obj",
            json!({"name":"bad.obj","obj":"v 0 0 0\nv 1 0 0\n","compId":1})
        )
        .is_err());
    let after = s.command("save_project", json!({})).unwrap().to_string();
    assert_eq!(before, after, "failed imports must not mutate the project");
    assert_eq!(layer_count(&s), 0);
}

#[test]
fn imports_undo_in_one_step_and_round_trip_through_save() {
    let mut s = session();
    s.command("import_svg", json!({"name":"a.svg","svg":SVG,"compId":1}))
        .unwrap();
    s.command("import_obj", json!({"name":"m.obj","obj":OBJ,"compId":1}))
        .unwrap();
    assert_eq!(layer_count(&s), 3, "1 assembled SVG group + 2 OBJ layers");
    s.command("undo", json!({})).unwrap();
    assert_eq!(layer_count(&s), 1, "OBJ batch undoes whole");
    s.command("undo", json!({})).unwrap();
    assert_eq!(layer_count(&s), 0, "SVG assembly undoes whole");
    s.command("redo", json!({})).unwrap();
    assert_eq!(layer_count(&s), 1);
    // Round-trip: save → reopen → layers intact.
    let serialized = s
        .command("save_project", json!({}))
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned();
    let reopened = EditorSession::new(parse_project(&serialized).unwrap()).unwrap();
    assert_eq!(layer_count(&reopened), 1);
    // The assembled group survives serialization with its vector children.
    let reopened_comp = reopened.project.comp(CompId(1)).unwrap();
    let LayerKind::PreComp { comp: child_id } = &reopened
        .project
        .layer(CompId(1), reopened_comp.layer_order[0])
        .unwrap()
        .kind
    else {
        panic!("group must survive save/reopen");
    };
    let child = reopened.project.comp(*child_id).unwrap();
    let points_shapes = child
        .layer_order
        .iter()
        .filter(|id| {
            matches!(
                &child.layers[id].kind,
                LayerKind::Shape { points, .. } if !points.is_empty()
            )
        })
        .count();
    assert_eq!(points_shapes, 2, "vector points survive serialization");
}

#[test]
fn vectorize_image_traces_embedded_pixels_into_shape_layers() {
    let mut s = session();
    // 16×16 opaque image: left half teal, right half cream.
    let mut rgba = Vec::new();
    for _ in 0..16 {
        for x in 0..16 {
            let c = if x < 8 {
                [0u8, 150, 140, 255]
            } else {
                [245u8, 240, 225, 255]
            };
            rgba.extend_from_slice(&c);
        }
    }
    s.command(
        "import_image",
        json!({"name":"shot.png","width":16,"height":16,"rgbaBase64":STANDARD.encode(&rgba),"compId":1}),
    )
    .unwrap();
    let media_layer = LayerId(1);
    let state = s
        .command(
            "vectorize_image",
            json!({"compId":1,"layerId":media_layer,"maxColors":4}),
        )
        .unwrap();
    let order = state["project"]["comps"]["1"]["layer_order"]
        .as_array()
        .unwrap();
    assert!(
        order.len() >= 3,
        "original + at least 2 traced layers, got {}",
        order.len()
    );
    let comp = s.project.comp(CompId(1)).unwrap();
    let original = s.project.layer(CompId(1), media_layer).unwrap();
    assert!(!original.visible, "source pixels hidden after vectorizing");
    let traced_shapes = order
        .iter()
        .filter(|id| {
            let lid = LayerId(id.as_u64().expect("layer id"));
            matches!(
                &s.project.layer(CompId(1), lid).unwrap().kind,
                LayerKind::Shape { points, .. } if !points.is_empty()
            )
        })
        .count();
    assert!(traced_shapes >= 2, "teal + cream regions traced");
    // Traced layers inherit the source placement.
    for id in &comp.layer_order {
        let l = s.project.layer(CompId(1), *id).unwrap();
        if l.id != media_layer {
            assert_eq!(l.transform.scale, original.transform.scale);
        }
    }
    // Undo brings back the raster original whole.
    s.command("undo", json!({})).unwrap();
    assert!(s.project.layer(CompId(1), media_layer).unwrap().visible);
    assert_eq!(layer_count(&s), 1);
    // Rejects non-image layers.
    assert!(s
        .command(
            "vectorize_image",
            json!({"compId":1,"layerId":LayerId(999)})
        )
        .is_err());
}
