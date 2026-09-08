//! JSON-RPC 2.0 stdio server implementation for Bonaparte MCP.

use std::io::{BufRead, Write};

use serde_json::json;

use crate::protocol::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};
use crate::session::McpSession;
use crate::tools::{execute_tool, list_tool_definitions};

/// Like [`handle_request`], but a panic inside a tool becomes an internal-
/// error response instead of killing the server loop. The process survives;
/// the session survives (document mutations are atomic per-op).
pub fn handle_request_safe(
    session: &mut McpSession,
    req: JsonRpcRequest,
) -> Option<JsonRpcResponse> {
    let id = req.id.clone();
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        handle_request(session, req)
    })) {
        Ok(response) => response,
        Err(panic) => {
            let message = panic
                .downcast_ref::<&str>()
                .map(|s| (*s).to_string())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "tool panicked".to_string());
            id.map(|id| {
                JsonRpcResponse::error(
                    id,
                    JsonRpcError::internal_error(&format!(
                        "tool crashed and was contained: {message}"
                    )),
                )
            })
        }
    }
}

/// Handles a single JSON-RPC request against an active McpSession.
/// Returns `None` for notifications (requests without `id`), or `Some(response)`.
pub fn handle_request(session: &mut McpSession, req: JsonRpcRequest) -> Option<JsonRpcResponse> {
    let id = req.id?;

    let method = req.method.as_str();
    let params = req.params.unwrap_or(serde_json::Value::Null);

    let response = match method {
        // Standard MCP Protocol Lifecycle
        "initialize" => {
            let result = json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {
                        "listChanged": false
                    }
                },
                "serverInfo": {
                    "name": "bonaparte-mcp",
                    "version": env!("CARGO_PKG_VERSION")
                }
            });
            JsonRpcResponse::success(id, result)
        }
        "ping" => JsonRpcResponse::success(id, json!({})),
        "notifications/initialized" => return None,

        // MCP Tools Listing
        "tools/list" => {
            let tools = list_tool_definitions();
            JsonRpcResponse::success(id, json!({ "tools": tools }))
        }

        // MCP Tool Execution via tools/call
        "tools/call" => {
            let tool_name = match params.get("name").and_then(|v| v.as_str()) {
                Some(n) => n,
                None => {
                    return Some(JsonRpcResponse::error(
                        id,
                        JsonRpcError::invalid_params("tools/call requires 'name' property"),
                    ))
                }
            };
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or(serde_json::Value::Object(Default::default()));

            let call_result = execute_tool(session, tool_name, args);
            JsonRpcResponse::success(id, serde_json::to_value(call_result).unwrap())
        }

        // Direct JSON-RPC Tool Invocation (e.g. method: "project.create", "op.apply", etc.)
        direct if direct.contains('.') => {
            let call_result = execute_tool(session, direct, params);
            if call_result.is_error {
                let err_msg = call_result
                    .content
                    .first()
                    .map(|c| c.text.as_deref().unwrap_or(""))
                    .unwrap_or("Tool execution failed");
                JsonRpcResponse::error(id, JsonRpcError::internal_error(err_msg))
            } else {
                let text = call_result
                    .content
                    .first()
                    .map(|c| c.text.as_deref().unwrap_or(""))
                    .unwrap_or("");
                let result_value = serde_json::from_str::<serde_json::Value>(text)
                    .unwrap_or_else(|_| json!({ "result": text }));
                JsonRpcResponse::success(id, result_value)
            }
        }

        unknown => JsonRpcResponse::error(id, JsonRpcError::method_not_found(unknown)),
    };

    Some(response)
}

/// Runs the MCP server loop over buffered reader/writer streams (e.g. stdio).
pub fn run_stdio<R: BufRead, W: Write>(mut reader: R, mut writer: W) -> Result<(), std::io::Error> {
    let mut session = McpSession::new();
    let mut line = String::new();

    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line)?;
        if bytes_read == 0 {
            break; // EOF
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let req: JsonRpcRequest = match serde_json::from_str(trimmed) {
            Ok(r) => r,
            Err(e) => {
                let err_resp = JsonRpcResponse::error(
                    serde_json::Value::Null,
                    JsonRpcError::parse_error(format!("Malformed JSON-RPC request: {e}")),
                );
                let json_str = serde_json::to_string(&err_resp)?;
                writeln!(writer, "{json_str}")?;
                writer.flush()?;
                continue;
            }
        };

        if let Some(resp) = handle_request_safe(&mut session, req) {
            let json_str = serde_json::to_string(&resp)?;
            writeln!(writer, "{json_str}")?;
            writer.flush()?;
        }
    }

    Ok(())
}
