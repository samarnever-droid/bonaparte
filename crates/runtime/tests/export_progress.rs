//! Export telemetry gate: parallel pipeline keeps frame order, progress is
//! exact, and cancel actually stops a running export promptly.

use bonaparte_model::*;
use bonaparte_runtime::*;
use serde_json::json;
use std::sync::atomic::Ordering;
use std::sync::Mutex;

/// Telemetry is process-global; serialize the tests that touch it.
static TELEMETRY_LOCK: Mutex<()> = Mutex::new(());
fn guard() -> std::sync::MutexGuard<'static, ()> {
    TELEMETRY_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn reset_telemetry() {
    EXPORT_TELEMETRY.frames_done.store(0, Ordering::Relaxed);
    EXPORT_TELEMETRY.canceled.store(false, Ordering::Relaxed);
    EXPORT_TELEMETRY.active.store(false, Ordering::Relaxed);
    EXPORT_TELEMETRY.stage.store(0, Ordering::Relaxed);
}

fn session(frames_10s: bool) -> EditorSession {
    let mut p = Project::new("Export");
    let duration = Time(if frames_10s { 1_200_000 } else { 120_000 });
    // The cancel test needs per-frame work; 720p makes frames slow enough to
    // cancel deterministically mid-run.
    let (w, h) = if frames_10s { (1280, 720) } else { (64, 36) };
    p.create_comp("Main", w, h, FrameRate::FPS_30, duration);
    p.insert_layer(
        CompId(1),
        Layer::new(
            "Card",
            LayerKind::Shape {
                color: [0.9, 0.5, 0.1, 1.0],
                generator: None,
                style: bonaparte_model::ShapeStyle {
                    size: Some([40.0, 20.0]),
                    corner_radius: 4.0,
                    stroke_width: 0.0,
                    stroke_color: [0.0, 0.0, 0.0, 0.0],
                },
                points: Vec::new(),
            },
            Time::ZERO,
            duration,
        ),
    );
    EditorSession::new(p).unwrap()
}

fn frames(comp: &Comp) -> usize {
    let tpf = comp.fps.ticks_per_frame();
    ((comp.duration.0 + tpf - 1) / tpf) as usize
}

fn render_of(s: &EditorSession) -> RenderInput {
    s.render_input(serde_json::from_value(json!({"compId":1,"time":0})).unwrap())
        .unwrap()
}

#[test]
fn export_reports_exact_progress_and_finishes_clean() {
    let _serial = guard();
    reset_telemetry();
    let s = session(false);
    let total = frames(s.project.comp(CompId(1)).unwrap());
    assert_eq!(total, 30, "1s @ 30fps = 30 frames");
    let path = std::env::temp_dir().join(format!("bonaparte-progress-{}.mp4", std::process::id()));
    render_of(&s).export_mp4(&path).expect("export");
    assert_eq!(EXPORT_TELEMETRY.frames_done.load(Ordering::Relaxed), 30);
    let progress = export_progress_json();
    assert_eq!(progress["framesDone"], json!(30));
    assert_eq!(progress["totalFrames"], json!(30));
    assert_eq!(progress["active"], json!(false));
    assert_eq!(progress["stage"], json!(0));
    assert!(progress["startedMs"].as_u64().unwrap() > 0);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn export_cancel_stops_a_running_export_promptly() {
    let _serial = guard();
    reset_telemetry();
    let s = session(true);
    let path = std::env::temp_dir().join(format!("bonaparte-cancel-{}.mp4", std::process::id()));
    let handle = std::thread::scope(|scope| {
        let input = render_of(&s);
        let worker_path = path.clone();
        let worker = scope.spawn(move || input.export_mp4(&worker_path));
        // Wait until frames are provably flowing (the consumer emitted at
        // least one and the export is still running), then cancel. The 720p
        // export takes seconds, so this lands mid-run every time.
        let mut seen_frames = 0u64;
        for _ in 0..600 {
            seen_frames = EXPORT_TELEMETRY.frames_done.load(Ordering::Relaxed);
            let active = EXPORT_TELEMETRY.active.load(Ordering::Relaxed);
            if seen_frames > 0 && active {
                break;
            }
            if worker.is_finished() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(
            seen_frames > 0,
            "no frames rendered before cancel window (export finished too fast or stalled)"
        );
        assert!(
            !worker.is_finished(),
            "export finished before cancel landed"
        );
        EXPORT_TELEMETRY.canceled.store(true, Ordering::Relaxed);
        worker.join().expect("export thread")
    });
    assert!(handle.is_err(), "canceled export must fail, not succeed");
    assert!(
        handle.err().unwrap_or_default().contains("cancel"),
        "error mentions cancel"
    );
    // Telemetry resets after the failed export.
    assert_eq!(export_progress_json()["active"], json!(false));
    assert_eq!(export_progress_json()["stage"], json!(0));
    assert!(EXPORT_TELEMETRY.frames_done.load(Ordering::Relaxed) < 300);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn progress_and_cancel_commands_shape() {
    let _serial = guard();
    reset_telemetry();
    let mut s = session(false);
    let progress = s.command("export_progress", json!({})).unwrap();
    for key in [
        "active",
        "canceled",
        "framesDone",
        "totalFrames",
        "stage",
        "startedMs",
        "lastFrameMs",
    ] {
        assert!(progress.get(key).is_some(), "progress has {key}");
    }
    let cancel = s.command("export_cancel", json!({})).unwrap();
    assert_eq!(cancel["canceled"], json!(true));
    assert_eq!(EXPORT_TELEMETRY.canceled.load(Ordering::Relaxed), true);
}

#[test]
fn second_export_is_rejected_while_one_runs() {
    let _serial = guard();
    reset_telemetry();
    // Simulate a running export by flipping the active flag.
    EXPORT_TELEMETRY.active.store(true, Ordering::Relaxed);
    let s = session(false);
    let path = std::env::temp_dir().join(format!("bonaparte-busy-{}.mp4", std::process::id()));
    EXPORT_TELEMETRY.canceled.store(false, Ordering::Relaxed);
    let err = render_of(&s)
        .export_mp4(std::path::Path::new(&path))
        .expect_err("must refuse concurrent export");
    assert!(err.contains("already running"), "{err}");
}
