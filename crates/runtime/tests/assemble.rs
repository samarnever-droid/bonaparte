//! Assembled SVG import + disassemble: SVGs arrive as ONE object
//! (a PreComp group), right-click Disassemble explodes them back into
//! layers with composed transforms — and undo restores the group.

use bonaparte_engine::reference::render_comp;
use bonaparte_model::*;
use bonaparte_runtime::{decode_embedded_frames, EditorSession};
use serde_json::json;

const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="120" viewBox="0 0 200 120">
  <rect x="10" y="10" width="80" height="60" fill="#ff5555"/>
  <circle cx="150" cy="60" r="40" fill="#5588ff"/>
</svg>"##;

fn session() -> EditorSession {
    let mut p = Project::new("Assemble");
    p.create_comp("Main", 400, 300, FrameRate::FPS_30, Time(TICKS_PER_SEC));
    EditorSession::new(p).unwrap()
}

fn import_svg(s: &mut EditorSession) {
    s.command(
        "import_svg",
        json!({"name": "logo.svg", "svg": SVG, "compId": 1}),
    )
    .expect("svg import");
}

#[test]
fn svg_imports_as_one_assembled_object() {
    let mut s = session();
    import_svg(&mut s);

    let comp = s.project.comp(CompId(1)).unwrap();
    assert_eq!(
        comp.layer_order.len(),
        1,
        "the comp holds ONE assembled layer"
    );
    let layer = &comp.layers[&comp.layer_order[0]];
    let LayerKind::PreComp { comp: child_id } = &layer.kind else {
        panic!("expected PreComp group, got {:?}", layer.kind);
    };
    // The group carries the fit scale; the artwork stays untouched inside.
    assert!(layer.transform.scale[0] <= 200.0);
    let child = s.project.comp(*child_id).expect("sub composition");
    assert_eq!(child.layer_order.len(), 2, "rect + circle inside");
    let child_scale = child.layers[&child.layer_order[0]].transform.scale;
    assert_eq!(child_scale, [100.0, 100.0], "inner art is unscaled");

    // The assembled object renders (engine accepts the group).
    let frames = decode_embedded_frames(&s.project).unwrap();
    let rendered = render_comp(&s.project, CompId(1), Time::ZERO, &frames);
    assert!(rendered.is_ok(), "group renders: {:?}", rendered.err());
}

#[test]
fn disassemble_explodes_with_composed_transforms_and_one_undo() {
    let mut s = session();
    import_svg(&mut s);
    let comp = s.project.comp(CompId(1)).unwrap();
    let group_id = comp.layer_order[0];
    let group_scale = comp.layers[&group_id].transform.scale[0];
    let comps_before = s.project.comps.len();
    let history_before = s.snapshot().history.len();

    let reply = s
        .command("disassemble", json!({"compId": 1, "layerId": group_id.0}))
        .expect("disassemble");
    assert!(reply.get("error").is_none());

    // Group gone, children in, sub-comp removed.
    let comp = s.project.comp(CompId(1)).unwrap();
    assert_eq!(comp.layer_order.len(), 2, "rect + circle now siblings");
    assert_eq!(s.project.comps.len(), comps_before - 1);
    for id in &comp.layer_order {
        let layer = &comp.layers[id];
        assert!(
            matches!(layer.kind, LayerKind::Shape { .. }),
            "exploded into shapes"
        );
        // The fit scale composed down onto the children.
        assert!(
            (layer.transform.scale[0] - group_scale).abs() < 0.01,
            "child carries the group's fit scale"
        );
        assert!(layer.name.contains("logo.svg"), "traceable names");
    }

    // ONE undo restores the assembled group + the sub-comp.
    s.command("undo", json!({})).unwrap();
    let comp = s.project.comp(CompId(1)).unwrap();
    assert_eq!(comp.layer_order.len(), 1);
    assert!(matches!(
        comp.layers[&comp.layer_order[0]].kind,
        LayerKind::PreComp { .. }
    ));
    assert_eq!(s.project.comps.len(), comps_before);
    assert_eq!(s.snapshot().history.len(), history_before);
}

#[test]
fn disassemble_rejects_plain_layers_helpfully() {
    let mut s = session();
    let mut layer = Layer::new(
        "plain",
        LayerKind::Solid { color: [1.0; 4] },
        Time::ZERO,
        Time(TICKS_PER_SEC),
    );
    layer.id = LayerId(1);
    s.command(
        "apply",
        json!({"op": {"type": "addLayer", "comp": 1, "layer": {
            "id": 1, "name": "plain", "kind": {"Solid": {"color": [1,1,1,1]}},
            "start": 0, "duration": 120000,
            "transform": {"position": [0,0], "scale": [100,100], "rotation": 0, "opacity": 1, "anchor_point": [0,0]},
            "tracks": {}, "effects": [], "visible": true, "locked": false, "parent": null, "blend_mode": "Normal"
        }}}),
    )
    .unwrap();
    let err = s
        .command("disassemble", json!({"compId": 1, "layerId": layer.id.0}))
        .unwrap_err();
    assert!(err.contains("assembled"), "{err}");
}
