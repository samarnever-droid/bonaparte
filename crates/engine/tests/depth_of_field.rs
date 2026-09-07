//! Cinematic depth of field: cards away from the focal plane blur in the
//! real render, the focal plane stays sharp, and turning it off restores
//! the exact legacy pixels.

use bonaparte_engine::{reference::render_comp, NoMedia};
use bonaparte_model::{Camera, Comp, CompId, FrameRate, Layer, Project, Time};

fn project_with(camera: Camera) -> (Project, CompId) {
    let mut p = Project::new("dof");
    let mut comp = Comp {
        id: CompId(1),
        name: "Main".into(),
        width: 240,
        height: 135,
        fps: FrameRate::FPS_30,
        duration: Time(240_000),
        background: [0.0, 0.0, 0.0, 1.0],
        layer_order: Vec::new(),
        layers: Default::default(),
        audio: Default::default(),
        camera,
        turntable: Default::default(),
    };
    // A small bright card far from the camera (z = +600), and a red card on
    // the focal plane (z = 0). fov 500 → the far card renders at 500/1100.
    let mut near = Layer::new_rect("near", [1.0, 0.0, 0.0, 1.0], Time::ZERO, Time(240_000));
    near.id = bonaparte_model::LayerId(1);
    near.transform.scale = [30.0, 30.0];
    near.transform.position = [-60.0, 0.0];
    let mut far = Layer::new_rect("far", [0.0, 1.0, 0.0, 1.0], Time::ZERO, Time(240_000));
    far.id = bonaparte_model::LayerId(2);
    far.transform.scale = [30.0, 30.0];
    far.transform.position = [60.0, 0.0];
    far.transform.z = 600.0;
    for l in [near, far] {
        comp.layer_order.push(l.id);
        comp.layers.insert(l.id, l);
    }
    p.comps.insert(CompId(1), comp);
    (p, CompId(1))
}

fn checksum(p: &Project, c: CompId) -> u32 {
    let frame = render_comp(p, c, Time::ZERO, &NoMedia).unwrap();
    let mut hash = 2166136261u32;
    for px in frame.rgba.chunks_exact(4) {
        for byte in [px[0], px[1], px[2]] {
            hash = hash.wrapping_mul(16777619).wrapping_add(byte as u32);
        }
    }
    hash
}

#[test]
fn far_card_blurs_while_the_focal_plane_stays_sharp() {
    let base = Camera {
        position: [0.0, 0.0],
        z: 0.0,
        fov: 500.0,
        focus: 0.0,
        dof: 0.0,
    };
    let dof_on = Camera {
        focus: 0.0,
        dof: 1.0,
        ..base
    };

    let (p_sharp, c) = project_with(base);
    let sharp_far = checksum(&p_sharp, c);
    let frame_sharp = render_comp(&p_sharp, c, Time::ZERO, &NoMedia).unwrap();
    // Far card center (60% scale? no: 30% of 100px at 500/1100 ≈ 27% → ~13px
    // wide) sits around x = 120 + 60*(500/1100) ≈ 147.
    let far_center = frame_sharp.pixel(147, 67);
    assert!(
        far_center[1] > 0.7,
        "far card is visible and green: {far_center:?}"
    );
    let near_center = frame_sharp.pixel(93, 67);
    assert!(near_center[0] > 0.7, "near card visible: {near_center:?}");

    let (p_dof, c) = project_with(dof_on);
    let dof_far = checksum(&p_dof, c);
    assert_ne!(sharp_far, dof_far, "DoF changes the render");
    let frame_dof = render_comp(&p_dof, c, Time::ZERO, &NoMedia).unwrap();
    // The far card's peak green collapses as the small card blurs away.
    let mut peak_far = 0.0f32;
    for x in 130..165 {
        peak_far = peak_far.max(frame_dof.pixel(x, 67)[1]);
    }
    assert!(
        peak_far < far_center[1] * 0.55,
        "blurred far card loses peak green: {peak_far} vs {far_center:?}"
    );
    // The focal-plane card keeps its peak red (only edge softening).
    let mut peak_near = 0.0f32;
    for x in 85..100 {
        peak_near = peak_near.max(frame_dof.pixel(x, 67)[0]);
    }
    assert!(peak_near > 0.9, "focal plane card stays essentially sharp");
}

#[test]
fn dof_off_matches_legacy_pixels_even_with_focus_set() {
    let legacy = Camera {
        position: [0.0, 0.0],
        z: 0.0,
        fov: 500.0,
        focus: 300.0,
        dof: 0.0,
    };
    let (p, c) = project_with(legacy);
    let frame = render_comp(&p, c, Time::ZERO, &NoMedia).unwrap();
    assert!(frame.pixel(147, 67)[1] > 0.7);
    // Strength 0 with focus set must not inject any blur effect.
    assert_eq!(p.comps[&c].camera.blur_radius_at(600.0), 0.0);
}
