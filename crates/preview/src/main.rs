//! Development-only bridge. Vite proxies /api so browser clients never use
//! localhost. Intentionally no arbitrary filesystem read/write endpoints and
//! no CORS bypass. Request logic lives in the library so it stays testable;
//! this binary is only the HTTP transport, and its workers respawn if
//! anything ever escapes the containment layer.
use bonaparte_preview::{safe_route_reply, Reply};
use bonaparte_runtime::{EditorSession, MAX_PROJECT_BYTES};
use std::{
    io::Read,
    sync::{Arc, Mutex},
};
use tiny_http::{Header, Method, Request, Response, Server};

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name, value).expect("static header")
}

fn reply(request: Request, reply: Reply) {
    match reply {
        Reply::Bytes { status, mime, body } => {
            let response = Response::from_data(body)
                .with_status_code(status)
                .with_header(header("Content-Type", mime))
                .with_header(header("Cache-Control", "no-store"));
            let _ = request.respond(response);
        }
        // Stream the export straight off disk; the handoff file goes as soon
        // as the response is out. No length cap, no whole-file read.
        Reply::File { mime, path } => {
            match std::fs::File::open(&path) {
                Ok(file) => {
                    let response = Response::from_file(file)
                        .with_header(header("Content-Type", mime))
                        .with_header(header("Cache-Control", "no-store"));
                    let _ = request.respond(response);
                }
                Err(error) => {
                    let _ = request.respond(
                        Response::from_data(
                            format!("{{\"error\":\"export file unreadable: {error}\"}}")
                                .into_bytes(),
                        )
                        .with_status_code(500)
                        .with_header(header("Content-Type", "application/json")),
                    );
                }
            }
            let _ = std::fs::remove_file(&path);
        }
    }
}

fn handle(mut request: Request, session: &Arc<Mutex<EditorSession>>) {
    if request.method() == &Method::Get && request.url() == "/api/health" {
        reply(
            request,
            Reply::Bytes {
                status: 200,
                mime: "application/json",
                body: b"{\"status\":\"ok\",\"renderer\":\"Rust preview runtime\",\"previewProtocol\":3,\"mcp\":\"unified\"}"
                    .to_vec(),
            },
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
            Reply::Bytes {
                status: 405,
                mime: "application/json",
                body: b"{\"error\":\"Use the editor's same-origin POST transport\"}".to_vec(),
            },
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
            Reply::Bytes {
                status: 400,
                mime: "application/json",
                body: b"{\"error\":\"Request too large\"}".to_vec(),
            },
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
            Reply::Bytes {
                status: 400,
                mime: "application/json",
                body: b"{\"error\":\"Request too large\"}".to_vec(),
            },
        );
        return;
    }
    reply(request, safe_route_reply(session, name, body));
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
