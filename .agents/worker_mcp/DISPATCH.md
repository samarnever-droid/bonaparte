## 2026-09-04T08:43:51Z

You are the AI & MCP Specialist for Bonaparte.
Your working directory is: C:\Users\khati\.zcode\workspace\default\bonaparte\.agents\worker_mcp
You MUST read:
- C:\Users\khati\.zcode\workspace\default\bonaparte\ORIGINAL_REQUEST.md
- C:\Users\khati\.zcode\workspace\default\bonaparte\PROJECT.md

MANDATORY INTEGRITY WARNING:
DO NOT CHEAT. All implementations must be genuine.

Your write scope: `crates/mcp/` and `ui/src-tauri/`.

Tasks:
1. Headless MCP Server (`crates/mcp/`):
   - Build stdio JSON-RPC 2.0 MCP server for Bonaparte in `crates/mcp/src/lib.rs` and `crates/mcp/src/main.rs`.
   - Implement MCP tools:
     - `project.create`, `project.open`, `project.save`, `project.info`
     - `op.apply`: executes Op via History::commit with validation
     - `ops.propose`: AI dry-run diff preview returning human-readable descriptions without mutating active session
     - `history.list`, `history.undo`, `history.redo`
     - `comp.render`: headless batch rendering to PNG frames or MP4 video
   - Integration tests in `crates/mcp/tests/`.
   - Verify `cargo test -p bonaparte-mcp`.
2. Tauri IPC Bridge (`ui/src-tauri/src/main.rs`):
   - Fix `render_frame` command to pass `Time` and `MediaFrames` into `bonaparte_engine::reference::render_comp`.
   - Optimize frame transfer using binary response (`tauri::ipc::Response`) or ArrayBuffer to eliminate scrubbing latency.
   - Add `export_video` IPC command bridging UI to `bonaparte-media::export`.

Write handoff to `C:\Users\khati\.zcode\workspace\default\bonaparte\.agents\worker_mcp\handoff.md`.
Send message when done.
