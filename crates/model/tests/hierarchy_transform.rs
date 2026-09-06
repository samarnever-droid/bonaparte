use bonaparte_model::*;

#[test]
fn test_blend_mode_display_and_defaults() {
    let default_layer = Layer::new(
        "Test",
        LayerKind::Solid { color: [1.0; 4] },
        Time::ZERO,
        Time(100),
    );
    assert_eq!(default_layer.blend_mode, BlendMode::Normal);
    assert!(default_layer.visible);
    assert!(!default_layer.locked);
    assert_eq!(default_layer.parent, None);
    assert_eq!(default_layer.transform.anchor_point, [0.0, 0.0]);

    assert_eq!(BlendMode::Normal.to_string(), "Normal");
    assert_eq!(BlendMode::Multiply.to_string(), "Multiply");
    assert_eq!(BlendMode::Screen.to_string(), "Screen");
    assert_eq!(BlendMode::Overlay.to_string(), "Overlay");
    assert_eq!(BlendMode::Add.to_string(), "Add");
    assert_eq!(BlendMode::Darken.to_string(), "Darken");
    assert_eq!(BlendMode::Lighten.to_string(), "Lighten");
    assert_eq!(BlendMode::Difference.to_string(), "Difference");
}

#[test]
fn test_single_layer_effective_transform() {
    let mut p = Project::new("Single Layer Test");
    let comp = p.create_comp(
        "Comp",
        1920,
        1080,
        FrameRate::FPS_30,
        Time(10 * TICKS_PER_SEC),
    );
    let mut layer = Layer::new(
        "Layer",
        LayerKind::Solid { color: [1.0; 4] },
        Time::ZERO,
        Time(10 * TICKS_PER_SEC),
    );
    layer.transform.position = [100.0, 50.0];
    layer.transform.scale = [150.0, 150.0];
    layer.transform.rotation = 90.0;
    layer.transform.opacity = 0.8;
    layer.transform.anchor_point = [10.0, 10.0];

    let layer_id = p.insert_layer(comp, layer);
    let c = p.comp(comp).unwrap();

    let eff = c.effective_transform(layer_id, Time::ZERO).unwrap();
    assert_eq!(eff.scale, [150.0, 150.0]);
    assert_eq!(eff.rotation, 90.0);
    assert_eq!(eff.opacity, 0.8);
    assert_eq!(eff.anchor_point, [10.0, 10.0]);
    assert!((eff.position[0] - 100.0).abs() < 1e-4);
    assert!((eff.position[1] - 50.0).abs() < 1e-4);

    // Transforming the anchor point [10.0, 10.0] in layer space must land at [100.0, 50.0] in comp space
    let pt = eff.transform_point([10.0, 10.0]);
    assert!((pt[0] - 100.0).abs() < 1e-4);
    assert!((pt[1] - 50.0).abs() < 1e-4);
}

#[test]
fn test_parent_child_hierarchy_transform() {
    let mut p = Project::new("Hierarchy Test");
    let comp = p.create_comp(
        "Comp",
        1920,
        1080,
        FrameRate::FPS_30,
        Time(10 * TICKS_PER_SEC),
    );

    // Parent layer at position [200.0, 100.0], scale 200%, opacity 0.5
    let mut parent = Layer::new(
        "Parent",
        LayerKind::Solid { color: [1.0; 4] },
        Time::ZERO,
        Time(10 * TICKS_PER_SEC),
    );
    parent.transform.position = [200.0, 100.0];
    parent.transform.scale = [200.0, 200.0];
    parent.transform.opacity = 0.5;
    let parent_id = p.insert_layer(comp, parent);

    // Child layer at local position [50.0, 0.0], scale 150%, opacity 0.8, parented to Parent
    let mut child = Layer::new(
        "Child",
        LayerKind::Solid { color: [1.0; 4] },
        Time::ZERO,
        Time(10 * TICKS_PER_SEC),
    );
    child.transform.position = [50.0, 0.0];
    child.transform.scale = [150.0, 150.0];
    child.transform.opacity = 0.8;
    child.parent = Some(parent_id);
    let child_id = p.insert_layer(comp, child);

    let c = p.comp(comp).unwrap();

    let eff_child = c.effective_transform(child_id, Time::ZERO).unwrap();
    // Combined scale = 200% * 150% = 300%
    assert_eq!(eff_child.scale, [300.0, 300.0]);
    // Combined opacity = 0.5 * 0.8 = 0.4
    assert!((eff_child.opacity - 0.4).abs() < 1e-5);
    // Effective position = parent pos [200, 100] + parent scale (2.0) * child pos [50, 0] = [300.0, 100.0]
    assert!((eff_child.position[0] - 300.0).abs() < 1e-4);
    assert!((eff_child.position[1] - 100.0).abs() < 1e-4);
}

#[test]
fn test_three_level_deep_hierarchy() {
    let mut p = Project::new("Deep Hierarchy Test");
    let comp = p.create_comp(
        "Comp",
        1920,
        1080,
        FrameRate::FPS_30,
        Time(10 * TICKS_PER_SEC),
    );

    // L1: Grandparent (translated 100, 100; rotated 45 deg)
    let mut l1 = Layer::new(
        "L1",
        LayerKind::Solid { color: [1.0; 4] },
        Time::ZERO,
        Time(10 * TICKS_PER_SEC),
    );
    l1.transform.position = [100.0, 100.0];
    l1.transform.rotation = 45.0;
    let l1_id = p.insert_layer(comp, l1);

    // L2: Parent (local pos 50, 0; rotated 45 deg)
    let mut l2 = Layer::new(
        "L2",
        LayerKind::Solid { color: [1.0; 4] },
        Time::ZERO,
        Time(10 * TICKS_PER_SEC),
    );
    l2.transform.position = [50.0, 0.0];
    l2.transform.rotation = 45.0;
    l2.parent = Some(l1_id);
    let l2_id = p.insert_layer(comp, l2);

    // L3: Child (local pos 0, 0; rotated 0 deg)
    let mut l3 = Layer::new(
        "L3",
        LayerKind::Solid { color: [1.0; 4] },
        Time::ZERO,
        Time(10 * TICKS_PER_SEC),
    );
    l3.transform.position = [0.0, 0.0];
    l3.parent = Some(l2_id);
    let l3_id = p.insert_layer(comp, l3);

    let c = p.comp(comp).unwrap();
    let eff3 = c.effective_transform(l3_id, Time::ZERO).unwrap();

    // Total rotation = 45 + 45 = 90 deg
    assert!((eff3.rotation - 90.0).abs() < 1e-4);
}

#[test]
fn test_cycle_detection() {
    let mut p = Project::new("Cycle Test");
    let comp = p.create_comp(
        "Comp",
        1920,
        1080,
        FrameRate::FPS_30,
        Time(10 * TICKS_PER_SEC),
    );

    let l1 = p.insert_layer(
        comp,
        Layer::new(
            "L1",
            LayerKind::Solid { color: [1.0; 4] },
            Time::ZERO,
            Time(10 * TICKS_PER_SEC),
        ),
    );
    let l2 = p.insert_layer(
        comp,
        Layer::new(
            "L2",
            LayerKind::Solid { color: [1.0; 4] },
            Time::ZERO,
            Time(10 * TICKS_PER_SEC),
        ),
    );
    let l3 = p.insert_layer(
        comp,
        Layer::new(
            "L3",
            LayerKind::Solid { color: [1.0; 4] },
            Time::ZERO,
            Time(10 * TICKS_PER_SEC),
        ),
    );

    let c = p.comp_mut(comp).unwrap();
    // Self parenting cycle
    assert!(c.has_parent_cycle(l1, Some(l1)));

    // Setup: 2 -> 1, 3 -> 2
    c.layers.get_mut(&l2).unwrap().parent = Some(l1);
    c.layers.get_mut(&l3).unwrap().parent = Some(l2);

    // Setting 1's parent to 3 would create cycle 1 -> 3 -> 2 -> 1
    assert!(c.has_parent_cycle(l1, Some(l3)));
    // Setting 1's parent to 2 would create cycle 1 -> 2 -> 1
    assert!(c.has_parent_cycle(l1, Some(l2)));
    // Setting 1's parent to None is valid
    assert!(!c.has_parent_cycle(l1, None));
}
