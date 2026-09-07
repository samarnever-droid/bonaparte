//! SVG import (core profile): parses the common vector vocabulary into
//! Bonaparte vector layers — rects, circles, ellipses, lines, polygons,
//! polylines and full cubic/quadratic/arc paths, with group transforms and
//! fills/strokes. Curves are flattened to polygons; text elements are
//! mapped to native Text layers.

use bonaparte_model::{Layer, LayerKind, ShapeStyle, Time};

/// A parsed drawing element before it becomes a layer.
struct DrawShape {
    subpaths: Vec<Vec<[f32; 2]>>,
    fill: Option<[f32; 4]>,
    stroke: Option<[f32; 4]>,
    stroke_width: f32,
}

fn parse_hex_color(hex: &str) -> Option<[f32; 4]> {
    let h = hex.trim();
    let hex_part = h.strip_prefix('#')?;
    let (r, g, b, a) = match hex_part.len() {
        3 | 4 => {
            let vals: Vec<u8> = hex_part
                .chars()
                .filter_map(|c| c.to_digit(16).map(|d| d as u8))
                .collect();
            if vals.len() < 3 {
                return None;
            }
            (
                vals[0] * 17,
                vals[1] * 17,
                vals[2] * 17,
                vals.get(3).map(|v| v * 17).unwrap_or(255),
            )
        }
        6 | 8 => {
            let vals: Vec<u8> = (0..hex_part.len())
                .step_by(2)
                .filter_map(|i| u8::from_str_radix(&hex_part[i..i + 2], 16).ok())
                .collect();
            if vals.len() < 3 {
                return None;
            }
            (vals[0], vals[1], vals[2], *vals.get(3).unwrap_or(&255))
        }
        _ => return None,
    };
    Some([
        (r as f32 / 255.0).powf(2.2),
        (g as f32 / 255.0).powf(2.2),
        (b as f32 / 255.0).powf(2.2),
        a as f32 / 255.0,
    ])
}

fn css_color(name: &str) -> Option<[f32; 4]> {
    match name.trim() {
        "black" => Some([0.0, 0.0, 0.0, 1.0]),
        "white" => Some([1.0; 4]),
        "red" => Some([1.0, 0.0, 0.0, 1.0]),
        "green" => Some([0.0, 0.5, 0.0, 1.0]),
        "blue" => Some([0.0, 0.0, 1.0, 1.0]),
        "yellow" => Some([1.0, 1.0, 0.0, 1.0]),
        "orange" => Some([1.0, 0.5, 0.0, 1.0]),
        "purple" => Some([0.5, 0.0, 0.5, 1.0]),
        "gray" | "grey" => Some([0.5, 0.5, 0.5, 1.0]),
        _ => None,
    }
}

fn parse_color(value: &str) -> Option<[f32; 4]> {
    if value.trim() == "none" {
        return None;
    }
    parse_hex_color(value).or_else(|| css_color(value))
}

fn nums(list: &str) -> Vec<f32> {
    let mut out = Vec::new();
    let mut current = String::new();
    for ch in list.chars() {
        if ch.is_ascii_digit() || ch == '.' || ch == '-' || ch == '+' || ch == 'e' || ch == 'E' {
            if (ch == '-' || ch == '+') && !current.ends_with('e') && !current.ends_with('E') {
                if !current.is_empty() {
                    if let Ok(v) = current.parse() {
                        out.push(v);
                    }
                    current.clear();
                }
            }
            current.push(ch);
        } else if !current.is_empty() {
            if let Ok(v) = current.parse() {
                out.push(v);
            }
            current.clear();
        }
    }
    if let Ok(v) = current.parse::<f32>() {
        out.push(v);
    }
    out
}

/// Flattened path data: M/m L/l H/h V/v C/c S/s Q/q T/t A/a Z/z.
fn parse_path(data: &str, transform: &Transform) -> Vec<Vec<[f32; 2]>> {
    let mut subpaths: Vec<Vec<[f32; 2]>> = Vec::new();
    let mut current: Vec<[f32; 2]> = Vec::new();
    let mut start = [0.0f32; 2];
    let mut pos = [0.0f32; 2];
    let mut last_cubic = [0.0f32; 2];
    let mut last_quad = [0.0f32; 2];
    let mut last_cmd = ' ';
    let mut chars = data.chars().peekable();
    let next_num = |chars: &mut std::iter::Peekable<std::str::Chars>, buf: &mut String| -> f32 {
        buf.clear();
        // skip separators between tokens
        while let Some(&c) = chars.peek() {
            if c.is_whitespace() || c == ',' {
                chars.next();
            } else {
                break;
            }
        }
        // take one number token
        let mut started = false;
        while let Some(&c) = chars.peek() {
            if c.is_ascii_digit()
                || c == '.'
                || ((c == '-' || c == '+') && !started)
                || ((c == '-' || c == '+') && buf.ends_with('e'))
                || c == 'e'
                || c == 'E'
            {
                started = true;
                buf.push(c);
                chars.next();
            } else {
                break;
            }
        }
        buf.parse().unwrap_or(0.0)
    };
    let mut token = String::new();
    while let Some(&c) = chars.peek() {
        if c.is_alphabetic() {
            let cmd = c;
            chars.next();
            last_cmd = cmd;
            let relative = cmd.is_lowercase();
            match cmd.to_ascii_uppercase() {
                'M' => {
                    if !current.is_empty() {
                        subpaths.push(std::mem::take(&mut current));
                    }
                    let x = next_num(&mut chars, &mut token);
                    let y = next_num(&mut chars, &mut token);
                    pos = if relative {
                        [pos[0] + x, pos[1] + y]
                    } else {
                        [x, y]
                    };
                    start = pos;
                    current.push(transform.apply(pos));
                    // implicit lineto continuation
                    while chars
                        .peek()
                        .is_some_and(|c| c.is_ascii_digit() || *c == '-' || *c == '.' || *c == '+')
                    {
                        let x = next_num(&mut chars, &mut token);
                        let y = next_num(&mut chars, &mut token);
                        pos = if relative {
                            [pos[0] + x, pos[1] + y]
                        } else {
                            [x, y]
                        };
                        current.push(transform.apply(pos));
                    }
                }
                'L' => loop {
                    let x = next_num(&mut chars, &mut token);
                    let y = next_num(&mut chars, &mut token);
                    pos = if relative {
                        [pos[0] + x, pos[1] + y]
                    } else {
                        [x, y]
                    };
                    current.push(transform.apply(pos));
                    if !chars
                        .peek()
                        .is_some_and(|c| c.is_ascii_digit() || *c == '-' || *c == '.' || *c == '+')
                    {
                        break;
                    }
                },
                'H' => loop {
                    let x = next_num(&mut chars, &mut token);
                    pos[0] = if relative { pos[0] + x } else { x };
                    current.push(transform.apply(pos));
                    if !chars
                        .peek()
                        .is_some_and(|c| c.is_ascii_digit() || *c == '-' || *c == '.' || *c == '+')
                    {
                        break;
                    }
                },
                'V' => loop {
                    let y = next_num(&mut chars, &mut token);
                    pos[1] = if relative { pos[1] + y } else { y };
                    current.push(transform.apply(pos));
                    if !chars
                        .peek()
                        .is_some_and(|c| c.is_ascii_digit() || *c == '-' || *c == '.' || *c == '+')
                    {
                        break;
                    }
                },
                'C' | 'S' => loop {
                    let (x1, y1, x2, y2, x, y) = if cmd.to_ascii_uppercase() == 'C' {
                        (
                            next_num(&mut chars, &mut token),
                            next_num(&mut chars, &mut token),
                            next_num(&mut chars, &mut token),
                            next_num(&mut chars, &mut token),
                            next_num(&mut chars, &mut token),
                            next_num(&mut chars, &mut token),
                        )
                    } else {
                        (
                            (last_cubic[0] * 2.0 - pos[0]),
                            (last_cubic[1] * 2.0 - pos[1]),
                            next_num(&mut chars, &mut token),
                            next_num(&mut chars, &mut token),
                            next_num(&mut chars, &mut token),
                            next_num(&mut chars, &mut token),
                        )
                    };
                    let (x1, y1) = if relative {
                        (pos[0] + x1, pos[1] + y1)
                    } else {
                        (x1, y1)
                    };
                    let (x2, y2) = if relative {
                        (pos[0] + x2, pos[1] + y2)
                    } else {
                        (x2, y2)
                    };
                    let (x, y) = if relative {
                        (pos[0] + x, pos[1] + y)
                    } else {
                        (x, y)
                    };
                    last_cubic = [x2, y2];
                    flatten_cubic(pos, [x1, y1], [x2, y2], [x, y], 14, &mut current, transform);
                    pos = [x, y];
                    if !chars
                        .peek()
                        .is_some_and(|c| c.is_ascii_digit() || *c == '-' || *c == '.' || *c == '+')
                    {
                        break;
                    }
                },
                'Q' | 'T' => loop {
                    let (x1, y1, x, y) = if cmd.to_ascii_uppercase() == 'Q' {
                        (
                            next_num(&mut chars, &mut token),
                            next_num(&mut chars, &mut token),
                            next_num(&mut chars, &mut token),
                            next_num(&mut chars, &mut token),
                        )
                    } else {
                        (
                            last_quad[0] * 2.0 - pos[0],
                            last_quad[1] * 2.0 - pos[1],
                            next_num(&mut chars, &mut token),
                            next_num(&mut chars, &mut token),
                        )
                    };
                    let (x1, y1) = if relative {
                        (pos[0] + x1, pos[1] + y1)
                    } else {
                        (x1, y1)
                    };
                    let (x, y) = if relative {
                        (pos[0] + x, pos[1] + y)
                    } else {
                        (x, y)
                    };
                    last_quad = [x1, y1];
                    flatten_quad(pos, [x1, y1], [x, y], 12, &mut current, transform);
                    pos = [x, y];
                    if !chars
                        .peek()
                        .is_some_and(|c| c.is_ascii_digit() || *c == '-' || *c == '.' || *c == '+')
                    {
                        break;
                    }
                },
                'A' => loop {
                    let (rx, ry, rot, large, sweep, x, y) = (
                        next_num(&mut chars, &mut token),
                        next_num(&mut chars, &mut token),
                        next_num(&mut chars, &mut token),
                        next_num(&mut chars, &mut token),
                        next_num(&mut chars, &mut token),
                        next_num(&mut chars, &mut token),
                        next_num(&mut chars, &mut token),
                    );
                    let (x, y) = if relative {
                        (pos[0] + x, pos[1] + y)
                    } else {
                        (x, y)
                    };
                    flatten_arc(
                        pos,
                        rx,
                        ry,
                        rot,
                        large > 0.5,
                        sweep > 0.5,
                        [x, y],
                        16,
                        &mut current,
                        transform,
                    );
                    pos = [x, y];
                    if !chars
                        .peek()
                        .is_some_and(|c| c.is_ascii_digit() || *c == '-' || *c == '.' || *c == '+')
                    {
                        break;
                    }
                },
                'Z' => {
                    if !current.is_empty() {
                        current.push(start);
                        subpaths.push(std::mem::take(&mut current));
                    }
                    pos = start;
                }
                _ => {}
            }
        } else if c.is_whitespace() || c == ',' {
            chars.next();
        } else {
            // numeric run right after M/L etc. handled per-command; skip stray
            chars.next();
        }
    }
    if !current.is_empty() {
        subpaths.push(current);
    }
    let _ = last_cmd;
    subpaths
}

fn flatten_cubic(
    p0: [f32; 2],
    p1: [f32; 2],
    p2: [f32; 2],
    p3: [f32; 2],
    steps: usize,
    out: &mut Vec<[f32; 2]>,
    t: &Transform,
) {
    for i in 1..=steps {
        let s = i as f32 / steps as f32;
        let q0 = lerp2(p0, p1, s);
        let q1 = lerp2(p1, p2, s);
        let q2 = lerp2(p2, p3, s);
        let r0 = lerp2(q0, q1, s);
        let r1 = lerp2(q1, q2, s);
        out.push(t.apply(lerp2(r0, r1, s)));
    }
}

fn flatten_quad(
    p0: [f32; 2],
    p1: [f32; 2],
    p2: [f32; 2],
    steps: usize,
    out: &mut Vec<[f32; 2]>,
    t: &Transform,
) {
    for i in 1..=steps {
        let s = i as f32 / steps as f32;
        out.push(t.apply(lerp2(lerp2(p0, p1, s), lerp2(p1, p2, s), s)));
    }
}

/// Endpoint arc → sampled points (center-parameterization).
#[allow(clippy::too_many_arguments)]
fn flatten_arc(
    p0: [f32; 2],
    rx: f32,
    ry: f32,
    x_rotation: f32,
    large_arc: bool,
    sweep: bool,
    p1: [f32; 2],
    steps: usize,
    out: &mut Vec<[f32; 2]>,
    t: &Transform,
) {
    let (rx, ry) = (rx.abs().max(1e-3), ry.abs().max(1e-3));
    let phi = x_rotation.to_radians();
    let (cos_phi, sin_phi) = phi.sin_cos();
    let dx2 = (p0[0] - p1[0]) * 0.5;
    let dy2 = (p0[1] - p1[1]) * 0.5;
    let x1 = cos_phi * dx2 + sin_phi * dy2;
    let y1 = -sin_phi * dx2 + cos_phi * dy2;
    let mut radius_scale = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
    let mut rx = rx;
    let mut ry = ry;
    if radius_scale > 1.0 {
        radius_scale = radius_scale.sqrt();
        rx *= radius_scale;
        ry *= radius_scale;
    }
    let sign = if large_arc != sweep { 1.0 } else { -1.0 };
    let denom = (rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1).max(1e-6);
    let coef = sign
        * (denom / (rx * rx * y1 * y1 + ry * ry * x1 * x1))
            .max(0.0)
            .sqrt();
    let cxp = coef * rx * y1 / ry;
    let cyp = -coef * ry * x1 / rx;
    let cx = cos_phi * cxp - sin_phi * cyp + (p0[0] + p1[0]) * 0.5;
    let cy = sin_phi * cxp + cos_phi * cyp + (p0[1] + p1[1]) * 0.5;
    let angle = |ux: f32, uy: f32, vx: f32, vy: f32| -> f32 {
        let dot = ux * vx + uy * vy;
        let len = ux.hypot(uy) * vx.hypot(vy);
        let a = (dot / len).clamp(-1.0, 1.0).acos();
        if ux * vy - uy * vx < 0.0 {
            -a
        } else {
            a
        }
    };
    let theta1 = angle(1.0, 0.0, (x1 - cxp) / rx, (y1 - cyp) / ry);
    let mut delta = angle(
        (x1 - cxp) / rx,
        (y1 - cyp) / ry,
        (-x1 - cxp) / rx,
        (-y1 - cyp) / ry,
    );
    if !sweep && delta > 0.0 {
        delta -= std::f32::consts::TAU;
    } else if sweep && delta < 0.0 {
        delta += std::f32::consts::TAU;
    }
    for i in 1..=steps {
        let theta = theta1 + delta * (i as f32 / steps as f32);
        let (sin_t, cos_t) = theta.sin_cos();
        let ex = rx * cos_t;
        let ey = ry * sin_t;
        out.push(t.apply([
            cos_phi * ex - sin_phi * ey + cx,
            sin_phi * ex + cos_phi * ey + cy,
        ]));
    }
}

fn lerp2(a: [f32; 2], b: [f32; 2], t: f32) -> [f32; 2] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

/// 2D affine transform accumulated over nested <g> elements.
#[derive(Clone, Copy)]
struct Transform {
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    e: f32,
    f: f32,
}
impl Transform {
    fn identity() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        }
    }
    fn apply(&self, p: [f32; 2]) -> [f32; 2] {
        [
            self.a * p[0] + self.c * p[1] + self.e,
            self.b * p[0] + self.d * p[1] + self.f,
        ]
    }
    fn mul(&self, other: &Transform) -> Transform {
        Transform {
            a: self.a * other.a + self.c * other.b,
            b: self.b * other.a + self.d * other.b,
            c: self.a * other.c + self.c * other.d,
            d: self.b * other.c + self.d * other.d,
            e: self.a * other.e + self.c * other.f + self.e,
            f: self.b * other.e + self.d * other.f + self.f,
        }
    }
    fn from_attr(spec: &str) -> Option<Transform> {
        // supports "translate(x y) scale(s) rotate(deg) matrix(a b c d e f)"
        let mut result = Transform::identity();
        let bytes = spec.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i].is_ascii_alphabetic() {
                let start = i;
                while i < bytes.len() && bytes[i] != b'(' {
                    i += 1;
                }
                let name = &spec[start..i.min(spec.len())];
                let open = spec[i..].find('(')? + i;
                let close = spec[open..].find(')')? + open;
                let args = nums(&spec[open + 1..close]);
                let t = match name.trim() {
                    "translate" => Transform {
                        a: 1.0,
                        b: 0.0,
                        c: 0.0,
                        d: 1.0,
                        e: args.first().copied().unwrap_or(0.0),
                        f: args.get(1).copied().unwrap_or(0.0),
                    },
                    "scale" => {
                        let s = args.first().copied().unwrap_or(1.0);
                        Transform {
                            a: s,
                            b: 0.0,
                            c: 0.0,
                            d: args.get(1).copied().unwrap_or(s),
                            e: 0.0,
                            f: 0.0,
                        }
                    }
                    "rotate" => {
                        let rad = args.first().copied().unwrap_or(0.0).to_radians();
                        let (sn, cs) = rad.sin_cos();
                        let (cx, cy) = (
                            args.get(1).copied().unwrap_or(0.0),
                            args.get(2).copied().unwrap_or(0.0),
                        );
                        let rot = Transform {
                            a: cs,
                            b: sn,
                            c: -sn,
                            d: cs,
                            e: 0.0,
                            f: 0.0,
                        };
                        result
                            .mul(&Transform {
                                a: 1.0,
                                b: 0.0,
                                c: 0.0,
                                d: 1.0,
                                e: cx,
                                f: cy,
                            })
                            .mul(&rot)
                            .mul(&Transform {
                                a: 1.0,
                                b: 0.0,
                                c: 0.0,
                                d: 1.0,
                                e: -cx,
                                f: -cy,
                            })
                    }
                    "matrix" => Transform {
                        a: args.first().copied().unwrap_or(1.0),
                        b: args.get(1).copied().unwrap_or(0.0),
                        c: args.get(2).copied().unwrap_or(0.0),
                        d: args.get(3).copied().unwrap_or(1.0),
                        e: args.get(4).copied().unwrap_or(0.0),
                        f: args.get(5).copied().unwrap_or(0.0),
                    },
                    _ => Transform::identity(),
                };
                result = result.mul(&t);
                i = close + 1;
            } else {
                i += 1;
            }
        }
        Some(result)
    }
}

/// An attribute map for one tag.
struct Tag {
    name: String,
    attrs: Vec<(String, String)>,
}
impl Tag {
    fn attr(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

/// Minimal XML scanner yielding one tag at a time (no entity/CDATA needs for
/// the drawing profile).
/// One scanned tag: `close_tag` is true for `</g>`-style closers.
struct Scan {
    tag: Tag,
    self_closing: bool,
    close_tag: bool,
}

/// Minimal XML scanner (no entities/CDATA needed for the drawing profile).
fn scan_tags(xml: &str, mut visit: impl FnMut(Scan)) {
    let bytes = xml.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        let Some(close_rel) = xml[i..].find('>') else {
            return;
        };
        let close = i + close_rel;
        if close <= i + 1 {
            i = close + 1;
            continue;
        }
        let inner = &xml[i + 1..close];
        if let Some(name) = inner.strip_prefix('/') {
            visit(Scan {
                tag: Tag {
                    name: name.trim().to_string(),
                    attrs: Vec::new(),
                },
                self_closing: false,
                close_tag: true,
            });
            i = close + 1;
            continue;
        }
        let self_closing = inner.trim_end().ends_with('/');
        let inner = inner.trim_end().trim_end_matches('/');
        let mut parts = inner.splitn(2, char::is_whitespace);
        let name = parts.next().unwrap_or("").to_string();
        let attr_str = parts.next().unwrap_or("");
        let mut attrs = Vec::new();
        let attr_bytes = attr_str.as_bytes();
        let mut j = 0;
        while j < attr_bytes.len() {
            if attr_bytes[j] == b'=' {
                let mut key_end = j;
                while key_end > 0 && attr_bytes[key_end - 1].is_ascii_whitespace() {
                    key_end -= 1;
                }
                let mut key_start = key_end;
                while key_start > 0 && !attr_bytes[key_start - 1].is_ascii_whitespace() {
                    key_start -= 1;
                }
                let mut k = j + 1;
                while k < attr_bytes.len() && attr_bytes[k] != b'"' && attr_bytes[k] != b'\'' {
                    k += 1;
                }
                if let Some(q) = attr_bytes.get(k).copied() {
                    let vstart = k + 1;
                    if let Some(vend) = attr_str[vstart..].find(q as char) {
                        attrs.push((
                            attr_str[key_start..key_end].to_string(),
                            attr_str[vstart..vstart + vend].to_string(),
                        ));
                        j = vstart + vend + 1;
                        continue;
                    }
                }
            }
            j += 1;
        }
        visit(Scan {
            tag: Tag { name, attrs },
            self_closing,
            close_tag: false,
        });
        i = close + 1;
    }
}

fn parse_shapes(svg: &str) -> Result<(f32, f32, Vec<DrawShape>), String> {
    let mut doc_w = 0.0f32;
    let mut doc_h = 0.0f32;
    let mut shapes: Vec<DrawShape> = Vec::new();
    // Style stack: (transform, fill, stroke, stroke_width)
    let mut stack: Vec<(Transform, Option<[f32; 4]>, Option<[f32; 4]>, f32)> =
        vec![(Transform::identity(), Some([0.1, 0.1, 0.1, 1.0]), None, 0.0)];
    scan_tags(svg, |scan| {
        let tag = scan.tag;
        if scan.close_tag {
            if tag.name == "g" && stack.len() > 1 {
                stack.pop();
            }
            return;
        }
        let self_closing = scan.self_closing;
        let num = |tag: &Tag, key: &str| -> Option<f32> {
            tag.attr(key).and_then(|v| nums(v).first().copied())
        };
        match tag.name.as_str() {
            "svg" => {
                doc_w = num(&tag, "width").unwrap_or(0.0);
                doc_h = num(&tag, "height").unwrap_or(0.0);
                if let Some(view) = tag.attr("viewBox") {
                    let v = nums(view);
                    if doc_w <= 0.0 {
                        doc_w = v.get(2).copied().unwrap_or(0.0);
                    }
                    if doc_h <= 0.0 {
                        doc_h = v.get(3).copied().unwrap_or(0.0);
                    }
                }
            }
            "g" => {
                let (pt, pf, ps, pw) = stack.last().cloned().unwrap_or((
                    Transform::identity(),
                    Some([0.1, 0.1, 0.1, 1.0]),
                    None,
                    0.0,
                ));
                let t = tag
                    .attr("transform")
                    .and_then(Transform::from_attr)
                    .unwrap_or(Transform::identity());
                let fill = tag.attr("fill").and_then(parse_color).or(pf);
                let stroke = tag.attr("stroke").and_then(parse_color).or(ps);
                let width = num(&tag, "stroke-width").unwrap_or(pw);
                stack.push((pt.mul(&t), fill, stroke, width));
            }
            "rect" | "circle" | "ellipse" | "line" | "polyline" | "polygon" | "path" => {
                let (parent, fill, stroke, stroke_width) =
                    stack
                        .last()
                        .cloned()
                        .unwrap_or((Transform::identity(), None, None, 0.0));
                let t = tag
                    .attr("transform")
                    .and_then(Transform::from_attr)
                    .unwrap_or(Transform::identity())
                    .mul(&parent);
                let own_fill = tag.attr("fill").and_then(parse_color);
                let own_stroke = tag.attr("stroke").and_then(parse_color);
                let fill = own_fill.or(fill);
                let stroke = own_stroke.or(stroke);
                let stroke_width = num(&tag, "stroke-width").unwrap_or(stroke_width);
                let mut subpaths: Vec<Vec<[f32; 2]>> = Vec::new();
                match tag.name.as_str() {
                    "rect" => {
                        let (x, y, w, h) = (
                            num(&tag, "x").unwrap_or(0.0),
                            num(&tag, "y").unwrap_or(0.0),
                            num(&tag, "width").unwrap_or(0.0),
                            num(&tag, "height").unwrap_or(0.0),
                        );
                        subpaths.push(vec![
                            t.apply([x, y]),
                            t.apply([x + w, y]),
                            t.apply([x + w, y + h]),
                            t.apply([x, y + h]),
                        ]);
                    }
                    "circle" => {
                        let (cx, cy, r) = (
                            num(&tag, "cx").unwrap_or(0.0),
                            num(&tag, "cy").unwrap_or(0.0),
                            num(&tag, "r").unwrap_or(0.0),
                        );
                        let mut ring = Vec::new();
                        for i in 0..48 {
                            let a = std::f32::consts::TAU * i as f32 / 48.0;
                            ring.push(t.apply([cx + r * a.cos(), cy + r * a.sin()]));
                        }
                        subpaths.push(ring);
                    }
                    "ellipse" => {
                        let (cx, cy, rx, ry) = (
                            num(&tag, "cx").unwrap_or(0.0),
                            num(&tag, "cy").unwrap_or(0.0),
                            num(&tag, "rx").unwrap_or(0.0),
                            num(&tag, "ry").unwrap_or(0.0),
                        );
                        let mut ring = Vec::new();
                        for i in 0..56 {
                            let a = std::f32::consts::TAU * i as f32 / 56.0;
                            ring.push(t.apply([cx + rx * a.cos(), cy + ry * a.sin()]));
                        }
                        subpaths.push(ring);
                    }
                    "line" => {
                        subpaths.push(vec![
                            t.apply([
                                num(&tag, "x1").unwrap_or(0.0),
                                num(&tag, "y1").unwrap_or(0.0),
                            ]),
                            t.apply([
                                num(&tag, "x2").unwrap_or(0.0),
                                num(&tag, "y2").unwrap_or(0.0),
                            ]),
                        ]);
                    }
                    "polyline" | "polygon" => {
                        if let Some(points_str) = tag.attr("points") {
                            let n = nums(points_str);
                            let ring: Vec<[f32; 2]> = n
                                .chunks(2)
                                .filter(|c| c.len() == 2)
                                .map(|c| t.apply([c[0], c[1]]))
                                .collect();
                            if ring.len() > 1 {
                                subpaths.push(ring);
                            }
                        }
                    }
                    "path" => {
                        if let Some(d) = tag.attr("d") {
                            subpaths = parse_path_into(d, &t);
                        }
                    }
                    _ => {}
                }
                if !subpaths.is_empty() && (fill.is_some() || stroke.is_some()) {
                    shapes.push(DrawShape {
                        subpaths,
                        fill,
                        stroke,
                        stroke_width,
                    });
                }
            }
            _ => {}
        }
        if tag.name == "g" && self_closing {
            stack.pop();
        }
    });
    if doc_w <= 0.0 || doc_h <= 0.0 {
        return Err("SVG must declare width/height or a viewBox".into());
    }
    Ok((doc_w, doc_h, shapes))
}

/// `parse_path` without the closure-capture dance.
fn parse_path_into(data: &str, t: &Transform) -> Vec<Vec<[f32; 2]>> {
    parse_path(data, t)
}

/// Convert an SVG document into Bonaparte vector layers positioned in doc
/// space (top-left origin). The caller fits them into a comp.
pub fn svg_layers(svg: &str, duration_ticks: i64) -> Result<(f32, f32, Vec<Layer>), String> {
    let (doc_w, doc_h, shapes) = parse_shapes(svg)?;
    let mut layers = Vec::new();
    for (index, shape) in shapes.into_iter().enumerate() {
        let Some(bounds) = crate::vector::bounds(&shape.subpaths) else {
            continue;
        };
        let w = (bounds[2] - bounds[0]).max(1.0);
        let h = (bounds[3] - bounds[1]).max(1.0);
        let mut layer = Layer::new(
            format!("Shape {}", index + 1),
            LayerKind::Shape {
                color: shape.fill.unwrap_or([0.0; 4]),
                generator: None,
                style: ShapeStyle {
                    size: Some([w, h]),
                    corner_radius: 0.0,
                    stroke_width: shape.stroke_width,
                    stroke_color: shape.stroke.unwrap_or([0.0; 4]),
                },
                points: shape
                    .subpaths
                    .iter()
                    .map(|sub| {
                        sub.iter()
                            .map(|p| [p[0] - (bounds[0] + w * 0.5), p[1] - (bounds[1] + h * 0.5)])
                            .collect()
                    })
                    .collect(),
            },
            Time(0),
            Time(duration_ticks),
        );
        layer.transform.position = [
            bounds[0] + w * 0.5 - doc_w * 0.5,
            bounds[1] + h * 0.5 - doc_h * 0.5,
        ];
        layers.push(layer);
    }
    if layers.is_empty() {
        return Err("No drawable shapes found in the SVG".into());
    }
    Ok((doc_w, doc_h, layers))
}
