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
        Self {
            width,
            height,
            rgba,
        }
    }

    pub fn from_rgba(width: u32, height: u32, rgba: Vec<u8>) -> Self {
        assert_eq!(rgba.len(), (width * height * 4) as usize);
        Self {
            width,
            height,
            rgba,
        }
    }

    #[inline]
    pub fn get_pixel_u8(&self, x: u32, y: u32) -> [u8; 4] {
        if x >= self.width || y >= self.height {
            return [0, 0, 0, 0];
        }
        let i = ((y * self.width + x) * 4) as usize;
        [
            self.rgba[i],
            self.rgba[i + 1],
            self.rgba[i + 2],
            self.rgba[i + 3],
        ]
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
    #[error("Effect {0} has no CPU evaluator; GPU execution is not available in this host")]
    UnsupportedBackend(String),
}

pub use bonaparte_model::EffectValue as ParamValue;

fn f32_to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Alpha-aware Gaussian glow. Halos contribute alpha outside the original silhouette.
pub fn evaluate_glow(input: &CpuFrame, radius: f32, intensity: f32, tint: [f32; 4]) -> CpuFrame {
    if intensity <= 0.0 {
        return input.clone();
    }
    let halo = evaluate_blur(input, radius, false);
    let mut out = input.clone();
    for (i, pixel) in out.rgba.chunks_exact_mut(4).enumerate() {
        let src = &input.rgba[i * 4..i * 4 + 4];
        let glow = &halo.rgba[i * 4..i * 4 + 4];
        let sa = src[3] as f32 / 255.0;
        let ha = (glow[3] as f32 / 255.0 * intensity * tint[3]).clamp(0.0, 1.0);
        let alpha = sa + ha * (1.0 - sa);
        for ch in 0..3 {
            let value = if alpha > 0.0 {
                (src[ch] as f32 / 255.0 * sa + glow[ch] as f32 / 255.0 * tint[ch] * ha) / alpha
            } else {
                0.0
            };
            pixel[ch] = f32_to_u8(value);
        }
        pixel[3] = f32_to_u8(alpha);
    }
    out
}

/// Separable convolution: O(width × height × radius), rather than radius².
/// Filters premultiplied RGB/alpha and unpremultiplies only at the output edge.
pub fn evaluate_blur(input: &CpuFrame, radius: f32, repeat_edge: bool) -> CpuFrame {
    if radius <= 0.0 || input.width == 0 || input.height == 0 {
        return input.clone();
    }
    let radius = radius.min(100.0);
    let r = radius.ceil() as i32;
    let sigma = (radius * 0.5).max(0.5);
    let mut kernel: Vec<f32> = (-r..=r)
        .map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp())
        .collect();
    let sum: f32 = kernel.iter().sum();
    for weight in &mut kernel {
        *weight /= sum;
    }
    let w = input.width as i32;
    let h = input.height as i32;
    let mut horizontal = vec![[0.0f32; 4]; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0.0; 4];
            for (k, weight) in kernel.iter().enumerate() {
                let sx = x + k as i32 - r;
                if !repeat_edge && (sx < 0 || sx >= w) {
                    continue;
                }
                let i = ((y * w + sx.clamp(0, w - 1)) * 4) as usize;
                let alpha = input.rgba[i + 3] as f32 / 255.0;
                for ch in 0..3 {
                    acc[ch] += input.rgba[i + ch] as f32 / 255.0 * alpha * weight;
                }
                acc[3] += alpha * weight;
            }
            horizontal[(y * w + x) as usize] = acc;
        }
    }
    let mut out = CpuFrame::new(input.width, input.height);
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0.0; 4];
            for (k, weight) in kernel.iter().enumerate() {
                let sy = y + k as i32 - r;
                if !repeat_edge && (sy < 0 || sy >= h) {
                    continue;
                }
                let pixel = horizontal[(sy.clamp(0, h - 1) * w + x) as usize];
                for ch in 0..4 {
                    acc[ch] += pixel[ch] * weight;
                }
            }
            let i = ((y * w + x) * 4) as usize;
            if acc[3] > 0.000001 {
                for ch in 0..3 {
                    out.rgba[i + ch] = f32_to_u8(acc[ch] / acc[3]);
                }
            }
            out.rgba[i + 3] = f32_to_u8(acc[3]);
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
    let blurred = evaluate_blur(input, radius, false);
    let mut out = input.clone();
    for y in 0..input.height {
        for x in 0..input.width {
            let uv = [
                (x as f32 + 0.5 - offset_x) / input.width as f32,
                (y as f32 + 0.5 - offset_y) / input.height as f32,
            ];
            let shadow = blurred.sample_uv_bilinear(uv[0], uv[1], false)[3] * color[3] * opacity;
            let i = ((y * input.width + x) * 4) as usize;
            let alpha = input.rgba[i + 3] as f32 / 255.0;
            let out_alpha = alpha + shadow * (1.0 - alpha);
            for ch in 0..3 {
                let v = if out_alpha > 0.0 {
                    (input.rgba[i + ch] as f32 / 255.0 * alpha + color[ch] * shadow * (1.0 - alpha))
                        / out_alpha
                } else {
                    0.0
                };
                out.rgba[i + ch] = f32_to_u8(v);
            }
            out.rgba[i + 3] = f32_to_u8(out_alpha);
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

pub fn evaluate_chromatic_aberration(input: &CpuFrame, amount: f32, angle: f32) -> CpuFrame {
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

/// Evaluate any registered built-in through the same public registry path as plugins.
pub fn evaluate_effect(
    effect_id: &str,
    input: &CpuFrame,
    params: &HashMap<String, ParamValue>,
) -> Result<CpuFrame, CpuEvalError> {
    crate::registry::builtin_registry().evaluate(effect_id, input, params)
}

/// Algorithm dispatch is confined to the first-party plugin pack, never the engine.
/// The registry injects defaults and validates every field before this callback.
pub(crate) fn evaluate_builtin(
    effect_id: &str,
    input: &CpuFrame,
    p: &HashMap<String, ParamValue>,
) -> Result<CpuFrame, CpuEvalError> {
    let f = |id: &str| match p[id] {
        ParamValue::Float(v) => v,
        _ => unreachable!("validated slider"),
    };
    let c = |id: &str| match p[id] {
        ParamValue::Color(v) => v,
        _ => unreachable!("validated color"),
    };
    let b = |id: &str| match p[id] {
        ParamValue::Bool(v) => v,
        _ => unreachable!("validated checkbox"),
    };
    Ok(match effect_id {
        "builtin.glow" => evaluate_glow(input, f("radius"), f("intensity"), c("tint")),
        "builtin.blur" => evaluate_blur(input, f("radius"), b("repeat_edge")),
        "builtin.drop_shadow" => evaluate_drop_shadow(
            input,
            f("offset_x"),
            f("offset_y"),
            f("radius"),
            c("color"),
            f("opacity"),
        ),
        "builtin.color_adjust" => evaluate_color_adjust(
            input,
            f("brightness"),
            f("contrast"),
            f("saturation"),
            f("hue_shift"),
        ),
        "builtin.transform" => evaluate_transform(
            input,
            f("offset_x"),
            f("offset_y"),
            f("scale_x"),
            f("scale_y"),
            f("rotation"),
        ),
        "builtin.vignette" => evaluate_vignette(
            input,
            f("radius"),
            f("softness"),
            f("intensity"),
            c("color"),
        ),
        "builtin.chromatic_aberration" => {
            evaluate_chromatic_aberration(input, f("amount"), f("angle"))
        }
        "builtin.invert" => evaluate_invert(input, f("amount"), b("invert_alpha")),
        "builtin.tint" => evaluate_tint(input, c("black_color"), c("white_color"), f("amount")),
        "builtin.directional_blur" => evaluate_directional_blur(input, f("length"), f("angle")),
        "builtin.circle" => evaluate_circle(
            input.width,
            input.height,
            c("color"),
            f("radius"),
            c("stroke_color"),
            f("stroke_width"),
        ),
        "builtin.color_grade" => crate::grading::grade(input, p),
        "builtin.gradient" => {
            crate::grading::gradient(input, c("start_color"), c("end_color"), f("angle"))
        }
        _ => return Err(CpuEvalError::UnknownEffect(effect_id.into())),
    })
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
                    let stroke_cov =
                        ((dist - (r - sw_norm - edge_w)) / (2.0 * edge_w)).clamp(0.0, 1.0);
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
