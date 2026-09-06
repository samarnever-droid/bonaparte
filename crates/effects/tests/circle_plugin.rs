use bonaparte_effects::{builtin_manifests, evaluate_circle};

#[test]
fn test_microkernel_circle_evaluator() {
    let frame = evaluate_circle(
        100,
        100,
        [1.0, 0.0, 0.0, 1.0],
        1.0,
        [0.0, 0.0, 1.0, 1.0],
        0.0,
    );
    let center = frame.get_pixel_u8(50, 50);
    assert!(
        center[0] > 200 && center[3] > 200,
        "Center should be red, got: {:?}",
        center
    );

    let corner = frame.get_pixel_u8(5, 5);
    assert_eq!(
        corner[3], 0,
        "Corner (5, 5) must be transparent, got: {:?}",
        corner
    );
}

#[test]
fn test_circle_manifest_registered() {
    let manifests = builtin_manifests();
    let circle = manifests.iter().find(|m| m.id == "builtin.circle");
    assert!(
        circle.is_some(),
        "builtin.circle manifest should be registered"
    );
    let c = circle.unwrap();
    assert_eq!(c.name, "Circle Shape");
    assert!(
        c.inputs.is_empty(),
        "Generator plugin should have zero inputs"
    );
    assert!(c.params.iter().any(|p| p.id == "color"));
    assert!(c.params.iter().any(|p| p.id == "radius"));
    assert!(c.params.iter().any(|p| p.id == "stroke_color"));
    assert!(c.params.iter().any(|p| p.id == "stroke_width"));
}
