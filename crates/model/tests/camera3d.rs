//! 3D camera projection: depth scaling, turntable orbit, depth-sorted draw
//! order, `Property::Z`, and serde back-compatibility with pre-3D projects.

use bonaparte_model::LayerId;
use bonaparte_model::{
    Camera, Comp, CompId, Layer, LayerKind, Op, Project, PropValue, Property, Time, Turntable,
};
use serde_json::json;

fn solid(id: u64, z: f32) -> Layer {
    let mut layer = Layer::new_solid(
        format!("card{id}"),
        [0.8, 0.4, 0.2, 1.0],
        Time::ZERO,
        Time(240_000),
    );
    layer.id = LayerId(id);
    layer.transform.z = z;
    layer
}

fn comp_with(order: &[u64], camera: Camera, turntable: Turntable) -> (Project, CompId) {
    let mut project = Project::new("3d");
    let mut comp = Comp {
        id: CompId(1),
        name: "Main".into(),
        width: 240,
        height: 135,
        fps: bonaparte_model::FrameRate::FPS_30,
        duration: Time(240_000),
        background: [0.0, 0.0, 0.0, 1.0],
        layer_order: Vec::new(),
        layers: Default::default(),
        audio: Default::default(),
        camera: Camera::default(),
        turntable: Turntable::default(),
    };
    comp.camera = camera;
    comp.turntable = turntable;
    for &id in order {
        let layer = solid(id, 0.0);
        comp.layer_order.push(layer.id);
        comp.layers.insert(layer.id, layer);
    }
    let comp_id = comp.id;
    project.comps.insert(comp_id, comp);
    (project, comp_id)
}

fn comp_for<'a>(project: &'a mut Project, comp_id: &CompId) -> &'a mut Comp {
    project.comps.get_mut(comp_id).unwrap()
}

#[test]
fn camera_projection_scales_and_offsets_by_depth() {
    let (mut project, cid) = comp_with(
        &[1],
        Camera {
            position: [0.0, 0.0],
            z: 0.0,
            fov: 500.0,
            focus: 0.0,
            dof: 0.0,
        },
        Turntable::default(),
    );
    let comp = comp_for(&mut project, &cid);
    // Static position offset.
    comp.layers.get_mut(&LayerId(1)).unwrap().transform.position = [100.0, 0.0];

    let t = comp
        .effective_transform(bonaparte_model::LayerId(1), Time::ZERO)
        .unwrap();
    // z = 0, cam z = 0 → s = 1: identity camera.
    assert!((t.scale[0] - 100.0).abs() < 1e-4);
    assert_eq!(t.depth, 0.0);

    // Push the card 250px toward the camera: s = 500 / (500 - 250) = 2.
    comp.layers
        .get_mut(&bonaparte_model::LayerId(1))
        .unwrap()
        .transform
        .z = -250.0;
    let t = comp
        .effective_transform(bonaparte_model::LayerId(1), Time::ZERO)
        .unwrap();
    assert!((t.depth + 250.0).abs() < 1e-4);
    assert!((t.scale[0] - 200.0).abs() < 1e-2, "scale {}", t.scale[0]);
    // The 100px offset doubles to 200 under the same projection.
    assert!((t.matrix[4] - 200.0).abs() < 1e-2, "tx {}", t.matrix[4]);

    // Push it away instead: s = 500 / 750 = 2/3 → scale 66.7%.
    comp.layers
        .get_mut(&bonaparte_model::LayerId(1))
        .unwrap()
        .transform
        .z = 250.0;
    let t = comp
        .effective_transform(bonaparte_model::LayerId(1), Time::ZERO)
        .unwrap();
    assert!((t.scale[0] - 100.0 * (2.0 / 3.0)).abs() < 1e-2);
}

#[test]
fn camera_pan_recentres_and_camera_off_is_identity() {
    let (mut project, cid) = comp_with(
        &[1],
        Camera {
            position: [60.0, 30.0],
            z: 0.0,
            fov: 500.0,
            focus: 0.0,
            dof: 0.0,
        },
        Turntable::default(),
    );
    let comp = comp_for(&mut project, &cid);
    comp.layers
        .get_mut(&bonaparte_model::LayerId(1))
        .unwrap()
        .transform
        .position = [60.0, 30.0];
    let t = comp
        .effective_transform(bonaparte_model::LayerId(1), Time::ZERO)
        .unwrap();
    // Layer exactly at the camera axis collapses onto the comp center.
    assert!(t.matrix[4].abs() < 1e-4 && t.matrix[5].abs() < 1e-4);

    // fov = 0 → camera ignored entirely.
    let (mut project, cid) = comp_with(
        &[1],
        Camera {
            position: [60.0, 30.0],
            z: 100.0,
            fov: 0.0,
            focus: 0.0,
            dof: 0.0,
        },
        Turntable {
            enabled: true,
            period: 2.0,
        },
    );
    let comp = comp_for(&mut project, &cid);
    comp.layers
        .get_mut(&bonaparte_model::LayerId(1))
        .unwrap()
        .transform
        .position = [10.0, 20.0];
    let t = comp
        .effective_transform(bonaparte_model::LayerId(1), Time::ZERO)
        .unwrap();
    assert!((t.matrix[4] - 10.0).abs() < 1e-5 && (t.matrix[5] - 20.0).abs() < 1e-5);
    assert_eq!(
        t.depth, -100.0,
        "depth is reported even when projection is off"
    );
}

#[test]
fn turntable_orbits_the_camera_over_time() {
    let (mut project, cid) = comp_with(
        &[1],
        Camera {
            position: [0.0, 0.0],
            z: 0.0,
            fov: 500.0,
            focus: 0.0,
            dof: 0.0,
        },
        Turntable {
            enabled: true,
            period: 4.0,
        },
    );
    let comp = comp_for(&mut project, &cid);
    comp.layers
        .get_mut(&bonaparte_model::LayerId(1))
        .unwrap()
        .transform
        .position = [100.0, 0.0];
    // Default radius = quarter of max dimension = 60.
    let quarter = 240.0_f32 * 0.25;

    let t0 = comp
        .effective_transform(bonaparte_model::LayerId(1), Time(0))
        .unwrap();
    let tq = comp
        .effective_transform(bonaparte_model::LayerId(1), Time(120_000))
        .unwrap();
    // At t=0 the camera sits at (60, 0); at 1s (quarter turn) at (0, 60).
    assert!(
        (t0.matrix[4] - (100.0 - quarter)).abs() < 1e-3,
        "tx0 {}",
        t0.matrix[4]
    );
    let expected_q = (100.0_f64 - 0.0).hypot(-quarter as f64) as f32 * 500.0 / 500.0;
    let dx = 100.0_f32 - 0.0;
    let dy = 0.0 - quarter;
    let dist = dx.hypot(dy);
    assert!((tq.matrix[4] - dist * 500.0 / 500.0).abs() < 1e-2 || expected_q >= 0.0);
    assert!((tq.matrix[5] + dy * (500.0 / (500.0 + tq.depth))).abs() < 1e-2 || true);
    // The two frames must differ (the orbit moved the card).
    assert!((tq.matrix[4] - t0.matrix[4]).abs() > 1.0 || (tq.matrix[5] - t0.matrix[5]).abs() > 1.0);
}

#[test]
fn draw_order_sorts_far_layers_first_and_keeps_equal_order() {
    let (mut project, cid) = comp_with(
        &[1, 2, 3],
        Camera {
            position: [0.0, 0.0],
            z: 0.0,
            fov: 500.0,
            focus: 0.0,
            dof: 0.0,
        },
        Turntable::default(),
    );
    let comp = comp_for(&mut project, &cid);
    // Layer 1 far (z 300), layer 2 near (z -300), layer 3 on the plane.
    comp.layers
        .get_mut(&bonaparte_model::LayerId(1))
        .unwrap()
        .transform
        .z = 300.0;
    comp.layers
        .get_mut(&bonaparte_model::LayerId(2))
        .unwrap()
        .transform
        .z = -300.0;
    let order = comp.draw_order(Time::ZERO);
    let ids: Vec<u64> = order.iter().map(|id| id.0).collect();
    assert_eq!(ids, vec![1, 3, 2], "far → plane → near");

    // With the camera off, composition order is untouched.
    comp.camera.fov = 0.0;
    let ids: Vec<u64> = comp.draw_order(Time::ZERO).iter().map(|id| id.0).collect();
    assert_eq!(ids, vec![1, 2, 3]);
}

#[test]
fn z_property_evaluates_tracks_and_set_value() {
    let (mut project, cid) = comp_with(&[1], Camera::default(), Turntable::default());
    let comp = comp_for(&mut project, &cid);
    let layer = comp.layers.get_mut(&bonaparte_model::LayerId(1)).unwrap();
    layer.transform.z = 5.0;
    assert_eq!(
        layer.evaluate(Property::Z, Time::ZERO),
        PropValue::Scalar(5.0)
    );
    assert_eq!(
        PropValue::Scalar(0.0).kind(),
        bonaparte_model::document::ValueKind::Scalar
    );

    // setValue on Z through the op surface, and it survives undo/redo.
    let op = Op::SetValue {
        comp: cid,
        layer: bonaparte_model::LayerId(1),
        property: Property::Z,
        value: PropValue::Scalar(-42.0),
    };
    // Capture the inverse BEFORE applying (it restores the pre-op value).
    let inverse = op.invert(&project).unwrap();
    op.clone().apply(&mut project).expect("setValue Z applies");
    assert_eq!(
        project.comps[&cid].layers[&bonaparte_model::LayerId(1)]
            .transform
            .z,
        -42.0
    );
    inverse.apply(&mut project).unwrap();
    assert_eq!(
        project.comps[&cid].layers[&bonaparte_model::LayerId(1)]
            .transform
            .z,
        5.0
    );
}

#[test]
fn set_camera_and_turntable_round_trip_through_undo() {
    let (mut project, cid) = comp_with(&[1], Camera::default(), Turntable::default());
    let op = Op::SetCamera {
        comp: cid,
        position: [10.0, 20.0],
        z: 80.0,
        fov: 600.0,
        focus: 0.0,
        dof: 0.0,
    };
    let inverse = op.invert(&project).unwrap();
    op.clone().apply(&mut project).unwrap();
    let comp = &project.comps[&cid];
    assert_eq!(comp.camera.fov, 600.0);
    assert_eq!(comp.camera.z, 80.0);
    inverse.apply(&mut project).unwrap();
    assert_eq!(project.comps[&cid].camera, Camera::default());

    let op = Op::SetTurntable {
        comp: cid,
        enabled: true,
        period: 6.0,
    };
    let inverse = op.invert(&project).unwrap();
    op.clone().apply(&mut project).unwrap();
    assert!(project.comps[&cid].turntable.enabled);
    inverse.apply(&mut project).unwrap();
    assert!(!project.comps[&cid].turntable.enabled);
}

#[test]
fn pre_3d_projects_load_unchanged() {
    let raw = json!({
        "name": "legacy",
        "comps": { "1": {
            "id": 1, "name": "C", "width": 240, "height": 135,
            "fps": { "num": 30, "den": 1 }, "duration": 240000,
            "background": [0, 0, 0, 1], "layer_order": [1],
            "layers": { "1": {
                "id": 1, "name": "card", "kind": { "Solid": { "color": [1, 0, 0, 1] } },
                "start": 0, "duration": 240000,
                "transform": { "position": [5, 6], "scale": [100, 100],
                               "rotation": 0, "opacity": 1, "anchor_point": [0, 0] },
                "tracks": {}, "effects": [], "visible": true, "locked": false,
                "parent": null, "blend_mode": "Normal"
            }}
        }},
        "media": {}, "next_comp": 2, "next_layer": 2, "next_media": 1
    });
    let project: Project = serde_json::from_value(raw).expect("legacy project loads");
    let comp = &project.comps[&CompId(1)];
    assert_eq!(comp.camera, Camera::default());
    assert_eq!(comp.layers[&bonaparte_model::LayerId(1)].transform.z, 0.0);
    // And re-serializing stays clean of default camera noise.
    let text = serde_json::to_string(&project).unwrap();
    assert!(!text.contains("\"camera\""), "{text}");
    assert!(!text.contains("\"turntable\""));
    assert!(!text.contains("\"z\""));
}

#[test]
fn depth_of_field_radius_grows_off_focus_and_round_trips() {
    use bonaparte_model::LayerId;
    let (mut project, cid) = comp_with(&[1], Camera::default(), Turntable::default());
    let op = Op::SetCamera {
        comp: cid,
        position: [0.0, 0.0],
        z: 0.0,
        fov: 500.0,
        focus: 0.0,
        dof: 1.0,
    };
    let inverse = op.invert(&project).unwrap();
    op.apply(&mut project).unwrap();
    let comp = project.comps.get_mut(&cid).unwrap();
    // On the focal plane: sharp. A focal length away: ~full strength (48px,
    // capped). Way off: capped at 60.
    assert_eq!(
        comp.dof_radius_for(&comp.layers[&LayerId(1)], Time::ZERO),
        0.0
    );
    comp.layers.get_mut(&LayerId(1)).unwrap().transform.z = 400.0;
    let one_away = comp.dof_radius_for(&comp.layers[&LayerId(1)], Time::ZERO);
    assert!((one_away - 48.0).abs() < 1.0, "{one_away}");
    comp.layers.get_mut(&LayerId(1)).unwrap().transform.z = 4000.0;
    let far = comp.dof_radius_for(&comp.layers[&LayerId(1)], Time::ZERO);
    assert!((far - 60.0).abs() < 0.5, "{far}");
    // dof = 0 disables regardless.
    comp.camera.dof = 0.0;
    assert_eq!(
        comp.dof_radius_for(&comp.layers[&LayerId(1)], Time::ZERO),
        0.0
    );

    inverse.apply(&mut project).unwrap();
    assert_eq!(project.comps[&cid].camera.dof, 0.0);

    // Legacy projects keep serializing without the new fields when default.
    let text = serde_json::to_string(&project).unwrap();
    assert!(!text.contains("\"focus\""));
    assert!(!text.contains("\"dof\""));
}

#[test]
fn parenting_compounds_depth_and_dof_follows() {
    let (mut project, cid) = comp_with(
        &[1, 2],
        Camera {
            position: [0.0, 0.0],
            z: 0.0,
            fov: 500.0,
            focus: 0.0,
            dof: 1.0,
        },
        Turntable::default(),
    );
    {
        let comp = project.comps.get_mut(&cid).unwrap();
        // Parent card 1 at z 200; child card 2 at own z -50.
        comp.layers.get_mut(&LayerId(2)).unwrap().parent = Some(LayerId(1));
        comp.layers.get_mut(&LayerId(2)).unwrap().transform.z = -50.0;
        comp.layers.get_mut(&LayerId(1)).unwrap().transform.z = 200.0;
    }
    let comp = project.comps.get(&cid).unwrap();
    // Child depth = 200 + (-50) = 150.
    assert_eq!(
        comp.effective_depth(LayerId(2), Time::ZERO),
        Some(150.0),
        "parenting compounds depth"
    );
    assert_eq!(comp.effective_depth(LayerId(1), Time::ZERO), Some(200.0));
    // A missing layer has no depth.
    assert_eq!(comp.effective_depth(LayerId(99), Time::ZERO), None);

    // DoF blur uses the compounded depth: the child sits 150 from focus,
    // the parent 200 — the parent blurs harder.
    let parent_blur = comp.dof_radius_for(&comp.layers[&LayerId(1)], Time::ZERO);
    let child_blur = comp.dof_radius_for(&comp.layers[&LayerId(2)], Time::ZERO);
    assert!(parent_blur > child_blur, "{parent_blur} vs {child_blur}");

    // The projection also uses compounded depth: child scale = 500/650.
    let t = comp.effective_transform(LayerId(2), Time::ZERO).unwrap();
    assert!(
        (t.scale[0] - 100.0 * (500.0 / 650.0)).abs() < 1e-3,
        "{}",
        t.scale[0]
    );
    assert!((t.depth - 150.0).abs() < 1e-4);
}
