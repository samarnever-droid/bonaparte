//! Display-referred grading with linear-light exposure and relative white balance.
use crate::{CpuFrame, ParamValue};
use std::collections::HashMap;

use crate::cpu_reference::par_rows;

pub fn srgb_to_linear(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}
pub fn linear_to_srgb(v: f32) -> f32 {
    if v <= 0.0031308 {
        12.92 * v
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}
fn smooth(a: f32, b: f32, v: f32) -> f32 {
    let t = ((v - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Narkowicz ACES fit: filmic highlight rolloff to 1.0 with no hard clip.
fn aces_fitted(x: f32) -> f32 {
    (x * (2.51 * x + 0.03) / (x * (2.43 * x + 0.59) + 0.14)).clamp(0.0, 1.0)
}

/// A saturated constant-hue color, h in degrees — identical table in the
/// WGSL mirror so split toning stays CPU/GPU parity.
fn hue_rgb(h_deg: f32) -> [f32; 3] {
    let h = (h_deg.rem_euclid(360.0)) / 60.0;
    let sector = h.floor() as i32;
    let f = h - sector as f32;
    let q = 1.0 - f;
    match sector {
        0 => [1.0, f, 0.0],
        1 => [q, 1.0, 0.0],
        2 => [0.0, 1.0, f],
        3 => [0.0, q, 1.0],
        4 => [f, 0.0, 1.0],
        _ => [1.0, 0.0, q],
    }
}

/// Called after manifest-driven type/range/default validation by EffectRegistry.
pub(crate) fn grade(input: &CpuFrame, p: &HashMap<String, ParamValue>) -> CpuFrame {
    let f = |id: &str| match p[id] {
        ParamValue::Float(v) => v,
        _ => unreachable!("validated slider"),
    };
    let exposure = 2.0f32.powf(f("exposure"));
    let temp = f("temperature") * 0.35;
    let tint = f("tint") * 0.25;
    let balance = [
        2.0f32.powf(temp + tint * 0.5),
        2.0f32.powf(-tint),
        2.0f32.powf(-temp + tint * 0.5),
    ];
    let contrast = f("contrast");
    let highlights = f("highlights");
    let shadows = f("shadows");
    let whites = f("whites");
    let blacks = f("blacks");
    let saturation = f("saturation");
    let vibrance = f("vibrance");
    let gamma = f("gamma");
    let fade = f("fade") * 0.25;
    let lift_wheel = f("lift");
    let gain = f("gain");
    let filmic = f("filmic");
    let neutral = [0.5f32, 0.5, 0.5, 1.0];
    let (lift_color, gain_color) = match (p.get("lift_color"), p.get("gain_color")) {
        (Some(ParamValue::Color(l)), Some(ParamValue::Color(g))) => (*l, *g),
        _ => (neutral, neutral),
    };
    let split_shadow_hue = f("split_shadow_hue");
    let split_highlight_hue = f("split_highlight_hue");
    let split_balance = f("split_balance");
    let split_strength = f("split_strength");
    // The linear stage (exposure × balance × gain × filmic) is skippable only
    // when every stage input is the identity; otherwise gain or filmic must
    // still run even with neutral exposure and white balance.
    let neutral_light = exposure == 1.0 && balance == [1.0; 3];
    let skip_linear = neutral_light && gain == 1.0 && filmic == 0.0;
    if neutral_light
        && contrast == 1.0
        && highlights == 0.0
        && shadows == 0.0
        && whites == 0.0
        && blacks == 0.0
        && saturation == 1.0
        && vibrance == 0.0
        && gamma == 1.0
        && fade == 0.0
        && lift_wheel == 0.0
        && gain == 1.0
        && filmic == 0.0
        && split_strength == 0.0
        && lift_color[0] == 0.5
        && lift_color[1] == 0.5
        && lift_color[2] == 0.5
        && gain_color[0] == 0.5
        && gain_color[1] == 0.5
        && gain_color[2] == 0.5
    {
        return input.clone();
    }
    let mut out = input.clone();
    // Consecutive flat colors are common in motion graphics. This tiny local
    // memo avoids repeated tone math without quantization or a frame-sized LUT.
    // The memo only caches results of a pure function for identical inputs, so
    // a per-row memo (needed for row-parallelism) yields byte-identical output.
    let row_len = out.width as usize * 4;
    par_rows(&mut out.rgba, row_len, |_, row| {
        let mut previous: Option<([u8; 3], [u8; 3])> = None;
        for pixel in row.chunks_exact_mut(4) {
            if pixel[3] == 0 {
                continue;
            }
            let original = [pixel[0], pixel[1], pixel[2]];
            if let Some((before, after)) = previous {
                if original == before {
                    pixel[..3].copy_from_slice(&after);
                    continue;
                }
            }
            let mut rgb = [0.0f32; 3];
            for c in 0..3 {
                if skip_linear {
                    rgb[c] = pixel[c] as f32 / 255.0;
                } else {
                    let lin = srgb_to_linear(pixel[c] as f32 / 255.0) * exposure * balance[c];
                    // Gain is a plain linear-light multiplier with a per-
                    // channel chromatic wheel (center gray = neutral, one
                    // stop of swing per channel); filmic blends an ACES-
                    // fitted rolloff so super-whites curve instead of
                    // clipping. Both are exact identities at their defaults.
                    let mut lin = lin * gain * 2f32.powf((gain_color[c] - 0.5) * 2.0);
                    if filmic > 0.0 {
                        lin = lin * (1.0 - filmic) + aces_fitted(lin) * filmic;
                    }
                    rgb[c] = linear_to_srgb(lin).clamp(0.0, 1.0);
                }
            }
            let luma = rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722;
            let lift = shadows * (1.0 - smooth(0.0, 0.65, luma)) * 0.35
                + highlights * smooth(0.35, 1.0, luma) * 0.35
                + blacks * (1.0 - smooth(0.0, 0.3, luma)) * 0.2
                + whites * smooth(0.7, 1.0, luma) * 0.2;
            for (c, v) in rgb.iter_mut().enumerate() {
                *v = ((*v + lift - 0.5) * contrast + 0.5).clamp(0.0, 1.0);
                if gamma != 1.0 {
                    *v = v.powf(1.0 / gamma);
                }
                if lift_wheel != 0.0 {
                    *v = (*v + lift_wheel).clamp(0.0, 1.0);
                }
                *v = (*v + (lift_color[c] - 0.5) * 0.5).clamp(0.0, 1.0);
            }
            let luma = rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722;
            let max = rgb[0].max(rgb[1]).max(rgb[2]);
            let min = rgb[0].min(rgb[1]).min(rgb[2]);
            let chroma = if max > 0.00001 {
                (max - min) / max
            } else {
                0.0
            };
            let sat = saturation * (1.0 + vibrance * (1.0 - chroma));
            let shadow_hue = hue_rgb(split_shadow_hue);
            let highlight_hue = hue_rgb(split_highlight_hue);
            let crossover = (luma + split_balance * 0.5).clamp(0.0, 1.0);
            let w_hi = smooth(0.35, 0.65, crossover);
            for c in 0..3 {
                let mut value = (luma + (rgb[c] - luma) * sat).clamp(0.0, 1.0);
                if split_strength > 0.0 {
                    let tint = shadow_hue[c] * (1.0 - w_hi) + highlight_hue[c] * w_hi;
                    value = (value + tint * split_strength * 0.35).clamp(0.0, 1.0);
                }
                pixel[c] = ((value * (1.0 - fade) + fade) * 255.0).round() as u8;
            }
            previous = Some((original, [pixel[0], pixel[1], pixel[2]]));
        }
    });
    out
}

pub(crate) fn gradient(input: &CpuFrame, start: [f32; 4], end: [f32; 4], angle: f32) -> CpuFrame {
    let rad = angle.to_radians();
    let (s, c) = rad.sin_cos();
    let extent = c.abs() + s.abs();
    let mut out = input.clone();
    let row_len = out.width as usize * 4;
    crate::cpu_reference::par_rows(&mut out.rgba, row_len, |y, row| {
        for x in 0..input.width {
            let u = (x as f32 + 0.5) / input.width as f32 - 0.5;
            let v = (y as f32 + 0.5) / input.height as f32 - 0.5;
            let t = ((u * c + v * s) / extent + 0.5).clamp(0.0, 1.0);
            let i = (x * 4) as usize;
            for ch in 0..3 {
                row[i + ch] = ((start[ch] * (1.0 - t) + end[ch] * t) * 255.0).round() as u8;
            }
            row[i + 3] = (input.rgba[y * row_len + i + 3] as f32
                * (start[3] * (1.0 - t) + end[3] * t))
                .round() as u8;
        }
    });
    out
}
