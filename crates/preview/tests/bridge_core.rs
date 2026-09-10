//! Bridge core guarantees: the /api/mcp unified tool surface drives the live
//! session, and poisoned mutexes never brick request handling.

use bonaparte_preview::{route, safe_route};
use bonaparte_runtime::EditorSession;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

fn session() -> Arc<Mutex<EditorSession>> {
    Arc::new(Mutex::new(EditorSession::default()))
}

fn mcp_call(name: &str, args: Value) -> (u16, Value) {
    let session = session();
    let body = json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": name, "arguments": args}
    })
    .to_string();
    let (status, mime, bytes) = route(&session, "mcp".into(), body);
    assert_eq!(mime, "application/json");
    let envelope: Value = serde_json::from_slice(&bytes).unwrap();
    let text = envelope["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or("null");
    let payload: Value = serde_json::from_str(text).unwrap_or(Value::Null);
    (status, payload)
}

#[test]
fn the_mcp_route_drives_the_live_editor_session() {
    let session = session();
    let body = json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": "op.apply", "arguments": {"op": {"type": "renameProject", "name": "unified"}}}
    })
    .to_string();
    let (status, _, bytes) = route(&session, "mcp".into(), body);
    assert_eq!(status, 200);
    let _ = bytes;
    // The editor's own state now carries the AI's rename.
    let state = session.lock().unwrap().command("state", json!({})).unwrap();
    assert_eq!(state["project"]["name"], json!("unified"));
    // And the editor undo stack owns the change: one undo reverts it.
    session.lock().unwrap().command("undo", json!({})).unwrap();
    let state = session.lock().unwrap().command("state", json!({})).unwrap();
    assert_eq!(state["project"]["name"], json!("Orbit — Studio ident"));
}

#[test]
fn panicking_tools_return_contained_errors_not_dead_workers() {
    let session = session();
    let body = json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": "debug.panic", "arguments": {}}
    })
    .to_string();
    let (status, _, bytes) = safe_route(&session, "mcp".into(), body);
    assert_eq!(status, 200, "JSON-RPC layer reports tool errors in-band");
    let envelope: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(
        envelope["result"]["isError"] == json!(true) || envelope["error"].is_object(),
        "panic must surface as an MCP error: {envelope}"
    );
    // The bridge keeps serving afterwards.
    let (status, _, _) = route(&session, "state".into(), "{}".into());
    assert_eq!(status, 200);
}

#[test]
fn poisoned_locks_recover_instead_of_failing_requests() {
    let session = session();
    // Poison it the way a panicking request would.
    let poisoner = {
        let session = Arc::clone(&session);
        std::thread::spawn(move || {
            let _guard = session.lock().unwrap();
            panic!("poison drill");
        })
    };
    while !session.is_poisoned() && !poisoner.is_finished() {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let _ = poisoner.join();
    assert!(session.is_poisoned());
    let (status, _, _) = route(&session, "state".into(), "{}".into());
    assert_eq!(status, 200, "poisoned mutex must recover, not fail");
}

#[test]
fn export_lut_bakes_a_layers_effect_stack_through_the_bridge() {
    let session = session();
    // The default project's text layer gets a +1EV grade; LUT baking must
    // refuse layers without effects and produce a parseable cube otherwise.
    let layer_id = {
        let host = session.lock().unwrap();
        let comp = *host.project.comps.keys().next().unwrap();
        host.project.comps[&comp].layer_order[0]
    };
    let (status, _, bytes) = route(
        &session,
        "export_lut".into(),
        json!({"compId": 1, "layerId": layer_id.0, "size": 8}).to_string(),
    );
    assert_eq!(status, 400, "layers without effects must be refused");
    let error: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(error["error"].as_str().unwrap().contains("no effects"));

    session
        .lock()
        .unwrap()
        .command(
            "apply",
            json!({"op": {"type": "setLayerEffects", "comp": 1, "layer": layer_id.0, "effects": [
                {"id": "grade", "effect_id": "builtin.color_grade", "enabled": true,
                 "params": {"exposure": {"Float": 1.0}}, "tracks": {}}
            ]}}),
        )
        .unwrap();
    let (status, mime, bytes) = route(
        &session,
        "export_lut".into(),
        json!({"compId": 1, "layerId": layer_id.0, "size": 8}).to_string(),
    );
    assert_eq!(status, 200);
    assert_eq!(mime, "text/plain");
    let cube = String::from_utf8(bytes).unwrap();
    assert!(cube.starts_with("TITLE \"Bonaparte baked grade\"\nLUT_3D_SIZE 8\n"));
    assert_eq!(cube.lines().count(), 4 + 8 * 8 * 8);
    // Deterministic across cache clears.
    session
        .lock()
        .unwrap()
        .command("clear_preview_cache", json!({}))
        .unwrap();
    let (status, _, bytes2) = route(
        &session,
        "export_lut".into(),
        json!({"compId": 1, "layerId": layer_id.0, "size": 8}).to_string(),
    );
    assert_eq!(status, 200);
    assert_eq!(cube, String::from_utf8(bytes2).unwrap());
}

#[test]
fn describe_maps_every_capability_and_hints_rejected_ops() {
    let session = session();
    let (status, _, bytes) = route(&session, "describe".into(), "{}".to_string());
    assert_eq!(status, 200);
    let doc: Value = serde_json::from_slice(&bytes).unwrap();
    // Units and time semantics are the #1 thing agents get wrong.
    assert!(doc["units"]["time"].as_str().unwrap().contains("120000"));
    let op_types: Vec<&str> = doc["ops"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["type"].as_str().unwrap())
        .collect();
    for expected in [
        "setValue",
        "addKeyframe",
        "setLayerEffects",
        "setCamera",
        "batch",
    ] {
        assert!(op_types.contains(&expected), "missing op doc: {expected}");
    }
    assert!(doc["effects"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["id"] == "builtin.color_grade"));
    assert!(doc["project"]["comps"].as_array().unwrap().len() >= 1);
    // Every op doc carries a concrete example an agent can paste.
    for op in doc["ops"].as_array().unwrap() {
        assert!(
            op["example"].is_object(),
            "op {} lacks an example",
            op["type"]
        );
    }

    // A rejected op explains itself.
    let (status, _, bytes) = route(
        &session,
        "apply".into(),
        json!({"op": {"type": "setValue", "comp": 1, "layer": 1, "property": "Age", "value": {"Scalar": 3.0}}})
            .to_string(),
    );
    assert_eq!(status, 400);
    let message = String::from_utf8(bytes).unwrap();
    assert!(message.contains("Position|Scale|Rotation"), "{message}");
    assert!(message.contains("120000"), "{message}");
    assert!(message.contains("/api/describe"), "{message}");
}

#[test]
fn undo_and_redo_responses_name_what_changed() {
    let session = session();
    let (status, _, bytes) = route(
        &session,
        "apply".into(),
        json!({"op": {"type": "renameProject", "name": "labelled"}}).to_string(),
    );
    assert_eq!(status, 200);
    let (status, _, bytes) = route(&session, "undo".into(), "{}".to_string());
    assert_eq!(status, 200);
    let response: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(response["lastUndone"], "Renamed project to “labelled”");
    let (status, _, bytes) = route(&session, "redo".into(), "{}".to_string());
    assert_eq!(status, 200);
    let response: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(response["lastRedone"], "Renamed project to “labelled”");
    // Undoing past the bottom of history carries no label (nothing to name).
    let mut label_present = true;
    for _ in 0..10 {
        let (_, _, bytes) = route(&session, "undo".into(), "{}".to_string());
        let response: Value = serde_json::from_slice(&bytes).unwrap();
        label_present = response.get("lastUndone").is_some();
        if !label_present {
            break;
        }
    }
    assert!(!label_present, "drained undo must stop naming entries");
}

// ── Built-in scripting: the same twelve tools, sequenced on the server ────

fn script(session: &Arc<Mutex<EditorSession>>, body: Value) -> (u16, Value) {
    let (status, _, bytes) = route(session, "script.run".into(), body.to_string());
    let payload: Value = serde_json::from_slice(&bytes).unwrap();
    (status, payload)
}

#[test]
fn a_script_runs_the_mcp_tools_in_order_against_the_live_session() {
    let session = session();
    let (status, payload) = script(
        &session,
        json!({"steps": [
            {"tool": "op.apply", "args": {"op": {"type": "renameProject", "name": "sequenced"}}},
            {"tool": "op.apply", "args": {"op": {"type": "renameProject", "name": "twice renamed"}}},
            {"tool": "project.info", "args": {}}
        ]}),
    );
    assert_eq!(status, 200);
    assert_eq!(payload["ran"], json!(3));
    assert_eq!(payload["failed"], json!(false));
    assert!(payload["steps"].is_array());
    let state = session.lock().unwrap().command("state", json!({})).unwrap();
    assert_eq!(state["project"]["name"], json!("twice renamed"));
    // Editor undo history owns the script: two steps, two undos.
    session.lock().unwrap().command("undo", json!({})).unwrap();
    let state = session.lock().unwrap().command("state", json!({})).unwrap();
    assert_eq!(state["project"]["name"], json!("sequenced"));
    session.lock().unwrap().command("undo", json!({})).unwrap();
    let state = session.lock().unwrap().command("state", json!({})).unwrap();
    assert_eq!(state["project"]["name"], json!("Orbit — Studio ident"));
}

#[test]
fn scripts_stop_at_the_first_failure_unless_asked_to_continue() {
    let session = session();
    let (_, payload) = script(
        &session,
        json!({"steps": [
            {"tool": "not.a.tool", "args": {}},
            {"tool": "project.info", "args": {}}
        ]}),
    );
    assert_eq!(payload["failed"], json!(true));
    assert_eq!(payload["ran"], json!(1), "failure stops the run by default");
    let (_, payload) = script(
        &session,
        json!({"stopOnFailure": false, "steps": [
            {"tool": "not.a.tool", "args": {}},
            {"tool": "project.info", "args": {}}
        ]}),
    );
    assert_eq!(payload["ran"], json!(2), "continueOnError keeps going");
    assert_eq!(payload["steps"][0]["ok"], json!(false));
    assert_eq!(payload["steps"][1]["ok"], json!(true));
}

#[test]
fn a_panicking_script_step_is_a_recorded_failure_and_the_bridge_survives() {
    let session = session();
    let (status, payload) = script(
        &session,
        json!({"steps": [
            {"tool": "debug.panic", "args": {}},
            {"tool": "project.info", "args": {}}
        ]}),
    );
    assert_eq!(status, 200, "the script protocol answers in-band");
    assert_eq!(payload["ran"], json!(1), "the panic stops the run");
    assert_eq!(payload["steps"][0]["ok"], json!(false));
    let (status, _, _) = route(&session, "state".into(), "{}".into());
    assert_eq!(status, 200, "and the bridge is still healthy");
}

#[test]
fn malformed_scripts_are_rejected_without_touching_the_session() {
    let session = session();
    let (status, _, _) = route(&session, "script.run".into(), "{ nope".into());
    assert_eq!(status, 400);
    let (status, _, _) = route(
        &session,
        "script.run".into(),
        json!({"steps": []}).to_string(),
    );
    assert_eq!(status, 400);
    let before = session.lock().unwrap().command("state", json!({})).unwrap();
    let name = before["project"]["name"].clone();
    let (status, _, _) = route(
        &session,
        "script.run".into(),
        json!({"steps": [{"noTool": true}]}).to_string(),
    );
    assert_eq!(
        status, 200,
        "a step without a tool fails as a step, not a request"
    );
    let after = session.lock().unwrap().command("state", json!({})).unwrap();
    assert_eq!(after["project"]["name"], name);
}
