use bonaparte_model::*;

#[test]
fn test_serde_all_model_entities() {
    let mut p = Project::new("Serde Test Project");
    let comp = p.create_comp(
        "Comp1",
        1920,
        1080,
        FrameRate::FPS_30,
        Time(10 * TICKS_PER_SEC),
    );

    let mut layer = Layer::new(
        "SolidLayer",
        LayerKind::Solid {
            color: [0.1, 0.2, 0.3, 0.4],
        },
        Time::ZERO,
        Time(5 * TICKS_PER_SEC),
    );
    layer.blend_mode = BlendMode::Overlay;
    layer.visible = true;
    layer.locked = false;
    layer.transform.anchor_point = [50.0, 50.0];

    let mut track = Track::default();
    track.set_key(Keyframe {
        time: Time::ZERO,
        value: PropValue::Scalar(0.0),
        easing: Easing::Linear,
    });
    track.set_key(Keyframe {
        time: Time(TICKS_PER_SEC),
        value: PropValue::Scalar(1.0),
        easing: Easing::Bezier {
            p1: [0.42, 0.0],
            p2: [0.58, 1.0],
        },
    });
    layer.tracks.insert(Property::Opacity, track);

    let layer_id = p.insert_layer(comp, layer);

    let media = p.insert_media(MediaAsset {
        embedded: None,
        id: MediaId(0),
        name: "test_image.png".into(),
        path: Some("assets/img.png".into()),
        kind: MediaKind::Image,
        audio: None,
        slot: Some(SlotDef {
            label: "Logo Slot".into(),
        }),
        alias: Some("logo".into()),
        perception: Some(PerceptionCard {
            role: AssetRole::LogoCandidate,
            width: 512,
            height: 512,
            palette: vec![[255, 0, 0], [0, 255, 0]],
            mean_luma: 0.5,
            entropy: 4.2,
            alpha_fraction: Some(0.1),
            evidence: vec!["OCR: Acme Corp".into()],
        }),
    });

    // Serialize Project to JSON
    let json_str = serde_json::to_string_pretty(&p).unwrap();
    let deserialized_p: Project = serde_json::from_str(&json_str).unwrap();

    assert_eq!(deserialized_p.name, "Serde Test Project");
    assert_eq!(deserialized_p.comps.len(), 1);
    assert_eq!(deserialized_p.media.len(), 1);

    let c = deserialized_p.comp(comp).unwrap();
    assert_eq!(c.name, "Comp1");
    assert_eq!(c.width, 1920);
    assert_eq!(c.height, 1080);
    assert_eq!(c.fps, FrameRate::FPS_30);
    assert_eq!(c.duration, Time(10 * TICKS_PER_SEC));

    let l = deserialized_p.layer(comp, layer_id).unwrap();
    assert_eq!(l.name, "SolidLayer");
    assert_eq!(l.blend_mode, BlendMode::Overlay);
    assert!(l.visible);
    assert!(!l.locked);
    assert_eq!(l.transform.anchor_point, [50.0, 50.0]);
    assert_eq!(l.tracks[&Property::Opacity].keys.len(), 2);

    let m = deserialized_p.media.get(&media).unwrap();
    assert_eq!(m.name, "test_image.png");
    assert_eq!(m.alias, Some("logo".into()));
    assert_eq!(
        m.perception.as_ref().unwrap().role,
        AssetRole::LogoCandidate
    );
}

#[test]
fn test_serde_layer_defaults_backward_compatibility() {
    // JSON representing an older schema without parent, blend_mode, visible, locked, anchor_point
    let raw_json = r#"{
        "id": 1,
        "name": "LegacyLayer",
        "kind": { "Solid": { "color": [1.0, 0.0, 0.0, 1.0] } },
        "start": 0,
        "duration": 600000,
        "transform": {
            "position": [0.0, 0.0],
            "scale": [100.0, 100.0],
            "rotation": 0.0,
            "opacity": 1.0
        },
        "tracks": {}
    }"#;

    let layer: Layer = serde_json::from_str(raw_json).unwrap();
    assert_eq!(layer.parent, None);
    assert_eq!(layer.blend_mode, BlendMode::Normal);
    assert!(layer.visible);
    assert!(!layer.locked);
    assert_eq!(layer.transform.anchor_point, [0.0, 0.0]);
}

#[test]
fn test_serde_all_op_variants() {
    let dummy_layer = Layer::new(
        "L",
        LayerKind::Solid { color: [1.0; 4] },
        Time::ZERO,
        Time(100),
    );
    let dummy_comp = Comp {
        id: CompId(1),
        name: "C".into(),
        width: 1920,
        height: 1080,
        fps: FrameRate::FPS_24,
        duration: Time(100),
        background: [0.0; 4],
        layer_order: vec![],
        audio: Default::default(),
        layers: std::collections::BTreeMap::new(),
    };
    let dummy_media = MediaAsset {
        id: MediaId(1),
        name: "m".into(),
        path: None,
        kind: MediaKind::Image,
        embedded: None,
        audio: None,
        slot: None,
        alias: None,
        perception: None,
    };

    let sample_ops = vec![
        Op::CreateComp {
            name: "Comp".into(),
            width: 1920,
            height: 1080,
            fps: FrameRate::FPS_24,
            duration: Time(100),
        },
        Op::RestoreComp {
            comp: Box::new(dummy_comp.clone()),
        },
        Op::RemoveComp { comp: CompId(1) },
        Op::SetCompProps {
            comp: CompId(1),
            name: "Props".into(),
            width: 1280,
            height: 720,
            fps: FrameRate::FPS_30,
            duration: Time(200),
            background: [0.5, 0.5, 0.5, 1.0],
        },
        Op::AddLayer {
            comp: CompId(1),
            layer: dummy_layer.clone(),
        },
        Op::RestoreLayer {
            comp: CompId(1),
            layer: Box::new(dummy_layer.clone()),
            index: 0,
        },
        Op::RemoveLayer {
            comp: CompId(1),
            layer: LayerId(2),
        },
        Op::RenameLayer {
            comp: CompId(1),
            layer: LayerId(2),
            name: "Renamed".into(),
        },
        Op::SetLayerTime {
            comp: CompId(1),
            layer: LayerId(2),
            start: Time(10),
            duration: Time(50),
        },
        Op::SetLayerParent {
            comp: CompId(1),
            layer: LayerId(2),
            parent: Some(LayerId(1)),
        },
        Op::SetLayerBlendMode {
            comp: CompId(1),
            layer: LayerId(2),
            blend_mode: BlendMode::Multiply,
        },
        Op::SetLayerVisible {
            comp: CompId(1),
            layer: LayerId(2),
            visible: false,
        },
        Op::SetLayerLocked {
            comp: CompId(1),
            layer: LayerId(2),
            locked: true,
        },
        Op::SetValue {
            comp: CompId(1),
            layer: LayerId(2),
            property: Property::Position,
            value: PropValue::Vec2([10.0, 20.0]),
        },
        Op::AddKeyframe {
            comp: CompId(1),
            layer: LayerId(2),
            property: Property::Scale,
            key: Keyframe {
                time: Time(0),
                value: PropValue::Vec2([100.0, 100.0]),
                easing: Easing::Linear,
            },
        },
        Op::RemoveKeyframe {
            comp: CompId(1),
            layer: LayerId(2),
            property: Property::Scale,
            time: Time(0),
        },
        Op::MoveKeyframe {
            comp: CompId(1),
            layer: LayerId(2),
            property: Property::Scale,
            from: Time(0),
            to: Time(100),
        },
        Op::SetEasing {
            comp: CompId(1),
            layer: LayerId(2),
            property: Property::Scale,
            time: Time(0),
            easing: Easing::default(),
        },
        Op::ReorderLayer {
            comp: CompId(1),
            layer: LayerId(2),
            new_index: 1,
        },
        Op::AddMedia {
            asset: dummy_media.clone(),
        },
        Op::RestoreMedia {
            asset: dummy_media.clone(),
        },
        Op::RemoveMedia { media: MediaId(1) },
    ];

    for op in sample_ops {
        let json = serde_json::to_string(&op).unwrap();
        let deserialized: Op = serde_json::from_str(&json).unwrap();
        let json2 = serde_json::to_string(&deserialized).unwrap();
        assert_eq!(json, json2);
    }
}
