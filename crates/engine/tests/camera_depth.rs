//! 3D depth: the perspective camera scales layers by depth and the renderer
//! paints far cards behind near ones regardless of composition order.

use bonaparte_engine::{reference::render_comp, NoMedia};
use bonaparte_model::{Camera, Comp, CompId, FrameRate, Layer, Project, Time, Turntable};

fn comp_scale(p: &mut Project, c: CompId, id: u64, scale: [f32; 2]) {
    p.comps
        .get_mut(&c)
        .unwrap()
        .layers
        .get_mut(&bonaparte_model::LayerId(id))
        .unwrap()
        .transform
        .scale = scale;
}

fn project_with(layers: &[(u64, f32, [f32; 2])], camera: Camera) -> (Project, CompId) {
    let mut p = Project::new("depth");
    let mut comp = Comp {
        id: CompId(1),
        name: "Main".into(),
        width: 120,
        height: 120,
        fps: FrameRate::FPS_30,
        duration: Time(240_000),
        background: [0.0, 0.0, 0.0, 1.0],
        layer_order: Vec::new(),
        layers: Default::default(),
        audio: Default::default(),
        camera,
        turntable: Turntable::default(),
    };
    for &(id, z, pos) in layers {
        let mut layer = Layer::new_solid(
            format!("card{id}"),
            if id == 1 {
                [1.0, 0.0, 0.0, 1.0]
            } else {
                [0.0, 1.0, 0.0, 1.0]
            },
            Time::ZERO,
            Time(240_000),
        );
        layer.id = bonaparte_model::LayerId(id);
        layer.transform.scale = [60.0, 60.0]; // 60% of 100px solid = 60x60
        layer.transform.position = pos;
        layer.transform.z = z;
        comp.layer_order.push(layer.id);
        comp.layers.insert(layer.id, layer);
    }
    p.comps.insert(CompId(1), comp);
    (p, CompId(1))
}

#[test]
fn near_card_paints_over_far_card_regardless_of_order() {
    // Red is composed BELOW green (layer_order [1, 2]) but sits near the
    // camera; green is far. Red (40% card, 2x near) spans 20..100; green
    // (60% card, 2/3 far, at +80) spans 93..133 — both win somewhere.
    let (mut p, c) = project_with(
        &[(1, -200.0, [0.0, 0.0]), (2, 200.0, [80.0, 0.0])],
        Camera {
            position: [0.0, 0.0],
            z: 0.0,
            fov: 400.0,
            focus: 0.0,
            dof: 0.0,
        },
    );
    comp_scale(&mut p, c, 1, [40.0, 40.0]);
    let frame = render_comp(&p, c, Time::ZERO, &NoMedia).unwrap();
    assert_eq!(frame.pixel(60, 60)[0], 1.0, "near red covers the center");
    assert_eq!(frame.pixel(112, 60)[1], 1.0, "far green visible past red");

    // Composed order (red first) without a camera: green covers red.
    let (mut p, c) = project_with(
        &[(1, -200.0, [0.0, 0.0]), (2, 200.0, [80.0, 0.0])],
        Camera::default(),
    );
    comp_scale(&mut p, c, 1, [40.0, 40.0]);
    let frame = render_comp(&p, c, Time::ZERO, &NoMedia).unwrap();
    assert_eq!(frame.pixel(115, 60)[1], 1.0, "legacy stacking unchanged");
}

#[test]
fn depth_scaling_shrinks_far_layers_in_the_render() {
    // Green card at z=+600 with fov 300 renders at half size (30px wide).
    let (p, c) = project_with(
        &[(2, 600.0, [0.0, 0.0])],
        Camera {
            position: [0.0, 0.0],
            z: 0.0,
            fov: 300.0,
            focus: 0.0,
            dof: 0.0,
        },
    );
    let frame = render_comp(&p, c, Time::ZERO, &NoMedia).unwrap();
    assert_eq!(frame.pixel(60, 60)[1], 1.0, "center is green");
    // Half-size card spans ±15px: (60+18) is outside.
    assert_eq!(frame.pixel(78, 60)[1], 0.0, "card is only half size");
}
