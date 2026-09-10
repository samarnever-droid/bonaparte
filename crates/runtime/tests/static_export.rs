//! Static-span export reuse: the analysis is provably right, and the export
//! pipeline produces the same film either way. The frame-identity test needs
//! FFmpeg (same convention as the export-pipeline tests elsewhere) and skips
//! cleanly when it is absent.

use bonaparte_model::{
    Comp, Easing, FrameRate, Keyframe, Layer, LayerId, LayerKind, Op, PropValue, Time, Track,
};
use bonaparte_runtime::statics;

fn comp() -> Comp {
    Comp {
        id: bonaparte_model::CompId(1),
        name: "Statics".into(),
        width: 160,
        height: 96,
        fps: FrameRate { num: 30, den: 1 },
        duration: Time::from_secs_f64(6.0),
        background: [0.0, 0.0, 0.0, 1.0],
        layer_order: Vec::new(),
        layers: Default::default(),
        audio: Default::default(),
        camera: Default::default(),
        turntable: Default::default(),
    }
}

fn solid(id: u64, mut comp: Comp, mut layer: Layer) -> Comp {
    layer.id = LayerId(id);
    comp.layer_order.push(LayerId(id));
    comp.layers.insert(LayerId(id), layer);
    comp
}

fn project(comp: Comp) -> bonaparte_model::Project {
    let max_layer = comp.layer_order.iter().map(|id| id.0).max().unwrap_or(0);
    let mut p = bonaparte_model::Project::new("Statics");
    p.comps.insert(comp.id, comp);
    p.next_comp = bonaparte_model::CompId(2);
    p.next_layer = bonaparte_model::LayerId(max_layer + 1);
    p
}

fn layer(name: &str, start: f64, dur: f64) -> Layer {
    Layer::new(
        name,
        LayerKind::Solid {
            color: [1.0, 0.5, 0.0, 1.0],
        },
        Time::from_secs_f64(start),
        Time::from_secs_f64(dur),
    )
}

#[test]
fn a_still_comp_is_one_solid_span() {
    let mut c = comp();
    c = solid(1, c, layer("floor", 0.0, 6.0));
    let cid = c.id;
    let p = project(c);
    let spans = statics::static_spans(&p, cid, Time::ZERO, Time::from_secs_f64(6.0)).unwrap();
    assert_eq!(spans.len(), 1, "the whole range is static");
    assert_eq!(
        (spans[0].0, spans[0].1),
        (0, Time::from_secs_f64(6.0).0),
        "span covers the export range"
    );
    // 180 frames, one anchor: frames 1..179 all reuse frame 0.
    let tpf = 120_000 / 30;
    let plan = statics::reuse_plan(
        &p,
        cid,
        Time::ZERO,
        Time::from_secs_f64(6.0),
        tpf,
        180,
        160 * 96 * 4,
    )
    .expect("a still export must reuse");
    assert!(plan.iter().all(|&anchor| anchor == 0));
    assert_eq!(plan[0], 0, "the anchor frame anchors itself");
}

#[test]
fn settled_motion_reuses_only_after_the_last_keyframe() {
    let mut c = comp();
    let mut l = layer("slider", 0.0, 6.0);
    let mut track = Track::new();
    track.set_key(Keyframe {
        time: Time::from_secs_f64(0.0),
        value: PropValue::Scalar(0.0),
        easing: Easing::Linear,
    });
    track.set_key(Keyframe {
        time: Time::from_secs_f64(2.0),
        value: PropValue::Scalar(90.0),
        easing: Easing::Linear,
    });
    l.tracks.insert(bonaparte_model::Property::Rotation, track);
    c = solid(1, c, l);
    let cid = c.id;
    let p = project(c);
    let end = Time::from_secs_f64(6.0);
    let spans = statics::static_spans(&p, cid, Time::ZERO, end).unwrap();
    assert_eq!(
        spans,
        vec![(Time::from_secs_f64(2.0).0, end.0)],
        "only after the ramp settles is content constant"
    );
    let tpf = 120_000 / 30;
    let count = 180;
    let plan = statics::reuse_plan(&p, cid, Time::ZERO, end, tpf, count, 160 * 96 * 4).unwrap();
    for k in 0..60 {
        assert_eq!(plan[k], k, "moving frames render individually");
    }
    for k in 60..count {
        assert_eq!(plan[k], 60, "settled frames reuse the anchor at 2s");
    }
}

#[test]
fn hold_keys_are_static_between_the_steps() {
    let mut c = comp();
    let mut l = layer("steps", 0.0, 6.0);
    let mut track = Track::new();
    for (t, v) in [(0.0, 10.0), (2.0, 50.0), (4.0, 90.0)] {
        track.set_key(Keyframe {
            time: Time::from_secs_f64(t),
            value: PropValue::Scalar(v),
            easing: Easing::Hold,
        });
    }
    l.tracks.insert(bonaparte_model::Property::Rotation, track);
    c = solid(1, c, l);
    let cid = c.id;
    let p = project(c);
    let spans = statics::static_spans(&p, cid, Time::ZERO, Time::from_secs_f64(6.0)).unwrap();
    assert_eq!(spans.len(), 3, "three held segments, boundary at each step");
}

#[test]
fn self_moving_content_forbids_reuse() {
    // Footage layer (file-backed): dynamic by contract.
    let mut c = comp();
    let media = bonaparte_model::MediaAsset {
        id: bonaparte_model::MediaId(1),
        name: "clip.mp4".into(),
        path: Some("/nonexistent/clip.mp4".into()),
        kind: bonaparte_model::MediaKind::Video {
            fps: FrameRate { num: 30, den: 1 },
            duration: Time::from_secs_f64(6.0),
        },
        embedded: None,
        audio: None,
        slot: None,
        alias: None,
        perception: None,
        video: None,
        footage: Some(bonaparte_model::FootageSource {
            path: "/nonexistent/clip.mp4".into(),
            width: 160,
            height: 96,
            frame_rate: FrameRate { num: 30, den: 1 },
            duration: Time::from_secs_f64(6.0),
            proxy_path: None,
            proxy_size: None,
        }),
    };
    c.layers.insert(
        LayerId(1),
        Layer::new(
            "clip",
            LayerKind::Footage {
                media: bonaparte_model::MediaId(1),
                source_start: Time::ZERO,
            },
            Time::ZERO,
            Time::from_secs_f64(6.0),
        ),
    );
    c.layer_order.push(LayerId(1));
    let cid = c.id;
    let mut p = project(c);
    p.media.insert(bonaparte_model::MediaId(1), media);
    assert_eq!(
        statics::static_spans(&p, cid, Time::ZERO, Time::from_secs_f64(6.0)),
        None,
        "footage plays independently of keys"
    );

    // Turntable on a still comp also forbids it.
    let mut c2 = comp();
    c2 = solid(1, c2, layer("floor", 0.0, 6.0));
    c2.turntable.enabled = true;
    let c2_id = c2.id;
    let p2 = project(c2);
    assert_eq!(
        statics::static_spans(&p2, c2_id, Time::ZERO, Time::from_secs_f64(6.0)),
        None
    );
}

#[test]
fn precomp_motion_opens_a_hole_in_the_parent_span() {
    // Child spins from 1s to 3s (child-local); the parent's static range must
    // break around exactly that window shifted by the layer start.
    let mut child = comp();
    child.id = bonaparte_model::CompId(2);
    let mut cl = layer("spinner", 0.0, 6.0);
    let mut track = Track::new();
    track.set_key(Keyframe {
        time: Time::from_secs_f64(1.0),
        value: PropValue::Scalar(0.0),
        easing: Easing::Linear,
    });
    track.set_key(Keyframe {
        time: Time::from_secs_f64(3.0),
        value: PropValue::Scalar(180.0),
        easing: Easing::Linear,
    });
    cl.tracks.insert(bonaparte_model::Property::Rotation, track);
    child = solid(10, child, cl);

    let mut parent = comp();
    let mut shell = Layer::new(
        "nested",
        LayerKind::PreComp {
            comp: bonaparte_model::CompId(2),
        },
        Time::from_secs_f64(0.5),
        Time::from_secs_f64(5.5),
    );
    shell.id = LayerId(1);
    parent.layer_order.push(LayerId(1));
    parent.layers.insert(LayerId(1), shell);

    let mut p = bonaparte_model::Project::new("Nested");
    p.comps.insert(bonaparte_model::CompId(1), parent.clone());
    p.comps.insert(bonaparte_model::CompId(2), child);
    p.next_comp = bonaparte_model::CompId(3);
    p.next_layer = bonaparte_model::LayerId(11);
    let spans = statics::static_spans(&p, parent.id, Time::ZERO, Time::from_secs_f64(6.0)).unwrap();
    // The moving window lands at parent time 1.5s..3.5s.
    assert!(
        spans
            .iter()
            .all(|(a, b)| !(a < &(Time::from_secs_f64(3.0).0)
                && b > &(Time::from_secs_f64(2.0).0)
                && *b - *a < 120_000)),
        "no span may cross the child animation: {spans:?}"
    );
    assert!(
        spans
            .iter()
            .any(|(a, b)| a <= &(Time::from_secs_f64(4.0).0) && b >= &(Time::from_secs_f64(5.0).0)),
        "after the child settles the parent is static again: {spans:?}"
    );
}

fn ffmpeg_available() -> bool {
    std::process::Command::new("ffmpeg")
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[test]
fn reused_frames_are_frame_identical_in_the_written_file() {
    if !ffmpeg_available() {
        eprintln!("skipping frame identity: no FFmpeg");
        return;
    }
    let _serial = static_export_lock();
    let mut c = comp();
    // Two solids with a hold step at 2s: three static spans, 120 frames.
    c = solid(1, c, layer("floor", 0.0, 6.0));
    let mut top = layer("top", 0.0, 6.0);
    top.kind = LayerKind::Solid {
        color: [0.1, 0.2, 1.0, 1.0],
    };
    let mut track = Track::new();
    track.set_key(Keyframe {
        time: Time::from_secs_f64(2.0),
        value: PropValue::Vec2([0.0, 0.0]),
        easing: Easing::Hold,
    });
    track.set_key(Keyframe {
        time: Time::from_secs_f64(4.0),
        value: PropValue::Vec2([40.0, 0.0]),
        easing: Easing::Hold,
    });
    top.tracks
        .insert(bonaparte_model::Property::Position, track);
    c = solid(2, c, top);
    let session = bonaparte_runtime::EditorSession::new(project(c)).unwrap();
    let out = std::env::temp_dir().join(format!(
        "bonaparte-statics-{}-identity.mp4",
        std::process::id()
    ));
    // The runtime entry point of the shared pipeline (the bridge's
    // export_video lands on the very same RenderInput::export_mp4).
    let input = session
        .render_input(bonaparte_runtime::RenderRequest {
            comp_id: bonaparte_model::CompId(1),
            time: Time::ZERO,
            bypass_effects: false,
            layer_override: None,
            bit_depth: 8,
            output_space: Default::default(),
            source_quality: true,
            draft: false,
        })
        .unwrap();
    input.export_mp4(&out).unwrap();
    let grab = |t: f64| {
        let png = out.with_file_name(format!("bonaparte-statics-frame{t}.png"));
        std::process::Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-ss", &t.to_string(), "-i"])
            .arg(&out)
            .args(["-frames:v", "1"])
            .arg(&png)
            .status()
            .unwrap();
        std::fs::read(&png).unwrap_or_else(|_| panic!("no frame at {t}"))
    };
    // 0.5s and 1.5s sit in one static span; their frames must be identical.
    assert_eq!(grab(0.5), grab(1.5), "reused span produced stable pixels");
    // 2.5s and 3.5s are the second hold; also identical.
    assert_eq!(grab(2.5), grab(3.5), "second span stable too");
    // The layer steps at 4s, so the third span differs from the first.
    assert_ne!(
        grab(0.5),
        grab(4.5),
        "the hold step must actually change pixels — reuse cannot flatten a moving comp"
    );
    let _ = std::fs::remove_file(&out);
    for t in [0.5f64, 1.5, 2.5, 3.5, 4.5] {
        let _ = std::fs::remove_file(out.with_file_name(format!("bonaparte-statics-frame{t}.png")));
    }
}

fn static_export_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|p| p.into_inner())
}
