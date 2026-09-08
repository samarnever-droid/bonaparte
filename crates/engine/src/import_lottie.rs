//! Lottie (Bodymovin) import — the motion-graphics JSON interchange format.
//!
//! Converts shape layers (paths with cubic sampling, ellipses, rects) plus
//! animated/static transforms (position, scale, rotation, opacity) with
//! bezier easing into first-class Bonaparte layers. Precomps, images, text
//! and mattes are skipped with a count — the import reports exactly what it
//! left behind instead of guessing.

use bonaparte_model::{
    Easing, Keyframe, Layer, LayerKind, PropValue, StaticTransform, Time, TICKS_PER_SEC,
};
use serde_json::Value;

pub struct LottieDoc {
    pub width: f64,
    pub height: f64,
    pub fps: f64,
    pub in_frame: f64,
    pub out_frame: f64,
    pub layers: Vec<Layer>,
    pub skipped: usize,
    pub skipped_kinds: Vec<&'static str>,
}

fn f(v: &Value, key: &str) -> Option<f64> {
    v.get(key).and_then(|x| x.as_f64())
}

/// Static (a:0) or animated (a:1) property value at its base.
enum Prop {
    Static(Vec<f64>),
    Animated(Vec<(f64, Vec<f64>, Option<Bezier>)>),
    Missing,
}

struct Bezier {
    o: [f64; 2],
    i: [f64; 2],
}

fn prop(layer: &Value, path: &[&str]) -> Prop {
    let mut cur = layer;
    for key in path {
        match cur.get(key) {
            Some(next) => cur = next,
            None => return Prop::Missing,
        }
    }
    let animated = cur.get("a").and_then(|a| a.as_i64()).unwrap_or(0) == 1;
    let Some(k) = cur.get("k") else {
        return Prop::Missing;
    };
    if animated {
        let mut keys = Vec::new();
        if let Some(list) = k.as_array() {
            for item in list {
                let t = f(item, "t").unwrap_or(0.0);
                let values = match item.get("s").and_then(|s| s.as_array()) {
                    Some(vals) => vals.iter().filter_map(|v| v.as_f64()).collect::<Vec<_>>(),
                    None => continue,
                };
                let easing = Bezier {
                    o: [
                        item.pointer("/o/x").and_then(|v| v.as_f64()).unwrap_or(0.5),
                        item.pointer("/o/y").and_then(|v| v.as_f64()).unwrap_or(0.5),
                    ],
                    i: [
                        item.pointer("/i/x").and_then(|v| v.as_f64()).unwrap_or(0.5),
                        item.pointer("/i/y").and_then(|v| v.as_f64()).unwrap_or(0.5),
                    ],
                };
                keys.push((t, values, Some(easing)));
            }
        }
        if keys.is_empty() {
            Prop::Missing
        } else {
            Prop::Animated(keys)
        }
    } else {
        match k.as_array() {
            Some(vals) => Prop::Static(vals.iter().filter_map(|v| v.as_f64()).collect()),
            None => match k.as_f64() {
                Some(scalar) => Prop::Static(vec![scalar]),
                None => Prop::Missing,
            },
        }
    }
}

/// Convert a lottie transform property to Bonaparte keyframes.
/// `map` translates lottie values into Bonaparte units.
fn animated_keys(
    keys: &[(f64, Vec<f64>, Option<Bezier>)],
    fps: f64,
    dims: [f64; 2],
    map: impl Fn(&[f64]) -> PropValue,
) -> bonaparte_model::Track {
    let mut track = bonaparte_model::Track::new();
    let to_ticks = |frame: f64| Time((frame / fps.max(1.0) * TICKS_PER_SEC as f64).round() as i64);
    for (index, (t, values, easing)) in keys.iter().enumerate() {
        let easing = match easing {
            Some(b) => {
                let clamp01 = |x: f64| x.clamp(0.0, 1.0);
                Easing::Bezier {
                    p1: [clamp01(b.o[0]) as f32, b.o[1].clamp(-0.5, 1.5) as f32],
                    p2: [clamp01(b.i[0]) as f32, b.i[1].clamp(-0.5, 1.5) as f32],
                }
            }
            None if index + 1 < keys.len() => Easing::Linear,
            None => Easing::Linear,
        };
        track.set_key(Keyframe {
            time: to_ticks(*t),
            value: map(values),
            easing,
        });
    }
    // Lottie keyframes hold the segment START value; ours tween from key to
    // key, so the FINAL value comes from the last segment's end (= the last
    // key's own s, which lottie also stores) — already covered above.
    let _ = dims;
    track
}

/// Sample a lottie bezier path item into polygon points.
fn sample_path(k: &Value, steps: usize) -> Option<Vec<[f32; 2]>> {
    let vertices: Vec<[f64; 2]> = k.get("v").and_then(|v| v.as_array()).map(|list| {
        list.chunks(2)
            .filter_map(|c| match (c.first(), c.get(1)) {
                (Some(x), Some(y)) => Some([x.as_f64()?, y.as_f64()?]),
                _ => None,
            })
            .collect()
    })?;
    if vertices.is_empty() {
        return None;
    }
    let tangents = |key: &str| -> Vec<[f64; 2]> {
        k.get(key)
            .and_then(|v| v.as_array())
            .map(|list| {
                list.chunks(2)
                    .filter_map(|c| match (c.first(), c.get(1)) {
                        (Some(x), Some(y)) => Some([x.as_f64()?, y.as_f64()?]),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let outs = tangents("o");
    let ins = tangents("i");
    let closed = k.get("c").and_then(|c| c.as_bool()).unwrap_or(false);
    let count = if closed {
        vertices.len()
    } else {
        vertices.len().saturating_sub(1)
    };
    let mut points = Vec::with_capacity(count * steps + 1);
    for seg in 0..count {
        let p0 = vertices[seg];
        let p1 = vertices[(seg + 1) % vertices.len()];
        let c1 = [
            p0[0] + outs.get(seg).map(|t| t[0]).unwrap_or(0.0),
            p0[1] + outs.get(seg).map(|t| t[1]).unwrap_or(0.0),
        ];
        let c2 = [
            p1[0]
                + ins
                    .get((seg + 1) % vertices.len())
                    .map(|t| t[0])
                    .unwrap_or(0.0),
            p1[1]
                + ins
                    .get((seg + 1) % vertices.len())
                    .map(|t| t[1])
                    .unwrap_or(0.0),
        ];
        for s in 0..steps {
            let t = s as f64 / steps as f64;
            let mt = 1.0 - t;
            let x = mt * mt * mt * p0[0]
                + 3.0 * mt * mt * t * c1[0]
                + 3.0 * mt * t * t * c2[0]
                + t * t * t * p1[0];
            let y = mt * mt * mt * p0[1]
                + 3.0 * mt * mt * t * c1[1]
                + 3.0 * mt * t * t * c2[1]
                + t * t * t * p1[1];
            points.push([x as f32, y as f32]);
        }
    }
    if !closed {
        if let Some(last) = vertices.last() {
            points.push([last[0] as f32, last[1] as f32]);
        }
    }
    Some(points)
}

fn ellipse_points(size: [f64; 2], steps: usize) -> Vec<[f32; 2]> {
    let (rx, ry) = (size[0] / 2.0, size[1] / 2.0);
    (0..steps)
        .map(|i| {
            let a = i as f64 / steps as f64 * std::f64::consts::TAU;
            [(rx * a.cos()) as f32, (ry * a.sin()) as f32]
        })
        .collect()
}

fn rect_points(size: [f64; 2]) -> Vec<[f32; 2]> {
    let (w, h) = (size[0] / 2.0, size[1] / 2.0);
    vec![
        [-w as f32, -h as f32],
        [w as f32, -h as f32],
        [w as f32, h as f32],
        [-w as f32, h as f32],
    ]
}

/// Build one Bonaparte layer from a lottie layer. Returns None for skipped kinds.
fn convert_layer(l: &Value, doc: &LottieDoc) -> Result<Option<Layer>, String> {
    let kind = l.get("ty").and_then(|t| t.as_i64()).unwrap_or(-1);
    if !matches!(kind, 1 | 4) {
        return Ok(None);
    }
    let name = l
        .get("nm")
        .and_then(|n| n.as_str())
        .unwrap_or("Lottie layer")
        .to_owned();
    let ip = f(l, "ip").unwrap_or(doc.in_frame);
    let op = f(l, "op").unwrap_or(doc.out_frame);
    let start_ticks =
        ((ip - doc.in_frame) / doc.fps.max(1.0) * TICKS_PER_SEC as f64).round() as i64;
    let dur_ticks = (((op - ip).max(1.0)) / doc.fps.max(1.0) * TICKS_PER_SEC as f64).round() as i64;

    // Geometry from shape items (first fill wins, strokes compose the style).
    let mut subpaths: Vec<Vec<[f32; 2]>> = Vec::new();
    let mut color = [0.8f32, 0.8, 0.85, 1.0];
    let mut has_color = false;
    let mut stroke_width = 0.0f32;
    let mut stroke_color = [0.0f32; 4];
    let mut has_stroke = false;
    if let Some(shapes) = l.get("shapes").and_then(|s| s.as_array()) {
        for item in shapes {
            match item.get("ty").and_then(|t| t.as_str()) {
                Some("sh") => {
                    let ks = item.pointer("/ks/k");
                    if let (Some(k), true) = (ks, ks.is_some()) {
                        if let Some(points) = sample_path(k, 12) {
                            subpaths.push(points);
                        }
                    }
                }
                Some("el") => {
                    let size = item
                        .pointer("/s/k")
                        .and_then(|s| s.as_array())
                        .map(|a| a.iter().filter_map(|v| v.as_f64()).collect::<Vec<_>>());
                    if let Some(s) = size {
                        if s.len() >= 2 {
                            subpaths.push(ellipse_points([s[0], s[1]], 40));
                        }
                    }
                }
                Some("rc") => {
                    let size = item
                        .pointer("/s/k")
                        .and_then(|s| s.as_array())
                        .map(|a| a.iter().filter_map(|v| v.as_f64()).collect::<Vec<_>>());
                    if let Some(s) = size {
                        if s.len() >= 2 {
                            subpaths.push(rect_points([s[0], s[1]]));
                        }
                    }
                }
                Some("fl") => {
                    if !has_color {
                        if let Some(c) = item.pointer("/c/k").and_then(|c| c.as_array()) {
                            let vals: Vec<f64> = c.iter().filter_map(|v| v.as_f64()).collect();
                            if vals.len() >= 3 {
                                color = [
                                    vals[0] as f32,
                                    vals[1] as f32,
                                    vals[2] as f32,
                                    vals.get(3).copied().unwrap_or(1.0) as f32,
                                ];
                                has_color = true;
                            }
                        }
                    }
                }
                Some("st") => {
                    if let Some(w) = item.pointer("/w/k").and_then(|w| w.as_f64()) {
                        stroke_width = w as f32;
                        if let Some(c) = item.pointer("/c/k").and_then(|c| c.as_array()) {
                            let vals: Vec<f64> = c.iter().filter_map(|v| v.as_f64()).collect();
                            if vals.len() >= 3 {
                                stroke_color = [
                                    vals[0] as f32,
                                    vals[1] as f32,
                                    vals[2] as f32,
                                    vals.get(3).copied().unwrap_or(1.0) as f32,
                                ];
                                has_stroke = true;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    if subpaths.is_empty() && kind == 4 {
        return Ok(None);
    }
    let shape = if kind == 1 {
        // Solid: full-frame rectangle of the solid's color.
        let sc = l
            .get("sc")
            .and_then(|c| c.as_str())
            .unwrap_or("#cccccc")
            .trim_start_matches('#');
        let hex = |i: usize| {
            u8::from_str_radix(sc.get(i * 2..i * 2 + 2).unwrap_or("cc"), 16).unwrap_or(200) as f32
                / 255.0
        };
        LayerKind::Solid {
            color: [hex(0), hex(1), hex(2), 1.0],
        }
    } else {
        LayerKind::Shape {
            color,
            generator: None,
            style: bonaparte_model::ShapeStyle {
                size: None,
                corner_radius: 0.0,
                stroke_width: if has_stroke { stroke_width } else { 0.0 },
                stroke_color: if has_stroke { stroke_color } else { [1.0; 4] },
            },
            points: subpaths,
        }
    };
    let mut layer = Layer::new(name, shape, Time(start_ticks), Time(dur_ticks));

    // Transform: static + animated tracks. Lottie space is top-left-origin
    // comp pixels; Bonaparte position is comp-center-relative.
    let center = [doc.width / 2.0, doc.height / 2.0];
    let mut transform = StaticTransform {
        position: [0.0, 0.0],
        scale: [100.0, 100.0],
        rotation: 0.0,
        opacity: 1.0,
        ..StaticTransform::default()
    };
    match prop(l, &["ks", "p"]) {
        Prop::Static(v) if v.len() >= 2 => {
            transform.position = [(v[0] - center[0]) as f32, (v[1] - center[1]) as f32];
        }
        Prop::Animated(keys) => {
            if let Some((_, first, _)) = keys.first() {
                if first.len() >= 2 {
                    transform.position =
                        [(first[0] - center[0]) as f32, (first[1] - center[1]) as f32];
                }
            }
            layer.tracks.insert(
                bonaparte_model::Property::Position,
                animated_keys(&keys, doc.fps, [doc.width, doc.height], |v| {
                    PropValue::Vec2([(v[0] - center[0]) as f32, (v[1] - center[1]) as f32])
                }),
            );
        }
        _ => {}
    }
    match prop(l, &["ks", "s"]) {
        Prop::Static(v) if v.len() >= 2 => transform.scale = [v[0] as f32, v[1] as f32],
        Prop::Animated(keys) => {
            if let Some((_, first, _)) = keys.first() {
                if first.len() >= 2 {
                    transform.scale = [first[0] as f32, first[1] as f32];
                }
            }
            layer.tracks.insert(
                bonaparte_model::Property::Scale,
                animated_keys(&keys, doc.fps, [0.0; 2], |v| {
                    PropValue::Vec2([
                        v[0] as f32,
                        v[1].min(v.get(1).copied().unwrap_or(100.0)) as f32,
                    ])
                }),
            );
        }
        _ => {}
    }
    match prop(l, &["ks", "r"]) {
        Prop::Static(v) => transform.rotation = v.first().copied().unwrap_or(0.0) as f32,
        Prop::Animated(keys) => {
            if let Some((_, first, _)) = keys.first() {
                transform.rotation = first.first().copied().unwrap_or(0.0) as f32;
            }
            layer.tracks.insert(
                bonaparte_model::Property::Rotation,
                animated_keys(&keys, doc.fps, [0.0; 2], |v| {
                    PropValue::Scalar(v.first().copied().unwrap_or(0.0) as f32)
                }),
            );
        }
        _ => {}
    }
    match prop(l, &["ks", "o"]) {
        Prop::Static(v) => transform.opacity = (v.first().copied().unwrap_or(100.0) / 100.0) as f32,
        Prop::Animated(keys) => {
            if let Some((_, first, _)) = keys.first() {
                transform.opacity = (first.first().copied().unwrap_or(100.0) / 100.0) as f32;
            }
            layer.tracks.insert(
                bonaparte_model::Property::Opacity,
                animated_keys(&keys, doc.fps, [0.0; 2], |v| {
                    PropValue::Scalar((v.first().copied().unwrap_or(100.0) / 100.0) as f32)
                }),
            );
        }
        _ => {}
    }
    layer.transform = transform;
    Ok(Some(layer))
}

/// Parse a Bodymovin JSON string into a converted document.
pub fn lottie_doc(json: &str) -> Result<LottieDoc, String> {
    if json.len() > 32 * 1024 * 1024 {
        return Err("Lottie files must be smaller than 32 MB.".into());
    }
    let root: Value =
        serde_json::from_str(json).map_err(|e| format!("Invalid Lottie JSON: {e}"))?;
    let is_lottie =
        root.get("layers").is_some() && root.get("fr").is_some() && root.get("op").is_some();
    if !is_lottie {
        return Err("This JSON is not a Lottie (Bodymovin) composition".into());
    }
    let width = f(&root, "w").ok_or("Lottie is missing its width")?;
    let height = f(&root, "h").ok_or("Lottie is missing its height")?;
    let fps = f(&root, "fr").unwrap_or(30.0).max(1.0);
    let in_frame = f(&root, "ip").unwrap_or(0.0);
    let out_frame = f(&root, "op").unwrap_or(in_frame + fps);
    if !(width.is_finite() && height.is_finite()) || width < 1.0 || height < 1.0 {
        return Err("Lottie dimensions are invalid".into());
    }
    if out_frame <= in_frame {
        return Err("Lottie has no duration".into());
    }
    let mut doc = LottieDoc {
        width,
        height,
        fps,
        in_frame,
        out_frame,
        layers: Vec::new(),
        skipped: 0,
        skipped_kinds: Vec::new(),
    };
    if let Some(layers) = root.get("layers").and_then(|l| l.as_array()) {
        for l in layers {
            match convert_layer(l, &doc) {
                Ok(Some(layer)) => doc.layers.push(layer),
                Ok(None) => {
                    doc.skipped += 1;
                    let kind = l.get("ty").and_then(|t| t.as_i64()).unwrap_or(-1);
                    let name = match kind {
                        0 => "precomp",
                        2 => "image",
                        3 => "null",
                        5 => "text",
                        _ => "unsupported",
                    };
                    if !doc.skipped_kinds.contains(&name) {
                        doc.skipped_kinds.push(name);
                    }
                }
                Err(e) => return Err(e),
            }
        }
    }
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOTTIE: &str = r#"{
      "v": "5.7.4", "fr": 30, "ip": 0, "op": 60, "w": 400, "h": 300, "nm": "Bounce",
      "layers": [
        {
          "ty": 4, "nm": "Ball", "ind": 1, "ip": 0, "op": 60, "st": 0,
          "ks": {
            "o": { "a": 0, "k": 80 },
            "r": { "a": 0, "k": 0 },
            "p": { "a": 1, "k": [
              { "t": 0, "s": [100, 60, 0], "o": {"x": 0.4, "y": 0}, "i": {"x": 0.6, "y": 1} },
              { "t": 30, "s": [300, 240, 0], "o": {"x": 0.4, "y": 0}, "i": {"x": 0.6, "y": 1} },
              { "t": 60, "s": [100, 60, 0] }
            ]},
            "a": { "a": 0, "k": [0, 0, 0] },
            "s": { "a": 0, "k": [120, 120, 100] }
          },
          "shapes": [
            { "ty": "el", "nm": "ellipse", "p": { "a": 0, "k": [0, 0] }, "s": { "a": 0, "k": [80, 80] } },
            { "ty": "fl", "nm": "fill", "c": { "a": 0, "k": [1, 0.3, 0.2, 1] }, "o": { "a": 0, "k": 100 } }
          ]
        },
        {
          "ty": 5, "nm": "SkippedText", "ind": 2, "ip": 0, "op": 60, "st": 0, "ks": {}
        }
      ]
    }"#;

    #[test]
    fn parses_shapes_transforms_and_keyframes() {
        let doc = lottie_doc(LOTTIE).unwrap();
        assert_eq!(doc.width, 400.0);
        assert_eq!(doc.fps, 30.0);
        assert_eq!(doc.layers.len(), 1, "text layer skipped, shape converted");
        assert_eq!(doc.skipped, 1);
        assert!(doc.skipped_kinds.contains(&"text"));

        let ball = &doc.layers[0];
        let (w, h) = (ball.start.0, ball.duration.0);
        assert_eq!(w, 0);
        assert_eq!(h, 2 * TICKS_PER_SEC, "60 frames at 30 fps = 2 s");
        assert_eq!(ball.transform.opacity, 0.8);
        assert_eq!(ball.transform.scale, [120.0, 120.0]);
        // Keyframes: 3 position keys (0, 1 s, 2 s), center-relative.
        let track = ball
            .tracks
            .get(&bonaparte_model::Property::Position)
            .unwrap();
        assert_eq!(track.keys.len(), 3);
        assert_eq!(track.keys[0].value, PropValue::Vec2([-100.0, -90.0]));
        assert_eq!(track.keys[1].value, PropValue::Vec2([100.0, 90.0]));
        assert_eq!(track.keys[1].time.0, TICKS_PER_SEC);
        assert!(matches!(track.keys[0].easing, Easing::Bezier { .. }));
    }

    #[test]
    fn ellipse_becomes_a_closed_polygon() {
        let doc = lottie_doc(LOTTIE).unwrap();
        let LayerKind::Shape { points, color, .. } = &doc.layers[0].kind else {
            panic!("shape layer");
        };
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].len(), 40);
        // Extremes reach ±40 px (the 80 px ellipse) around the path origin.
        let max_x = points[0].iter().fold(0.0f32, |m, p| m.max(p[0]));
        assert!((max_x - 40.0).abs() < 0.5, "max_x {max_x}");
        assert!((color[0] - 1.0).abs() < 0.01 && (color[1] - 0.3).abs() < 0.01);
    }

    #[test]
    fn rejects_non_lottie_json() {
        assert!(lottie_doc("{\"hello\": 1}").is_err());
        assert!(lottie_doc("not json").is_err());
    }
}
