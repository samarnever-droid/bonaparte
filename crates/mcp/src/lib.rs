//! # bonaparte-mcp
//!
//! The headless twin: an MCP server exposing the SAME `Op` vocabulary the
//! UI uses (ARCHITECTURE.md — one model, three consumers).
//!
//! Exposes:
//! - stdio JSON-RPC 2.0 transport
//! - MCP tools:
//!   - `project.create`, `project.open`, `project.save`, `project.info`
//!   - `op.apply`: executes Op via History::commit with validation
//!   - `ops.propose`: AI dry-run diff preview returning descriptions without mutating session
//!   - `history.list`, `history.undo`, `history.redo`
//!   - `comp.render`: headless batch rendering to PNG frames or MP4 video

pub mod png;
pub mod protocol;
pub mod server;
pub mod session;
pub mod tools;

pub use png::encode_png;
pub use protocol::{JsonRpcError, JsonRpcRequest, JsonRpcResponse, ToolCallResult, ToolDefinition};
pub use server::{handle_request, run_stdio};
pub use session::McpSession;
pub use tools::{execute_tool, list_tool_definitions};
