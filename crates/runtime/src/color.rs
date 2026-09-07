//! Deep-color export: beyond 8-bit sRGB. The editor canvas and preview stay
//! 8-bit sRGB for speed; exports can request 16 bits per channel and any
//! first-party output space. The master frame is sRGB-encoded; exports
//! linearize it, optionally convert primaries, then re-encode with the
//! sRGB transfer (PQ/HDR transfer functions are a later milestone).

use serde::Deserialize;

/// Output color spaces. `DisplayP3` and `Rec2020` keep the sRGB transfer
/// function but re-map primaries (wide gamut); `Linear` removes the transfer
/// entirely for downstream compositing/grading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OutputSpace {
    #[default]
    Srgb,
    DisplayP3,
    Rec2020,
    Linear,
}

/// Linear-light sRGB → Display P3 (D65, Bradford-adapted primaries).
const SRGB_TO_P3: [[f32; 3]; 3] = [
    [0.822_462_1, 0.177_538_0, 0.000_000_0],
    [0.033_194_1, 0.966_805_8, 0.000_000_0],
    [0.017_082_7, 0.072_397_4, 0.910_519_9],
];

/// Linear-light sRGB → Rec. 2020.
const SRGB_TO_REC2020: [[f32; 3]; 3] = [
    [0.627_402_0, 0.329_292_0, 0.043_306_0],
    [0.069_096_0, 0.929_546_0, 0.001_358_0],
    [0.016_391_0, 0.088_013_0, 0.895_596_0],
];

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

fn apply_matrix(m: &[[f32; 3]; 3], px: [f32; 3]) -> [f32; 3] {
    let mut out = [0.0f32; 3];
    for r in 0..3 {
        out[r] = m[r][0] * px[0] + m[r][1] * px[1] + m[r][2] * px[2];
    }
    out
}

/// Inverse primaries, exported for tests and round-trip verification.
pub fn p3_to_srgb() -> [[f32; 3]; 3] {
    invert(&SRGB_TO_P3)
}
pub fn rec2020_to_srgb() -> [[f32; 3]; 3] {
    invert(&SRGB_TO_REC2020)
}

fn invert(m: &[[f32; 3]; 3]) -> [[f32; 3]; 3] {
    fn det3(m: &[[f32; 3]; 3]) -> f32 {
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    }
    let d = det3(m);
    let mut inv = [[0.0f32; 3]; 3];
    for r in 0..3 {
        for c in 0..3 {
            let a = m[(r + 1) % 3][(c + 1) % 3];
            let b = m[(r + 1) % 3][(c + 2) % 3];
            let cc = m[(r + 2) % 3][(c + 1) % 3];
            let e = m[(r + 2) % 3][(c + 2) % 3];
            inv[c][r] = (a * e - b * cc) / d;
        }
    }
    inv
}

/// Convert sRGB-encoded RGBA8 pixels into `space`-encoded samples.
/// Returns big-endian bytes per sample (PNG sample order) at the requested
/// bit depth: 4 bytes/pixel at 8-bit, 8 bytes/pixel at 16-bit.
pub fn convert_rgba(
    width: u32,
    height: u32,
    rgba8: &[u8],
    space: OutputSpace,
    bit_depth: u8,
) -> Result<Vec<u8>, String> {
    if width as usize * height as usize * 4 != rgba8.len() {
        return Err("Pixel buffer does not match canvas dimensions".into());
    }
    if bit_depth != 8 && bit_depth != 16 {
        return Err("PNG export supports 8 or 16 bits per channel".into());
    }
    let max = if bit_depth == 16 { 65535.0f32 } else { 255.0 };
    let mut out = Vec::with_capacity(rgba8.len() * if bit_depth == 16 { 2 } else { 1 });
    for px in rgba8.chunks_exact(4) {
        let mut lin = [
            srgb_to_linear(px[0] as f32 / 255.0),
            srgb_to_linear(px[1] as f32 / 255.0),
            srgb_to_linear(px[2] as f32 / 255.0),
        ];
        lin = match space {
            OutputSpace::Srgb | OutputSpace::Linear => lin,
            OutputSpace::DisplayP3 => apply_matrix(&SRGB_TO_P3, lin),
            OutputSpace::Rec2020 => apply_matrix(&SRGB_TO_REC2020, lin),
        };
        for channel in lin {
            let sample = if space == OutputSpace::Linear {
                channel
            } else {
                linear_to_srgb(channel)
            }
            .clamp(0.0, 1.0);
            let value = (sample * max).round();
            // clamp(0..=1) above bounds value to max; round cannot exceed it.
            let value = value.min(max);
            if bit_depth == 16 {
                let v = (value as u16).to_be_bytes();
                out.extend_from_slice(&v);
            } else {
                out.push(value as u8);
            }
        }
        let alpha = (px[3] as f32 / 255.0 * max).round().min(max);
        if bit_depth == 16 {
            out.extend_from_slice(&(alpha as u16).to_be_bytes());
        } else {
            out.push(alpha as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_matrices_round_trip() {
        for (name, forward, inverse) in [
            ("display-p3", &SRGB_TO_P3, &p3_to_srgb()),
            ("rec2020", &SRGB_TO_REC2020, &rec2020_to_srgb()),
        ] {
            for r in 0..3 {
                for k in 0..3 {
                    let product: f32 = (0..3).map(|c| forward[r][c] * inverse[c][k]).sum();
                    let expected = if r == k { 1.0 } else { 0.0 };
                    assert!(
                        (product - expected).abs() < 1e-3,
                        "{name} matrix round trip failed at ({r},{k}): {product}"
                    );
                }
            }
        }
    }

    #[test]
    fn pure_red_survives_every_space_and_depth() {
        let red = [255, 0, 0, 255];
        for space in [
            OutputSpace::Srgb,
            OutputSpace::DisplayP3,
            OutputSpace::Rec2020,
            OutputSpace::Linear,
        ] {
            let out = convert_rgba(1, 1, &red, space, 8).unwrap();
            assert_eq!(out.len(), 4);
            assert_eq!(out[3], 255, "alpha must be untouched");
        }
        // sRGB 16-bit pure red is exactly full scale in every channel store.
        let out = convert_rgba(1, 1, &red, OutputSpace::Srgb, 16).unwrap();
        assert_eq!(&out[..2], &[255 as u8, 255]);
        assert_eq!(&out[2..4], &[0, 0]);
    }

    #[test]
    fn sixteen_bit_linear_carries_information_eight_bit_cannot() {
        // sRGB 8 is ~0.00243 linear; the nearest 8-bit linear step is 40%
        // brighter, while 16-bit represents it to a part in 65535.
        let px = [8u8, 8, 8, 255];
        let linear16 = convert_rgba(1, 1, &px, OutputSpace::Linear, 16).unwrap();
        let sample = u16::from_be_bytes([linear16[0], linear16[1]]);
        let linear = sample as f32 / 65535.0;
        let expected = srgb_to_linear(8.0 / 255.0);
        assert!((linear - expected).abs() < 1.0 / 65535.0);
        // 8-bit linear would quantize that to 8 (0.0314): a 5% error.
        let linear8 = convert_rgba(1, 1, &px, OutputSpace::Linear, 8).unwrap();
        let error8 = (linear8[0] as f32 / 255.0 - expected).abs();
        assert!(error8 > 0.001, "8-bit should visibly quantize here");
    }

    #[test]
    fn wide_gamut_moves_primaries_not_neutrals() {
        // A neutral gray stays neutral in every space.
        let gray = [128u8, 128, 128, 255];
        for space in [OutputSpace::DisplayP3, OutputSpace::Rec2020] {
            let out = convert_rgba(1, 1, &gray, space, 16).unwrap();
            let r = u16::from_be_bytes([out[0], out[1]]);
            let g = u16::from_be_bytes([out[2], out[3]]);
            let b = u16::from_be_bytes([out[4], out[5]]);
            assert!((r as i32 - g as i32).abs() <= 1, "{space:?} gray R≠G");
            assert!((g as i32 - b as i32).abs() <= 1, "{space:?} gray G≠B");
        }
        // And saturated red lands inside the target gamut with distinct channels.
        let red = convert_rgba(1, 1, &[255, 0, 0, 255], OutputSpace::Rec2020, 16).unwrap();
        let g = u16::from_be_bytes([red[2], red[3]]);
        assert!(g > 0, "rec2020 red should carry a green term");
    }
}
