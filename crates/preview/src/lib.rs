//! Bridge core shared by the HTTP loop and tests: every request is routed
//! through [`route_reply`], which contains panics and survives poisoned locks.
//! The editor process must not die because one request did.
//!
//! Large media replies (MP4/WAV exports) are handed to the transport as file
//! paths and streamed straight from disk: no size cap, and neither the editor
//! nor the browser needs to hold the whole file in memory.
use bonaparte_mcp::{handle_request_safe, McpSession};
use bonaparte_runtime::{EditorSession, PreviewRequest, RenderRequest, MAX_PROJECT_BYTES};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

/// A routed reply: either inline bytes, or a file streamed from disk.
pub enum Reply {
    Bytes {
        status: u16,
        mime: &'static str,
        body: Vec<u8>,
    },
    /// A file the consumer must send and then delete.
    File { mime: &'static str, path: PathBuf },
}

impl Reply {
    /// Consume as bytes; file payloads are read and removed. Tests and the
    /// tuple-shaped [`route`]/[`safe_route`] wrappers use this — the HTTP
    /// transport streams [`Reply::File`] straight off disk instead.
    pub fn into_bytes(self) -> (u16, &'static str, Vec<u8>) {
        match self {
            Reply::Bytes { status, mime, body } => (status, mime, body),
            Reply::File { mime, path } => {
                let body = std::fs::read(&path).unwrap_or_default();
                let _ = std::fs::remove_file(&path);
                (200, mime, body)
            }
        }
    }
}

/// Unique handoff file under `.cache/exports`; the consumer deletes it once
/// the response is on the wire.
fn export_path(extension: &str) -> std::io::Result<PathBuf> {
    static EXPORT_ID: AtomicU64 = AtomicU64::new(0);
    let id = EXPORT_ID.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::current_dir()?.join(".cache/exports");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join(format!("export-{}-{id}.{extension}", std::process::id())))
}

fn ok(mime: &'static str, bytes: Vec<u8>) -> Reply {
    Reply::Bytes {
        status: 200,
        mime,
        body: bytes,
    }
}
fn bad(error: String) -> Reply {
    Reply::Bytes {
        status: 400,
        mime: "application/json",
        body: serde_json::to_vec(&json!({"error": error})).expect("JSON error"),
    }
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
pub fn safe_route_reply(session: &Arc<Mutex<EditorSession>>, name: String, body: String) -> Reply {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        route_reply(session, name, body)
    })) {
        Ok(reply) => reply,
        Err(panic) => {
            let message = panic
                .downcast_ref::<&str>()
                .map(|s| (*s).to_string())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "request panicked".to_string());
            Reply::Bytes {
                status: 500,
                mime: "application/json",
                body: serde_json::to_vec(&json!({
                    "error": format!("request crashed and was contained: {message}")
                }))
                .expect("JSON error"),
            }
        }
    }
}

/// Tuple-shaped wrapper kept for tests and simple callers: file replies are
/// read into memory and removed.
pub fn safe_route(
    session: &Arc<Mutex<EditorSession>>,
    name: String,
    body: String,
) -> (u16, &'static str, Vec<u8>) {
    safe_route_reply(session, name, body).into_bytes()
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

/// Built-in scripting is the MCP tool surface — deliberately so, per the
/// project constitution: no second API, no script-only verbs. A script is a
/// JSON array of `{"tool": …, "args": …}` steps dispatched one by one through
/// the very same hosted `McpSession` an external AI client drives, each step
/// syncing the mirror first so it sees exactly what a separate MCP request
/// would see. Steps commit through `Op`s, so they land in the editor's undo
/// history (the journal keeps them as deep as any human edit). Panics in one
/// tool are recorded as that step's failure and never take the bridge down.
fn route_script(session: &Arc<Mutex<EditorSession>>, body: String) -> Reply {
    let parsed = match serde_json::from_str::<serde_json::Value>(&body) {
        Ok(value) => value,
        Err(e) => return bad(format!("Malformed script: {e}")),
    };
    let Some(steps) = parsed.get("steps").and_then(|s| s.as_array()) else {
        return bad("A script is {steps: [{tool, args}, …]}".into());
    };
    // Same transaction ceiling as Batch ops: a script is a transaction.
    if steps.is_empty() {
        return bad("A script needs at least one step".into());
    }
    if steps.len() > 8192 {
        return bad("Scripts carry at most 8192 steps (same ceiling as Batch)".into());
    }
    let stop_on_failure = parsed
        .get("stopOnFailure")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let mut hosted = McpSession::hosted(Arc::clone(session));
    let mut results = Vec::with_capacity(steps.len());
    let mut failed = false;
    for (index, step) in steps.iter().enumerate() {
        let Some(tool) = step.get("tool").and_then(|t| t.as_str()) else {
            results.push(serde_json::json!({
                "index": index,
                "tool": null,
                "ok": false,
                "result": "each step needs a \"tool\" name",
            }));
            failed = true;
            if stop_on_failure {
                break;
            }
            continue;
        };
        let args = step.get("args").cloned().unwrap_or(serde_json::Value::Null);
        hosted.sync_from_host();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            bonaparte_mcp::tools::execute_tool(&mut hosted, tool, args)
        }));
        let outcome = match outcome {
            Ok(result) => result,
            Err(panic) => {
                let message = panic
                    .downcast_ref::<&str>()
                    .map(|s| (*s).to_string())
                    .or_else(|| panic.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "tool panicked".to_string());
                bonaparte_mcp::ToolCallResult::error(format!("step {index} panicked: {message}"))
            }
        };
        let text: String = outcome
            .content
            .iter()
            .filter_map(|c| c.text.as_ref())
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        let entry = serde_json::json!({
            "index": index,
            "tool": tool,
            "ok": !outcome.is_error,
            "result": serde_json::from_str::<serde_json::Value>(&text)
                .unwrap_or_else(|_| serde_json::Value::String(text)),
        });
        results.push(entry);
        if outcome.is_error {
            failed = true;
            if stop_on_failure {
                break;
            }
        }
    }
    ok(
        "application/json",
        serde_json::to_vec(&serde_json::json!({
            "ran": results.len(),
            "failed": failed,
            "steps": results,
        }))
        .unwrap_or_default(),
    )
}

pub fn route_reply(session: &Arc<Mutex<EditorSession>>, name: String, body: String) -> Reply {
    if name == "mcp" {
        return route_mcp(session, body);
    }
    if name == "script.run" {
        return route_script(session, body);
    }
    let result = (|| -> Result<Reply, String> {
        if body.len() > MAX_PROJECT_BYTES {
            return Err("Request too large".into());
        }
        let args: Value =
            serde_json::from_str(&body).map_err(|e| format!("Malformed request body: {e}"))?;
        if name == "import_audio" {
            let prepared = bonaparte_runtime::audio::prepare_import(args)?;
            let result = lock_session(session).import_audio(prepared)?;
            return Ok(ok(
                "application/json",
                serde_json::to_vec(&result).map_err(|e| e.to_string())?,
            ));
        }
        if name == "audio_chunk" {
            let request: bonaparte_runtime::AudioChunkRequest =
                serde_json::from_value(args).map_err(|e| e.to_string())?;
            let input = lock_session(session).audio_input(request.comp_id)?;
            return Ok(ok("application/octet-stream", input.packet(request)?));
        }
        if name == "export_wav" {
            let comp = serde_json::from_value(args["compId"].clone()).map_err(|e| e.to_string())?;
            let input = lock_session(session).audio_input(comp)?;
            // Disk handoff, exactly like MP4: the mix streams to the client
            // whatever its length — an hour-long multitrack WAV included.
            let path = export_path("wav").map_err(|e| e.to_string())?;
            if let Err(error) = input.wav(&path) {
                let _ = std::fs::remove_file(&path);
                return Err(error);
            }
            return Ok(Reply::File {
                mime: "audio/wav",
                path,
            });
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
            return Ok(ok("text/plain", cube.into_bytes()));
        }
        if name == "interaction_planes" {
            let request = serde_json::from_value(args).map_err(|e| e.to_string())?;
            let input = lock_session(session).interaction_input(request)?;
            return Ok(ok("application/octet-stream", input.packet()?));
        }
        if name == "preview_frame" {
            let request: PreviewRequest =
                serde_json::from_value(args).map_err(|e| e.to_string())?;
            let job = lock_session(session).preview_input(request)?;
            return Ok(ok("application/octet-stream", job.packet()?));
        }
        if matches!(
            name.as_str(),
            "render_frame_raw" | "export_png" | "export_video"
        ) {
            let mut request: RenderRequest =
                serde_json::from_value(args).map_err(|e| e.to_string())?;
            if name != "render_frame_raw" {
                // Delivered files never encode proxy pixels.
                request.source_quality = true;
            }
            let input = lock_session(session).render_input(request)?;
            // Rendering never holds the editor's state mutex.
            return match name.as_str() {
                "render_frame_raw" => Ok(ok("application/octet-stream", input.raw()?)),
                "export_png" => Ok(ok("image/png", input.png()?)),
                _ => {
                    let path = export_path("mp4").map_err(|e| e.to_string())?;
                    if let Err(error) = input.export_mp4(&path) {
                        let _ = std::fs::remove_file(&path);
                        return Err(error);
                    }
                    Ok(Reply::File {
                        mime: "video/mp4",
                        path,
                    })
                }
            };
        }
        if name == "describe" {
            let host = lock_session(session);
            let doc = bonaparte_mcp::editor_describe(
                &host.project,
                &bonaparte_effects::registry::builtin_registry(),
            );
            return Ok(ok(
                "application/json",
                serde_json::to_vec(&doc).map_err(|e| e.to_string())?,
            ));
        }
        let result = lock_session(session).command(&name, args)?;
        Ok(ok(
            "application/json",
            serde_json::to_vec(&result).map_err(|e| e.to_string())?,
        ))
    })();
    match result {
        Ok(reply) => reply,
        Err(error) => bad(error),
    }
}

/// Tuple-shaped wrapper kept for tests: a file reply is read into memory and
/// removed, so every previous expectation still holds.
pub fn route(
    session: &Arc<Mutex<EditorSession>>,
    name: String,
    body: String,
) -> (u16, &'static str, Vec<u8>) {
    route_reply(session, name, body).into_bytes()
}
