//! # bonaparte-app
//!
//! The Tauri shell: windows, menus, dialogs, IPC commands, and the UI↔model
//! bridge. Every IPC command wraps an `Op` — the UI has no powers the MCP
//! server doesn't, and vice versa (microkernel dogfooding, RULES §4.1).
//!
//! The frame endpoint serves CPU reference-rendered frames; when the WASM
//! preview engine lands in the webview, playback stops crossing the bridge
//! and this endpoint remains only for export and thumbnails.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::{Mutex, MutexGuard};

use bonaparte_engine::reference::render_comp;
use bonaparte_model::{CompId, History, Layer, LayerKind, Op, Project, StaticTransform};

/// The app's single source of truth. Both mutexes guard small, fast critical
/// sections; commands clone the document out and never hold a lock across
/// await points (there are none — every command is synchronous).
pub struct AppState {
    project: Mutex<Project>,
    history: Mutex<History>,
}

fn locked<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // Poisoned = a command panicked mid-mutation. The document is the source
    // of truth, so we recover the data rather than guess about consistency.
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[tauri::command]
fn project_state(state: tauri::State<AppState>) -> Project {
    locked(&state.project).clone()
}

/// The only mutation path. Validates via `Op::apply` and records the inverse
/// for undo — identical semantics to what the MCP server will use.
#[tauri::command]
fn apply_op(state: tauri::State<AppState>, op: Op) -> Result<Project, String> {
    let mut project = locked(&state.project);
    let mut history = locked(&state.history);
    history
        .commit(&mut project, op)
        .map_err(|e| e.to_string())?;
    Ok(project.clone())
}

#[tauri::command]
fn undo(state: tauri::State<AppState>) -> Result<Option<Project>, String> {
    let mut project = locked(&state.project);
    let changed = locked(&state.history)
        .undo(&mut project)
        .map_err(|e| e.to_string())?;
    Ok(changed.then(|| project.clone()))
}

#[tauri::command]
fn redo(state: tauri::State<AppState>) -> Result<Option<Project>, String> {
    let mut project = locked(&state.project);
    let changed = locked(&state.history)
        .redo(&mut project)
        .map_err(|e| e.to_string())?;
    Ok(changed.then(|| project.clone()))
}

/// Raw RGBA8 frame (sRGB bytes, top-left origin, row-major) for the viewport.
#[derive(serde::Serialize)]
struct FrameDto {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

#[tauri::command]
fn render_frame(state: tauri::State<AppState>, comp_id: u64, time: f64) -> Result<FrameDto, String> {
    let project = locked(&state.project);
    let t = if time.abs() >= 10_000.0 {
        bonaparte_model::Time(time as i64)
    } else {
        bonaparte_model::Time::from_secs_f64(time)
    };
    let frame = render_comp(&project, CompId(comp_id), t, &bonaparte_engine::reference::NoMedia)
        .map_err(|e| e.to_string())?;
    Ok(FrameDto {
        width: frame.width,
        height: frame.height,
        rgba: frame.rgba,
    })
}

/// Fast binary frame endpoint returning 8-byte header (u32 width, u32 height LE) + raw RGBA bytes.
/// Resolves as ArrayBuffer in JS, bypassing JSON serialization overhead during scrubbing.
#[tauri::command]
fn render_frame_raw(state: tauri::State<AppState>, comp_id: u64, time: f64) -> Result<tauri::ipc::Response, String> {
    let project = locked(&state.project);
    let t = if time.abs() >= 10_000.0 {
        bonaparte_model::Time(time as i64)
    } else {
        bonaparte_model::Time::from_secs_f64(time)
    };
    let frame = render_comp(&project, CompId(comp_id), t, &bonaparte_engine::reference::NoMedia)
        .map_err(|e| e.to_string())?;
    let mut data = Vec::with_capacity(8 + frame.rgba.len());
    data.extend_from_slice(&frame.width.to_le_bytes());
    data.extend_from_slice(&frame.height.to_le_bytes());
    data.extend_from_slice(&frame.rgba);
    Ok(tauri::ipc::Response::new(data))
}

/// Streaming frame-by-frame MP4 video export into FFmpeg stdin with O(1) memory footprint.
#[tauri::command]
fn export_video(
    state: tauri::State<AppState>,
    comp_id: u64,
    output_path: String,
    fps: Option<f64>,
) -> Result<String, String> {
    let project = locked(&state.project);
    let comp = project
        .comp(CompId(comp_id))
        .ok_or_else(|| format!("Composition {comp_id} not found"))?;

    let width = comp.width;
    let height = comp.height;
    let comp_fps_f64 = comp.fps.num as f64 / comp.fps.den as f64;
    let fps_val = fps.unwrap_or(comp_fps_f64);
    let duration_secs = comp.duration.as_secs_f64();
    let total_frames = (duration_secs * fps_val).round().max(1.0) as usize;

    if let Some(parent) = std::path::Path::new(&output_path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }

    let mut child = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-f", "rawvideo",
            "-pix_fmt", "rgba",
            "-s", &format!("{width}x{height}"),
            "-r", &format!("{fps_val}"),
            "-i", "-",
            "-c:v", "libx264",
            "-pix_fmt", "yuv420p",
            &output_path,
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| {
            format!("FFmpeg spawn failed ('{e}'). Ensure FFmpeg is installed and on system PATH.")
        })?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "Failed to open FFmpeg stdin pipe".to_string())?;

    for f in 0..total_frames {
        let frame_time_secs = f as f64 / fps_val;
        let frame_time = bonaparte_model::Time::from_secs_f64(frame_time_secs);
        let frame = render_comp(&project, CompId(comp_id), frame_time, &bonaparte_engine::reference::NoMedia)
            .map_err(|e| {
                let _ = child.kill();
                format!("Render error at frame {f}: {e}")
            })?;

        use std::io::Write;
        stdin.write_all(&frame.rgba).map_err(|e| {
            let _ = child.kill();
            format!("Error streaming frame {f} into FFmpeg stdin: {e}")
        })?;
    }

    drop(stdin); // Signal EOF to FFmpeg

    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("FFmpeg export failed: {stderr}"));
    }

    Ok(format!("Successfully exported {total_frames} frames to {output_path}"))
}

/// Human-readable undo history for the legible-undo UI (RULES §7).
#[tauri::command]
fn history_list(state: tauri::State<AppState>) -> Vec<String> {
    locked(&state.history).undo_descriptions()
}

/// Seed the demo comp the shell opens with: one solid + one shape, so the
/// first launch already has something to keyframe and drag.
fn demo_project() -> Project {
    use bonaparte_model::{BlendMode, FrameRate, Property, Time};

    let mut project = Project::new("Untitled");
    let comp = project.create_comp("Main", 640, 360, FrameRate::FPS_30, Time::from_secs_f64(8.0));
    let c = project.comps.get_mut(&comp).unwrap();
    c.background = [0.08, 0.08, 0.10, 1.0];

    let backdrop = Layer {
        id: bonaparte_model::LayerId(0),
        name: "Backdrop".into(),
        kind: LayerKind::Solid {
            color: [0.12, 0.13, 0.18, 1.0],
        },
        start: Time::from_secs_f64(0.0),
        duration: Time::from_secs_f64(8.0),
        parent: None,
        blend_mode: BlendMode::Normal,
        visible: true,
        locked: false,
        transform: StaticTransform::default(),
        tracks: Default::default(),
    };
    project.insert_layer(comp, backdrop);

    let mut logo = Layer::new_rect(
        "Logo",
        [0.42, 0.54, 1.0, 1.0],
        Time::from_secs_f64(0.0),
        Time::from_secs_f64(8.0),
    );
    // The welcome animation: Logo slides in and eases to rest — proof that
    // keyframes round-trip through Op → History → renderer on first launch.
    let mut track = bonaparte_model::Track::new();
    track.set_key(bonaparte_model::Keyframe {
        time: Time::from_secs_f64(0.0),
        value: bonaparte_model::PropValue::Vec2([-260.0, 0.0]),
        easing: bonaparte_model::Easing::Bezier {
            p1: [0.22, 1.0],
            p2: [0.36, 1.0],
        },
    });
    track.set_key(bonaparte_model::Keyframe {
        time: Time::from_secs_f64(1.2),
        value: bonaparte_model::PropValue::Vec2([-40.0, 0.0]),
        easing: bonaparte_model::Easing::default(),
    });
    logo.tracks.insert(Property::Position, track);

    project.insert_layer(comp, logo);
    project
}

fn main() {
    tauri::Builder::default()
        .manage(AppState {
            project: Mutex::new(demo_project()),
            history: Mutex::new(History::new()),
        })
        .invoke_handler(tauri::generate_handler![
            project_state,
            apply_op,
            undo,
            redo,
            render_frame,
            render_frame_raw,
            export_video,
            history_list
        ])
        .run(tauri::generate_context!())
        .expect("error while running Bonaparte");
}
