//! Run with: cargo run -p bonaparte-runtime --example preview_benchmark
//! Uses an isolated in-process example, never the user's live editing session.
use bonaparte_runtime::{EditorSession, PreviewRequest, RenderRequest};
use serde_json::{json, Value};
use std::time::Instant;
fn median(values: &[f64]) -> f64 {
    let mut v = values.to_vec();
    v.sort_by(|a, b| a.total_cmp(b));
    (v[3] + v[4]) / 2.0
}
fn main() {
    let session = EditorSession::default();
    let request =
        |time| serde_json::from_value::<RenderRequest>(json!({"compId":1,"time":time})).unwrap();
    session
        .render_input(request(144000))
        .unwrap()
        .raw()
        .unwrap();
    let mut legacy = vec![];
    for i in 1..=8 {
        let start = Instant::now();
        let _ = session
            .render_input(request(144000 + i * 4000))
            .unwrap()
            .raw()
            .unwrap();
        legacy.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    let mut rows = vec![
        json!({"path":"legacy CPU raw","divisor":1,"samplesMs":legacy,"medianMs":median(&legacy)}),
    ];
    let modes = if std::env::args().any(|s| s == "--include-gpu") {
        vec!["cpu", "gpu"]
    } else {
        vec!["cpu"]
    };
    for backend in modes {
        for divisor in [1, 2, 4] {
            session.preview.invalidate();
            let req = |time| {
                serde_json::from_value::<PreviewRequest>(
                    json!({"compId":1,"time":time,"divisor":divisor,"backend":backend}),
                )
                .unwrap()
            };
            let warm = session
                .preview_input(req(144000))
                .unwrap()
                .render()
                .unwrap();
            let mut samples = vec![];
            for i in 1..=8 {
                let start = Instant::now();
                let _ = session
                    .preview_input(req(144000 + i * 4000))
                    .unwrap()
                    .packet()
                    .unwrap();
                samples.push(start.elapsed().as_secs_f64() * 1000.0);
            }
            let mut cached = vec![];
            for _ in 0..8 {
                let start = Instant::now();
                let _ = session
                    .preview_input(req(176000))
                    .unwrap()
                    .packet()
                    .unwrap();
                cached.push(start.elapsed().as_secs_f64() * 1000.0);
            }
            rows.push(json!({"path":"prepared preview packet","requestedBackend":backend,"actualBackend":warm.metadata.backend,"adapter":warm.metadata.adapter,"divisor":divisor,"samplesMs":samples,"medianMs":median(&samples),"cacheHitSamplesMs":cached,"cacheHitMedianMs":median(&cached),"width":warm.metadata.width,"height":warm.metadata.height}));
        }
    }
    let output: Value = json!({"project":"examples/orbit.bonaparte.json","method":"8 successive 4000-tick frames after warm-up; isolated in-process dev build; includes snapshot/packet work, not network or browser painting. Cache samples repeat the last frame. Software GPU results do not establish hardware performance.","results":rows,"cache":session.preview.status()});
    println!("{}", serde_json::to_string_pretty(&output).unwrap());
}
