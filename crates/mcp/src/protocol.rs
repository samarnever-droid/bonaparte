//! JSON-RPC 2.0 and MCP Protocol types.

use serde::{Deserialize, Serialize};

/// Standard JSON-RPC 2.0 Request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    #[serde(default = "default_jsonrpc")]
    pub jsonrpc: String,
    #[serde(default)]
    pub id: Option<serde_json::Value>,
    pub method: String,
    #[serde(default)]
    pub params: Option<serde_json::Value>,
}

fn default_jsonrpc() -> String {
    "2.0".to_string()
}

/// Standard JSON-RPC 2.0 Response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    pub fn success(id: serde_json::Value, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: serde_json::Value, error: JsonRpcError) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(error),
        }
    }
}

/// Standard JSON-RPC 2.0 Error.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl JsonRpcError {
    pub const PARSE_ERROR: i32 = -32700;
    pub const INVALID_REQUEST: i32 = -32600;
    pub const METHOD_NOT_FOUND: i32 = -32601;
    pub const INVALID_PARAMS: i32 = -32602;
    pub const INTERNAL_ERROR: i32 = -32603;

    pub fn parse_error(msg: impl Into<String>) -> Self {
        Self {
            code: Self::PARSE_ERROR,
            message: msg.into(),
            data: None,
        }
    }

    pub fn invalid_request(msg: impl Into<String>) -> Self {
        Self {
            code: Self::INVALID_REQUEST,
            message: msg.into(),
            data: None,
        }
    }

    pub fn method_not_found(method: &str) -> Self {
        Self {
            code: Self::METHOD_NOT_FOUND,
            message: format!("Method '{method}' not found"),
            data: None,
        }
    }

    pub fn invalid_params(msg: impl Into<String>) -> Self {
        Self {
            code: Self::INVALID_PARAMS,
            message: msg.into(),
            data: None,
        }
    }

    pub fn internal_error(msg: impl Into<String>) -> Self {
        Self {
            code: Self::INTERNAL_ERROR,
            message: msg.into(),
            data: None,
        }
    }
}

/// MCP Tool schema definition returned by `tools/list`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

/// Content block in MCP tool call responses. Text is the classic block;
/// `image` and `audio` blocks (mimeType + base64 data) feed vision- and
/// audio-native models natively — the model SEES the frame, it does not
/// read a description of it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextContent {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(rename = "mimeType", default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
}

impl TextContent {
    pub fn text(value: impl Into<String>) -> Self {
        Self {
            kind: "text".into(),
            text: Some(value.into()),
            mime_type: None,
            data: None,
        }
    }
    pub fn image(mime_type: &str, base64_data: impl Into<String>) -> Self {
        Self {
            kind: "image".into(),
            text: None,
            mime_type: Some(mime_type.into()),
            data: Some(base64_data.into()),
        }
    }
    pub fn audio(mime_type: &str, base64_data: impl Into<String>) -> Self {
        Self {
            kind: "audio".into(),
            text: None,
            mime_type: Some(mime_type.into()),
            data: Some(base64_data.into()),
        }
    }
}

/// MCP Tool call response object conforming to MCP spec.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResult {
    pub content: Vec<TextContent>,
    #[serde(rename = "isError", default)]
    pub is_error: bool,
}

impl ToolCallResult {
    pub fn success(text: impl Into<String>) -> Self {
        Self {
            content: vec![TextContent::text(text)],
            is_error: false,
        }
    }

    /// Multi-block result: leading text plus inline image/audio blocks.
    pub fn rich(blocks: Vec<TextContent>) -> Self {
        Self {
            content: blocks,
            is_error: false,
        }
    }

    pub fn json<T: Serialize>(value: &T) -> Result<Self, serde_json::Error> {
        let text = serde_json::to_string_pretty(value)?;
        Ok(Self::success(text))
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self {
            content: vec![TextContent::text(text)],
            is_error: true,
        }
    }
}
