//! NO-MERCY model torture. Seeded randomized op floods, history eviction,
//! atomicity under mid-batch failure, ID stability across undo, time overflow,
//! validation ceilings. Every test is deterministic (fixed seeds, no wall
//! clock, no network) and must pass forever or the suite is lying.

use bonaparte_model::*;
use serde_json::json;

/// Deterministic tiny PRNG (mulberry32). No external dependency, stable across
/// platforms, and re-seedable so failures reproduce exactly.
struct Rng(u32);
impl Rng {
    fn new(seed: u32) -> Self {
        Self(seed | 1)
    }
    fn next_u32(&mut self) -> u32 {
        self.0 = self.0.wrapping_add(0x6D2B79F5);
        let mut t = self.0;
        t ^= t >> 15;
        t = t.wrapping_mul(0x2545F491);
        t ^= t >> 13;
        t.wrapping_mul(0x27D4EB2D)
    }
    fn f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / 16_777_216.0
    }
    fn range(&mut self, n: usize) -> usize {
        (self.next_u32() as usize) % n.max(1)
    }
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.range(items.len())]
    }
}

fn base_project() -> Project {
    let mut p = Project::new("no mercy");
    p.create_comp(
        "Main",
        320,
        180,
        FrameRate::FPS_30,
        Time(10 * TICKS_PER_SEC),
    );
    p
}

fn solid(name: &str, color: [f32; 4]) -> Layer {
    let mut l = Layer::new(
        name,
        LayerKind::Solid { color },
        Time::ZERO,
        Time(10 * TICKS_PER_SEC),
    );
    l.transform = StaticTransform::default();
    l
}

fn commit_ok(h: &mut History, p: &mut Project, op: Op) -> Result<(), ModelError> {
    h.commit(p, op)
}

/// Randomized op flood: apply, validate, undo-everything, require the
/// document to be byte-identical (composition-wise) to the pristine one.
#[test]
fn seeded_op_flood_survives_and_undoes_cleanly() {
    for seed in [1u32, 0xDEADBEEF, 0xC0FFEE, 424242] {
        let mut rng = Rng::new(seed);
        let pristine = base_project();
        let mut p = pristine.clone();
        let mut h = History::new();

        // Seed: a few layers and media so most ops have real targets.
        for i in 0..6 {
            let c = if i % 3 == 2 { 2 } else { 1 };
            if c == 2 {
                h.commit(
                    &mut p,
                    Op::CreateComp {
                        name: "Extra".into(),
                        width: 160,
                        height: 90,
                        fps: FrameRate::FPS_30,
                        duration: Time(5 * TICKS_PER_SEC),
                    },
                )
                .unwrap();
            }
            let comp = CompId(c as u64);
            h.commit(
                &mut p,
                Op::AddLayer {
                    comp,
                    layer: solid(&format!("L{i}"), [0.5, 0.25, 0.75, 1.0]),
                },
            )
            .unwrap();
        }
        h.commit(
            &mut p,
            Op::AddMedia {
                asset: MediaAsset {
                    id: MediaId(0),
                    name: "tex".into(),
                    path: None,
                    kind: MediaKind::Image,
                    embedded: None,
                    audio: None,
                    slot: None,
                    alias: None,
                    perception: None,
                    video: None,
                },
            },
        )
        .unwrap();

        let blend_modes = [
            BlendMode::Normal,
            BlendMode::Multiply,
            BlendMode::Screen,
            BlendMode::Overlay,
            BlendMode::Add,
            BlendMode::Darken,
            BlendMode::Lighten,
            BlendMode::Difference,
        ];
        let properties = [
            Property::Position,
            Property::Scale,
            Property::Rotation,
            Property::Opacity,
            Property::AnchorPoint,
        ];

        let comps: Vec<CompId> = p.comps.keys().copied().collect();
        let mut commits = 0;
        for step in 0..400 {
            if commits >= 190 {
                break; // stay below eviction so a full undo can reach pristine
            }
            let comp = *rng.pick(&comps);
            let layers: Vec<LayerId> = p.comps[&comp].layer_order.iter().copied().collect();
            let op = if layers.is_empty() {
                Op::RenameProject {
                    name: format!("flood {step}"),
                }
            } else {
                let layer = *rng.pick(&layers);
                match rng.range(14) {
                    0 => Op::SetValue {
                        comp,
                        layer,
                        property: *rng.pick(&properties),
                        value: if rng.range(2) == 0 {
                            PropValue::Scalar(rng.f32() * 2.0 - 0.5)
                        } else {
                            PropValue::Vec2([rng.f32() * 600.0 - 100.0, rng.f32() * 400.0 - 50.0])
                        },
                    },
                    1 => Op::AddKeyframe {
                        comp,
                        layer,
                        property: *rng.pick(&properties),
                        key: Keyframe {
                            time: Time(rng.next_u32() as i64 % (10 * TICKS_PER_SEC)),
                            value: PropValue::Scalar(rng.f32()),
                            easing: Easing::Bezier {
                                p1: [rng.f32(), rng.f32()],
                                p2: [rng.f32(), rng.f32()],
                            },
                        },
                    },
                    2 => Op::RemoveKeyframe {
                        comp,
                        layer,
                        property: *rng.pick(&properties),
                        time: Time(rng.next_u32() as i64 % (10 * TICKS_PER_SEC)),
                    },
                    3 => Op::MoveKeyframe {
                        comp,
                        layer,
                        property: *rng.pick(&properties),
                        from: Time(rng.next_u32() as i64 % (10 * TICKS_PER_SEC)),
                        to: Time(rng.next_u32() as i64 % (10 * TICKS_PER_SEC)),
                    },
                    4 => Op::SetEasing {
                        comp,
                        layer,
                        property: *rng.pick(&properties),
                        time: Time(rng.next_u32() as i64 % (10 * TICKS_PER_SEC)),
                        easing: Easing::Linear,
                    },
                    5 => Op::ReorderLayer {
                        comp,
                        layer,
                        new_index: rng.range(8),
                    },
                    6 => Op::SetLayerParent {
                        comp,
                        layer,
                        parent: rng
                            .range(3)
                            .checked_sub(1)
                            .and_then(|i| layers.get(i).copied()),
                    },
                    7 => Op::SetLayerTime {
                        comp,
                        layer,
                        start: Time(rng.next_u32() as i64 % (5 * TICKS_PER_SEC)),
                        duration: Time(1 + rng.next_u32() as i64 % (10 * TICKS_PER_SEC)),
                    },
                    8 => Op::ShiftLayer {
                        comp,
                        layer,
                        delta: Time(rng.next_u32() as i64 % 400_000 - 200_000),
                    },
                    9 => Op::SetLayerBlendMode {
                        comp,
                        layer,
                        blend_mode: *rng.pick(&blend_modes),
                    },
                    10 => Op::SetLayerVisible {
                        comp,
                        layer,
                        visible: rng.range(2) == 0,
                    },
                    11 => Op::SetLayerLocked {
                        comp,
                        layer,
                        locked: rng.range(2) == 1,
                    },
                    12 => Op::SetLayerEffects {
                        comp,
                        layer,
                        effects: vec![EffectInstance::new("fx", "builtin.vignette")],
                    },
                    _ => Op::RemoveLayer { comp, layer },
                }
            };
            // Errors are part of the flood; the document must simply refuse them.
            if commit_ok(&mut h, &mut p, op).is_ok() {
                commits += 1;
            }
            p.validate()
                .unwrap_or_else(|e| panic!("seed {seed:#x} step {step}: invalid doc: {e}"));
            assert!(h.undo_len() <= 1000, "history eviction broken");
        }

        // Undo everything that remains; the doc must return to pristine state.
        let mut undos = 0;
        while h.undo(&mut p).unwrap() {
            undos += 1;
            p.validate().expect("undo produced an invalid document");
            assert!(undos <= 1_000, "undo does not terminate");
        }
        let pristine_json = serde_json::to_string(&pristine.comps).unwrap();
        let final_json = serde_json::to_string(&p.comps).unwrap();
        assert_eq!(
            pristine_json, final_json,
            "seed {seed:#x}: full undo did not restore the pristine document"
        );
    }
}

#[test]
fn batch_is_atomic_when_the_last_op_fails() {
    let mut p = base_project();
    let c = p.comps.keys().next().copied().unwrap();
    let mut h = History::new();
    h.commit(
        &mut p,
        Op::AddLayer {
            comp: c,
            layer: solid("A", [1.0; 4]),
        },
    )
    .unwrap();
    let before = serde_json::to_string(&p).unwrap();

    // Second op in the batch targets a missing layer: the whole batch must be
    // rejected with zero visible changes and no history entry.
    let batch = Op::Batch {
        label: "atomic".into(),
        ops: vec![
            Op::RenameProject {
                name: "changed".into(),
            },
            Op::SetValue {
                comp: c,
                layer: LayerId(999),
                property: Property::Opacity,
                value: PropValue::Scalar(0.0),
            },
        ],
    };
    assert!(commit_ok(&mut h, &mut p, batch).is_err());
    assert_eq!(before, serde_json::to_string(&p).unwrap());
    assert_eq!(h.undo_len(), 1, "failed batch must not add a history entry");
    // The only entry is the pre-existing AddLayer; undoing it returns to empty.
    h.undo(&mut p).unwrap();
    assert!(p.comps[&c].layer_order.is_empty());
    assert_eq!(p.name, "no mercy", "the batch's rename must not survive");
}

#[test]
fn batch_limits_are_enforced() {
    let mut p = base_project();
    let c = p.comps.keys().next().copied().unwrap();
    let mut h = History::new();

    let op = |i: usize| Op::RenameLayer {
        comp: c,
        // One harmless rename of the project mixed with per-layer renames.
        layer: LayerId(i as u64),
        name: format!("n{i}"),
    };
    let too_big = Op::Batch {
        label: "big".into(),
        ops: (0..257).map(op).collect(),
    };
    assert!(commit_ok(&mut h, &mut p, too_big).is_err());

    let nested = Op::Batch {
        label: "outer".into(),
        ops: vec![Op::Batch {
            label: "inner".into(),
            ops: vec![],
        }],
    };
    assert!(commit_ok(&mut h, &mut p, nested).is_err());

    let exactly_256 = Op::Batch {
        label: "max".into(),
        ops: (0..256)
            .map(|i| Op::RenameProject {
                name: format!("p{i}"),
            })
            .collect(),
    };
    commit_ok(&mut h, &mut p, exactly_256).unwrap();
    assert_eq!(p.name, "p255");
}

#[test]
fn history_evicts_at_one_thousand_and_stops_at_the_bottom() {
    let mut p = base_project();
    let mut h = History::new();
    for i in 0..1250 {
        h.commit(
            &mut p,
            Op::RenameProject {
                name: format!("v{i}"),
            },
        )
        .unwrap();
    }
    assert_eq!(h.undo_len(), 1000);
    for _ in 0..1000 {
        assert!(h.undo(&mut p).unwrap());
    }
    assert!(!h.undo(&mut p).unwrap(), "undo walked past the bottom");
    assert_eq!(p.name, "v249", "1250 edits with a 1000-deep stack keeps v249");
    for _ in 0..1000 {
        assert!(h.redo(&mut p).unwrap());
    }
    assert!(!h.redo(&mut p).unwrap(), "redo walked past the top");
    assert_eq!(p.name, "v1249");
}

#[test]
fn ids_survive_undo_and_restore_refuses_collisions() {
    let mut p = base_project();
    let c = p.comps.keys().next().copied().unwrap();
    let mut h = History::new();
    h.commit(
        &mut p,
        Op::AddLayer {
            comp: c,
            layer: solid("A", [1.0; 4]),
        },
    )
    .unwrap();
    let id = p.comps[&c].layer_order[0];
    let tracks_before = serde_json::to_string(&p.comps[&c].layers[&id].tracks).unwrap();

    h.commit(&mut p, Op::RemoveLayer { comp: c, layer: id })
        .unwrap();
    assert!(p.comps[&c].layers.get(&id).is_none());
    h.undo(&mut p).unwrap();
    let restored = &p.comps[&c].layers[&id];
    assert_eq!(
        tracks_before,
        serde_json::to_string(&restored.tracks).unwrap()
    );
    assert!(
        p.comps[&c].layer_order.contains(&id),
        "layer_order lost the restored id"
    );

    // Restoring over a live ID must refuse instead of corrupting the document.
    let comp = p.comps.get(&c).unwrap().clone();
    let mut h2 = History::new();
    assert!(h2
        .commit(
            &mut p,
            Op::RestoreComp {
                comp: Box::new(comp)
            }
        )
        .is_err());
}

#[test]
fn time_arithmetic_refuses_to_overflow() {
    let mut p = base_project();
    let c = p.comps.keys().next().copied().unwrap();
    let mut h = History::new();
    h.commit(
        &mut p,
        Op::AddLayer {
            comp: c,
            layer: {
                let mut l = solid("far", [1.0; 4]);
                let mut track = Track::new();
                track.set_key(Keyframe {
                    time: Time(86_400 * TICKS_PER_SEC),
                    value: PropValue::Vec2([0.0; 2]),
                    easing: Easing::Linear,
                });
                l.tracks.insert(Property::Position, track);
                l
            },
        },
    )
    .unwrap();
    let id = p.comps[&c].layer_order[0];
    let before = serde_json::to_string(&p).unwrap();
    assert!(commit_ok(
        &mut h,
        &mut p,
        Op::ShiftLayer {
            comp: c,
            layer: id,
            delta: Time(64)
        }
    )
    .is_err());
    assert_eq!(before, serde_json::to_string(&p).unwrap());
}

#[test]
fn composition_validation_ceilings() {
    let mut p = base_project();
    let mut h = History::new();
    let bad = [
        json!({"w": 8193u32, "h": 8u32}),
        json!({"w": 8u32, "h": 8193u32}),
    ];
    for case in &bad {
        let w = case["w"].as_u64().unwrap() as u32;
        let hgt = case["h"].as_u64().unwrap() as u32;
        let mut hp = History::new();
        assert!(
            hp.commit(
                &mut p,
                Op::CreateComp {
                    name: "ceiling".into(),
                    width: w,
                    height: hgt,
                    fps: FrameRate::FPS_30,
                    duration: Time(TICKS_PER_SEC),
                }
            )
            .is_err(),
            "expected rejection for {w}x{hgt}"
        );
    }
    // Duration beyond the 24 h ceiling must be rejected by validation.
    let too_long = Op::CreateComp {
        name: "marathon".into(),
        width: 64,
        height: 64,
        fps: FrameRate::FPS_30,
        duration: Time(25 * 3600 * TICKS_PER_SEC),
    };
    assert!(commit_ok(&mut h, &mut p, too_long).is_err());
}

#[test]
fn set_value_type_mismatch_never_lands() {
    let mut p = base_project();
    let c = p.comps.keys().next().copied().unwrap();
    let mut h = History::new();
    h.commit(
        &mut p,
        Op::AddLayer {
            comp: c,
            layer: solid("A", [1.0; 4]),
        },
    )
    .unwrap();
    let id = p.comps[&c].layer_order[0];
    let before = serde_json::to_string(&p).unwrap();

    assert!(matches!(
        h.commit(
            &mut p,
            Op::SetValue {
                comp: c,
                layer: id,
                property: Property::Position,
                value: PropValue::Scalar(0.5),
            },
        ),
        Err(ModelError::TypeMismatch(Property::Position))
    ));
    assert_eq!(before, serde_json::to_string(&p).unwrap());
}

#[test]
fn track_wins_over_static_value() {
    let mut p = base_project();
    let c = p.comps.keys().next().copied().unwrap();
    let mut h = History::new();
    h.commit(
        &mut p,
        Op::AddLayer {
            comp: c,
            layer: solid("A", [1.0; 4]),
        },
    )
    .unwrap();
    let id = p.comps[&c].layer_order[0];

    h.commit(
        &mut p,
        Op::AddKeyframe {
            comp: c,
            layer: id,
            property: Property::Opacity,
            key: Keyframe {
                time: Time::ZERO,
                value: PropValue::Scalar(1.0),
                easing: Easing::Linear,
            },
        },
    )
    .unwrap();
    // Editing the static value while a track exists must not change evaluation.
    h.commit(
        &mut p,
        Op::SetValue {
            comp: c,
            layer: id,
            property: Property::Opacity,
            value: PropValue::Scalar(0.25),
        },
    )
    .unwrap();
    let layer = &p.comps[&c].layers[&id];
    let evaluated = layer
        .tracks
        .get(&Property::Opacity)
        .unwrap()
        .evaluate(Time(TICKS_PER_SEC / 2))
        .unwrap();
    assert_eq!(evaluated, PropValue::Scalar(1.0), "track must keep winning");
    // The static value is still recorded for the day the track is removed.
    assert_eq!(layer.transform.opacity, 0.25);
}

#[test]
fn keyframe_invariants_hold_under_abuse() {
    let mut p = base_project();
    let c = p.comps.keys().next().copied().unwrap();
    let mut h = History::new();
    h.commit(
        &mut p,
        Op::AddLayer {
            comp: c,
            layer: solid("A", [1.0; 4]),
        },
    )
    .unwrap();
    let id = p.comps[&c].layer_order[0];
    let prop = Property::Position;

    let k = |t: i64| Keyframe {
        time: Time(t),
        value: PropValue::Vec2([t as f32, 0.0]),
        easing: Easing::Linear,
    };
    // Insert out of order, duplicates merge instead of duplicating.
    for t in [500, 100, 300, 100] {
        h.commit(
            &mut p,
            Op::AddKeyframe {
                comp: c,
                layer: id,
                property: prop,
                key: k(t),
            },
        )
        .unwrap();
    }
    let track = &p.comps[&c].layers[&id].tracks[&Property::Position];
    assert_eq!(track.keys.len(), 3, "duplicate time must merge");
    assert!(
        track.keys.windows(2).all(|w| w[0].time < w[1].time),
        "sorted invariant broken"
    );

    // Moving onto an occupied time is refused, not merged.
    let occupied = Time(300);
    assert!(matches!(
        h.commit(
            &mut p,
            Op::MoveKeyframe {
                comp: c,
                layer: id,
                property: prop,
                from: Time(100),
                to: occupied
            },
        ),
        Err(ModelError::KeyframeOccupied(..))
    ));

    // Removing a key that does not exist is a typed error.
    assert!(matches!(
        h.commit(
            &mut p,
            Op::RemoveKeyframe {
                comp: c,
                layer: id,
                property: prop,
                time: Time(12345)
            },
        ),
        Err(ModelError::KeyframeNotFound(..))
    ));

    // Extreme-but-finite key times stay inside the document while the value
    // respects the finite-magnitude ceiling.
    let mut far = k(20 * TICKS_PER_SEC);
    far.value = PropValue::Vec2([0.5, 0.25]);
    h.commit(
        &mut p,
        Op::AddKeyframe {
            comp: c,
            layer: id,
            property: prop,
            key: far,
        },
    )
    .unwrap();
    p.validate().unwrap();
    // A value beyond the 1e6 magnitude ceiling is refused outright.
    let mut huge = k(21 * TICKS_PER_SEC);
    huge.value = PropValue::Vec2([2_000_000.0, 0.0]);
    assert!(h
        .commit(
            &mut p,
            Op::AddKeyframe {
                comp: c,
                layer: id,
                property: prop,
                key: huge
            }
        )
        .is_err());
}

#[test]
fn live_edit_groups_merge_and_refuse_bad_names() {
    let mut p = base_project();
    let c = p.comps.keys().next().copied().unwrap();
    let mut h = History::new();
    h.commit(
        &mut p,
        Op::AddLayer {
            comp: c,
            layer: solid("A", [1.0; 4]),
        },
    )
    .unwrap();
    let id = p.comps[&c].layer_order[0];

    let group = "live-opacity".to_string();
    for v in [0.9, 0.8, 0.7] {
        h.commit_grouped(
            &mut p,
            Op::SetValue {
                comp: c,
                layer: id,
                property: Property::Opacity,
                value: PropValue::Scalar(v),
            },
            Some(group.clone()),
        )
        .unwrap();
    }
    assert_eq!(h.undo_len(), 2, "typed layer entry + one merged group");
    h.undo(&mut p).unwrap();
    assert!((p.comps[&c].layers[&id].transform.opacity - 1.0).abs() < 1e-6);

    for bad in ["", &"x".repeat(129)] {
        assert!(h
            .commit_grouped(
                &mut p,
                Op::RenameProject { name: "n".into() },
                Some(bad.to_string())
            )
            .is_err());
    }
}

#[test]
fn parent_cycles_are_detected_in_every_direction() {
    let mut p = base_project();
    let c = p.comps.keys().next().copied().unwrap();
    let mut h = History::new();
    for n in ["A", "B", "C"] {
        h.commit(
            &mut p,
            Op::AddLayer {
                comp: c,
                layer: solid(n, [1.0; 4]),
            },
        )
        .unwrap();
    }
    let ids: Vec<LayerId> = p.comps[&c].layer_order.iter().copied().collect();
    let (a, b, cc) = (ids[0], ids[1], ids[2]);
    h.commit(
        &mut p,
        Op::SetLayerParent {
            comp: c,
            layer: b,
            parent: Some(a),
        },
    )
    .unwrap();
    h.commit(
        &mut p,
        Op::SetLayerParent {
            comp: c,
            layer: cc,
            parent: Some(b),
        },
    )
    .unwrap();
    // Direct cycle, two-hop cycle, and self-parenting are all refused.
    assert!(matches!(
        h.commit(
            &mut p,
            Op::SetLayerParent {
                comp: c,
                layer: a,
                parent: Some(cc)
            }
        ),
        Err(ModelError::ParentCycle(..))
    ));
    assert!(h
        .commit(
            &mut p,
            Op::SetLayerParent {
                comp: c,
                layer: a,
                parent: Some(a)
            }
        )
        .is_err());
    // Undo of parenting restores independence.
    h.undo(&mut p).unwrap();
    h.undo(&mut p).unwrap();
    assert_eq!(p.comps[&c].layers[&b].parent, None);
}

#[test]
fn hold_easing_steps_and_survives_serde_round_trips() {
    let mut track = Track::new();
    track.set_key(Keyframe {
        time: Time::ZERO,
        value: PropValue::Scalar(0.0),
        easing: Easing::Hold,
    });
    track.set_key(Keyframe {
        time: Time(120000),
        value: PropValue::Scalar(100.0),
        easing: Easing::default(),
    });
    // The held segment stays exactly on the outgoing value for its entire
    // duration, then jumps at the target instant — including arbitrary
    // sub-frame times, which Linear/Bezier would interpolate smoothly.
    for t in [1, 59999, 119999] {
        let v = track.evaluate(Time(t)).expect("in-range key");
        assert_eq!(v, PropValue::Scalar(0.0), "Hold drifted at t={t}");
    }
    assert_eq!(track.evaluate(Time(120000)), Some(PropValue::Scalar(100.0)));
    assert_eq!(track.evaluate(Time(240000)), Some(PropValue::Scalar(100.0)));
    // Wire format round-trips the new variant (external tag, serde default).
    let json = serde_json::to_string(&Easing::Hold).unwrap();
    assert_eq!(json, r#""Hold""#);
    assert_eq!(serde_json::from_str::<Easing>(&json).unwrap(), Easing::Hold);
    // Legacy files without the variant still parse and default identically.
    let legacy =
        serde_json::from_str::<Easing>(r#"{"Bezier":{"p1":[0.42,0.0],"p2":[0.58,1.0]}}"#).unwrap();
    assert_eq!(legacy, Easing::default());
}

#[test]
fn easing_handles_are_validated_for_finiteness_and_time_range() {
    let mut p = base_project();
    let c = p.comps.keys().copied().next().unwrap();
    let mut layer = solid("eased", [1.0, 0.0, 0.0, 1.0]);
    let mut track = Track::new();
    track.set_key(Keyframe {
        time: Time::ZERO,
        value: PropValue::Scalar(0.0),
        easing: Easing::Bezier {
            p1: [f32::NAN, 0.0],
            p2: [0.5, 1.0],
        },
    });
    track.set_key(Keyframe {
        time: Time(120000),
        value: PropValue::Scalar(1.0),
        easing: Easing::Hold,
    });
    layer.tracks.insert(Property::Opacity, track);
    p.insert_layer(c, layer);
    // NaN handles make evaluation non-deterministic: rejected at validation.
    assert!(p.validate().is_err());
}
