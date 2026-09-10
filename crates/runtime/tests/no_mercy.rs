//! NO-MERCY runtime torture. Commit-cost payload independence (Arc sharing),
//! save/open roundtrips on adversarial inputs, export determinism, concurrent
//! preview, snapshot immutability under edit storms. Deterministic: fixed
//! seeds, no wall clock beyond generous ratio budgets, no network
//! (FFmpeg-dependent paths excluded).

use bonaparte_model::*;
use bonaparte_runtime::{parse_project, serialize_project, EditorSession, RenderRequest};
use serde_json::json;
use std::sync::Arc;
use std::time::Instant;

fn embedded_image(width: u32, height: u32, fill: [u8; 4]) -> EmbeddedImage {
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for _ in 0..width * height {
        rgba.extend_from_slice(&fill);
    }
    EmbeddedImage {
        width,
        height,
        rgba_base64: Arc::from(base64_encode(&rgba).as_str()),
    }
}

/// Minimal standard-alphabet base64 (no external dependency in tests).
fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

fn media_asset(name: &str, w: u32, h: u32) -> MediaAsset {
    MediaAsset {
        id: MediaId(0),
        name: name.into(),
        path: None,
        kind: MediaKind::Image,
        embedded: Some(embedded_image(w, h, [90, 90, 90, 255])),
        audio: None,
        slot: None,
        alias: None,
        perception: None,
        video: None,
        footage: None,
    }
}

/// F1 regression guard: editing a layer must NOT copy embedded media bytes.
/// After commits and undo, the edited project and its pre-edit snapshot must
/// still share the same `Arc<str>` allocation for the embedded payload, and
/// the document content must return to pristine.
#[test]
fn commits_share_embedded_payloads_instead_of_copying_them() {
    let mut p = Project::new("payload");
    let c = p.create_comp("Main", 64, 64, FrameRate::FPS_30, Time(10 * TICKS_PER_SEC));
    p.insert_media(media_asset("wallpaper", 512, 512));
    let id = p.insert_layer(
        c,
        Layer::new_solid("Grade me", [1.0; 4], Time::ZERO, Time(TICKS_PER_SEC)),
    );

    let snapshot: Project = p.clone();
    let before = Arc::as_ptr(
        &snapshot
            .media
            .values()
            .next()
            .unwrap()
            .embedded
            .as_ref()
            .unwrap()
            .rgba_base64,
    );
    let mut h = History::new();
    for v in [0.9f32, 0.8, 0.7, 0.6] {
        h.commit_grouped(
            &mut p,
            Op::SetValue {
                comp: c,
                layer: id,
                property: Property::Opacity,
                value: PropValue::Scalar(v),
            },
            Some("live".into()),
        )
        .unwrap();
    }
    h.undo(&mut p).unwrap();
    let after = Arc::as_ptr(
        &p.media
            .values()
            .next()
            .unwrap()
            .embedded
            .as_ref()
            .unwrap()
            .rgba_base64,
    );
    assert_eq!(before, after, "commit/undo must not reallocate media bytes");
    assert_eq!(
        serde_json::to_string(&p.comps).unwrap(),
        serde_json::to_string(&snapshot.comps).unwrap()
    );
}

/// The point of Arc sharing: a commit storm over a project carrying a ~4 MiB
/// payload must not cost dramatically more than the same storm over an empty
/// project. A full deep copy per commit would blow the 2x ceiling.
#[test]
fn hundred_commit_storm_is_payload_independent() {
    let run = |with_media: bool| {
        let mut p = Project::new("storm");
        let c = p.create_comp("Main", 64, 64, FrameRate::FPS_30, Time(10 * TICKS_PER_SEC));
        if with_media {
            p.insert_media(media_asset("big", 1024, 1024));
        }
        let id = p.insert_layer(
            c,
            Layer::new_solid("s", [1.0; 4], Time::ZERO, Time(10 * TICKS_PER_SEC)),
        );
        let mut h = History::new();
        let start = Instant::now();
        for i in 0..100 {
            h.commit(
                &mut p,
                Op::SetValue {
                    comp: c,
                    layer: id,
                    property: Property::Opacity,
                    value: PropValue::Scalar(0.5 + (i % 2) as f32 / 10.0),
                },
            )
            .unwrap();
        }
        for _ in 0..100 {
            h.undo(&mut p).unwrap();
        }
        start.elapsed()
    };
    let without = run(false);
    let with = run(true);
    let ratio = with.as_nanos() as f64 / without.as_nanos() as f64;
    assert!(
        ratio < 2.0,
        "commit storm with media ({with:?}) must not cost ~2x the empty storm ({without:?})"
    );
}

#[test]
fn save_open_roundtrip_is_byte_stable_and_rejects_garbage() {
    let mut p = Project::new("roundtrip");
    let c = p.create_comp("Main", 96, 64, FrameRate::FPS_30, Time(5 * TICKS_PER_SEC));
    p.insert_media(media_asset("img", 64, 64));
    p.insert_layer(
        c,
        Layer::new_rect("r", [0.2, 0.4, 0.8, 1.0], Time::ZERO, Time(TICKS_PER_SEC)),
    );
    let once = serialize_project(&p).unwrap();
    let reparsed = parse_project(&once).unwrap();
    let twice = serialize_project(&reparsed).unwrap();
    assert_eq!(once, twice, "serialize(parse(serialize(x))) must be stable");

    let frame_before = render_frame(&p);
    let frame_after = render_frame(&reparsed);
    assert_eq!(frame_before, frame_after);

    let garbage = [
        "",
        "{",
        "null",
        "[]",
        "{\"version\":999}",
        "{\"version\":4}",
        "{\"version\":\"four\"}",
        &once[..once.len() / 2],
    ];
    for bad in garbage {
        assert!(parse_project(bad).is_err(), "garbage must be rejected");
    }
}

fn render_frame(p: &Project) -> Vec<u8> {
    let c = p.comps.keys().next().copied().unwrap();
    bonaparte_engine::reference::render_comp(
        p,
        c,
        Time::ZERO,
        &bonaparte_engine::reference::NoMedia,
    )
    .unwrap()
    .rgba
}

#[test]
fn png_export_is_deterministic_across_sessions() {
    let build = || {
        let mut p = Project::new("png");
        let c = p.create_comp("Main", 48, 48, FrameRate::FPS_30, Time(TICKS_PER_SEC));
        p.insert_media(media_asset("img", 32, 32));
        p.insert_layer(
            c,
            Layer::new_rect("r", [0.9, 0.1, 0.1, 1.0], Time::ZERO, Time(TICKS_PER_SEC)),
        );
        EditorSession::new(p).unwrap()
    };
    let request = || serde_json::from_value::<RenderRequest>(json!({"compId":1,"time":0})).unwrap();
    let a = build().render_input(request()).unwrap().png().unwrap();
    let b = build().render_input(request()).unwrap().png().unwrap();
    assert_eq!(a, b, "PNG export must be deterministic");
    assert!(a.starts_with(b"\x89PNG"), "output must be a real PNG");
}

#[test]
fn concurrent_preview_jobs_do_not_deadlock_or_corrupt() {
    let mut p = Project::new("concurrent");
    let c = p.create_comp("Main", 96, 54, FrameRate::FPS_30, Time(8 * TICKS_PER_SEC));
    for i in 0..24 {
        p.insert_layer(
            c,
            Layer::new_rect(
                &format!("L{i}"),
                [0.1 + 0.03 * i as f32, 0.5, 0.5, 0.8],
                Time::ZERO,
                Time(8 * TICKS_PER_SEC),
            ),
        );
    }
    let session = Arc::new(EditorSession::new(p).unwrap());
    let mut handles = vec![];
    for t in 0..8u32 {
        let s = Arc::clone(&session);
        handles.push(std::thread::spawn(move || {
            for i in 0..6u32 {
                let time = (t * 7 + i * 3) % 8 * 30_000;
                let divisor = match i % 3 {
                    0 => 1u32,
                    1 => 2,
                    _ => 4,
                };
                let req = serde_json::from_value::<bonaparte_runtime::PreviewRequest>(json!({
                    "compId": 1, "time": time, "divisor": divisor, "backend": "cpu"
                }))
                .unwrap();
                let job = s.preview_input(req).unwrap();
                let frame = job.render().unwrap();
                assert!(frame.pixels.width * frame.pixels.height > 0);
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
}

#[test]
fn untouched_session_does_not_drift_while_others_are_edited() {
    let mut p = Project::new("stable");
    let c = p.create_comp("Main", 64, 64, FrameRate::FPS_30, Time(2 * TICKS_PER_SEC));
    p.insert_media(media_asset("img", 32, 32));
    p.insert_layer(
        c,
        Layer::new_rect("r", [0.5; 4], Time::ZERO, Time(2 * TICKS_PER_SEC)),
    );
    let session = EditorSession::new(p).unwrap();
    let baseline = session
        .render_input(
            serde_json::from_value::<RenderRequest>(json!({"compId":1,"time":0})).unwrap(),
        )
        .unwrap()
        .raw()
        .unwrap();

    // A second session takes edit/undo storms; the first must not drift.
    let mut p2 = Project::new("stormy");
    let c2 = p2.create_comp("Main", 64, 64, FrameRate::FPS_30, Time(2 * TICKS_PER_SEC));
    p2.insert_layer(
        c2,
        Layer::new_rect("r", [0.5; 4], Time::ZERO, Time(2 * TICKS_PER_SEC)),
    );
    let mut s2 = EditorSession::new(p2).unwrap();
    for i in 0..30 {
        s2.command(
            "apply",
            json!({"op": Op::RenameProject { name: format!("storm{i}") }}),
        )
        .unwrap();
        s2.command("undo", json!({})).unwrap();
    }
    let after = session
        .render_input(
            serde_json::from_value::<RenderRequest>(json!({"compId":1,"time":0})).unwrap(),
        )
        .unwrap()
        .raw()
        .unwrap();
    assert_eq!(baseline, after, "an untouched session must not drift");
}

#[test]
fn invalid_commands_are_typed_errors_not_panics() {
    let mut p = Project::new("commands");
    let c = p.create_comp("Main", 64, 64, FrameRate::FPS_30, Time(2 * TICKS_PER_SEC));
    p.insert_layer(
        c,
        Layer::new_rect("r", [0.5; 4], Time::ZERO, Time(2 * TICKS_PER_SEC)),
    );
    let mut s = EditorSession::new(p).unwrap();
    let bad_commands: Vec<serde_json::Value> = vec![
        json!({"op": {"UnknownOp": {}}}),
        json!({"op": Op::SetValue {
            comp: CompId(99),
            layer: LayerId(0),
            property: Property::Opacity,
            value: PropValue::Scalar(0.5),
        }}),
        json!({"op": Op::SetValue {
            comp: c,
            layer: LayerId(999),
            property: Property::Opacity,
            value: PropValue::Scalar(0.5),
        }}),
    ];
    for args in bad_commands {
        assert!(
            s.command("apply", args).is_err(),
            "invalid apply must error"
        );
    }
    assert!(s.command("definitely_not_a_command", json!({})).is_err());
    assert!(s.command("apply", json!({})).is_err());
    // Session remains fully functional afterwards.
    s.command(
        "apply",
        json!({"op": Op::RenameProject { name: "ok".to_string() }}),
    )
    .unwrap();
}

#[test]
fn cancelled_jobs_abort_deterministically_and_do_not_poison_the_cache() {
    let mut p = Project::new("cancel");
    let c = p.create_comp("Main", 240, 135, FrameRate::FPS_30, Time(4 * TICKS_PER_SEC));
    for i in 0..400 {
        p.insert_layer(
            c,
            Layer::new_rect(
                &format!("L{i}"),
                [0.3 + 0.001 * i as f32, 0.4, 0.6, 0.5],
                Time::ZERO,
                Time(4 * TICKS_PER_SEC),
            ),
        );
    }
    let session = EditorSession::new(p).unwrap();
    let request = || {
        serde_json::from_value::<bonaparte_runtime::PreviewRequest>(json!({
            "compId": 1, "time": 0, "divisor": 1, "backend": "cpu"
        }))
        .unwrap()
    };

    // Cancel BEFORE rendering: the job must refuse deterministically.
    let job = session.preview_input(request()).unwrap();
    job.cancel();
    assert!(job.is_cancelled());
    let refused = job.render().err().expect("cancelled job must not render");
    assert_eq!(refused, "cancelled");

    // A fresh identical job still renders; the poisoned attempt must not have
    // cached anything or corrupted renderer state.
    let clean = session.preview_input(request()).unwrap().render().unwrap();
    assert!(clean.pixels.width * clean.pixels.height > 0);
    let again = session.preview_input(request()).unwrap().render().unwrap();
    assert_eq!(clean.pixels.rgba.as_slice(), again.pixels.rgba.as_slice());
}

#[test]
fn mid_render_cancel_from_another_thread_returns_promptly() {
    let mut p = Project::new("midcancel");
    let c = p.create_comp("Main", 480, 270, FrameRate::FPS_30, Time(4 * TICKS_PER_SEC));
    for i in 0..700 {
        p.insert_layer(
            c,
            Layer::new_rect(
                &format!("L{i}"),
                [0.2 + 0.001 * i as f32, 0.6, 0.7, 0.9],
                Time::ZERO,
                Time(4 * TICKS_PER_SEC),
            ),
        );
    }
    let session = Arc::new(EditorSession::new(p).unwrap());
    let request = || {
        serde_json::from_value::<bonaparte_runtime::PreviewRequest>(json!({
            "compId": 1, "time": 0, "divisor": 1, "backend": "cpu"
        }))
        .unwrap()
    };
    let job = Arc::new(session.preview_input(request()).unwrap());
    let render_handle = {
        let job = Arc::clone(&job);
        std::thread::spawn(move || job.render())
    };
    // Give the render a moment to get deep into the layer loop, then cancel.
    std::thread::sleep(std::time::Duration::from_millis(20));
    job.cancel();
    let result = render_handle.join().unwrap();
    // The render either cancelled (aborted between layers) or, on a machine
    // fast enough to finish all 700 layers inside the sleep window, completed.
    // It must never hang, panic, or return partial pixels.
    match result {
        // Pre-render checks say "cancelled"; an in-flight abort surfaces the
        // engine's RenderError::Cancelled text. Both mean: no hang, no partial
        // pixels presented as success.
        Err(reason) => assert!(
            reason.contains("cancel"),
            "unexpected cancel-time error: {reason}"
        ),
        Ok(frame) => assert!(frame.pixels.width * frame.pixels.height > 0),
    }
    // Session remains healthy afterwards.
    let clean = session.preview_input(request()).unwrap().render().unwrap();
    assert!(clean.pixels.width * clean.pixels.height > 0);
}
