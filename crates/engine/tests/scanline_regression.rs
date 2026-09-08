use bonaparte_engine::vector::rasterize_paths;
use bonaparte_model::ShapeStyle;
use std::f32::consts::PI;

#[test]
fn probe_row_profiles() {
    let n = 56usize;
    let pts: Vec<Vec<[f32; 2]>> = vec![(0..n)
        .map(|i| {
            let a = 2.0 * PI * i as f32 / n as f32;
            let x = (110.0 * a.cos() * 100.0).round() / 100.0;
            let y = (110.0 * a.sin() * 100.0).round() / 100.0;
            [x + 110.0, y + 110.0]
        })
        .collect()];
    let frame = rasterize_paths(
        220,
        220,
        [1.0, 1.0, 1.0, 1.0],
        &ShapeStyle {
            size: Some([220.0, 220.0]),
            corner_radius: 0.0,
            stroke_width: 0.0,
            stroke_color: [0.0; 4],
        },
        &pts,
    );
    for y in [50usize, 51, 52] {
        let mut runs: Vec<(char, usize)> = vec![];
        for x in 0..220usize {
            let a = frame.rgba[(y * 220 + x) * 4 + 3];
            let state = if a == 0 {
                '.'
            } else if a == 255 {
                '#'
            } else {
                '~'
            };
            if let Some(last) = runs.last_mut() {
                if last.0 == state {
                    last.1 += 1;
                    continue;
                }
            }
            runs.push((state, 1));
        }
        println!("row {y}: {runs:?}");
    }
}

#[test]
fn probe_moon_source_alpha_clean() {
    let n = 56usize;
    let pts: Vec<Vec<[f32; 2]>> = vec![(0..n)
        .map(|i| {
            let a = 2.0 * PI * i as f32 / n as f32;
            let x = (110.0 * a.cos() * 100.0).round() / 100.0;
            let y = (110.0 * a.sin() * 100.0).round() / 100.0;
            [x + 110.0, y + 110.0]
        })
        .collect()];
    let frame = rasterize_paths(
        220,
        220,
        [1.0, 1.0, 1.0, 1.0],
        &ShapeStyle {
            size: Some([220.0, 220.0]),
            corner_radius: 0.0,
            stroke_width: 0.0,
            stroke_color: [0.0; 4],
        },
        &pts,
    );
    let mut avgs = vec![];
    for y in 0..220usize {
        let mut sum = 0u64;
        for x in 0..220usize {
            sum += frame.rgba[(y * 220 + x) * 4 + 3] as u64;
        }
        avgs.push(sum / 220);
    }
    // The artifact signature: a row suddenly dropping far below BOTH
    // neighbors (old scanline lost an edge at polygon vertices).
    for y in 8..212usize {
        let (a, b, c) = (avgs[y - 1], avgs[y], avgs[y + 1]);
        assert!(
            b + 25 >= a.min(c),
            "row {y} dips to {} between {a} and {c} — scanline artifact",
            b
        );
    }
}
