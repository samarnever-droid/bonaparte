//! Implementations and schemas for all Bonaparte MCP tools.

use std::fs;
use std::path::Path;

use bonaparte_effects::registry::builtin_registry;
use bonaparte_engine::reference::render_comp;
use bonaparte_model::{FrameRate, Op, Project, Time};
use bonaparte_runtime::{decode_embedded_frames, RenderInput};
use serde::Deserialize;
use serde_json::json;

use crate::png::encode_png;
use crate::protocol::{ToolCallResult, ToolDefinition};
use crate::session::McpSession;

/// Converts an f64 fps to a rational FrameRate.
pub fn f64_to_framerate(fps: f64) -> FrameRate {
    if (fps - 23.976).abs() < 0.01 {
        FrameRate {
            num: 24000,
            den: 1001,
        }
    } else if (fps - 29.97).abs() < 0.01 {
        FrameRate {
            num: 30000,
            den: 1001,
        }
    } else if (fps - 59.94).abs() < 0.01 {
        FrameRate {
            num: 60000,
            den: 1001,
        }
    } else {
        let rounded = fps.round().max(1.0) as u32;
        FrameRate {
            num: rounded,
            den: 1,
        }
    }
}

/// Returns the complete catalog of all 10 MCP tools with JSON schemas.
pub fn list_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "effects.list".to_string(),
            description: "List effect manifests, parameter types, defaults, limits and documentation. Use setLayerEffects ops to attach ordered instances and numeric/point animation tracks. This host executes CPU evaluators, not GPU shaders.".to_string(),
            input_schema: json!({"type":"object", "properties":{}}),
        },
        ToolDefinition {
            name: "project.create".to_string(),
            description: "Create a new Bonaparte project with an optional initial composition.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Project name (default: 'Untitled')" },
                    "comp_name": { "type": "string", "description": "Initial composition name (default: 'Main')" },
                    "width": { "type": "integer", "description": "Composition width in pixels (default: 1920)" },
                    "height": { "type": "integer", "description": "Composition height in pixels (default: 1080)" },
                    "fps": { "type": "number", "description": "Composition frame rate (default: 30.0)" },
                    "duration": { "type": "number", "description": "Composition duration in seconds (default: 10.0)" }
                }
            }),
        },
        ToolDefinition {
            name: "project.open".to_string(),
            description: "Open a Bonaparte project from a filesystem path or serialized JSON string.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path to .bonaparte or .json project file" },
                    "json": { "type": "string", "description": "Direct JSON string representation of the project" }
                }
            }),
        },
        ToolDefinition {
            name: "project.save".to_string(),
            description: "Save active Bonaparte project to a file path or return project JSON.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Destination file path to save project JSON" }
                }
            }),
        },
        ToolDefinition {
            name: "project.info".to_string(),
            description: "Get detailed summary of active project, compositions, layers, media, and history state.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolDefinition {
            name: "op.apply".to_string(),
            description: "Apply an Op mutation to the project via History::commit with full validation and invertibility recording.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "op": { "type": "object", "description": "The Op mutation object to commit" }
                },
                "required": ["op"]
            }),
        },
        ToolDefinition {
            name: "ops.propose".to_string(),
            description: "AI dry-run diff preview: evaluates a sequence of Op mutations on a project snapshot without modifying the active session, returning human-readable descriptions and validation status.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "ops": {
                        "type": "array",
                        "items": { "type": "object" },
                        "description": "Sequence of Op mutations to simulate"
                    }
                },
                "required": ["ops"]
            }),
        },
        ToolDefinition {
            name: "history.list".to_string(),
            description: "List human-readable descriptions of committed undo history in chronological order.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolDefinition {
            name: "history.undo".to_string(),
            description: "Undo the last applied operation in the active project.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolDefinition {
            name: "history.redo".to_string(),
            description: "Redo the previously undone operation in the active project.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolDefinition {
            name: "comp.render".to_string(),
            description: "Headless batch rendering of a composition to PNG frame(s) or MP4 video.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "comp_id": { "type": "integer", "description": "Composition ID to render (defaults to active comp)" },
                    "time": { "type": "number", "description": "Single frame timestamp in seconds (for single PNG render)" },
                    "start_time": { "type": "number", "description": "Batch sequence start time in seconds (default: 0.0)" },
                    "end_time": { "type": "number", "description": "Batch sequence end time in seconds (default: comp duration)" },
                    "fps": { "type": "number", "description": "Render frame rate (default: comp fps)" },
                    "format": { "type": "string", "enum": ["png", "mp4"], "description": "Output format: 'png' or 'mp4' (inferred from output_path extension if omitted)" },
                    "output_path": { "type": "string", "description": "Destination file path or template (e.g. 'out/frame_%04d.png' or 'video.mp4')" }
                },
                "required": ["output_path"]
            }),
        },
    ]
}

/// Executes a tool call by name with provided JSON arguments.
pub fn execute_tool(
    session: &mut McpSession,
    name: &str,
    args: serde_json::Value,
) -> ToolCallResult {
    match name {
        "effects.list" => ToolCallResult::json(&json!({"effects": builtin_registry().list(), "renderer": "CPU reference", "gpu_available": false})).unwrap_or_else(|e| ToolCallResult::error(e.to_string())),
        "project.create" => tool_project_create(session, args),
        "project.open" => tool_project_open(session, args),
        "project.save" => tool_project_save(session, args),
        "project.info" => tool_project_info(session, args),
        "op.apply" => tool_op_apply(session, args),
        "ops.propose" => tool_ops_propose(session, args),
        "history.list" => tool_history_list(session, args),
        "history.undo" => tool_history_undo(session, args),
        "history.redo" => tool_history_redo(session, args),
        "comp.render" => tool_comp_render(session, args),
        unknown => ToolCallResult::error(format!("Unknown tool: '{unknown}'")),
    }
}

// ---------------------------------------------------------------------------
// Tool Implementations
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct CreateProjectArgs {
    name: Option<String>,
    comp_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    fps: Option<f64>,
    duration: Option<f64>,
}

fn tool_project_create(session: &mut McpSession, args: serde_json::Value) -> ToolCallResult {
    let parsed: CreateProjectArgs = match serde_json::from_value(args) {
        Ok(a) => a,
        Err(e) => {
            return ToolCallResult::error(format!("Invalid arguments for project.create: {e}"))
        }
    };

    let name = parsed.name.unwrap_or_else(|| "Untitled".to_string());
    let mut project = Project::new(name);
    let comp_name = parsed.comp_name.unwrap_or_else(|| "Main".to_string());
    let width = parsed.width.unwrap_or(1920);
    let height = parsed.height.unwrap_or(1080);
    let fps = f64_to_framerate(parsed.fps.unwrap_or(30.0));
    let duration = Time::from_secs_f64(parsed.duration.unwrap_or(10.0));

    let comp_id = project.create_comp(&comp_name, width, height, fps, duration);
    if let Err(error) = project.validate() {
        return ToolCallResult::error(error);
    }
    session.project = project;
    session.history = bonaparte_model::History::new();
    session.active_comp = Some(comp_id);

    let res = json!({
        "status": "created",
        "project_name": session.project.name,
        "comp_id": comp_id.0,
        "comp_name": comp_name,
        "width": width,
        "height": height,
        "fps": format!("{fps}"),
        "duration_secs": duration.as_secs_f64(),
    });
    ToolCallResult::json(&res).unwrap_or_else(|e| ToolCallResult::error(e.to_string()))
}

#[derive(Deserialize)]
struct OpenProjectArgs {
    path: Option<String>,
    json: Option<String>,
}

fn tool_project_open(session: &mut McpSession, args: serde_json::Value) -> ToolCallResult {
    let parsed: OpenProjectArgs = match serde_json::from_value(args) {
        Ok(a) => a,
        Err(e) => return ToolCallResult::error(format!("Invalid arguments for project.open: {e}")),
    };

    let content = if let Some(path_str) = parsed.path {
        match fs::read_to_string(&path_str) {
            Ok(c) => c,
            Err(e) => {
                return ToolCallResult::error(format!(
                    "Failed to read project file '{path_str}': {e}"
                ))
            }
        }
    } else if let Some(json_str) = parsed.json {
        json_str
    } else {
        return ToolCallResult::error("project.open requires either 'path' or 'json' argument");
    };

    let project: Project = match bonaparte_runtime::parse_project(&content) {
        Ok(p) => p,
        Err(e) => return ToolCallResult::error(format!("Failed to parse project JSON: {e}")),
    };

    let comp_count = project.comps.len();
    *session = McpSession::with_project(project);

    let res = json!({
        "status": "opened",
        "project_name": session.project.name,
        "comp_count": comp_count,
        "active_comp": session.active_comp.map(|c| c.0),
    });
    ToolCallResult::json(&res).unwrap_or_else(|e| ToolCallResult::error(e.to_string()))
}

#[derive(Deserialize)]
struct SaveProjectArgs {
    path: Option<String>,
}

fn tool_project_save(session: &mut McpSession, args: serde_json::Value) -> ToolCallResult {
    let parsed: SaveProjectArgs = match serde_json::from_value(args) {
        Ok(a) => a,
        Err(e) => return ToolCallResult::error(format!("Invalid arguments for project.save: {e}")),
    };

    let serialized = match bonaparte_runtime::serialize_project(&session.project) {
        Ok(s) => s,
        Err(e) => return ToolCallResult::error(format!("Failed to serialize project: {e}")),
    };

    if let Some(path_str) = parsed.path {
        if let Some(parent) = Path::new(&path_str).parent() {
            if !parent.as_os_str().is_empty() {
                if let Err(e) = fs::create_dir_all(parent) {
                    return ToolCallResult::error(format!(
                        "Failed to create parent directories: {e}"
                    ));
                }
            }
        }
        if let Err(e) =
            bonaparte_runtime::write_file_atomic(Path::new(&path_str), serialized.as_bytes())
        {
            return ToolCallResult::error(format!("Failed to write project to '{path_str}': {e}"));
        }
        let res = json!({
            "status": "saved",
            "path": path_str,
            "bytes_written": serialized.len(),
        });
        ToolCallResult::json(&res).unwrap_or_else(|e| ToolCallResult::error(e.to_string()))
    } else {
        let res = json!({
            "status": "serialized",
            "bytes": serialized.len(),
            "json": serialized,
        });
        ToolCallResult::json(&res).unwrap_or_else(|e| ToolCallResult::error(e.to_string()))
    }
}

fn tool_project_info(session: &mut McpSession, _args: serde_json::Value) -> ToolCallResult {
    let comps_info: Vec<serde_json::Value> = session
        .project
        .comps
        .iter()
        .map(|(cid, comp)| {
            let layers_info: Vec<serde_json::Value> = comp
                .layer_order
                .iter()
                .filter_map(|lid| comp.layers.get(lid))
                .map(|layer| {
                    json!({
                        "id": layer.id.0,
                        "name": layer.name,
                        "visible": layer.visible,
                        "locked": layer.locked,
                        "blend_mode": format!("{}", layer.blend_mode),
                        "parent": layer.parent.map(|p| p.0),
                        "start_secs": layer.start.as_secs_f64(),
                        "duration_secs": layer.duration.as_secs_f64(),
                        "kind": layer.kind,
                        "transform": layer.transform,
                        "tracks": layer.tracks,
                        "effects": layer.effects,
                    })
                })
                .collect();

            json!({
                "id": cid.0,
                "name": comp.name,
                "width": comp.width,
                "height": comp.height,
                "fps": format!("{}", comp.fps),
                "duration_secs": comp.duration.as_secs_f64(),
                "duration_ticks": comp.duration.0,
                "layers_count": comp.layers.len(),
                "layers": layers_info,
            })
        })
        .collect();

    let media_info: Vec<serde_json::Value> = session
        .project
        .media
        .iter()
        .map(|(mid, m)| {
            json!({
                "id": mid.0,
                "alias": m.alias,
            })
        })
        .collect();

    let undo_descriptions = session.history.undo_descriptions();
    let res = json!({
        "project_name": session.project.name,
        "active_comp": session.active_comp.map(|c| c.0),
        "comp_count": session.project.comps.len(),
        "comps": comps_info,
        "media_count": session.project.media.len(),
        "media": media_info,
        "undo_history_depth": undo_descriptions.len(),
    });

    ToolCallResult::json(&res).unwrap_or_else(|e| ToolCallResult::error(e.to_string()))
}

fn tool_op_apply(session: &mut McpSession, args: serde_json::Value) -> ToolCallResult {
    // Support either {"op": {...}} or direct {...}
    let op_value = if let Some(sub_op) = args.get("op") {
        sub_op.clone()
    } else {
        args
    };

    let op: Op = match serde_json::from_value(op_value) {
        Ok(o) => o,
        Err(e) => return ToolCallResult::error(format!("Invalid Op specification: {e}")),
    };

    let description = op.describe();
    match bonaparte_runtime::commit(
        &mut session.project,
        &mut session.history,
        builtin_registry(),
        op,
    ) {
        Ok(()) => {
            let res = json!({
                "success": true,
                "description": description,
                "undo_depth": session.history.undo_descriptions().len(),
            });
            ToolCallResult::json(&res).unwrap_or_else(|e| ToolCallResult::error(e.to_string()))
        }
        Err(e) => ToolCallResult::error(format!("Op validation failed: {e}")),
    }
}

#[derive(Deserialize)]
struct ProposeOpsArgs {
    ops: Vec<Op>,
}

fn tool_ops_propose(session: &mut McpSession, args: serde_json::Value) -> ToolCallResult {
    let parsed: ProposeOpsArgs = match serde_json::from_value(args) {
        Ok(a) => a,
        Err(e) => return ToolCallResult::error(format!("Invalid arguments for ops.propose: {e}")),
    };

    // AI dry-run preview: clone active state and simulate without mutating session!
    let mut preview_project = session.project.clone();
    let mut preview_history = session.history.clone();

    let mut descriptions = Vec::with_capacity(parsed.ops.len());
    let mut errors = Vec::new();

    for (idx, op) in parsed.ops.into_iter().enumerate() {
        let desc = op.describe();
        match bonaparte_runtime::commit(
            &mut preview_project,
            &mut preview_history,
            builtin_registry(),
            op,
        ) {
            Ok(()) => descriptions.push(desc),
            Err(e) => {
                errors.push(format!("Op #{idx} failed: {e}"));
                break;
            }
        }
    }

    let is_valid = errors.is_empty();
    let res = json!({
        "valid": is_valid,
        "applied_count": descriptions.len(),
        "descriptions": descriptions,
        "errors": errors,
    });
    ToolCallResult::json(&res).unwrap_or_else(|e| ToolCallResult::error(e.to_string()))
}

fn tool_history_list(session: &mut McpSession, _args: serde_json::Value) -> ToolCallResult {
    let descriptions = session.history.undo_descriptions();
    let res = json!({
        "undo_history": descriptions,
        "depth": descriptions.len(),
    });
    ToolCallResult::json(&res).unwrap_or_else(|e| ToolCallResult::error(e.to_string()))
}

fn tool_history_undo(session: &mut McpSession, _args: serde_json::Value) -> ToolCallResult {
    match session.history.undo(&mut session.project) {
        Ok(true) => {
            let res = json!({
                "undone": true,
                "remaining_undo_depth": session.history.undo_descriptions().len(),
            });
            ToolCallResult::json(&res).unwrap_or_else(|e| ToolCallResult::error(e.to_string()))
        }
        Ok(false) => {
            let res = json!({
                "undone": false,
                "message": "Undo stack is empty",
                "remaining_undo_depth": 0,
            });
            ToolCallResult::json(&res).unwrap_or_else(|e| ToolCallResult::error(e.to_string()))
        }
        Err(e) => ToolCallResult::error(format!("Undo failed: {e}")),
    }
}

fn tool_history_redo(session: &mut McpSession, _args: serde_json::Value) -> ToolCallResult {
    match session.history.redo(&mut session.project) {
        Ok(true) => {
            let res = json!({
                "redone": true,
                "remaining_undo_depth": session.history.undo_descriptions().len(),
            });
            ToolCallResult::json(&res).unwrap_or_else(|e| ToolCallResult::error(e.to_string()))
        }
        Ok(false) => {
            let res = json!({
                "redone": false,
                "message": "Redo stack is empty",
                "remaining_undo_depth": session.history.undo_descriptions().len(),
            });
            ToolCallResult::json(&res).unwrap_or_else(|e| ToolCallResult::error(e.to_string()))
        }
        Err(e) => ToolCallResult::error(format!("Redo failed: {e}")),
    }
}

#[derive(Deserialize)]
struct RenderCompArgs {
    comp_id: Option<u64>,
    time: Option<f64>,
    start_time: Option<f64>,
    end_time: Option<f64>,
    fps: Option<f64>,
    format: Option<String>,
    output_path: String,
}

fn tool_comp_render(session: &mut McpSession, args: serde_json::Value) -> ToolCallResult {
    let parsed: RenderCompArgs = match serde_json::from_value(args) {
        Ok(a) => a,
        Err(e) => return ToolCallResult::error(format!("Invalid arguments for comp.render: {e}")),
    };

    let comp_id = match session.target_comp(parsed.comp_id) {
        Some(cid) => cid,
        None => {
            return ToolCallResult::error("No composition available in active project to render")
        }
    };

    let comp = match session.project.comp(comp_id) {
        Some(c) => c,
        None => return ToolCallResult::error(format!("Composition {comp_id:?} not found")),
    };

    let width = comp.width;
    let height = comp.height;
    let comp_fps_f64 = comp.fps.num as f64 / comp.fps.den as f64;
    let fps_val = parsed.fps.unwrap_or(comp_fps_f64);
    let comp_duration_secs = comp.duration.as_secs_f64();

    let output_path = parsed.output_path;
    let format = parsed.format.unwrap_or_else(|| {
        if output_path.to_lowercase().ends_with(".mp4") {
            "mp4".to_string()
        } else {
            "png".to_string()
        }
    });

    if let Some(parent) = Path::new(&output_path).parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(e) = fs::create_dir_all(parent) {
                return ToolCallResult::error(format!(
                    "Failed to create destination directory: {e}"
                ));
            }
        }
    }

    if !fps_val.is_finite() || !(1.0..=240.0).contains(&fps_val) {
        return ToolCallResult::error("Frame rate must be between 1 and 240 fps");
    }
    if format != "png" && format != "mp4" {
        return ToolCallResult::error("Supported export formats are png and mp4");
    }
    let frames = match decode_embedded_frames(&session.project) {
        Ok(frames) => frames,
        Err(error) => return ToolCallResult::error(error),
    };
    if format == "mp4" {
        let input = RenderInput {
            project: session.project.clone(),
            comp_id,
            time: Time::ZERO,
            images: frames,
            registry: builtin_registry().clone(),
        };
        let start = Time::from_secs_f64(parsed.start_time.unwrap_or(0.0));
        let end = Time::from_secs_f64(parsed.end_time.unwrap_or(comp_duration_secs));
        match input.export_mp4_range(Path::new(&output_path), start, end, f64_to_framerate(fps_val)) {
            Ok(stats) => ToolCallResult::json(&json!({"rendered":true,"format":"mp4","frames_count":stats.frames_exported,"width":width,"height":height,"fps":fps_val,"output_path":output_path})).unwrap_or_else(|e| ToolCallResult::error(e.to_string())),
            Err(error) => ToolCallResult::error(error),
        }
    } else {
        // PNG rendering: single frame or sequence
        if let Some(single_time) = parsed.time {
            let t = Time::from_secs_f64(single_time);
            let frame = match render_comp(&session.project, comp_id, t, &frames) {
                Ok(f) => f,
                Err(e) => return ToolCallResult::error(format!("Render failed: {e}")),
            };

            let png_bytes = match encode_png(frame.width, frame.height, &frame.rgba) {
                Ok(b) => b,
                Err(e) => return ToolCallResult::error(format!("PNG encoding error: {e}")),
            };

            if let Err(e) =
                bonaparte_runtime::write_file_atomic(Path::new(&output_path), &png_bytes)
            {
                return ToolCallResult::error(format!(
                    "Failed writing PNG to '{output_path}': {e}"
                ));
            }

            let res = json!({
                "rendered": true,
                "format": "png",
                "width": frame.width,
                "height": frame.height,
                "time_secs": single_time,
                "bytes": png_bytes.len(),
                "output_path": output_path,
            });
            ToolCallResult::json(&res).unwrap_or_else(|e| ToolCallResult::error(e.to_string()))
        } else {
            // Sequence of PNG frames
            let start_sec = parsed.start_time.unwrap_or(0.0).max(0.0);
            let end_sec = parsed
                .end_time
                .unwrap_or(comp_duration_secs)
                .min(comp_duration_secs);
            if end_sec <= start_sec {
                return ToolCallResult::error("Export range is empty");
            }
            let total_frames = ((end_sec - start_sec) * fps_val).ceil() as usize;

            for f in 0..total_frames {
                let frame_time_secs = start_sec + (f as f64 / fps_val);
                let frame_time = Time::from_secs_f64(frame_time_secs);
                let frame = match render_comp(&session.project, comp_id, frame_time, &frames) {
                    Ok(f) => f,
                    Err(e) => {
                        return ToolCallResult::error(format!("Render error at frame {f}: {e}"))
                    }
                };

                let png_bytes = match encode_png(frame.width, frame.height, &frame.rgba) {
                    Ok(b) => b,
                    Err(e) => return ToolCallResult::error(format!("PNG encoding error: {e}")),
                };

                // If output_path contains % (e.g. "frame_%04d.png"), format with frame index, else append frame index
                let frame_path = if output_path.contains("%04d") {
                    output_path.replace("%04d", &format!("{f:04}"))
                } else if output_path.contains("%d") {
                    output_path.replace("%d", &format!("{f}"))
                } else if output_path.ends_with(".png") {
                    let stem = output_path.strip_suffix(".png").unwrap();
                    format!("{stem}_{f:04}.png")
                } else {
                    format!("{output_path}/frame_{f:04}.png")
                };

                if let Err(e) =
                    bonaparte_runtime::write_file_atomic(Path::new(&frame_path), &png_bytes)
                {
                    return ToolCallResult::error(format!(
                        "Failed writing frame {f} to '{frame_path}': {e}"
                    ));
                }
            }

            let res = json!({
                "rendered": true,
                "format": "png_sequence",
                "frames_count": total_frames,
                "width": width,
                "height": height,
                "fps": fps_val,
                "output_template": output_path,
            });
            ToolCallResult::json(&res).unwrap_or_else(|e| ToolCallResult::error(e.to_string()))
        }
    }
}
