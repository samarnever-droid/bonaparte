//! Render a project's native processed audio mix without opening the UI.
//! Usage: cargo run -p bonaparte-runtime --example render_audio -- project.bonaparte mix.wav
use bonaparte_model::CompId;
use bonaparte_runtime::{parse_project, EditorSession};
fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() < 3 {
        return Err("Usage: render_audio project.bonaparte output.wav [comp-id]".into());
    }
    let project = parse_project(&std::fs::read_to_string(&args[1]).map_err(|e| e.to_string())?)?;
    let comp = CompId(args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1));
    let session = EditorSession::new(project)?;
    let start = std::time::Instant::now();
    session
        .audio_input(comp)?
        .wav(std::path::Path::new(&args[2]))?;
    eprintln!(
        "Native audio exported in {:.3}s",
        start.elapsed().as_secs_f64()
    );
    Ok(())
}
