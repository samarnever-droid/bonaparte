# Progress Heartbeat

Last visited: 2026-09-04T09:03:00Z
Status: Complete
Current Task: Tasks completed, writing handoff report
- [x] Task 1: Headless MCP Server (crates/mcp/)
  - [x] JSON-RPC 2.0 stdio transport (protocol.rs, server.rs, main.rs, lib.rs)
  - [x] MCP tools: project.create, project.open, project.save, project.info, op.apply, ops.propose, history.list, history.undo, history.redo, comp.render
  - [x] Pure Rust PNG encoder (png.rs using miniz_oxide + crc32fast)
  - [x] MP4 streaming export via FFmpeg child process
  - [x] Integration tests in crates/mcp/tests/ (8 tests passing 100%)
  - [x] cargo test -p bonaparte-mcp passes
  - [x] cargo clippy -p bonaparte-mcp passes with zero warnings
- [x] Task 2: Tauri IPC Bridge (ui/src-tauri/)
  - [x] Fixed render_frame to pass Time and MediaFrames (&NoMedia) into render_comp
  - [x] Added render_frame_raw returning binary tauri::ipc::Response
  - [x] Added export_video streaming MP4 export IPC command with O(1) memory
  - [x] Fixed demo_project initialization with FrameRate::FPS_30 and Time
  - [x] cargo check -p bonaparte-app compiles cleanly
