use bonaparte_model::*;
use serde_json::json;

fn fixture() -> (Project, CompId, LayerId) {
    let mut project = Project::new("Foundation");
    let comp = project.create_comp("Main", 32, 32, FrameRate::FPS_30, Time(480_000));
    let layer = project.insert_layer(
        comp,
        Layer::new_solid("Solid", [0.25, 0.4, 0.6, 1.0], Time::ZERO, Time(480_000)),
    );
    (project, comp, layer)
}
fn key(time: i64, value: f32) -> Keyframe {
    Keyframe {
        time: Time(time),
        value: PropValue::Scalar(value),
        easing: Easing::Linear,
    }
}

#[test]
fn effect_stack_order_parameters_animation_and_bypass_roundtrip_exactly() {
    let (mut project, comp, layer) = fixture();
    let before = serde_json::to_value(&project).unwrap();
    let mut grade = EffectInstance::new("grade-1", "builtin.color_grade");
    grade
        .params
        .insert("exposure".into(), EffectValue::Float(1.0));
    grade.tracks.insert(
        "exposure".into(),
        Track {
            keys: vec![key(0, 0.0), key(120_000, 2.0)],
        },
    );
    let mut second = EffectInstance::new("grade-2", "builtin.color_grade");
    second.enabled = false;
    let mut history = History::new();
    history
        .commit(
            &mut project,
            Op::SetLayerEffects {
                comp,
                layer,
                effects: vec![grade, second],
            },
        )
        .unwrap();
    let after = serde_json::to_value(&project).unwrap();
    assert_eq!(
        project.layer(comp, layer).unwrap().effects[0].evaluated_params(Time(60_000))["exposure"],
        EffectValue::Float(1.0)
    );
    history.undo(&mut project).unwrap();
    assert_eq!(serde_json::to_value(&project).unwrap(), before);
    history.redo(&mut project).unwrap();
    assert_eq!(serde_json::to_value(&project).unwrap(), after);
}

#[test]
fn duplicate_effect_instance_ids_fail_without_history_or_state_changes() {
    let (mut project, comp, layer) = fixture();
    let before = serde_json::to_value(&project).unwrap();
    let mut history = History::new();
    let effect = EffectInstance::new("same", "builtin.invert");
    assert!(history
        .commit(
            &mut project,
            Op::SetLayerEffects {
                comp,
                layer,
                effects: vec![effect.clone(), effect]
            }
        )
        .is_err());
    assert_eq!(serde_json::to_value(&project).unwrap(), before);
    assert!(!history.can_undo());
}

#[test]
fn batch_is_atomic_and_records_one_human_readable_history_entry() {
    let (mut project, comp, layer) = fixture();
    let before = serde_json::to_value(&project).unwrap();
    let mut history = History::new();
    let good = Op::RenameLayer {
        comp,
        layer,
        name: "Renamed".into(),
    };
    let bad = Op::RenameLayer {
        comp,
        layer: LayerId(9999),
        name: "Invalid".into(),
    };
    assert!(history
        .commit(
            &mut project,
            Op::Batch {
                label: "Invalid transaction".into(),
                ops: vec![good.clone(), bad]
            }
        )
        .is_err());
    assert_eq!(serde_json::to_value(&project).unwrap(), before);
    assert!(!history.can_undo());
    history
        .commit(
            &mut project,
            Op::Batch {
                label: "Design edit".into(),
                ops: vec![
                    good,
                    Op::SetValue {
                        comp,
                        layer,
                        property: Property::Rotation,
                        value: PropValue::Scalar(45.0),
                    },
                ],
            },
        )
        .unwrap();
    assert_eq!(history.undo_descriptions(), vec!["Design edit"]);
    history.undo(&mut project).unwrap();
    assert_eq!(serde_json::to_value(&project).unwrap(), before);
}

#[test]
fn moving_layer_moves_transform_and_effect_keys_and_undo_is_exact() {
    let (mut project, comp, layer) = fixture();
    let l = project.layer_mut(comp, layer).unwrap();
    l.tracks.insert(
        Property::Opacity,
        Track {
            keys: vec![key(0, 0.0), key(120_000, 1.0)],
        },
    );
    let mut effect = EffectInstance::new("grade", "builtin.color_grade");
    effect.tracks.insert(
        "exposure".into(),
        Track {
            keys: vec![key(0, 0.0), key(120_000, 2.0)],
        },
    );
    l.effects.push(effect);
    let before = serde_json::to_value(&project).unwrap();
    let mut history = History::new();
    history
        .commit(
            &mut project,
            Op::ShiftLayer {
                comp,
                layer,
                delta: Time(24_000),
            },
        )
        .unwrap();
    let moved = project.layer(comp, layer).unwrap();
    assert_eq!(moved.start, Time(24_000));
    assert_eq!(moved.tracks[&Property::Opacity].keys[0].time, Time(24_000));
    assert_eq!(
        moved.effects[0].tracks["exposure"].keys[1].time,
        Time(144_000)
    );
    history.undo(&mut project).unwrap();
    assert_eq!(serde_json::to_value(&project).unwrap(), before);
}

#[test]
fn invalid_dimensions_parent_cycles_and_restoration_overwrites_are_refused() {
    let (mut project, comp, layer) = fixture();
    let mut history = History::new();
    let before = serde_json::to_value(&project).unwrap();
    assert!(history
        .commit(
            &mut project,
            Op::SetCompProps {
                comp,
                name: "Bad".into(),
                width: 0,
                height: 32,
                fps: FrameRate::FPS_30,
                duration: Time(120_000),
                background: [0.0, 0.0, 0.0, 1.0]
            }
        )
        .is_err());
    assert!(history
        .commit(
            &mut project,
            Op::SetLayerParent {
                comp,
                layer,
                parent: Some(layer)
            }
        )
        .is_err());
    let copy = project.comp(comp).unwrap().clone();
    assert!(history
        .commit(
            &mut project,
            Op::RestoreComp {
                comp: Box::new(copy)
            }
        )
        .is_err());
    assert_eq!(serde_json::to_value(&project).unwrap(), before);
}

#[test]
fn legacy_shape_files_get_styles_and_empty_effect_stacks() {
    let value = json!({"id":1,"name":"Legacy shape","kind":{"Shape":{"color":[1.0,1.0,1.0,1.0],"generator":"builtin.circle"}},"start":0,"duration":120000,"transform":{"position":[0,0],"scale":[100,100],"rotation":0,"opacity":1}});
    let layer: Layer = serde_json::from_value(value).unwrap();
    assert!(layer.effects.is_empty());
    if let LayerKind::Shape { style, .. } = layer.kind {
        assert_eq!(style.size, None);
        assert_eq!(style.stroke_width, 0.0);
    } else {
        panic!("shape expected");
    }
}

#[test]
fn nonfinite_content_and_unsorted_effect_tracks_are_rejected() {
    let (mut project, comp, layer) = fixture();
    let mut history = History::new();
    assert!(history
        .commit(
            &mut project,
            Op::SetLayerContent {
                comp,
                layer,
                kind: LayerKind::Text {
                    text: "Bad".into(),
                    size: f32::NAN,
                    style: TextStyle::default()
                }
            }
        )
        .is_err());
    let mut effect = EffectInstance::new("bad-track", "builtin.color_grade");
    effect.tracks.insert(
        "exposure".into(),
        Track {
            keys: vec![key(20, 0.0), key(10, 1.0)],
        },
    );
    assert!(history
        .commit(
            &mut project,
            Op::SetLayerEffects {
                comp,
                layer,
                effects: vec![effect]
            }
        )
        .is_err());
    assert!(!history.can_undo());
}

#[test]
fn shift_overflow_and_oversized_batch_leave_document_and_redo_intact() {
    let (mut project, comp, layer) = fixture();
    let mut history = History::new();
    history
        .commit(
            &mut project,
            Op::RenameProject {
                name: "Changed".into(),
            },
        )
        .unwrap();
    history.undo(&mut project).unwrap();
    let before = serde_json::to_value(&project).unwrap();
    for delta in [i64::MAX, i64::MIN] {
        assert!(history
            .commit(
                &mut project,
                Op::ShiftLayer {
                    comp,
                    layer,
                    delta: Time(delta)
                }
            )
            .is_err());
    }
    assert!(history
        .commit(
            &mut project,
            Op::Batch {
                label: "Too large".into(),
                ops: (0..8193)
                    .map(|_| Op::RenameProject {
                        name: "Partial edit".into()
                    })
                    .collect()
            }
        )
        .is_err());
    assert_eq!(serde_json::to_value(&project).unwrap(), before);
    assert!(history.can_redo());
    assert!(!history.can_undo());
}

#[test]
fn all_256_primitives_survive_the_batch_transaction_history_limit() {
    let (mut project, _, _) = fixture();
    let before = serde_json::to_value(&project).unwrap();
    let mut history = History::new();
    history
        .commit(
            &mut project,
            Op::Batch {
                label: "Full transaction".into(),
                ops: (0..256)
                    .map(|i| Op::RenameProject {
                        name: format!("Step {i}"),
                    })
                    .collect(),
            },
        )
        .unwrap();
    let after = serde_json::to_value(&project).unwrap();
    assert_eq!(history.undo_descriptions().len(), 1);
    history.undo(&mut project).unwrap();
    assert_eq!(serde_json::to_value(&project).unwrap(), before);
    history.redo(&mut project).unwrap();
    assert_eq!(serde_json::to_value(&project).unwrap(), after);
}

#[test]
fn unsafe_id_allocators_are_rejected_before_allocation_can_overflow() {
    let (mut project, comp, _) = fixture();
    project.next_layer = LayerId(u64::MAX);
    let before = serde_json::to_value(&project).unwrap();
    assert!(project.validate().is_err());
    let mut history = History::new();
    assert!(history
        .commit(
            &mut project,
            Op::AddLayer {
                comp,
                layer: Layer::new_solid("No overflow", [1.0; 4], Time::ZERO, Time(120000))
            }
        )
        .is_err());
    assert_eq!(serde_json::to_value(&project).unwrap(), before);
    project.next_layer = LayerId(9_007_199_254_740_992);
    assert!(project.validate().is_err());
    project.next_layer = LayerId(2);
    assert!(project.validate().is_ok());
}
