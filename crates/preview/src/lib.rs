//! Bridge core shared by the HTTP loop and tests: every request is routed
//! through [`route`], which contains panics and survives poisoned locks.
//! The editor process must not die because one request did.
use bonaparte_mcp::{handle_request_safe, McpSession};
use bonaparte_runtime::{EditorSession, PreviewRequest, RenderRequest, MAX_PROJECT_BYTES};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex, MutexGuard};

pub type Reply = (u16, &'static str, Vec<u8>);

fn ok(mime: &'static str, bytes: Vec<u8>) -> Reply {
    (200, mime, bytes)
}
fn bad(error: String) -> Reply {
    (
        400,
        "application/json",
        serde_json::to_vec(&json!({"error": error})).expect("JSON error"),
    )
}

/// Locks the editor session, recovering from poisoned mutexes: a panic in
/// one request must never brick the editor for every later request. Commits
/// are validated atomically, so a poisoned guard still holds a consistent
/// document.
pub fn lock_session(session: &Mutex<EditorSession>) -> MutexGuard<'_, EditorSession> {
    match session.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Panics in request handling become a 500 instead of a dead worker.
pub fn safe_route(session: &Arc<Mutex<EditorSession>>, name: String, body: String) -> Reply {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| route(session, name, body))) {
        Ok(reply) => reply,
        Err(panic) => {
            let message = panic
                .downcast_ref::<&str>()
                .map(|s| (*s).to_string())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "request panicked".to_string());
            (
                500,
                "application/json",
                serde_json::to_vec(&json!({
                    "error": format!("request crashed and was contained: {message}")
                }))
                .expect("JSON error"),
            )
        }
    }
}

/// MCP tools over the live editor session: same JSON-RPC envelope as the
/// stdio server, same tool surface, but hosted — AI edits land in the
/// editor's own undo history.
fn route_mcp(session: &Arc<Mutex<EditorSession>>, body: String) -> Reply {
    let request: serde_json::Value = match serde_json::from_str(&body) {
        Ok(value) => value,
        Err(e) => return bad(format!("Malformed MCP request: {e}")),
    };
    let parsed: bonaparte_mcp::protocol::JsonRpcRequest = match serde_json::from_value(request) {
        Ok(req) => req,
        Err(e) => return bad(format!("Malformed MCP request: {e}")),
    };
    let hosted = McpSession::hosted(Arc::clone(session));
    let mut hosted = hosted;
    match handle_request_safe(&mut hosted, parsed) {
        Some(response) => match serde_json::to_vec(&response) {
            Ok(bytes) => ok("application/json", bytes),
            Err(e) => bad(e.to_string()),
        },
        None => ok("application/json", b"{}".to_vec()),
    }
}

pub fn route(session: &Arc<Mutex<EditorSession>>, name: String, body: String) -> Reply {
    if name == "mcp" {
        return route_mcp(session, body);
    }
    let result = (|| -> Result<(&'static str, Vec<u8>), String> {
        if body.len() > MAX_PROJECT_BYTES {
            return Err("Request too large".into());
        }
        let args: Value =
            serde_json::from_str(&body).map_err(|e| format!("Malformed request body: {e}"))?;
        if name == "import_audio" {
            let prepared = bonaparte_runtime::audio::prepare_import(args)?;
            let result = lock_session(session).import_audio(prepared)?;
            return Ok((
                "application/json",
                serde_json::to_vec(&result).map_err(|e| e.to_string())?,
            ));
        }
        if name == "audio_chunk" {
            let request: bonaparte_runtime::AudioChunkRequest =
                serde_json::from_value(args).map_err(|e| e.to_string())?;
            let input = lock_session(session).audio_input(request.comp_id)?;
            return Ok(("application/octet-stream", input.packet(request)?));
        }
        if name == "export_wav" {
            let comp = serde_json::from_value(args["compId"].clone()).map_err(|e| e.to_string())?;
            let input = lock_session(session).audio_input(comp)?;
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
        if name == "export_lut" {
            let comp: bonaparte_model::CompId =
                serde_json::from_value(args["compId"].clone()).map_err(|e| e.to_string())?;
            let layer: bonaparte_model::LayerId =
                serde_json::from_value(args["layerId"].clone()).map_err(|e| e.to_string())?;
            let size = args["size"].as_u64().unwrap_or(33) as u32;
            let time: bonaparte_model::Time =
                serde_json::from_value(args["time"].clone()).unwrap_or(bonaparte_model::Time::ZERO);
            let host = lock_session(session);
            let layer = host
                .project
                .comps
                .get(&comp)
                .and_then(|c| c.layers.get(&layer))
                .ok_or_else(|| "Layer not found in composition".to_string())?;
            if layer.effects.is_empty() {
                return Err("The layer has no effects to bake into a LUT".into());
            }
            let cube = bonaparte_effects::export_cube(&host.registry, &layer.effects, size, time)?;
            return Ok(("text/plain", cube.into_bytes()));
        }
        if name == "interaction_planes" {
            let request = serde_json::from_value(args).map_err(|e| e.to_string())?;
            let input = lock_session(session).interaction_input(request)?;
            return Ok(("application/octet-stream", input.packet()?));
        }
        if name == "preview_frame" {
            let request: PreviewRequest =
                serde_json::from_value(args).map_err(|e| e.to_string())?;
            let job = lock_session(session).preview_input(request)?;
            return Ok(("application/octet-stream", job.packet()?));
        }
        if matches!(
            name.as_str(),
            "render_frame_raw" | "export_png" | "export_video"
        ) {
            let request: RenderRequest = serde_json::from_value(args).map_err(|e| e.to_string())?;
            let input = lock_session(session).render_input(request)?;
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
        if name == "describe" {
            let host = lock_session(session);
            let doc = bonaparte_mcp::editor_describe(
                &host.project,
                &bonaparte_effects::registry::builtin_registry(),
            );
            return Ok((
                "application/json",
                serde_json::to_vec(&doc).map_err(|e| e.to_string())?,
            ));
        }
        let result = lock_session(session).command(&name, args)?;
        Ok((
            "application/json",
            serde_json::to_vec(&result).map_err(|e| e.to_string())?,
        ))
    })();
    match result {
        Ok((mime, bytes)) => (200, mime, bytes),
        Err(error) => bad(error),
    }
}
