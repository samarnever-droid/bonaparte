//! Integration tests for bonaparte-mcp server and all tools.

use std::fs;
use std::io::Cursor;

use bonaparte_mcp::protocol::{JsonRpcRequest, JsonRpcResponse};
use bonaparte_mcp::server::{handle_request, run_stdio};
use bonaparte_mcp::session::McpSession;
use bonaparte_mcp::tools::execute_tool;
use bonaparte_model::{BlendMode, Layer, LayerId, LayerKind, Op, StaticTransform, Time};
use serde_json::json;

#[test]
fn test_mcp_initialize() {
    let mut session = McpSession::new();
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(1)),
        method: "initialize".to_string(),
        params: Some(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "test-client", "version": "1.0" }
        })),
    };

    let resp = handle_request(&mut session, req).expect("Expected response");
    assert_eq!(resp.id, json!(1));
    assert!(resp.error.is_none());
    let result = resp.result.expect("Expected result object");
    assert_eq!(result["protocolVersion"], "2024-11-05");
    assert_eq!(result["serverInfo"]["name"], "bonaparte-mcp");
}

#[test]
fn test_mcp_tools_list_catalog() {
    let mut session = McpSession::new();
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!("req-2")),
        method: "tools/list".to_string(),
        params: None,
    };

    let resp = handle_request(&mut session, req).expect("Expected response");
    let result = resp.result.expect("Expected result");
    let tools = result["tools"].as_array().expect("Tools must be an array");

    let tool_names: Vec<&str> = tools
        .iter()
        .map(|t| t["name"].as_str().expect("Tool name"))
        .collect();

    let expected = [
        "project.create",
        "project.open",
        "project.save",
        "project.info",
        "op.apply",
        "ops.propose",
        "history.list",
        "history.undo",
        "history.redo",
        "comp.render",
    ];

    for name in expected {
        assert!(
            tool_names.contains(&name),
            "Missing required tool in catalog: {name}"
        );
    }
}

#[test]
fn test_project_create_and_info() {
    let mut session = McpSession::new();

    // 1. Create a 720p 60fps project
    let res = execute_tool(
        &mut session,
        "project.create",
        json!({
            "name": "Commercial_Spot",
            "comp_name": "Teaser",
            "width": 1280,
            "height": 720,
            "fps": 60.0,
            "duration": 5.0
        }),
    );
    assert!(!res.is_error, "Create project failed: {:?}", res.content);

    // 2. Query project.info
    let info_res = execute_tool(&mut session, "project.info", json!({}));
    assert!(!info_res.is_error);
    let info: serde_json::Value =
        serde_json::from_str(&info_res.content[0].text).expect("Valid JSON");
    assert_eq!(info["project_name"], "Commercial_Spot");
    assert_eq!(info["comp_count"], 1);

    let comp = &info["comps"][0];
    assert_eq!(comp["name"], "Teaser");
    assert_eq!(comp["width"], 1280);
    assert_eq!(comp["height"], 720);
    assert_eq!(comp["duration_secs"], 5.0);
}

#[test]
fn test_op_apply_and_history_undo_redo() {
    let mut session = McpSession::new();
    let comp_id = session.active_comp.expect("Active comp exists");

    // Add a solid layer via Op::AddLayer
    let layer = Layer {
        id: LayerId(101),
        name: "Solid Background".to_string(),
        kind: LayerKind::Solid {
            color: [0.2, 0.4, 0.8, 1.0],
        },
        start: Time::ZERO,
        duration: Time::from_secs_f64(10.0),
        parent: None,
        blend_mode: BlendMode::Normal,
        visible: true,
        locked: false,
        effects: Default::default(),
        transform: StaticTransform::default(),
        tracks: Default::default(),
    };

    let op = Op::AddLayer {
        comp: comp_id,
        layer,
    };

    let apply_res = execute_tool(&mut session, "op.apply", json!({ "op": op }));
    assert!(
        !apply_res.is_error,
        "Failed to apply Op: {:?}",
        apply_res.content
    );

    // Verify layer is present
    let comp = session.project.comp(comp_id).unwrap();
    let allocated_id = *comp.layer_order.last().expect("Layer was inserted");
    assert!(comp.layers.contains_key(&allocated_id));
    assert_eq!(session.history.undo_descriptions().len(), 1);

    // Undo
    let undo_res = execute_tool(&mut session, "history.undo", json!({}));
    assert!(!undo_res.is_error);
    let undo_val: serde_json::Value = serde_json::from_str(&undo_res.content[0].text).unwrap();
    assert_eq!(undo_val["undone"], true);

    let comp_after_undo = session.project.comp(comp_id).unwrap();
    assert!(!comp_after_undo.layers.contains_key(&allocated_id));

    // Redo
    let redo_res = execute_tool(&mut session, "history.redo", json!({}));
    assert!(!redo_res.is_error);
    let redo_val: serde_json::Value = serde_json::from_str(&redo_res.content[0].text).unwrap();
    assert_eq!(redo_val["redone"], true);

    let comp_after_redo = session.project.comp(comp_id).unwrap();
    assert!(comp_after_redo.layers.contains_key(&allocated_id));
}

#[test]
fn test_ops_propose_dry_run_does_not_mutate_session() {
    let mut session = McpSession::new();
    let comp_id = session.active_comp.expect("Active comp");
    let initial_layer_count = session.project.comp(comp_id).unwrap().layers.len();
    let expected_allocated_id = session.project.next_layer;

    let mut layer1 = Layer::new_rect(
        "Layer 1",
        [1.0, 0.0, 0.0, 1.0],
        Time::ZERO,
        Time::from_secs_f64(5.0),
    );
    layer1.id = LayerId(201);

    let op1 = Op::AddLayer {
        comp: comp_id,
        layer: layer1,
    };
    let op2 = Op::RenameLayer {
        comp: comp_id,
        layer: expected_allocated_id,
        name: "Renamed Layer 1".to_string(),
    };

    let propose_res = execute_tool(&mut session, "ops.propose", json!({ "ops": [op1, op2] }));
    assert!(
        !propose_res.is_error,
        "Propose failed: {:?}",
        propose_res.content
    );
    let val: serde_json::Value = serde_json::from_str(&propose_res.content[0].text).unwrap();
    assert_eq!(val["valid"], true);
    assert_eq!(val["applied_count"], 2);
    let descs = val["descriptions"].as_array().unwrap();
    assert_eq!(descs.len(), 2);

    // CRITICAL: Verify active session was NOT mutated!
    let current_layer_count = session.project.comp(comp_id).unwrap().layers.len();
    assert_eq!(
        current_layer_count, initial_layer_count,
        "ops.propose must be pure dry-run and not mutate the active session"
    );
    assert!(
        !session
            .project
            .comp(comp_id)
            .unwrap()
            .layers
            .contains_key(&expected_allocated_id),
        "Proposed layer must not exist in live session"
    );
}

#[test]
fn test_project_save_and_open_roundtrip() {
    let mut session = McpSession::new();
    let comp_id = session.active_comp.unwrap();

    let layer = Layer {
        id: LayerId(303),
        name: "Persistent Layer".to_string(),
        kind: LayerKind::Solid {
            color: [0.5, 0.5, 0.5, 1.0],
        },
        start: Time::ZERO,
        duration: Time::from_secs_f64(10.0),
        parent: None,
        blend_mode: BlendMode::Normal,
        visible: true,
        locked: false,
        effects: Default::default(),
        transform: StaticTransform::default(),
        tracks: Default::default(),
    };

    let _ = session.history.commit(
        &mut session.project,
        Op::AddLayer {
            comp: comp_id,
            layer,
        },
    );

    let temp_dir = std::env::temp_dir().join("bonaparte_mcp_test");
    fs::create_dir_all(&temp_dir).unwrap();
    let file_path = temp_dir.join("test_save.bonaparte");
    let file_str = file_path.to_str().unwrap().to_string();

    // Save
    let save_res = execute_tool(&mut session, "project.save", json!({ "path": file_str }));
    assert!(!save_res.is_error);

    // Reset session
    execute_tool(&mut session, "project.create", json!({ "name": "Fresh" }));
    assert_eq!(session.project.name, "Fresh");

    // Open saved file
    let open_res = execute_tool(&mut session, "project.open", json!({ "path": file_str }));
    assert!(!open_res.is_error);

    let comp = session.project.comp(comp_id).expect("Comp restored");
    assert!(
        comp.layers.values().any(|l| l.name == "Persistent Layer"),
        "Layer must be restored after save/open"
    );

    let _ = fs::remove_file(file_path);
}

#[test]
fn test_comp_render_single_png() {
    let mut session = McpSession::new();
    let comp_id = session.active_comp.unwrap();

    let layer = Layer {
        id: LayerId(404),
        name: "Red Canvas".to_string(),
        kind: LayerKind::Solid {
            color: [1.0, 0.0, 0.0, 1.0],
        },
        start: Time::ZERO,
        duration: Time::from_secs_f64(5.0),
        parent: None,
        blend_mode: BlendMode::Normal,
        visible: true,
        locked: false,
        effects: Default::default(),
        transform: StaticTransform::default(),
        tracks: Default::default(),
    };
    let _ = session.history.commit(
        &mut session.project,
        Op::AddLayer {
            comp: comp_id,
            layer,
        },
    );

    let temp_dir = std::env::temp_dir().join("bonaparte_mcp_test");
    fs::create_dir_all(&temp_dir).unwrap();
    let out_png = temp_dir.join("single_frame.png");
    let out_str = out_png.to_str().unwrap().to_string();

    let render_res = execute_tool(
        &mut session,
        "comp.render",
        json!({
            "comp_id": comp_id.0,
            "time": 0.0,
            "format": "png",
            "output_path": out_str
        }),
    );
    assert!(
        !render_res.is_error,
        "comp.render failed: {:?}",
        render_res.content
    );

    assert!(out_png.exists(), "PNG file must exist");
    let bytes = fs::read(&out_png).unwrap();
    assert!(bytes.len() > 8);
    // Standard PNG signature
    assert_eq!(&bytes[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);

    let _ = fs::remove_file(out_png);
}

#[test]
fn test_stdio_jsonrpc_loop() {
    let input = concat!(
        "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}\n",
        "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}\n",
        "{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"tools/call\",\"params\":{\"name\":\"project.info\",\"arguments\":{}}}\n"
    );

    let reader = Cursor::new(input.as_bytes());
    let mut output = Vec::new();

    run_stdio(reader, &mut output).expect("run_stdio succeeded");

    let output_str = String::from_utf8(output).expect("Valid UTF-8");
    let lines: Vec<&str> = output_str.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines.len(), 3, "Expected 3 response lines");

    let r1: JsonRpcResponse = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(r1.id, json!(1));
    assert!(r1.error.is_none());

    let r2: JsonRpcResponse = serde_json::from_str(lines[1]).unwrap();
    assert_eq!(r2.id, json!(2));
    assert!(r2.error.is_none());

    let r3: JsonRpcResponse = serde_json::from_str(lines[2]).unwrap();
    assert_eq!(r3.id, json!(3));
    assert!(r3.error.is_none());
}
