use bonaparte_model::*;
fn state() -> (Project, CompId, LayerId) {
    let mut p = Project::new("Live edits");
    let c = p.create_comp("Main", 64, 48, FrameRate::FPS_30, Time(120000));
    let l = p.insert_layer(
        c,
        Layer::new_text("Text", "Before", 16.0, Time::ZERO, Time(120000)),
    );
    (p, c, l)
}
#[test]
fn grouped_content_edits_keep_original_inverse_and_latest_redo() {
    let (mut p, c, l) = state();
    let original = p.layer(c, l).unwrap().kind.clone();
    let mut h = History::new();
    for s in ["L", "LI", "LIVE"] {
        let mut k = original.clone();
        if let LayerKind::Text { text, .. } = &mut k {
            *text = s.into();
        }
        h.commit_grouped(
            &mut p,
            Op::SetLayerContent {
                comp: c,
                layer: l,
                kind: k,
            },
            Some("field-1".into()),
        )
        .unwrap();
    }
    assert_eq!(h.undo_descriptions().len(), 1);
    h.undo(&mut p).unwrap();
    assert_eq!(p.layer(c, l).unwrap().kind, original);
    h.redo(&mut p).unwrap();
    assert!(matches!(&p.layer(c,l).unwrap().kind,LayerKind::Text{text,..} if text=="LIVE"));
}
#[test]
fn group_cannot_merge_different_property_targets_and_failures_preserve_history() {
    let (mut p, c, l) = state();
    let mut h = History::new();
    h.commit_grouped(
        &mut p,
        Op::SetValue {
            comp: c,
            layer: l,
            property: Property::Position,
            value: PropValue::Vec2([2.0, 0.0]),
        },
        Some("field".into()),
    )
    .unwrap();
    h.commit_grouped(
        &mut p,
        Op::SetValue {
            comp: c,
            layer: l,
            property: Property::Rotation,
            value: PropValue::Scalar(30.0),
        },
        Some("field".into()),
    )
    .unwrap();
    assert_eq!(h.undo_descriptions().len(), 2);
    let mut bad = p.layer(c, l).unwrap().kind.clone();
    if let LayerKind::Text { size, .. } = &mut bad {
        *size = 0.0;
    }
    assert!(h
        .commit_grouped(
            &mut p,
            Op::SetLayerContent {
                comp: c,
                layer: l,
                kind: bad
            },
            Some("field".into())
        )
        .is_err());
    assert_eq!(h.undo_descriptions().len(), 2);
}
#[test]
fn grouped_animated_key_edits_undo_the_creation_as_a_unit() {
    let (mut p, c, l) = state();
    let mut h = History::new();
    for v in [10.0, 20.0, 40.0] {
        h.commit_grouped(
            &mut p,
            Op::AddKeyframe {
                comp: c,
                layer: l,
                property: Property::Rotation,
                key: Keyframe {
                    time: Time(4000),
                    value: PropValue::Scalar(v),
                    easing: Easing::Linear,
                },
            },
            Some("rotation".into()),
        )
        .unwrap();
    }
    assert_eq!(h.undo_descriptions().len(), 1);
    h.undo(&mut p).unwrap();
    assert!(p
        .layer(c, l)
        .unwrap()
        .tracks
        .get(&Property::Rotation)
        .is_none_or(|t| t.keys.is_empty()));
    h.redo(&mut p).unwrap();
    assert_eq!(
        p.layer(c, l).unwrap().tracks[&Property::Rotation].keys[0].value,
        PropValue::Scalar(40.0)
    );
}
