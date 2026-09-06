//! Native shell. All editor behavior lives in bonaparte-runtime, also exercised
//! by the development web bridge and shared with MCP. No hardcoded demo commands.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use bonaparte_runtime::{EditorSession, PreviewRequest, RenderRequest};
use serde_json::{json, Value};
use std::sync::Mutex;

struct AppState(Mutex<EditorSession>);

#[tauri::command]
async fn editor_command(
    state: tauri::State<'_, AppState>,
    command: String,
    args: Value,
) -> Result<Value, String> {
    if command == "import_audio" {
        let prepared = tauri::async_runtime::spawn_blocking(move || {
            bonaparte_runtime::audio::prepare_import(args)
        })
        .await
        .map_err(|e| e.to_string())??;
        return state
            .0
            .lock()
            .map_err(|_| "Editor session unavailable")?
            .import_audio(prepared);
    }
    state
        .0
        .lock()
        .map_err(|_| "Editor session unavailable")?
        .command(&command, args)
}

#[tauri::command]
async fn audio_chunk(
    state: tauri::State<'_, AppState>,
    args: bonaparte_runtime::AudioChunkRequest,
) -> Result<tauri::ipc::Response, String> {
    let input = state
        .0
        .lock()
        .map_err(|_| "Editor session unavailable")?
        .audio_input(args.comp_id)?;
    tauri::async_runtime::spawn_blocking(move || input.packet(args).map(tauri::ipc::Response::new))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn export_wav_file(
    state: tauri::State<'_, AppState>,
    args: RenderRequest,
    path: String,
) -> Result<(), String> {
    let input = state
        .0
        .lock()
        .map_err(|_| "Editor session unavailable")?
        .audio_input(args.comp_id)?;
    tauri::async_runtime::spawn_blocking(move || input.wav(std::path::Path::new(&path)))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn interaction_planes(
    state: tauri::State<'_, AppState>,
    args: bonaparte_runtime::interaction::InteractionRequest,
) -> Result<tauri::ipc::Response, String> {
    let input = state
        .0
        .lock()
        .map_err(|_| "Editor session unavailable")?
        .interaction_input(args)?;
    tauri::async_runtime::spawn_blocking(move || input.packet().map(tauri::ipc::Response::new))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn preview_frame(
    state: tauri::State<'_, AppState>,
    args: PreviewRequest,
) -> Result<tauri::ipc::Response, String> {
    let job = state
        .0
        .lock()
        .map_err(|_| "Editor session unavailable")?
        .preview_input(args)?;
    tauri::async_runtime::spawn_blocking(move || job.packet().map(tauri::ipc::Response::new))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn render_frame_raw(
    state: tauri::State<'_, AppState>,
    args: RenderRequest,
) -> Result<tauri::ipc::Response, String> {
    let input = state
        .0
        .lock()
        .map_err(|_| "Editor session unavailable")?
        .render_input(args)?;
    tauri::async_runtime::spawn_blocking(move || input.raw().map(tauri::ipc::Response::new))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn export_png(
    state: tauri::State<'_, AppState>,
    args: RenderRequest,
) -> Result<tauri::ipc::Response, String> {
    let input = state
        .0
        .lock()
        .map_err(|_| "Editor session unavailable")?
        .render_input(args)?;
    tauri::async_runtime::spawn_blocking(move || input.png().map(tauri::ipc::Response::new))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn open_project_file(state: tauri::State<'_, AppState>, path: String) -> Result<Value, String> {
    let len = std::fs::metadata(&path).map_err(|e| e.to_string())?.len();
    if len > bonaparte_runtime::MAX_PROJECT_BYTES as u64 {
        return Err("Project file exceeds 64 MB".into());
    }
    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    state
        .0
        .lock()
        .map_err(|_| "Editor session unavailable")?
        .command("open_project", json!({"json": content}))
}

/// The caller supplies a path selected by the native save dialog. A temporary
/// file in the same directory makes replacement atomic; failed saves keep the old file.
#[tauri::command]
fn save_project_file(state: tauri::State<'_, AppState>, path: String) -> Result<(), String> {
    let content = bonaparte_runtime::serialize_project(
        &state
            .0
            .lock()
            .map_err(|_| "Editor session unavailable")?
            .project,
    )?;
    bonaparte_runtime::write_file_atomic(std::path::Path::new(&path), content.as_bytes())
}

#[tauri::command]
async fn export_png_file(
    state: tauri::State<'_, AppState>,
    args: RenderRequest,
    path: String,
) -> Result<(), String> {
    let input = state
        .0
        .lock()
        .map_err(|_| "Editor session unavailable")?
        .render_input(args)?;
    tauri::async_runtime::spawn_blocking(move || {
        bonaparte_runtime::write_file_atomic(std::path::Path::new(&path), &input.png()?)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn export_video_file(
    state: tauri::State<'_, AppState>,
    args: RenderRequest,
    path: String,
) -> Result<(), String> {
    let input = state
        .0
        .lock()
        .map_err(|_| "Editor session unavailable")?
        .render_input(args)?;
    tauri::async_runtime::spawn_blocking(move || input.export_mp4(std::path::Path::new(&path)))
        .await
        .map_err(|e| e.to_string())?
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState(Mutex::new(EditorSession::default())))
        .invoke_handler(tauri::generate_handler![
            editor_command,
            render_frame_raw,
            preview_frame,
            interaction_planes,
            audio_chunk,
            export_wav_file,
            export_png,
            open_project_file,
            save_project_file,
            export_video_file,
            export_png_file
        ])
        .run(tauri::generate_context!())
        .expect("error while running Bonaparte");
}
