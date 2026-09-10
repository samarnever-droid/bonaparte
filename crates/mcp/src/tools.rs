//! Implementations and schemas for all Bonaparte MCP tools.

use std::fs;
use std::path::Path;

use bonaparte_effects::registry::builtin_registry;
use bonaparte_engine::reference::render_comp;
use bonaparte_model::{CompId, FrameRate, LayerId, Op, Project, Time};
use bonaparte_runtime::{decode_embedded_frames, RenderInput};
use serde::Deserialize;
use serde_json::json;

use crate::png::encode_png;
use crate::protocol::{TextContent, ToolCallResult, ToolDefinition};
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
            // Listed first on purpose: an agent should call this before
            // anything else — it maps every capability and the live project.
            name: "editor.describe".to_string(),
            description: "ONE-CALL self-discovery: units, the full op catalog with copy-paste examples, editor commands, every effect with typed params, and a summary of the current project. Call this before any other tool.".to_string(),
            input_schema: json!({"type":"object", "properties":{}}),
        },
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
        ToolDefinition {
            name: "frame.view".to_string(),
            description: "THE FRAME VIEWER — see the composition with your own vision. Renders the composition at one timestamp (or an evenly-spaced storyboard) and returns actual PNG images INLINE in this conversation: a vision-native model looks at the real pixels, not a description. Cheapest way to check design, layout, text, color or progress.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "comp_id": { "type": "integer", "description": "Composition ID (defaults to active comp)" },
                    "time": { "type": "number", "description": "Single timestamp in seconds" },
                    "count": { "type": "integer", "description": "Storyboard mode: N frames evenly spaced across the range (1-12, default 4)" },
                    "start_time": { "type": "number", "description": "Storyboard range start in seconds (default 0)" },
                    "end_time": { "type": "number", "description": "Storyboard range end in seconds (default comp duration)" },
                    "width": { "type": "integer", "description": "Downscaled width in pixels for token economy (16-1024, default 512; height preserves aspect)" }
                }
            }),
        },
        ToolDefinition {
            name: "audio.extract".to_string(),
            description: "THE AUDIO EXTRACTOR — hear the mix with your own audio sense. Renders the composition's audio for a time range and returns a WAV file INLINE (base64) plus a loudness summary: an audio-native model listens to the actual mix — music, voice, silence — instead of guessing from metadata.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "comp_id": { "type": "integer", "description": "Composition ID (defaults to active comp)" },
                    "start_secs": { "type": "number", "description": "Range start in seconds (default 0)" },
                    "duration_secs": { "type": "number", "description": "Seconds of audio to extract (default: min(comp length, 20); cap 30)" },
                    "rate": { "type": "integer", "description": "Sample rate 8000-48000 (default 16000 — plenty for listening)" },
                    "mono": { "type": "boolean", "description": "Mix down to mono (default true — half the tokens)" }
                }
            }),
        },
    ]
}

/// One static entry in the op catalog handed to AI agents.
fn op_doc(type_name: &str, summary: &str, example: serde_json::Value) -> serde_json::Value {
    json!({
        "type": type_name,
        "summary": summary,
        "example": example,
    })
}

/// A complete, machine-readable map of everything an AI agent can do to the
/// live editor: units, the full op catalog with examples, editor commands,
/// every effect with typed params, and a summary of the current project.
/// One call answers "what exists and how do I drive it".
pub fn editor_describe(
    project: &Project,
    registry: &bonaparte_effects::EffectRegistry,
) -> serde_json::Value {
    let ops = vec![
        op_doc("setValue", "Set a layer property. Property is one of Position|Scale|Rotation|Opacity|AnchorPoint|Z. Position/Scale/AnchorPoint take {\"Vec2\":[x,y]}; Rotation/Opacity/Z take {\"Scalar\":n}. Position is px from comp center, Scale is percent, Z is depth px (positive = away from the camera).",
            json!({"type":"setValue","comp":1,"layer":1,"property":"Position","value":{"Vec2":[100.0,0.0]}})),
        op_doc("addKeyframe", "Add one keyframe to a property track. Time is in ticks (120000 = 1 second). Easing is \"Linear\", \"Hold\", or {\"Bezier\":{\"p1\":[x,y],\"p2\":[x,y]}} with p in [-0.4..1.4].",
            json!({"type":"addKeyframe","comp":1,"layer":1,"property":"Position","key":{"time":120000,"value":{"Vec2":[0.0,-80.0]},"easing":{"Bezier":{"p1":[0.42,0.0],"p2":[0.58,1.0]}}}})),
        op_doc("removeKeyframe", "Remove the keyframe at a given tick time.", json!({"type":"removeKeyframe","comp":1,"layer":1,"property":"Opacity","time":120000})),
        op_doc("moveKeyframe", "Retiming: move one key from tick `from` to tick `to`.", json!({"type":"moveKeyframe","comp":1,"layer":1,"property":"Position","from":0,"to":240000})),
        op_doc("setEasing", "Set the easing on the segment starting at the key at `time`.", json!({"type":"setEasing","comp":1,"layer":1,"property":"Position","time":0,"easing":"Hold"})),
        op_doc("setLayerTime", "Trim/retime a layer bar: start and duration in ticks.", json!({"type":"setLayerTime","comp":1,"layer":1,"start":0,"duration":240000})),
        op_doc("shiftLayer", "Move a layer bar in time by `delta` ticks (keyframes ride along).", json!({"type":"shiftLayer","comp":1,"layer":1,"delta":-120000})),
        op_doc("setLayerEffects", "Replace a layer's whole effect stack (see effects catalog for param ids/types; omit params to use defaults).",
            json!({"type":"setLayerEffects","comp":1,"layer":1,"effects":[{"id":"grade-1","effect_id":"builtin.color_grade","enabled":true,"params":{"exposure":{"Float":0.5}},"tracks":{}}]})),
        op_doc("renameLayer", "Rename a layer.", json!({"type":"renameLayer","comp":1,"layer":1,"name":"Title card"})),
        op_doc("setLayerVisible", "Show/hide a layer.", json!({"type":"setLayerVisible","comp":1,"layer":1,"visible":false})),
        op_doc("setLayerLocked", "Lock/unlock a layer (locked layers reject edits).", json!({"type":"setLayerLocked","comp":1,"layer":1,"locked":true})),
        op_doc("setLayerParent", "Parent a layer to another (transforms inherit).", json!({"type":"setLayerParent","comp":1,"layer":2,"parent":1})),
        op_doc("setLayerBlendMode", "One of Normal|Multiply|Screen|Overlay|Add|Darken|Lighten|Difference.", json!({"type":"setLayerBlendMode","comp":1,"layer":1,"blendMode":"Screen"})),
        op_doc("reorderLayer", "Move a layer to an index in comp.layer_order (0 = bottom).", json!({"type":"reorderLayer","comp":1,"layer":2,"newIndex":0})),
        op_doc("addLayer", "Add a full layer object (id is reassigned by the model; give a unique provisional id).",
            json!({"type":"addLayer","comp":1,"layer":{"id":9999,"name":"Punch","kind":{"Text":{"text":"Hello","style":{"font_size":64,"color":[1,1,1,1]}}},"start":0,"duration":240000,"transform":{"position":[0,0],"scale":[100,100],"rotation":0,"opacity":1,"anchor_point":[0,0]},"tracks":{},"effects":[],"visible":true,"locked":false,"parent":null,"blend_mode":"Normal"}})),
        op_doc("removeLayer", "Delete a layer (undo restores it exactly).", json!({"type":"removeLayer","comp":1,"layer":2})),
        op_doc("setCamera", "The comp's 3D perspective camera. fov<=0 disables 3D; dof 0..1 is depth-of-field strength; focus is the sharp plane in camera space.",
            json!({"type":"setCamera","comp":1,"position":[0.0,0.0],"z":0.0,"fov":500.0,"focus":0.0,"dof":0.0})),
        op_doc("setTurntable", "Auto-orbit the camera around the comp center while playing.", json!({"type":"setTurntable","comp":1,"enabled":true,"period":12.0})),
        op_doc("createComp", "New composition.", json!({"type":"createComp","name":"Ending","width":1920,"height":1080,"fps":{"num":30,"den":1},"duration":480000})),
        op_doc("setCompProps", "Update comp name/size/fps/duration/background.", json!({"type":"setCompProps","comp":1,"name":"Main","width":1920,"height":1080,"fps":{"num":30,"den":1},"duration":480000,"background":[0.0,0.0,0.0,1.0]})),
        op_doc("batch", "Several ops as ONE undoable history entry.",
            json!({"type":"batch","label":"Rise and fade in","ops":[{"type":"setValue","comp":1,"layer":1,"property":"Position","value":{"Vec2":[0.0,-80.0]}},{"type":"setValue","comp":1,"layer":1,"property":"Opacity","value":{"Scalar":1.0}}]})),
    ];
    let effects: Vec<serde_json::Value> = registry
        .list()
        .iter()
        .map(|m| {
            json!({
                "id": m.id,
                "name": m.name,
                "category": m.category,
                "params": m.params,
            })
        })
        .collect();
    let comps: Vec<serde_json::Value> = project
        .comps
        .values()
        .map(|c| {
            json!({
                "id": c.id.0,
                "name": c.name,
                "size": [c.width, c.height],
                "duration_ticks": c.duration.0,
                "layers": c.layer_order.iter().map(|id| {
                    let l = &c.layers[id];
                    json!({
                        "id": l.id.0, "name": l.name, "locked": l.locked,
                        "visible": l.visible, "depth_z": l.transform.z,
                        "animated": l.tracks.keys().map(|k| format!("{k:?}")).collect::<Vec<_>>(),
                        "effects": l.effects.iter().map(|e| e.effect_id.clone()).collect::<Vec<_>>(),
                    })
                }).collect::<Vec<_>>(),
                "camera": c.camera,
                "turntable": c.turntable,
            })
        })
        .collect();
    json!({
        "units": {
            "time": "ticks; 120000 ticks = 1 second; comp duration is also ticks",
            "position": "pixels relative to the comp center (0,0 = center)",
            "scale": "percent (100 = natural size)",
            "rotation": "degrees, clockwise",
            "opacity": "0..1",
            "z": "depth in px; positive is farther from the camera; the 3D camera needs fov > 0; a child compounds its parent chain Z (parenting moves the subtree in depth)",
        },
        "properties": ["Position", "Scale", "Rotation", "Opacity", "AnchorPoint", "Z"],
        "value_shapes": {"Vec2": "[x, y]", "Scalar": "number"},
        "ops": ops,
        "commands": {
            "apply": "POST /api/apply {op, editGroup?} — one op; editGroup merges rapid edits into one undo entry",
            "state": "POST /api/state — full snapshot with revision",
            "undo": "POST /api/undo",
            "redo": "POST /api/redo",
            "open_project": "POST /api/open_project {json: <project json string>}",
            "save_project": "POST /api/save_project",
            "export_png": "POST /api/export_png {compId, time}",
            "describe": "POST /api/describe — this document",
        },
        "mcp_tools": ["project.info", "op.apply", "ops.propose", "effects.list", "editor.describe", "export.lut", "comp.render", "frame.view", "audio.extract", "history.undo", "history.redo", "debug.panic"],
        "effects": effects,
        "project": {
            "name": project.name,
            "comps": comps,
        },
        "recipes": {
            "animate": "batch a setValue for t=0 with an addKeyframe, scrub by setting nothing (time lives on keyframes), then addKeyframe at a later tick with the end value",
            "grade": "setLayerEffects with builtin.color_grade (exposure/contrast/saturation/temperature/lift_color/gain_color) or export a .cube via MCP export.lut",
            "diorama": "setCamera {fov:500, dof:0.8} + setValue Z on layers; setTurntable for the one-click orbit",
            "export": "export_video renders on every core in parallel and streams to ffmpeg; poll export_progress {framesDone,totalFrames,stage,startedMs} for live percent/ETA and call export_cancel to stop cleanly at the next frame boundary",
            "parent": "setLayerParent child->parent then animate the parent; children follow",
            "import": "editor commands import_svg {name, svg, compId} (SVG text → editable vector shape layers, auto-fitted, never upscaled), import_obj {name, obj, compId} (OBJ text → per-group wireframe layers with Z depth; scale them and animate Z for parallax), and vectorize_image {compId, layerId, maxColors?} (trace an embedded image layer into editable vector shapes)",
        },
    })
}

fn tool_editor_describe(session: &mut McpSession, _args: serde_json::Value) -> ToolCallResult {
    ToolCallResult::json(&editor_describe(&session.project, &builtin_registry()))
        .unwrap_or_else(|e| ToolCallResult::error(e.to_string()))
}

/// Executes a tool call by name with provided JSON arguments.
pub fn execute_tool(
    session: &mut McpSession,
    name: &str,
    args: serde_json::Value,
) -> ToolCallResult {
    match name {
        // Chaos drill: deliberately panics so hosts can verify that tool
        // crashes are contained (JSON-RPC error, process and session live).
        "debug.panic" => panic!("debug.panic drill: this tool always panics"),
        "export.lut" => tool_export_lut(session, args),
        "effects.list" => ToolCallResult::json(&json!({"effects": builtin_registry().list(), "renderer": "CPU reference", "gpu_available": false})).unwrap_or_else(|e| ToolCallResult::error(e.to_string())),
        "project.create" => tool_project_create(session, args),
        "project.open" => tool_project_open(session, args),
        "project.save" => tool_project_save(session, args),
        "project.info" => tool_project_info(session, args),
        "editor.describe" => tool_editor_describe(session, args),
        "op.apply" => tool_op_apply(session, args),
        "ops.propose" => tool_ops_propose(session, args),
        "history.list" => tool_history_list(session, args),
        "history.undo" => tool_history_undo(session, args),
        "history.redo" => tool_history_redo(session, args),
        "comp.render" => tool_comp_render(session, args),
    "frame.view" => tool_frame_view(session, args),
    "audio.extract" => tool_audio_extract(session, args),
        unknown => ToolCallResult::error(format!("Unknown tool: '{unknown}'")),
    }
}

// ---------------------------------------------------------------------------
// Tool Implementations
// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// Vision-Native MCP: the frame viewer and the audio extractor
// ---------------------------------------------------------------------------

/// Nearest-neighbour RGBA downscale — cheap, deterministic, good enough for
/// a model to read layout, text and color. Aspect is preserved.
fn downscale_rgba(width: u32, height: u32, rgba: &[u8], target_w: u32) -> (u32, u32, Vec<u8>) {
    let target_w = target_w.clamp(16, 1024).min(width);
    if target_w == width {
        return (width, height, rgba.to_vec());
    }
    let target_h = ((u64::from(height) * u64::from(target_w)) / u64::from(width)).max(1) as u32;
    let mut out = vec![0u8; (target_w as usize) * (target_h as usize) * 4];
    for y in 0..target_h {
        let sy = (u64::from(y) * u64::from(height) / u64::from(target_h)) as u32;
        for x in 0..target_w {
            let sx = (u64::from(x) * u64::from(width) / u64::from(target_w)) as u32;
            let src = ((sy as usize * width as usize) + sx as usize) * 4;
            let dst = ((y as usize * target_w as usize) + x as usize) * 4;
            out[dst..dst + 4].copy_from_slice(&rgba[src..src + 4]);
        }
    }
    (target_w, target_h, out)
}

fn resolve_comp_id(session: &McpSession, comp_id: Option<u64>) -> Result<CompId, String> {
    match comp_id {
        Some(id) => {
            if session.project.comp(CompId(id)).is_some() {
                Ok(CompId(id))
            } else {
                Err(format!("Composition {id} does not exist"))
            }
        }
        None => session
            .active_comp
            .or_else(|| session.project.comps.keys().next().copied())
            .ok_or_else(|| "The project has no compositions".to_string()),
    }
}

fn tool_frame_view(session: &McpSession, args: serde_json::Value) -> ToolCallResult {
    #[derive(Deserialize, Default)]
    #[serde(default)]
    struct View {
        comp_id: Option<u64>,
        time: Option<f64>,
        count: Option<usize>,
        start_time: Option<f64>,
        end_time: Option<f64>,
        width: Option<u32>,
    }
    let v: View = match serde_json::from_value(args) {
        Ok(v) => v,
        Err(e) => return ToolCallResult::error(format!("Invalid arguments for frame.view: {e}")),
    };
    let comp_id = match resolve_comp_id(session, v.comp_id) {
        Ok(c) => c,
        Err(e) => return ToolCallResult::error(e),
    };
    let comp = session.project.comp(comp_id).expect("resolved comp");
    let duration_secs = comp.duration.as_secs_f64();
    let times: Vec<f64> = if let Some(t) = v.time {
        if !t.is_finite() || t < 0.0 || t > duration_secs + 1.0 {
            return ToolCallResult::error(format!(
                "time must be within the composition (0–{duration_secs:.3}s)"
            ));
        }
        vec![t.min(duration_secs)]
    } else {
        let count = v.count.unwrap_or(4).clamp(1, 12);
        let start = v.start_time.unwrap_or(0.0).max(0.0);
        let end = v.end_time.unwrap_or(duration_secs).min(duration_secs);
        if end <= start {
            return ToolCallResult::error("end_time must be greater than start_time");
        }
        (0..count)
            .map(|i| start + (end - start) * i as f64 / count.max(2) as f64)
            .collect()
    };
    let target = v.width.unwrap_or(512);
    let frames = match decode_embedded_frames(&session.project) {
        Ok(f) => f,
        Err(e) => return ToolCallResult::error(e),
    };
    let mut meta = Vec::with_capacity(times.len());
    let mut blocks = Vec::with_capacity(times.len() + 1);
    for t in &times {
        let frame = match render_comp(&session.project, comp_id, Time::from_secs_f64(*t), &frames) {
            Ok(f) => f,
            Err(e) => return ToolCallResult::error(format!("Render failed: {e}")),
        };
        let (w, h, rgba) = downscale_rgba(frame.width, frame.height, &frame.rgba, target);
        let png = match encode_png(w, h, &rgba) {
            Ok(b) => b,
            Err(e) => return ToolCallResult::error(format!("PNG encoding error: {e}")),
        };
        meta.push(json!({
            "time_secs": t,
            "width": w,
            "height": h,
        }));
        use base64::Engine;
        blocks.push(TextContent::image(
            "image/png",
            base64::engine::general_purpose::STANDARD.encode(&png),
        ));
    }
    blocks.insert(
        0,
        TextContent::text(
            json!({
                "view": "storyboard",
                "comp": comp.name,
                "frames": meta,
                "note": "The images above are the actual composition — inspect them visually."
            })
            .to_string(),
        ),
    );
    ToolCallResult::rich(blocks)
}

fn tool_audio_extract(session: &mut McpSession, args: serde_json::Value) -> ToolCallResult {
    #[derive(Deserialize, Default)]
    #[serde(default)]
    struct Extract {
        comp_id: Option<u64>,
        start_secs: Option<f64>,
        duration_secs: Option<f64>,
        rate: Option<u32>,
        mono: Option<bool>,
    }
    let e: Extract = match serde_json::from_value(args) {
        Ok(e) => e,
        Err(err) => {
            return ToolCallResult::error(format!("Invalid arguments for audio.extract: {err}"))
        }
    };
    let comp_id = match resolve_comp_id(session, e.comp_id) {
        Ok(c) => c,
        Err(err) => return ToolCallResult::error(err),
    };
    let comp = session.project.comp(comp_id).expect("resolved comp");
    let comp_secs = comp.duration.as_secs_f64();
    let rate = e.rate.unwrap_or(16_000).clamp(8_000, 48_000);
    let mono = e.mono.unwrap_or(true);
    let start = e.start_secs.unwrap_or(0.0).max(0.0);
    let duration = e
        .duration_secs
        .unwrap_or_else(|| comp_secs.min(20.0))
        .clamp(0.1, 30.0)
        .min((comp_secs - start).max(0.1));
    if start >= comp_secs {
        return ToolCallResult::error(format!(
            "start_secs {start} is past the composition ({comp_secs:.3}s)"
        ));
    }
    // Live session's decoded cache when hosted (avoids re-decode); fresh otherwise.
    let sources = match &session.live {
        Some(live) => live.lock().expect("live session").audio.clone(),
        None => match bonaparte_runtime::decode_project_audio(&session.project) {
            Ok(s) => s,
            Err(err) => return ToolCallResult::error(err),
        },
    };
    let plan = match bonaparte_audio::MixPlan::new(&session.project, comp_id, &sources) {
        Ok(p) => p,
        Err(err) => return ToolCallResult::error(err),
    };
    let start_frame = (start * rate as f64).round() as i64;
    let frame_count = (duration * rate as f64).round() as usize;
    let block = match plan.render(start_frame, frame_count, rate) {
        Ok(b) => b,
        Err(err) => return ToolCallResult::error(err),
    };
    // The mix renders stereo interleaved; mix down when asked.
    let (channels, samples): (u16, Vec<f32>) = if mono {
        let stereo = &block.samples;
        let mut m = Vec::with_capacity(stereo.len() / 2);
        for pair in stereo.chunks(2) {
            m.push(match pair {
                [l, r] => (l + r) * 0.5,
                [only] => *only,
                _ => 0.0,
            });
        }
        (1, m)
    } else {
        (2, block.samples.clone())
    };
    let peak = samples.iter().fold(0.0f32, |a, s| a.max(s.abs()));
    let rms = if samples.is_empty() {
        0.0
    } else {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    };
    // 16-bit PCM WAV.
    let mut wav = Vec::with_capacity(44 + samples.len() * 2);
    let data_len = samples.len() * 2;
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate * u32::from(channels) * 2).to_le_bytes());
    wav.extend_from_slice(&(channels * 2).to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(data_len as u32).to_le_bytes());
    for s in &samples {
        let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        wav.extend_from_slice(&v.to_le_bytes());
    }
    let db = |x: f32| {
        if x > 0.0 {
            20.0 * (x as f64).log10()
        } else {
            -96.0
        }
    };
    let meta = json!({
        "comp": comp.name,
        "start_secs": start,
        "duration_secs": duration,
        "sampleRate": rate,
        "channels": channels,
        "peakDb": (db(peak) * 10.0).round() / 10.0,
        "rmsDb": (db(rms) * 10.0).round() / 10.0,
        "note": "The audio block above is the actual mix — listen to it."
    });
    use base64::Engine;
    ToolCallResult::rich(vec![
        TextContent::text(meta.to_string()),
        TextContent::audio(
            "audio/wav",
            base64::engine::general_purpose::STANDARD.encode(&wav),
        ),
    ])
}

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
    if session.is_hosted() {
        // Route through the host so the running editor adopts the new
        // document atomically (its own validation runs again there).
        let json = match bonaparte_runtime::serialize_project(&project) {
            Ok(json) => json,
            Err(e) => return ToolCallResult::error(e),
        };
        return match session
            .lock_host()
            .and_then(|mut host| host.command("open_project", json!({"json": json})))
        {
            Ok(_) => {
                session.sync_from_host();
                ToolCallResult::json(&json!({
                    "success": true,
                    "comp_id": comp_id.0,
                    "hosted": true,
                }))
                .unwrap_or_else(|e| ToolCallResult::error(e.to_string()))
            }
            Err(e) => ToolCallResult::error(e),
        };
    }
    session.project = project;
    session.history = bonaparte_runtime::journal::fresh_history();
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

    // Hosted sessions route inline JSON straight into the live editor; only
    // the path variant falls through to local file reading first.
    if session.is_hosted() && parsed.path.is_none() {
        if let Some(json_string) = parsed.json {
            return match session
                .lock_host()
                .and_then(|mut host| host.command("open_project", json!({"json": json_string})))
            {
                Ok(_) => {
                    session.sync_from_host();
                    ToolCallResult::json(&json!({ "success": true, "hosted": true }))
                        .unwrap_or_else(|e| ToolCallResult::error(e.to_string()))
                }
                Err(e) => ToolCallResult::error(e),
            };
        }
        return ToolCallResult::error("project.open requires either 'path' or 'json' argument");
    }

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

#[derive(Deserialize)]
struct ExportLutArgs {
    layer_id: u64,
    #[serde(default = "default_lut_size")]
    size: u32,
    #[serde(default)]
    time_ticks: i64,
}
fn default_lut_size() -> u32 {
    33
}

/// Bakes a layer's effect stack into an industry-standard .cube LUT.
fn tool_export_lut(session: &mut McpSession, args: serde_json::Value) -> ToolCallResult {
    let parsed: ExportLutArgs = match serde_json::from_value(args) {
        Ok(a) => a,
        Err(e) => return ToolCallResult::error(format!("Invalid arguments for export.lut: {e}")),
    };
    let Some(comp) = session.target_comp(None) else {
        return ToolCallResult::error("No composition to bake from");
    };
    let Some(layer) = session
        .project
        .comps
        .get(&comp)
        .and_then(|c| c.layers.get(&LayerId(parsed.layer_id)))
    else {
        return ToolCallResult::error(format!("Layer {} not found", parsed.layer_id));
    };
    if layer.effects.is_empty() {
        return ToolCallResult::error("The layer has no effects to bake into a LUT");
    }
    let cube = match bonaparte_effects::export_cube(
        builtin_registry(),
        &layer.effects,
        parsed.size,
        Time(parsed.time_ticks),
    ) {
        Ok(cube) => cube,
        Err(e) => return ToolCallResult::error(e),
    };
    ToolCallResult::json(&json!({
        "format": "cube",
        "size": parsed.size,
        "bytes": cube.len(),
        "content": cube,
    }))
    .unwrap_or_else(|e| ToolCallResult::error(e.to_string()))
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
    // Hosted sessions route through the live editor: the op lands in the
    // editor's native undo history, preview caches invalidate, and the UI
    // reflects AI edits immediately.
    if session.is_hosted() {
        return match session
            .lock_host()
            .and_then(|mut host| host.command("apply", json!({"op": op})))
        {
            Ok(_) => {
                session.sync_from_host();
                let undo_depth = session
                    .lock_host()
                    .map(|host| host.history.undo_descriptions().len())
                    .unwrap_or(0);
                ToolCallResult::json(&json!({
                    "success": true,
                    "description": description,
                    "undo_depth": undo_depth,
                }))
                .unwrap_or_else(|e| ToolCallResult::error(e.to_string()))
            }
            Err(e) => ToolCallResult::error(format!("Op validation failed: {e}")),
        };
    }
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
    let mut preview_history = session.history.window_clone();

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
        return ToolCallResult::error("Frame rate must be between 1 and 1000 fps");
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
            audio: match bonaparte_runtime::decode_project_audio(&session.project) {
                Ok(audio) => audio,
                Err(error) => return ToolCallResult::error(error),
            },
            registry: builtin_registry().clone(),
            bit_depth: 8,
            output_space: Default::default(),
            draft: false,
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
