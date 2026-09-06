//! Development-only bridge. Vite proxies /api so browser clients never use localhost.
//! Intentionally no arbitrary filesystem read/write endpoints and no CORS bypass.
use bonaparte_runtime::{EditorSession, PreviewRequest, RenderRequest, MAX_PROJECT_BYTES};
use serde_json::{json, Value};
use std::{
    io::Read,
    sync::{Arc, Mutex},
};
use tiny_http::{Header, Method, Request, Response, Server};

fn reply(request: Request, status: u16, mime: &str, bytes: Vec<u8>) {
    let response = Response::from_data(bytes)
        .with_status_code(status)
        .with_header(Header::from_bytes("Content-Type", mime).expect("static header"))
        .with_header(Header::from_bytes("Cache-Control", "no-store").expect("static header"));
    let _ = request.respond(response);
}
fn handle(mut request: Request, session: &Mutex<EditorSession>) {
    if request.method() == &Method::Get && request.url() == "/api/health" {
        reply(
            request,
            200,
            "application/json",
            b"{\"status\":\"ok\",\"renderer\":\"Rust preview runtime\",\"previewProtocol\":3}"
                .to_vec(),
        );
        return;
    }
    let custom_header = request
        .headers()
        .iter()
        .any(|h| h.field.equiv("X-Bonaparte-Client") && h.value.as_str() == "editor");
    if request.method() != &Method::Post || !custom_header {
        reply(
            request,
            405,
            "application/json",
            b"{\"error\":\"Use the editor's same-origin POST transport\"}".to_vec(),
        );
        return;
    }
    let name = request
        .url()
        .strip_prefix("/api/")
        .unwrap_or("")
        .to_string();
    let result = (|| -> Result<(&str, Vec<u8>), String> {
        if request.body_length().is_some_and(|n| n > MAX_PROJECT_BYTES) {
            return Err("Request too large".into());
        }
        let mut body = String::new();
        request
            .as_reader()
            .take(MAX_PROJECT_BYTES as u64 + 1)
            .read_to_string(&mut body)
            .map_err(|e| e.to_string())?;
        if body.len() > MAX_PROJECT_BYTES {
            return Err("Request too large".into());
        }
        let args: Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
        if name == "import_audio" {
            let prepared = bonaparte_runtime::audio::prepare_import(args)?;
            let result = session
                .lock()
                .map_err(|_| "Editor session unavailable")?
                .import_audio(prepared)?;
            return Ok((
                "application/json",
                serde_json::to_vec(&result).map_err(|e| e.to_string())?,
            ));
        }
        if name == "audio_chunk" {
            let request: bonaparte_runtime::AudioChunkRequest =
                serde_json::from_value(args).map_err(|e| e.to_string())?;
            let input = session
                .lock()
                .map_err(|_| "Editor session unavailable")?
                .audio_input(request.comp_id)?;
            return Ok(("application/octet-stream", input.packet(request)?));
        }
        if name == "export_wav" {
            let comp = serde_json::from_value(args["compId"].clone()).map_err(|e| e.to_string())?;
            let input = session
                .lock()
                .map_err(|_| "Editor session unavailable")?
                .audio_input(comp)?;
            let temp = tempfile::Builder::new()
                .suffix(".wav")
                .tempfile()
                .map_err(|e| e.to_string())?
                .into_temp_path();
            input.wav(&temp)?;
            if std::fs::metadata(&temp).map_err(|e| e.to_string())?.len() > 128 * 1024 * 1024 {
                return Err("Browser WAV exceeds 128 MiB; use the native file export".into());
            }
            return Ok((
                "audio/wav",
                std::fs::read(&temp).map_err(|e| e.to_string())?,
            ));
        }
        if name == "interaction_planes" {
            let request = serde_json::from_value(args).map_err(|e| e.to_string())?;
            let input = session
                .lock()
                .map_err(|_| "Editor session unavailable")?
                .interaction_input(request)?;
            return Ok(("application/octet-stream", input.packet()?));
        }
        if name == "preview_frame" {
            let request: PreviewRequest =
                serde_json::from_value(args).map_err(|e| e.to_string())?;
            let job = session
                .lock()
                .map_err(|_| "Editor session unavailable")?
                .preview_input(request)?;
            return Ok(("application/octet-stream", job.packet()?));
        }
        if matches!(
            name.as_str(),
            "render_frame_raw" | "export_png" | "export_video"
        ) {
            let request: RenderRequest = serde_json::from_value(args).map_err(|e| e.to_string())?;
            let input = session
                .lock()
                .map_err(|_| "Editor session unavailable")?
                .render_input(request)?;
            // Rendering never holds the editor's state mutex.
            return match name.as_str() {
                "render_frame_raw" => Ok(("application/octet-stream", input.raw()?)),
                "export_png" => Ok(("image/png", input.png()?)),
                _ => {
                    static EXPORT_ID: std::sync::atomic::AtomicU64 =
                        std::sync::atomic::AtomicU64::new(0);
                    let id = EXPORT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let dir = std::env::current_dir()
                        .map_err(|e| e.to_string())?
                        .join(".cache/exports");
                    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                    let path = dir.join(format!("export-{}-{id}.mp4", std::process::id()));
                    let result = input.export_mp4(&path).and_then(|_| {
                        let len = std::fs::metadata(&path).map_err(|e| e.to_string())?.len();
                        if len > 128 * 1024 * 1024 { return Err("Browser export exceeds 128 MB; use the desktop export for larger projects".into()); }
                        std::fs::read(&path).map_err(|e| e.to_string())
                    });
                    let _ = std::fs::remove_file(&path);
                    Ok(("video/mp4", result?))
                }
            };
        }
        let result = session
            .lock()
            .map_err(|_| "Editor session unavailable")?
            .command(&name, args)?;
        Ok((
            "application/json",
            serde_json::to_vec(&result).map_err(|e| e.to_string())?,
        ))
    })();
    match result {
        Ok((mime, bytes)) => reply(request, 200, mime, bytes),
        Err(error) => reply(
            request,
            400,
            "application/json",
            serde_json::to_vec(&json!({"error": error})).expect("JSON error"),
        ),
    }
}
fn main() {
    let port = std::env::var("BONAPARTE_API_PORT").unwrap_or_else(|_| "4317".into());
    let server = Arc::new(Server::http(format!("0.0.0.0:{port}")).expect("HTTP bind"));
    let session = Arc::new(Mutex::new(EditorSession::default()));
    eprintln!("Bonaparte Rust render service on 0.0.0.0:{port} (development only)");
    let mut workers = Vec::new();
    for _ in 0..4 {
        let server = Arc::clone(&server);
        let session = Arc::clone(&session);
        workers.push(std::thread::spawn(move || {
            for request in server.incoming_requests() {
                handle(request, &session);
            }
        }));
    }
    for worker in workers {
        worker.join().expect("HTTP worker");
    }
}
