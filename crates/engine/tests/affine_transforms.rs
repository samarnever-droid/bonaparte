use bonaparte_engine::{render_comp, Affine2D, NoMedia};
use bonaparte_model::{
    CompId, FrameRate, Layer, LayerKind, Project, StaticTransform, Time, TICKS_PER_SEC,
};

fn create_test_project() -> (Project, CompId) {
    let mut p = Project::new("Affine Tests");
    let c = p.create_comp(
        "main",
        100,
        100,
        FrameRate::FPS_30,
        Time(10 * TICKS_PER_SEC),
    );
    p.comps.get_mut(&c).unwrap().background = [0.0, 0.0, 0.0, 1.0];
    (p, c)
}

#[test]
fn test_affine_identity_and_inversion() {
    let aff = Affine2D::identity();
    assert_eq!(aff.transform_point([10.0, 20.0]), [10.0, 20.0]);

    let inv = aff.inverse().unwrap();
    assert_eq!(inv, Affine2D::identity());

    let scale_rot = Affine2D {
        a: 0.0,
        b: 2.0,
        c: -2.0,
        d: 0.0,
        tx: 50.0,
        ty: 30.0,
    };
    let pt = [5.0, 10.0];
    let transformed = scale_rot.transform_point(pt);
    let recovered = scale_rot.inverse().unwrap().transform_point(transformed);

    assert!((recovered[0] - pt[0]).abs() < 1e-4);
    assert!((recovered[1] - pt[1]).abs() < 1e-4);
}

#[test]
fn test_affine_translation_moves_layer() {
    let (mut p, c) = create_test_project();
    let mut layer = Layer::new_rect("red_box", [1.0, 0.0, 0.0, 1.0], Time::ZERO, Time(100));
    layer.transform = StaticTransform {
        position: [20.0, 20.0],
        scale: [40.0, 40.0],
        rotation: 0.0,
        opacity: 1.0,
        anchor_point: [0.0, 0.0],
        z: 0.0,
    };
    p.insert_layer(c, layer);

    let frame = render_comp(&p, c, Time::ZERO, &NoMedia).unwrap();
    // Center of comp is (50, 50). Layer position is (+20, +20), so center is (70, 70).
    // Scale is 40% of 100 = 40x40 pixels, centered at (70, 70), spanning [50, 90] x [50, 90].
    let center_px = frame.pixel(70, 70);
    assert!(center_px[0] > 0.9, "Pixel at (70, 70) should be red");

    let outside_px = frame.pixel(10, 10);
    assert!(
        outside_px[0] < 0.1,
        "Pixel at (10, 10) should be background black"
    );
}

#[test]
fn test_affine_rotation_90_degrees() {
    let (mut p, c) = create_test_project();
    let mut layer = Layer::new_rect("rect", [0.0, 1.0, 0.0, 1.0], Time::ZERO, Time(100));
    // Non-uniform aspect ratio: 80% width, 20% height
    layer.transform = StaticTransform {
        position: [0.0, 0.0],
        scale: [80.0, 20.0],
        rotation: 90.0, // Rotated 90 deg -> now 20% width, 80% height!
        opacity: 1.0,
        anchor_point: [0.0, 0.0],
        z: 0.0,
    };
    p.insert_layer(c, layer);

    let frame = render_comp(&p, c, Time::ZERO, &NoMedia).unwrap();

    // Center is (50, 50).
    // Since it's rotated 90 degrees, vertical extent is 80 (spanning y from 10 to 90),
    // and horizontal extent is 20 (spanning x from 40 to 60).
    // So (50, 20) should be green!
    let v_px = frame.pixel(50, 20);
    assert!(
        v_px[1] > 0.9,
        "Pixel at (50, 20) along vertical axis should be green"
    );

    // But (80, 50) is along horizontal axis outside the rotated 20px width -> should be black!
    let h_px = frame.pixel(80, 50);
    assert!(
        h_px[1] < 0.1,
        "Pixel at (80, 50) along horizontal axis should be black"
    );
}

#[test]
fn test_affine_parent_hierarchy_transform() {
    let (mut p, c) = create_test_project();
    // Parent at (+10, +10), scale 100%
    let mut parent = Layer::new(
        "parent",
        LayerKind::Solid {
            color: [0.0, 0.0, 1.0, 1.0],
        },
        Time::ZERO,
        Time(100),
    );
    parent.transform = StaticTransform {
        position: [10.0, 10.0],
        scale: [50.0, 50.0],
        rotation: 0.0,
        opacity: 0.5,
        anchor_point: [0.0, 0.0],
        z: 0.0,
    };
    let parent_id = p.insert_layer(c, parent);

    // Child parented to parent at (+10, +10) local, scale 50%
    let mut child = Layer::new_rect("child", [1.0, 0.0, 0.0, 1.0], Time::ZERO, Time(100));
    child.parent = Some(parent_id);
    child.transform = StaticTransform {
        position: [10.0, 10.0],
        scale: [50.0, 50.0],
        rotation: 0.0,
        opacity: 0.8,
        anchor_point: [0.0, 0.0],
        z: 0.0,
    };
    p.insert_layer(c, child);

    let frame = render_comp(&p, c, Time::ZERO, &NoMedia).unwrap();
    // Child effective position: parent pos (10, 10) + parent scale (0.5) * child pos (10, 10) = (15, 15).
    // In comp coords: (50 + 15, 50 + 15) = (65, 65).
    let child_center = frame.pixel(65, 65);
    // Opacity combined: 0.5 * 0.8 = 0.4 red over background.
    assert!(
        child_center[0] > 0.1,
        "Child red component should be visible at effective location"
    );
}
