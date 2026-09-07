//! Development-only bridge. Vite proxies /api so browser clients never use
//! localhost. Intentionally no arbitrary filesystem read/write endpoints and
//! no CORS bypass. Request logic lives in the library so it stays testable;
//! this binary is only the HTTP transport, and its workers respawn if
//! anything ever escapes the containment layer.
use bonaparte_preview::safe_route;
use bonaparte_runtime::{EditorSession, MAX_PROJECT_BYTES};
use std::{
    io::Read,
    sync::{Arc, Mutex},
};
use tiny_http::{Header, Method, Request, Response, Server};

fn reply(request: Request, status: u16, mime: &str, bytes: Vec<u8>) {
    let response = Response::from_data(bytes)
        .with_status_code(status)
        .with_header(Header::from_bytes("Content-Type", mime).expect("static header"))
        .with_header(Header::from_bytes("Cache-Control", "no-store").expect("static header"));
    let _ = request.respond(response);
}

fn handle(mut request: Request, session: &Arc<Mutex<EditorSession>>) {
    if request.method() == &Method::Get && request.url() == "/api/health" {
        reply(
            request,
            200,
            "application/json",
            b"{\"status\":\"ok\",\"renderer\":\"Rust preview runtime\",\"previewProtocol\":3,\"mcp\":\"unified\"}"
                .to_vec(),
        );
        return;
    }
    let custom_header = request
        .headers()
        .iter()
        .any(|h| h.field.equiv("X-Bonaparte-Client") && h.value.as_str() == "editor");
    if request.method() != &Method::Post || !custom_header {
        reply(
            request,
            405,
            "application/json",
            b"{\"error\":\"Use the editor's same-origin POST transport\"}".to_vec(),
        );
        return;
    }
    let name = request
        .url()
        .strip_prefix("/api/")
        .unwrap_or("")
        .to_string();
    if request.body_length().is_some_and(|n| n > MAX_PROJECT_BYTES) {
        reply(
            request,
            400,
            "application/json",
            b"{\"error\":\"Request too large\"}".to_vec(),
        );
        return;
    }
    let mut body = String::new();
    if request
        .as_reader()
        .take(MAX_PROJECT_BYTES as u64 + 1)
        .read_to_string(&mut body)
        .is_err()
        || body.len() > MAX_PROJECT_BYTES
    {
        reply(
            request,
            400,
            "application/json",
            b"{\"error\":\"Request too large\"}".to_vec(),
        );
        return;
    }
    let (status, mime, bytes) = safe_route(session, name, body);
    reply(request, status, mime, bytes);
}

fn serve_session(server: Arc<Server>, session: Arc<Mutex<EditorSession>>) {
    for request in server.incoming_requests() {
        handle(request, &session);
    }
}

fn main() {
    let port = std::env::var("BONAPARTE_API_PORT").unwrap_or_else(|_| "4317".into());
    let server = Arc::new(Server::http(format!("0.0.0.0:{port}")).expect("HTTP bind"));
    let session = Arc::new(Mutex::new(EditorSession::default()));
    eprintln!("Bonaparte Rust render service on 0.0.0.0:{port} (development only)");
    let mut workers = Vec::new();
    for _ in 0..4 {
        let server = Arc::clone(&server);
        let session = Arc::clone(&session);
        // Respawn belt: a worker that somehow dies is replaced immediately,
        // so the bridge keeps serving even after an unexpected exit.
        workers.push(std::thread::spawn(move || loop {
            let server = Arc::clone(&server);
            let session = Arc::clone(&session);
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                serve_session(server, session);
            }));
            std::thread::sleep(std::time::Duration::from_millis(50));
        }));
    }
    for worker in workers {
        worker.join().expect("HTTP worker");
    }
}
