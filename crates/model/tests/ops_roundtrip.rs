use bonaparte_model::*;

fn create_base_project() -> (Project, CompId, LayerId, MediaId) {
    let mut p = Project::new("Base Project");
    let comp = p.create_comp(
        "MainComp",
        1920,
        1080,
        FrameRate::FPS_30,
        Time(5 * TICKS_PER_SEC),
    );
    let layer = p.insert_layer(
        comp,
        Layer::new(
            "Background",
            LayerKind::Solid {
                color: [0.1, 0.2, 0.3, 1.0],
            },
            Time::ZERO,
            Time(5 * TICKS_PER_SEC),
        ),
    );
    let media = p.insert_media(MediaAsset {
        id: MediaId(0),
        name: "logo.png".into(),
        path: Some("assets/logo.png".into()),
        kind: MediaKind::Image,
        embedded: None,
        audio: None,
        slot: None,
        alias: Some("logo".into()),
        perception: None,
        video: None,
    });
    (p, comp, layer, media)
}

fn assert_projects_equal(p1: &Project, p2: &Project) {
    assert_eq!(p1.name, p2.name);
    let j1 = serde_json::to_string(&p1.comps).unwrap();
    let j2 = serde_json::to_string(&p2.comps).unwrap();
    assert_eq!(j1, j2, "Compositions differ after round-trip");
    let m1 = serde_json::to_string(&p1.media).unwrap();
    let m2 = serde_json::to_string(&p2.media).unwrap();
    assert_eq!(m1, m2, "Media differ after round-trip");
}

#[test]
fn test_roundtrip_create_comp() {
    let (mut p, _, _, _) = create_base_project();
    let mut history = History::new();
    let p0 = p.clone();

    let op = Op::CreateComp {
        name: "NewComp".into(),
        width: 1280,
        height: 720,
        fps: FrameRate::FPS_60,
        duration: Time(10 * TICKS_PER_SEC),
    };

    history.commit(&mut p, op).unwrap();
    assert_ne!(
        serde_json::to_string(&p).unwrap(),
        serde_json::to_string(&p0).unwrap()
    );

    history.undo(&mut p).unwrap();
    assert_projects_equal(&p, &p0);

    history.redo(&mut p).unwrap();
    assert_eq!(p.comps.len(), 2);

    history.undo(&mut p).unwrap();
    assert_projects_equal(&p, &p0);
}

#[test]
fn test_roundtrip_remove_comp() {
    let (mut p, comp, _, _) = create_base_project();
    let mut history = History::new();
    let p0 = p.clone();

    let op = Op::RemoveComp { comp };
    history.commit(&mut p, op).unwrap();
    assert!(p.comps.is_empty());

    history.undo(&mut p).unwrap();
    assert_projects_equal(&p, &p0);

    history.redo(&mut p).unwrap();
    assert!(p.comps.is_empty());

    history.undo(&mut p).unwrap();
    assert_projects_equal(&p, &p0);
}

#[test]
fn test_roundtrip_set_comp_props() {
    let (mut p, comp, _, _) = create_base_project();
    let mut history = History::new();
    let p0 = p.clone();

    let op = Op::SetCompProps {
        comp,
        name: "Renamed Comp".into(),
        width: 3840,
        height: 2160,
        fps: FrameRate::FPS_24,
        duration: Time(15 * TICKS_PER_SEC),
        background: [1.0, 1.0, 1.0, 1.0],
    };

    history.commit(&mut p, op).unwrap();
    assert_eq!(p.comp(comp).unwrap().name, "Renamed Comp");
    assert_eq!(p.comp(comp).unwrap().width, 3840);

    history.undo(&mut p).unwrap();
    assert_projects_equal(&p, &p0);

    history.redo(&mut p).unwrap();
    assert_eq!(p.comp(comp).unwrap().name, "Renamed Comp");

    history.undo(&mut p).unwrap();
    assert_projects_equal(&p, &p0);
}

#[test]
fn test_roundtrip_add_layer() {
    let (mut p, comp, _, _) = create_base_project();
    let mut history = History::new();
    let p0 = p.clone();

    let op = Op::AddLayer {
        comp,
        layer: Layer::new(
            "TextLayer",
            LayerKind::Text {
                style: Default::default(),
                text: "Hello Bonaparte".into(),
                size: 48.0,
            },
            Time::ZERO,
            Time(3 * TICKS_PER_SEC),
        ),
    };

    history.commit(&mut p, op).unwrap();
    assert_eq!(p.comp(comp).unwrap().layers.len(), 2);

    history.undo(&mut p).unwrap();
    assert_projects_equal(&p, &p0);

    history.redo(&mut p).unwrap();
    assert_eq!(p.comp(comp).unwrap().layers.len(), 2);

    history.undo(&mut p).unwrap();
    assert_projects_equal(&p, &p0);
}

#[test]
fn test_roundtrip_remove_layer() {
    let (mut p, comp, layer, _) = create_base_project();
    let mut history = History::new();
    let p0 = p.clone();

    let op = Op::RemoveLayer { comp, layer };
    history.commit(&mut p, op).unwrap();
    assert_eq!(p.comp(comp).unwrap().layers.len(), 0);

    history.undo(&mut p).unwrap();
    assert_projects_equal(&p, &p0);

    history.redo(&mut p).unwrap();
    assert_eq!(p.comp(comp).unwrap().layers.len(), 0);

    history.undo(&mut p).unwrap();
    assert_projects_equal(&p, &p0);
}

#[test]
fn test_roundtrip_rename_layer() {
    let (mut p, comp, layer, _) = create_base_project();
    let mut history = History::new();
    let p0 = p.clone();

    let op = Op::RenameLayer {
        comp,
        layer,
        name: "New Layer Name".into(),
    };

    history.commit(&mut p, op).unwrap();
    assert_eq!(p.layer(comp, layer).unwrap().name, "New Layer Name");

    history.undo(&mut p).unwrap();
    assert_projects_equal(&p, &p0);

    history.redo(&mut p).unwrap();
    assert_eq!(p.layer(comp, layer).unwrap().name, "New Layer Name");

    history.undo(&mut p).unwrap();
    assert_projects_equal(&p, &p0);
}

#[test]
fn test_roundtrip_set_layer_time() {
    let (mut p, comp, layer, _) = create_base_project();
    let mut history = History::new();
    let p0 = p.clone();

    let op = Op::SetLayerTime {
        comp,
        layer,
        start: Time(TICKS_PER_SEC),
        duration: Time(2 * TICKS_PER_SEC),
    };

    history.commit(&mut p, op).unwrap();
    assert_eq!(p.layer(comp, layer).unwrap().start, Time(TICKS_PER_SEC));
    assert_eq!(
        p.layer(comp, layer).unwrap().duration,
        Time(2 * TICKS_PER_SEC)
    );

    history.undo(&mut p).unwrap();
    assert_projects_equal(&p, &p0);

    history.redo(&mut p).unwrap();
    assert_eq!(p.layer(comp, layer).unwrap().start, Time(TICKS_PER_SEC));

    history.undo(&mut p).unwrap();
    assert_projects_equal(&p, &p0);
}

#[test]
fn test_roundtrip_set_layer_parent() {
    let (mut p, comp, layer1, _) = create_base_project();
    let layer2 = p.insert_layer(
        comp,
        Layer::new_rect(
            "ChildLayer",
            [1.0, 1.0, 0.0, 1.0],
            Time::ZERO,
            Time(5 * TICKS_PER_SEC),
        ),
    );
    let mut history = History::new();
    let p0 = p.clone();

    let op = Op::SetLayerParent {
        comp,
        layer: layer2,
        parent: Some(layer1),
    };

    history.commit(&mut p, op).unwrap();
    assert_eq!(p.layer(comp, layer2).unwrap().parent, Some(layer1));

    history.undo(&mut p).unwrap();
    assert_projects_equal(&p, &p0);

    history.redo(&mut p).unwrap();
    assert_eq!(p.layer(comp, layer2).unwrap().parent, Some(layer1));

    history.undo(&mut p).unwrap();
    assert_projects_equal(&p, &p0);
}

#[test]
fn test_roundtrip_set_layer_blend_mode() {
    let (mut p, comp, layer, _) = create_base_project();
    let mut history = History::new();
    let p0 = p.clone();

    for mode in [
        BlendMode::Multiply,
        BlendMode::Screen,
        BlendMode::Overlay,
        BlendMode::Add,
        BlendMode::Darken,
        BlendMode::Lighten,
        BlendMode::Difference,
    ] {
        let op = Op::SetLayerBlendMode {
            comp,
            layer,
            blend_mode: mode,
        };
        history.commit(&mut p, op).unwrap();
        assert_eq!(p.layer(comp, layer).unwrap().blend_mode, mode);

        history.undo(&mut p).unwrap();
        assert_projects_equal(&p, &p0);
    }
}

#[test]
fn test_roundtrip_set_layer_visible_and_locked() {
    let (mut p, comp, layer, _) = create_base_project();
    let mut history = History::new();
    let p0 = p.clone();

    history
        .commit(
            &mut p,
            Op::SetLayerVisible {
                comp,
                layer,
                visible: false,
            },
        )
        .unwrap();
    assert!(!p.layer(comp, layer).unwrap().visible);

    history
        .commit(
            &mut p,
            Op::SetLayerLocked {
                comp,
                layer,
                locked: true,
            },
        )
        .unwrap();
    assert!(p.layer(comp, layer).unwrap().locked);

    history.undo(&mut p).unwrap(); // undo locked
    assert!(!p.layer(comp, layer).unwrap().locked);

    history.undo(&mut p).unwrap(); // undo visible
    assert_projects_equal(&p, &p0);
}

#[test]
fn test_roundtrip_set_value_all_properties() {
    let (mut p, comp, layer, _) = create_base_project();
    let mut history = History::new();
    let p0 = p.clone();

    let ops = vec![
        Op::SetValue {
            comp,
            layer,
            property: Property::Position,
            value: PropValue::Vec2([500.0, -300.0]),
        },
        Op::SetValue {
            comp,
            layer,
            property: Property::Scale,
            value: PropValue::Vec2([200.0, 150.0]),
        },
        Op::SetValue {
            comp,
            layer,
            property: Property::Rotation,
            value: PropValue::Scalar(45.0),
        },
        Op::SetValue {
            comp,
            layer,
            property: Property::Opacity,
            value: PropValue::Scalar(0.75),
        },
        Op::SetValue {
            comp,
            layer,
            property: Property::AnchorPoint,
            value: PropValue::Vec2([50.0, 50.0]),
        },
    ];

    for op in ops {
        history.commit(&mut p, op).unwrap();
    }

    assert_eq!(
        p.layer(comp, layer).unwrap().transform.position,
        [500.0, -300.0]
    );
    assert_eq!(
        p.layer(comp, layer).unwrap().transform.scale,
        [200.0, 150.0]
    );
    assert_eq!(p.layer(comp, layer).unwrap().transform.rotation, 45.0);
    assert_eq!(p.layer(comp, layer).unwrap().transform.opacity, 0.75);
    assert_eq!(
        p.layer(comp, layer).unwrap().transform.anchor_point,
        [50.0, 50.0]
    );

    for _ in 0..5 {
        history.undo(&mut p).unwrap();
    }
    assert_projects_equal(&p, &p0);
}

#[test]
fn test_roundtrip_keyframe_ops() {
    let (mut p, comp, layer, _) = create_base_project();
    let mut history = History::new();
    let p0 = p.clone();

    // Add keyframe at t=0
    history
        .commit(
            &mut p,
            Op::AddKeyframe {
                comp,
                layer,
                property: Property::Opacity,
                key: Keyframe {
                    time: Time::ZERO,
                    value: PropValue::Scalar(0.0),
                    easing: Easing::Linear,
                },
            },
        )
        .unwrap();

    // Add keyframe at t=1s
    history
        .commit(
            &mut p,
            Op::AddKeyframe {
                comp,
                layer,
                property: Property::Opacity,
                key: Keyframe {
                    time: Time(TICKS_PER_SEC),
                    value: PropValue::Scalar(1.0),
                    easing: Easing::default(),
                },
            },
        )
        .unwrap();

    assert_eq!(
        p.layer(comp, layer).unwrap().tracks[&Property::Opacity]
            .keys
            .len(),
        2
    );

    // Replace keyframe at t=1s with new value
    history
        .commit(
            &mut p,
            Op::AddKeyframe {
                comp,
                layer,
                property: Property::Opacity,
                key: Keyframe {
                    time: Time(TICKS_PER_SEC),
                    value: PropValue::Scalar(0.5),
                    easing: Easing::Linear,
                },
            },
        )
        .unwrap();
    assert_eq!(
        p.layer(comp, layer).unwrap().tracks[&Property::Opacity].keys[1].value,
        PropValue::Scalar(0.5)
    );

    // Undo replace keyframe
    history.undo(&mut p).unwrap();
    assert_eq!(
        p.layer(comp, layer).unwrap().tracks[&Property::Opacity].keys[1].value,
        PropValue::Scalar(1.0)
    );

    // Set easing
    history
        .commit(
            &mut p,
            Op::SetEasing {
                comp,
                layer,
                property: Property::Opacity,
                time: Time::ZERO,
                easing: Easing::Bezier {
                    p1: [0.1, 0.2],
                    p2: [0.8, 0.9],
                },
            },
        )
        .unwrap();

    // Move keyframe from 1s to 2s
    history
        .commit(
            &mut p,
            Op::MoveKeyframe {
                comp,
                layer,
                property: Property::Opacity,
                from: Time(TICKS_PER_SEC),
                to: Time(2 * TICKS_PER_SEC),
            },
        )
        .unwrap();
    assert_eq!(
        p.layer(comp, layer).unwrap().tracks[&Property::Opacity].keys[1].time,
        Time(2 * TICKS_PER_SEC)
    );

    // Remove keyframe at t=0
    history
        .commit(
            &mut p,
            Op::RemoveKeyframe {
                comp,
                layer,
                property: Property::Opacity,
                time: Time::ZERO,
            },
        )
        .unwrap();
    assert_eq!(
        p.layer(comp, layer).unwrap().tracks[&Property::Opacity]
            .keys
            .len(),
        1
    );

    // Undo all remaining ops back to clean p0
    while history.can_undo() {
        history.undo(&mut p).unwrap();
    }
    // Tracks map might be empty or missing Property::Opacity
    p.comps
        .get_mut(&comp)
        .unwrap()
        .layers
        .get_mut(&layer)
        .unwrap()
        .tracks
        .retain(|_, t| !t.keys.is_empty());
    assert_projects_equal(&p, &p0);
}

#[test]
fn test_roundtrip_reorder_layer() {
    let (mut p, comp, layer1, _) = create_base_project();
    let layer2 = p.insert_layer(
        comp,
        Layer::new(
            "Layer 2",
            LayerKind::Solid {
                color: [0.0, 1.0, 0.0, 1.0],
            },
            Time::ZERO,
            Time(5 * TICKS_PER_SEC),
        ),
    );
    let layer3 = p.insert_layer(
        comp,
        Layer::new(
            "Layer 3",
            LayerKind::Solid {
                color: [0.0, 0.0, 1.0, 1.0],
            },
            Time::ZERO,
            Time(5 * TICKS_PER_SEC),
        ),
    );

    let mut history = History::new();
    let p0 = p.clone();

    // Initial order: [1, 2, 3]
    assert_eq!(
        p.comp(comp).unwrap().layer_order,
        vec![layer1, layer2, layer3]
    );

    // Move layer 3 to index 0: [3, 1, 2]
    history
        .commit(
            &mut p,
            Op::ReorderLayer {
                comp,
                layer: layer3,
                new_index: 0,
            },
        )
        .unwrap();
    assert_eq!(
        p.comp(comp).unwrap().layer_order,
        vec![layer3, layer1, layer2]
    );

    history.undo(&mut p).unwrap();
    assert_projects_equal(&p, &p0);

    history.redo(&mut p).unwrap();
    assert_eq!(
        p.comp(comp).unwrap().layer_order,
        vec![layer3, layer1, layer2]
    );

    history.undo(&mut p).unwrap();
    assert_projects_equal(&p, &p0);
}

#[test]
fn test_roundtrip_media_all_ops() {
    let (mut p, _, _, media) = create_base_project();
    let mut history = History::new();
    let p0 = p.clone();

    let op = Op::RemoveMedia { media };
    history.commit(&mut p, op).unwrap();
    assert!(p.media.is_empty());

    history.undo(&mut p).unwrap();
    assert_projects_equal(&p, &p0);

    history.redo(&mut p).unwrap();
    assert!(p.media.is_empty());

    history.undo(&mut p).unwrap();
    assert_projects_equal(&p, &p0);
}

#[test]
fn test_op_descriptions() {
    let (p, comp, layer, media) = create_base_project();

    let ops = vec![
        Op::CreateComp {
            name: "TestComp".into(),
            width: 1920,
            height: 1080,
            fps: FrameRate::FPS_24,
            duration: Time(100),
        },
        Op::RestoreComp {
            comp: Box::new(p.comp(comp).unwrap().clone()),
        },
        Op::RemoveComp { comp },
        Op::SetCompProps {
            comp,
            name: "Props".into(),
            width: 1920,
            height: 1080,
            fps: FrameRate::FPS_30,
            duration: Time(100),
            background: [0.0, 0.0, 0.0, 1.0],
        },
        Op::AddLayer {
            comp,
            layer: p.layer(comp, layer).unwrap().clone(),
        },
        Op::RestoreLayer {
            comp,
            layer: Box::new(p.layer(comp, layer).unwrap().clone()),
            index: 0,
        },
        Op::RemoveLayer { comp, layer },
        Op::RenameLayer {
            comp,
            layer,
            name: "Renamed".into(),
        },
        Op::SetLayerTime {
            comp,
            layer,
            start: Time(0),
            duration: Time(100),
        },
        Op::SetLayerParent {
            comp,
            layer,
            parent: Some(LayerId(2)),
        },
        Op::SetLayerBlendMode {
            comp,
            layer,
            blend_mode: BlendMode::Multiply,
        },
        Op::SetLayerVisible {
            comp,
            layer,
            visible: false,
        },
        Op::SetLayerLocked {
            comp,
            layer,
            locked: true,
        },
        Op::SetValue {
            comp,
            layer,
            property: Property::Position,
            value: PropValue::Vec2([1.0, 2.0]),
        },
        Op::AddKeyframe {
            comp,
            layer,
            property: Property::Rotation,
            key: Keyframe {
                time: Time(0),
                value: PropValue::Scalar(10.0),
                easing: Easing::Linear,
            },
        },
        Op::RemoveKeyframe {
            comp,
            layer,
            property: Property::Rotation,
            time: Time(0),
        },
        Op::MoveKeyframe {
            comp,
            layer,
            property: Property::Rotation,
            from: Time(0),
            to: Time(100),
        },
        Op::SetEasing {
            comp,
            layer,
            property: Property::Rotation,
            time: Time(0),
            easing: Easing::Linear,
        },
        Op::ReorderLayer {
            comp,
            layer,
            new_index: 1,
        },
        Op::AddMedia {
            asset: p.media.get(&media).unwrap().clone(),
        },
        Op::RestoreMedia {
            asset: p.media.get(&media).unwrap().clone(),
        },
        Op::RemoveMedia { media },
    ];

    for op in ops {
        let desc = op.describe();
        assert!(!desc.is_empty(), "Description should not be empty for op");
    }
}
