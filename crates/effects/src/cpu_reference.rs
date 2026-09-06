//! Deterministic CPU reference evaluators for all 10 first-party GPU effect packs.
//!
//! Purpose:
//! - Provides bit-for-bit / pixel-deterministic verification of effect algorithms.
//! - Used by golden-frame tests and headless offline evaluation when GPU is unavailable.
//! - Golden standard against which the GPU WGSL passes are validated.

use std::collections::HashMap;

/// An RGBA8 image buffer used for CPU evaluation.
#[derive(Debug, Clone, PartialEq)]
pub struct CpuFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl CpuFrame {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            rgba: vec![0; (width * height * 4) as usize],
        }
    }

    pub fn filled(width: u32, height: u32, color: [u8; 4]) -> Self {
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for _ in 0..(width * height) {
            rgba.extend_from_slice(&color);
        }
        Self { width, height, rgba }
    }

    pub fn from_rgba(width: u32, height: u32, rgba: Vec<u8>) -> Self {
        assert_eq!(rgba.len(), (width * height * 4) as usize);
        Self { width, height, rgba }
    }

    #[inline]
    pub fn get_pixel_u8(&self, x: u32, y: u32) -> [u8; 4] {
        if x >= self.width || y >= self.height {
            return [0, 0, 0, 0];
        }
        let i = ((y * self.width + x) * 4) as usize;
        [self.rgba[i], self.rgba[i + 1], self.rgba[i + 2], self.rgba[i + 3]]
    }

    #[inline]
    pub fn set_pixel_u8(&mut self, x: u32, y: u32, color: [u8; 4]) {
        if x >= self.width || y >= self.height {
            return;
        }
        let i = ((y * self.width + x) * 4) as usize;
        self.rgba[i..i + 4].copy_from_slice(&color);
    }

    /// Sample pixel with normalized UV coordinates (0.0..1.0).
    /// If repeat_edge is true, clamps to edges. If false, out-of-bounds UVs return transparent black.
    pub fn sample_uv(&self, mut u: f32, mut v: f32, repeat_edge: bool) -> [f32; 4] {
        if repeat_edge {
            u = u.clamp(0.0, 1.0);
            v = v.clamp(0.0, 1.0);
        } else if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
            return [0.0, 0.0, 0.0, 0.0];
        }

        let px = ((u * self.width as f32 - 0.5).round() as i32)
            .clamp(0, self.width.saturating_sub(1) as i32) as u32;
        let py = ((v * self.height as f32 - 0.5).round() as i32)
            .clamp(0, self.height.saturating_sub(1) as i32) as u32;
        let [r, g, b, a] = self.get_pixel_u8(px, py);
        [
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            a as f32 / 255.0,
        ]
    }

    /// Bilinear UV sampling for smoother continuous transforms.
    pub fn sample_uv_bilinear(&self, mut u: f32, mut v: f32, repeat_edge: bool) -> [f32; 4] {
        if repeat_edge {
            u = u.clamp(0.0, 1.0);
            v = v.clamp(0.0, 1.0);
        } else if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
            return [0.0, 0.0, 0.0, 0.0];
        }

        let fx = (u * self.width as f32 - 0.5).clamp(0.0, self.width.saturating_sub(1) as f32);
        let fy = (v * self.height as f32 - 0.5).clamp(0.0, self.height.saturating_sub(1) as f32);

        let x0 = fx.floor() as u32;
        let y0 = fy.floor() as u32;
        let x1 = (x0 + 1).min(self.width.saturating_sub(1));
        let y1 = (y0 + 1).min(self.height.saturating_sub(1));

        let tx = fx - fx.floor();
        let ty = fy - fy.floor();

        let c00 = self.get_pixel_f32(x0, y0);
        let c10 = self.get_pixel_f32(x1, y0);
        let c01 = self.get_pixel_f32(x0, y1);
        let c11 = self.get_pixel_f32(x1, y1);

        let mut out = [0.0f32; 4];
        for i in 0..4 {
            let top = c00[i] * (1.0 - tx) + c10[i] * tx;
            let bot = c01[i] * (1.0 - tx) + c11[i] * tx;
            out[i] = top * (1.0 - ty) + bot * ty;
        }
        out
    }

    #[inline]
    fn get_pixel_f32(&self, x: u32, y: u32) -> [f32; 4] {
        let [r, g, b, a] = self.get_pixel_u8(x, y);
        [
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            a as f32 / 255.0,
        ]
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CpuEvalError {
    #[error("Unknown effect ID: {0}")]
    UnknownEffect(String),
    #[error("Parameter {0} missing or wrong type")]
    InvalidParam(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParamValue {
    Float(f32),
    Bool(bool),
    Color([f32; 4]),
    Point([f32; 2]),
    Index(usize),
}

fn f32_to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

pub fn evaluate_glow(
    input: &CpuFrame,
    radius: f32,
    intensity: f32,
    tint: [f32; 4],
) -> CpuFrame {
    let mut out = CpuFrame::new(input.width, input.height);
    let texel_x = 1.0 / input.width as f32;
    let texel_y = 1.0 / input.height as f32;

    for y in 0..input.height {
        let v = (y as f32 + 0.5) * texel_y;
        for x in 0..input.width {
            let u = (x as f32 + 0.5) * texel_x;
            let src = input.sample_uv(u, v, false);

            let mut halo = [0.0f32; 4];
            for d in 0..4 {
                let dx = ((d & 1) * 2 - 1) as f32;
                let dy = (((d & 2) * 2 - 1) as f32) * 0.5;
                let su = u + dx * radius * texel_x;
                let sv = v + dy * radius * texel_y;
                let sample = input.sample_uv(su, sv, false);
                for c in 0..4 {
                    halo[c] += sample[c];
                }
            }

            for c in 0..3 {
                halo[c] = (halo[c] / 4.0) * tint[c] * intensity;
            }
            halo[3] = (halo[3] / 4.0) * tint[3] * intensity;

            let out_r = src[0].max(halo[0]);
            let out_g = src[1].max(halo[1]);
            let out_b = src[2].max(halo[2]);
            let out_a = src[3];

            out.set_pixel_u8(
                x,
                y,
                [
                    f32_to_u8(out_r),
                    f32_to_u8(out_g),
                    f32_to_u8(out_b),
                    f32_to_u8(out_a),
                ],
            );
        }
    }
    out
}

pub fn evaluate_blur(input: &CpuFrame, radius: f32, repeat_edge: bool) -> CpuFrame {
    if radius <= 0.0 {
        return input.clone();
    }

    let mut out = CpuFrame::new(input.width, input.height);
    let texel_x = 1.0 / input.width as f32;
    let texel_y = 1.0 / input.height as f32;
    let r = radius.ceil().max(1.0) as i32;
    let sigma = (radius * 0.5).max(0.5);
    let two_sigma_sq = 2.0 * sigma * sigma;

    for y in 0..input.height {
        let v = (y as f32 + 0.5) * texel_y;
        for x in 0..input.width {
            let u = (x as f32 + 0.5) * texel_x;
            let mut acc = [0.0f32; 4];
            let mut total_weight = 0.0f32;

            for dy in -r..=r {
                for dx in -r..=r {
                    let dist_sq = (dx * dx + dy * dy) as f32;
                    let weight = (-dist_sq / two_sigma_sq).exp();
                    let su = u + dx as f32 * texel_x;
                    let sv = v + dy as f32 * texel_y;
                    let sample = input.sample_uv(su, sv, repeat_edge);

                    for c in 0..4 {
                        acc[c] += sample[c] * weight;
                    }
                    total_weight += weight;
                }
            }

            out.set_pixel_u8(
                x,
                y,
                [
                    f32_to_u8(acc[0] / total_weight),
                    f32_to_u8(acc[1] / total_weight),
                    f32_to_u8(acc[2] / total_weight),
                    f32_to_u8(acc[3] / total_weight),
                ],
            );
        }
    }
    out
}

pub fn evaluate_drop_shadow(
    input: &CpuFrame,
    offset_x: f32,
    offset_y: f32,
    radius: f32,
    color: [f32; 4],
    opacity: f32,
) -> CpuFrame {
    let mut out = CpuFrame::new(input.width, input.height);
    let texel_x = 1.0 / input.width as f32;
    let texel_y = 1.0 / input.height as f32;
    let offset_u = offset_x * texel_x;
    let offset_v = offset_y * texel_y;

    for y in 0..input.height {
        let v = (y as f32 + 0.5) * texel_y;
        for x in 0..input.width {
            let u = (x as f32 + 0.5) * texel_x;
            let src = input.sample_uv(u, v, false);
            let shadow_u = u - offset_u;
            let shadow_v = v - offset_v;

            let shadow_alpha = if radius <= 0.0 {
                input.sample_uv(shadow_u, shadow_v, false)[3]
            } else {
                let r = radius.ceil().max(1.0) as i32;
                let sigma = (radius * 0.5).max(0.5);
                let two_sigma_sq = 2.0 * sigma * sigma;
                let mut acc = 0.0f32;
                let mut total_weight = 0.0f32;

                for dy in -r..=r {
                    for dx in -r..=r {
                        let dist_sq = (dx * dx + dy * dy) as f32;
                        let weight = (-dist_sq / two_sigma_sq).exp();
                        let su = shadow_u + dx as f32 * texel_x;
                        let sv = shadow_v + dy as f32 * texel_y;
                        let sample_a = input.sample_uv(su, sv, false)[3];
                        acc += sample_a * weight;
                        total_weight += weight;
                    }
                }
                acc / total_weight
            };

            let final_shadow_alpha = shadow_alpha * color[3] * opacity;
            let shadow_r = color[0] * final_shadow_alpha;
            let shadow_g = color[1] * final_shadow_alpha;
            let shadow_b = color[2] * final_shadow_alpha;

            let out_r = src[0] + shadow_r * (1.0 - src[3]);
            let out_g = src[1] + shadow_g * (1.0 - src[3]);
            let out_b = src[2] + shadow_b * (1.0 - src[3]);
            let out_a = src[3] + final_shadow_alpha * (1.0 - src[3]);

            out.set_pixel_u8(
                x,
                y,
                [
                    f32_to_u8(out_r),
                    f32_to_u8(out_g),
                    f32_to_u8(out_b),
                    f32_to_u8(out_a),
                ],
            );
        }
    }
    out
}

fn rgb_to_hsv(rgb: [f32; 3]) -> [f32; 3] {
    let cmax = rgb[0].max(rgb[1].max(rgb[2]));
    let cmin = rgb[0].min(rgb[1].min(rgb[2]));
    let delta = cmax - cmin;

    let mut h = 0.0f32;
    if delta > 1e-5 {
        if cmax == rgb[0] {
            h = ((rgb[1] - rgb[2]) / delta).rem_euclid(6.0);
        } else if cmax == rgb[1] {
            h = ((rgb[2] - rgb[0]) / delta) + 2.0;
        } else {
            h = ((rgb[0] - rgb[1]) / delta) + 4.0;
        }
        h /= 6.0;
        if h < 0.0 {
            h += 1.0;
        }
    }

    let s = if cmax > 1e-5 { delta / cmax } else { 0.0 };
    let v = cmax;
    [h, s, v]
}

fn hsv_to_rgb(hsv: [f32; 3]) -> [f32; 3] {
    let h = (hsv[0].rem_euclid(1.0) + 1.0).rem_euclid(1.0);
    let s = hsv[1].clamp(0.0, 1.0);
    let v = hsv[2].clamp(0.0, 1.0);

    let c = v * s;
    let x = c * (1.0 - ((h * 6.0).rem_euclid(2.0) - 1.0).abs());
    let m = v - c;

    let hi = (h * 6.0).floor() as u32 % 6;
    let (r, g, b) = match hi {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };

    [r + m, g + m, b + m]
}

pub fn evaluate_color_adjust(
    input: &CpuFrame,
    brightness: f32,
    contrast: f32,
    saturation: f32,
    hue_shift: f32,
) -> CpuFrame {
    let mut out = CpuFrame::new(input.width, input.height);
    for y in 0..input.height {
        for x in 0..input.width {
            let src = input.get_pixel_f32(x, y);
            let mut r = src[0] + brightness;
            let mut g = src[1] + brightness;
            let mut b = src[2] + brightness;

            r = (r - 0.5) * contrast + 0.5;
            g = (g - 0.5) * contrast + 0.5;
            b = (b - 0.5) * contrast + 0.5;

            let mut hsv = rgb_to_hsv([r.clamp(0.0, 1.0), g.clamp(0.0, 1.0), b.clamp(0.0, 1.0)]);
            let hue_rot = hue_shift / 360.0;
            hsv[0] = (hsv[0] + hue_rot).rem_euclid(1.0);
            hsv[1] = (hsv[1] * saturation).clamp(0.0, 1.0);
            let rgb = hsv_to_rgb(hsv);

            out.set_pixel_u8(
                x,
                y,
                [
                    f32_to_u8(rgb[0]),
                    f32_to_u8(rgb[1]),
                    f32_to_u8(rgb[2]),
                    f32_to_u8(src[3]),
                ],
            );
        }
    }
    out
}

pub fn evaluate_transform(
    input: &CpuFrame,
    offset_x: f32,
    offset_y: f32,
    scale_x: f32,
    scale_y: f32,
    rotation: f32,
) -> CpuFrame {
    let mut out = CpuFrame::new(input.width, input.height);
    let texel_x = 1.0 / input.width as f32;
    let texel_y = 1.0 / input.height as f32;

    let rad = -rotation.to_radians();
    let cos_r = rad.cos();
    let sin_r = rad.sin();

    let sx = if scale_x.abs() > 1e-4 { scale_x } else { 1.0 };
    let sy = if scale_y.abs() > 1e-4 { scale_y } else { 1.0 };

    for y in 0..input.height {
        let v = (y as f32 + 0.5) * texel_y;
        for x in 0..input.width {
            let u = (x as f32 + 0.5) * texel_x;

            let mut px = (u - 0.5) - offset_x * texel_x;
            let mut py = (v - 0.5) - offset_y * texel_y;

            let rx = px * cos_r - py * sin_r;
            let ry = px * sin_r + py * cos_r;

            px = rx / sx;
            py = ry / sy;

            let src_u = px + 0.5;
            let src_v = py + 0.5;

            let col = input.sample_uv_bilinear(src_u, src_v, false);
            out.set_pixel_u8(
                x,
                y,
                [
                    f32_to_u8(col[0]),
                    f32_to_u8(col[1]),
                    f32_to_u8(col[2]),
                    f32_to_u8(col[3]),
                ],
            );
        }
    }
    out
}

pub fn evaluate_vignette(
    input: &CpuFrame,
    radius: f32,
    softness: f32,
    intensity: f32,
    color: [f32; 4],
) -> CpuFrame {
    let mut out = CpuFrame::new(input.width, input.height);
    let texel_x = 1.0 / input.width as f32;
    let texel_y = 1.0 / input.height as f32;

    let inner = radius;
    let outer = radius + softness.max(0.001);

    for y in 0..input.height {
        let v = (y as f32 + 0.5) * texel_y;
        for x in 0..input.width {
            let u = (x as f32 + 0.5) * texel_x;
            let src = input.sample_uv(u, v, false);

            let du = u - 0.5;
            let dv = v - 0.5;
            let dist = (du * du + dv * dv).sqrt();

            let t = ((dist - inner) / (outer - inner)).clamp(0.0, 1.0);
            let factor = (t * t * (3.0 - 2.0 * t) * intensity).clamp(0.0, 1.0);

            let r = src[0] * (1.0 - factor * color[3]) + color[0] * factor * color[3];
            let g = src[1] * (1.0 - factor * color[3]) + color[1] * factor * color[3];
            let b = src[2] * (1.0 - factor * color[3]) + color[2] * factor * color[3];

            out.set_pixel_u8(
                x,
                y,
                [f32_to_u8(r), f32_to_u8(g), f32_to_u8(b), f32_to_u8(src[3])],
            );
        }
    }
    out
}

pub fn evaluate_chromatic_aberration(
    input: &CpuFrame,
    amount: f32,
    angle: f32,
) -> CpuFrame {
    if amount <= 0.0 {
        return input.clone();
    }

    let mut out = CpuFrame::new(input.width, input.height);
    let texel_x = 1.0 / input.width as f32;
    let texel_y = 1.0 / input.height as f32;
    let rad = angle.to_radians();
    let dir_u = rad.cos() * amount * texel_x;
    let dir_v = rad.sin() * amount * texel_y;

    for y in 0..input.height {
        let v = (y as f32 + 0.5) * texel_y;
        for x in 0..input.width {
            let u = (x as f32 + 0.5) * texel_x;

            let r_col = input.sample_uv(u + dir_u, v + dir_v, false);
            let g_col = input.sample_uv(u, v, false);
            let b_col = input.sample_uv(u - dir_u, v - dir_v, false);

            let max_a = r_col[3].max(g_col[3].max(b_col[3]));

            out.set_pixel_u8(
                x,
                y,
                [
                    f32_to_u8(r_col[0]),
                    f32_to_u8(g_col[1]),
                    f32_to_u8(b_col[2]),
                    f32_to_u8(max_a),
                ],
            );
        }
    }
    out
}

pub fn evaluate_invert(input: &CpuFrame, amount: f32, invert_alpha: bool) -> CpuFrame {
    let mut out = CpuFrame::new(input.width, input.height);
    let amt = amount.clamp(0.0, 1.0);

    for y in 0..input.height {
        for x in 0..input.width {
            let src = input.get_pixel_f32(x, y);
            let inv_r = 1.0 - src[0];
            let inv_g = 1.0 - src[1];
            let inv_b = 1.0 - src[2];

            let r = src[0] * (1.0 - amt) + inv_r * amt;
            let g = src[1] * (1.0 - amt) + inv_g * amt;
            let b = src[2] * (1.0 - amt) + inv_b * amt;

            let a = if invert_alpha {
                let inv_a = 1.0 - src[3];
                src[3] * (1.0 - amt) + inv_a * amt
            } else {
                src[3]
            };

            out.set_pixel_u8(
                x,
                y,
                [f32_to_u8(r), f32_to_u8(g), f32_to_u8(b), f32_to_u8(a)],
            );
        }
    }
    out
}

pub fn evaluate_tint(
    input: &CpuFrame,
    black_color: [f32; 4],
    white_color: [f32; 4],
    amount: f32,
) -> CpuFrame {
    let mut out = CpuFrame::new(input.width, input.height);
    let amt = amount.clamp(0.0, 1.0);

    for y in 0..input.height {
        for x in 0..input.width {
            let src = input.get_pixel_f32(x, y);
            let luma = (0.299 * src[0] + 0.587 * src[1] + 0.114 * src[2]).clamp(0.0, 1.0);

            let tinted_r = black_color[0] * (1.0 - luma) + white_color[0] * luma;
            let tinted_g = black_color[1] * (1.0 - luma) + white_color[1] * luma;
            let tinted_b = black_color[2] * (1.0 - luma) + white_color[2] * luma;

            let r = src[0] * (1.0 - amt) + tinted_r * amt;
            let g = src[1] * (1.0 - amt) + tinted_g * amt;
            let b = src[2] * (1.0 - amt) + tinted_b * amt;

            out.set_pixel_u8(
                x,
                y,
                [f32_to_u8(r), f32_to_u8(g), f32_to_u8(b), f32_to_u8(src[3])],
            );
        }
    }
    out
}

pub fn evaluate_directional_blur(input: &CpuFrame, length: f32, angle: f32) -> CpuFrame {
    if length <= 0.0 {
        return input.clone();
    }

    let mut out = CpuFrame::new(input.width, input.height);
    let texel_x = 1.0 / input.width as f32;
    let texel_y = 1.0 / input.height as f32;
    let rad = angle.to_radians();
    let dir_u = rad.cos() * texel_x;
    let dir_v = rad.sin() * texel_y;

    let samples = (length * 0.5).ceil().max(1.0) as i32;

    for y in 0..input.height {
        let v = (y as f32 + 0.5) * texel_y;
        for x in 0..input.width {
            let u = (x as f32 + 0.5) * texel_x;
            let mut acc = [0.0f32; 4];
            let mut total_weight = 0.0f32;

            for i in -samples..=samples {
                let t = i as f32 / samples as f32;
                let offset_u = dir_u * (t * length * 0.5);
                let offset_v = dir_v * (t * length * 0.5);
                let weight = 1.0 - t.abs() * 0.5;
                let sample = input.sample_uv(u + offset_u, v + offset_v, false);

                for c in 0..4 {
                    acc[c] += sample[c] * weight;
                }
                total_weight += weight;
            }

            out.set_pixel_u8(
                x,
                y,
                [
                    f32_to_u8(acc[0] / total_weight),
                    f32_to_u8(acc[1] / total_weight),
                    f32_to_u8(acc[2] / total_weight),
                    f32_to_u8(acc[3] / total_weight),
                ],
            );
        }
    }
    out
}

pub fn evaluate_effect(
    effect_id: &str,
    input: &CpuFrame,
    params: &HashMap<String, ParamValue>,
) -> Result<CpuFrame, CpuEvalError> {
    match effect_id {
        "builtin.glow" => {
            let radius = match params.get("radius") {
                Some(ParamValue::Float(f)) => *f,
                _ => 12.0,
            };
            let intensity = match params.get("intensity") {
                Some(ParamValue::Float(f)) => *f,
                _ => 1.0,
            };
            let tint = match params.get("tint") {
                Some(ParamValue::Color(c)) => *c,
                _ => [1.0, 1.0, 1.0, 1.0],
            };
            Ok(evaluate_glow(input, radius, intensity, tint))
        }
        "builtin.blur" => {
            let radius = match params.get("radius") {
                Some(ParamValue::Float(f)) => *f,
                _ => 10.0,
            };
            let repeat_edge = match params.get("repeat_edge") {
                Some(ParamValue::Bool(b)) => *b,
                _ => true,
            };
            Ok(evaluate_blur(input, radius, repeat_edge))
        }
        "builtin.drop_shadow" => {
            let offset_x = match params.get("offset_x") {
                Some(ParamValue::Float(f)) => *f,
                _ => 10.0,
            };
            let offset_y = match params.get("offset_y") {
                Some(ParamValue::Float(f)) => *f,
                _ => 10.0,
            };
            let radius = match params.get("radius") {
                Some(ParamValue::Float(f)) => *f,
                _ => 15.0,
            };
            let color = match params.get("color") {
                Some(ParamValue::Color(c)) => *c,
                _ => [0.0, 0.0, 0.0, 0.75],
            };
            let opacity = match params.get("opacity") {
                Some(ParamValue::Float(f)) => *f,
                _ => 0.75,
            };
            Ok(evaluate_drop_shadow(input, offset_x, offset_y, radius, color, opacity))
        }
        "builtin.color_adjust" => {
            let brightness = match params.get("brightness") {
                Some(ParamValue::Float(f)) => *f,
                _ => 0.0,
            };
            let contrast = match params.get("contrast") {
                Some(ParamValue::Float(f)) => *f,
                _ => 1.0,
            };
            let saturation = match params.get("saturation") {
                Some(ParamValue::Float(f)) => *f,
                _ => 1.0,
            };
            let hue_shift = match params.get("hue_shift") {
                Some(ParamValue::Float(f)) => *f,
                _ => 0.0,
            };
            Ok(evaluate_color_adjust(input, brightness, contrast, saturation, hue_shift))
        }
        "builtin.transform" => {
            let offset_x = match params.get("offset_x") {
                Some(ParamValue::Float(f)) => *f,
                _ => 0.0,
            };
            let offset_y = match params.get("offset_y") {
                Some(ParamValue::Float(f)) => *f,
                _ => 0.0,
            };
            let scale_x = match params.get("scale_x") {
                Some(ParamValue::Float(f)) => *f,
                _ => 1.0,
            };
            let scale_y = match params.get("scale_y") {
                Some(ParamValue::Float(f)) => *f,
                _ => 1.0,
            };
            let rotation = match params.get("rotation") {
                Some(ParamValue::Float(f)) => *f,
                _ => 0.0,
            };
            Ok(evaluate_transform(input, offset_x, offset_y, scale_x, scale_y, rotation))
        }
        "builtin.vignette" => {
            let radius = match params.get("radius") {
                Some(ParamValue::Float(f)) => *f,
                _ => 0.75,
            };
            let softness = match params.get("softness") {
                Some(ParamValue::Float(f)) => *f,
                _ => 0.45,
            };
            let intensity = match params.get("intensity") {
                Some(ParamValue::Float(f)) => *f,
                _ => 0.8,
            };
            let color = match params.get("color") {
                Some(ParamValue::Color(c)) => *c,
                _ => [0.0, 0.0, 0.0, 1.0],
            };
            Ok(evaluate_vignette(input, radius, softness, intensity, color))
        }
        "builtin.chromatic_aberration" => {
            let amount = match params.get("amount") {
                Some(ParamValue::Float(f)) => *f,
                _ => 5.0,
            };
            let angle = match params.get("angle") {
                Some(ParamValue::Float(f)) => *f,
                _ => 0.0,
            };
            Ok(evaluate_chromatic_aberration(input, amount, angle))
        }
        "builtin.invert" => {
            let amount = match params.get("amount") {
                Some(ParamValue::Float(f)) => *f,
                _ => 1.0,
            };
            let invert_alpha = match params.get("invert_alpha") {
                Some(ParamValue::Bool(b)) => *b,
                _ => false,
            };
            Ok(evaluate_invert(input, amount, invert_alpha))
        }
        "builtin.tint" => {
            let black = match params.get("black_color") {
                Some(ParamValue::Color(c)) => *c,
                _ => [0.0, 0.0, 0.0, 1.0],
            };
            let white = match params.get("white_color") {
                Some(ParamValue::Color(c)) => *c,
                _ => [1.0, 1.0, 1.0, 1.0],
            };
            let amount = match params.get("amount") {
                Some(ParamValue::Float(f)) => *f,
                _ => 1.0,
            };
            Ok(evaluate_tint(input, black, white, amount))
        }
        "builtin.directional_blur" => {
            let length = match params.get("length") {
                Some(ParamValue::Float(f)) => *f,
                _ => 20.0,
            };
            let angle = match params.get("angle") {
                Some(ParamValue::Float(f)) => *f,
                _ => 0.0,
            };
            Ok(evaluate_directional_blur(input, length, angle))
        }
        "builtin.circle" => {
            let color = match params.get("color") {
                Some(ParamValue::Color(c)) => *c,
                _ => [1.0, 1.0, 1.0, 1.0],
            };
            let radius = match params.get("radius") {
                Some(ParamValue::Float(f)) => *f,
                _ => 1.0,
            };
            let stroke_color = match params.get("stroke_color") {
                Some(ParamValue::Color(c)) => *c,
                _ => [0.42, 0.54, 1.0, 1.0],
            };
            let stroke_width = match params.get("stroke_width") {
                Some(ParamValue::Float(f)) => *f,
                _ => 0.0,
            };
            Ok(evaluate_circle(input.width, input.height, color, radius, stroke_color, stroke_width))
        }
        _ => Err(CpuEvalError::UnknownEffect(effect_id.to_string())),
    }
}

/// Evaluates the vector circle shape generator.
pub fn evaluate_circle(
    width: u32,
    height: u32,
    color: [f32; 4],
    radius: f32,
    stroke_color: [f32; 4],
    stroke_width: f32,
) -> CpuFrame {
    let mut out = CpuFrame::new(width, height);
    let min_dim = width.min(height) as f32;
    let edge_w = if min_dim > 1.0 { 2.0 / min_dim } else { 0.05 };
    let r = radius.clamp(0.001, 1.0);

    for y in 0..height {
        let v = (y as f32 + 0.5) / height as f32;
        let py = (v - 0.5) * 2.0;
        for x in 0..width {
            let u = (x as f32 + 0.5) / width as f32;
            let px = (u - 0.5) * 2.0;
            let dist = (px * px + py * py).sqrt();

            if dist > r + edge_w {
                continue;
            }

            let alpha_cov = ((r + edge_w - dist) / (2.0 * edge_w)).clamp(0.0, 1.0);
            let mut final_col = color;

            if stroke_width > 0.0 {
                let sw_norm = (stroke_width * 2.0) / min_dim;
                if dist >= r - sw_norm - edge_w {
                    let stroke_cov = ((dist - (r - sw_norm - edge_w)) / (2.0 * edge_w)).clamp(0.0, 1.0);
                    for c in 0..4 {
                        final_col[c] = color[c] * (1.0 - stroke_cov) + stroke_color[c] * stroke_cov;
                    }
                }
            }

            out.set_pixel_u8(
                x,
                y,
                [
                    f32_to_u8(final_col[0]),
                    f32_to_u8(final_col[1]),
                    f32_to_u8(final_col[2]),
                    f32_to_u8(final_col[3] * alpha_cov),
                ],
            );
        }
    }
    out
}
