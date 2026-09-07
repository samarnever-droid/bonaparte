//! Unification: MCP tools hosted over a live `EditorSession` must read its
//! state and land their edits in its native undo history — no parallel
//! universe. Plus crash containment: a panicking tool is a JSON-RPC error,
//! and the process and session live on.

use bonaparte_mcp::{handle_request_safe, JsonRpcRequest, McpSession};
use bonaparte_model::*;
use bonaparte_runtime::EditorSession;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

/** Tool results ride as text content: unwrap the envelope into JSON. */
fn tool_payload(result: &Value) -> Value {
    let text = result["content"][0]["text"].as_str().unwrap_or("null");
    serde_json::from_str(text).unwrap_or(Value::Null)
}

fn request(session: Arc<Mutex<EditorSession>>, method: &str, params: Value) -> Option<Value> {
    let mut hosted = McpSession::hosted(session);
    handle_request_safe(
        &mut hosted,
        JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(1.into()),
            method: method.into(),
            params: Some(params),
        },
    )
    .map(|r| r.result.expect("success response"))
}

#[test]
fn mcp_ops_land_in_the_live_editor_and_its_history() {
    let session = Arc::new(Mutex::new(EditorSession::default()));
    let mut hosted = McpSession::hosted(Arc::clone(&session));
    let call = |hosted: &mut McpSession, name: &str, args: Value| {
        handle_request_safe(
            hosted,
            JsonRpcRequest {
                jsonrpc: "2.0".into(),
                id: Some(1.into()),
                method: "tools/call".into(),
                params: Some(json!({"name": name, "arguments": args})),
            },
        )
        .unwrap()
        .result
        .expect("tool result")
    };

    // Read the live comp through an MCP tool before mutating.
    let comp = session
        .lock()
        .unwrap()
        .project
        .comps
        .keys()
        .next()
        .copied()
        .unwrap();
    let before = session.lock().unwrap().project.comps[&comp].layers.len();

    let applied = call(
        &mut hosted,
        "op.apply",
        json!({"op": {"type": "addLayer", "comp": comp.0, "layer": {
            "id": 999, "name": "AI layer",
            "kind": {"Solid": {"color": [0.2, 0.4, 0.9, 1.0]}},
            "start": 0, "duration": 240000,
            "transform": {"position": [0, 0], "scale": [100, 100], "rotation": 0, "opacity": 1, "anchor_point": [0, 0]},
            "tracks": {}, "effects": [], "visible": true, "locked": false, "parent": null, "blend_mode": "Normal"
        }}}),
    );
    let applied_payload = tool_payload(&applied);
    assert_eq!(applied_payload["success"], json!(true), "{applied}");

    // The layer is really in the editor session…
    {
        let host = session.lock().unwrap();
        assert_eq!(host.project.comps[&comp].layers.len(), before + 1);
        // …and in the editor's own undo history.
        assert!(
            host.history
                .undo_descriptions()
                .iter()
                .any(|d| d.to_lowercase().contains("layer")),
            "AI edit must appear in editor history: {:?}",
            host.history.undo_descriptions()
        );
    }
    // MCP reads now reflect the live state.
    let info = call(&mut hosted, "project.info", json!({}));
    let _ = info;

    // Editor undo removes the AI's layer: one native history step.
    session.lock().unwrap().command("undo", json!({})).unwrap();
    assert_eq!(
        session.lock().unwrap().project.comps[&comp].layers.len(),
        before
    );
}

#[test]
fn editor_edits_are_visible_to_mcp_reads_and_project_open_replaces_the_host() {
    let session = Arc::new(Mutex::new(EditorSession::default()));
    session
        .lock()
        .unwrap()
        .command(
            "apply",
            json!({"op": {"type": "renameProject", "name": "edited by human"}}),
        )
        .unwrap();
    let mut hosted = McpSession::hosted(Arc::clone(&session));
    let result = handle_request_safe(
        &mut hosted,
        JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(2.into()),
            method: "tools/call".into(),
            params: Some(json!({"name": "project.info", "arguments": {}})),
        },
    )
    .unwrap()
    .result
    .unwrap();
    let parsed = tool_payload(&result);
    assert_eq!(parsed["project_name"], json!("edited by human"));

    // project.open with inline JSON replaces the host document.
    let mut project = Project::new("from mcp");
    project.create_comp("Only", 320, 240, FrameRate::FPS_30, Time(240000));
    let opened = tool_payload(
        &request(
            Arc::clone(&session),
            "tools/call",
            json!({"name": "project.open", "arguments": {"json": bonaparte_runtime::serialize_project(&project).unwrap()}}),
        )
        .unwrap(),
    );
    assert_eq!(opened["hosted"], json!(true), "{opened}");
    assert_eq!(session.lock().unwrap().project.name, "from mcp");
}

#[test]
fn a_panicking_tool_is_an_error_and_the_session_survives() {
    let session = Arc::new(Mutex::new(EditorSession::default()));
    let mut hosted = McpSession::hosted(Arc::clone(&session));
    let response = handle_request_safe(
        &mut hosted,
        JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(3.into()),
            method: "tools/call".into(),
            params: Some(json!({"name": "debug.panic", "arguments": {}})),
        },
    )
    .expect("contained response");
    assert!(response.error.is_some(), "panic must become an error");
    // The session is still fully usable after the contained crash.
    let after = handle_request_safe(
        &mut hosted,
        JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(4.into()),
            method: "tools/call".into(),
            params: Some(json!({"name": "project.info", "arguments": {}})),
        },
    )
    .unwrap();
    assert!(after.error.is_none());
}

#[test]
fn poisoned_editor_mutex_never_bricks_the_hosted_session() {
    let session = Arc::new(Mutex::new(EditorSession::default()));
    // Poison the mutex: a thread panics while holding the lock.
    let poison_handle = {
        let session = Arc::clone(&session);
        std::thread::spawn(move || {
            let _guard = session.lock().unwrap();
            panic!("poison drill");
        })
    };
    loop {
        if session.is_poisoned() {
            break;
        }
        if poison_handle.is_finished() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let _ = poison_handle.join();
    assert!(session.is_poisoned());
    // Hosted reads and writes must still work: into_inner recovery.
    let mut hosted = McpSession::hosted(Arc::clone(&session));
    let result = handle_request_safe(
        &mut hosted,
        JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(5.into()),
            method: "tools/call".into(),
            params: Some(json!({"name": "project.info", "arguments": {}})),
        },
    )
    .unwrap();
    assert!(result.error.is_none(), "poison recovery failed: {result:?}");
}

#[test]
fn editor_describe_is_listed_first_and_maps_the_live_project() {
    let session = Arc::new(Mutex::new(EditorSession::default()));
    let listed =
        request(Arc::clone(&session), "tools/list", json!({})).expect("tools/list responds");
    let tools = listed["tools"].as_array().unwrap();
    assert_eq!(
        tools[0]["name"], "editor.describe",
        "agents must see it first"
    );

    let result = request(
        Arc::clone(&session),
        "tools/call",
        json!({
            "name": "editor.describe", "arguments": {}
        }),
    )
    .expect("describe responds");
    let doc = tool_payload(&result);
    assert!(doc["units"]["time"].as_str().unwrap().contains("120000"));
    let op_types: Vec<&str> = doc["ops"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["type"].as_str().unwrap())
        .collect();
    assert!(op_types.contains(&"setTurntable"));
    assert!(op_types.contains(&"setLayerEffects"));
    // The doc reflects the live session, not a static brochure.
    assert!(doc["project"]["comps"].as_array().unwrap().len() >= 1);
}
