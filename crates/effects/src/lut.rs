//! .cube LUT baking: turn any effect stack into an industry-standard 3D
//! lookup table. The lattice is baked by running the stack over an RGB grid
//! at the requested resolution (33³ is the DaVinci/Resolve default). The
//! stack's color transform is what a LUT can represent — spatial effects
//! (blur, glow) degrade to per-pixel color ops on the lattice by nature of
//! the format, which the doc on the command surfaces states.

use crate::{CpuFrame, EffectRegistry};
use bonaparte_model::{EffectInstance, Time};

/// Bakes `effects` (in stack order) into a neutral `size³` RGB lattice and
/// serializes it as a Maya/Iridas `.cube` payload. Lattice coordinates are
/// sRGB-encoded (the same convention as Resolve's "sRGB" LUTs), so color
/// effects grade the same values users see on screen. Animated parameters
/// are sampled at `time`.
pub fn export_cube(
    registry: &EffectRegistry,
    effects: &[EffectInstance],
    size: u32,
    time: Time,
) -> Result<String, String> {
    if !(2..=128).contains(&size) {
        return Err("LUT_3D_SIZE must be between 2 and 128".into());
    }
    if effects.is_empty() {
        return Err("The layer has no enabled effects to bake into a LUT".into());
    }
    let n = size as usize;
    let total = n * n * n;
    let mut pixels = Vec::with_capacity(total * 4);
    for b in 0..n {
        for g in 0..n {
            for r in 0..n {
                // .cube lattice order is red fastest; sRGB-encode the
                // normalized lattice coordinates so color effects grade the
                // same values users see on screen.
                let encode = |i: usize| {
                    let v = i as f32 / (n - 1) as f32;
                    crate::grading::linear_to_srgb(v)
                };
                pixels.extend_from_slice(&[
                    (encode(r) * 255.0).round() as u8,
                    (encode(g) * 255.0).round() as u8,
                    (encode(b) * 255.0).round() as u8,
                    255,
                ]);
            }
        }
    }
    let frame = CpuFrame::from_rgba(size, size * size, pixels);
    let mut graded = frame;
    for effect in effects.iter().filter(|e| e.enabled) {
        graded = registry
            .evaluate_instance(effect, &graded, time)
            .map_err(|e| e.to_string())?;
    }
    let expected = (size * size * size) as usize;
    if graded.width as usize * graded.height as usize != expected {
        return Err("Effect stack changed the pixel count; it cannot be baked into a LUT".into());
    }
    let mut out = String::with_capacity(total * 24 + 64);
    out.push_str("TITLE \"Bonaparte baked grade\"\n");
    out.push_str(&format!("LUT_3D_SIZE {size}\n"));
    out.push_str("DOMAIN_MIN 0.0 0.0 0.0\nDOMAIN_MAX 1.0 1.0 1.0\n");
    for px in graded.rgba.chunks_exact(4) {
        out.push_str(&format!(
            "{:.6} {:.6} {:.6}\n",
            px[0] as f32 / 255.0,
            px[1] as f32 / 255.0,
            px[2] as f32 / 255.0
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bonaparte_model::EffectValue;

    fn exposure_effect(stops: f32) -> EffectInstance {
        let mut exposure = EffectInstance::new("lut", "builtin.color_grade");
        exposure
            .params
            .insert("exposure".into(), EffectValue::Float(stops));
        exposure
    }

    #[test]
    fn a_baked_lut_reproduces_the_grades_corners_and_midtones() {
        let registry = EffectRegistry::new();
        let exposure = exposure_effect(1.0);
        let cube = export_cube(&registry, &[exposure], 33, Time::ZERO).unwrap();
        assert!(cube.starts_with("TITLE \"Bonaparte baked grade\"\nLUT_3D_SIZE 33\n"));
        // Line count: 4 header lines (TITLE, SIZE, two DOMAINs) + 33³ data.
        assert_eq!(cube.lines().count(), 4 + 33 * 33 * 33);
        // Deterministic byte-for-byte.
        let exposure2 = exposure_effect(1.0);
        assert_eq!(
            cube,
            export_cube(&registry, &[exposure2], 33, Time::ZERO).unwrap()
        );

        // Mid-gray through +1 EV: sRGB(2·linear(0.5 srgb)) > 0.5 srgb. The
        // lattice's center entry (16,16,16) must show that brightening.
        let data: Vec<[f32; 3]> = cube
            .lines()
            .skip(4)
            .map(|l| {
                let mut it = l.split_whitespace().map(|v| v.parse::<f32>().unwrap());
                [it.next().unwrap(), it.next().unwrap(), it.next().unwrap()]
            })
            .collect();
        let mid = data[(16 * 33 * 33) + (16 * 33) + 16];
        for channel in mid {
            assert!(channel > 0.5 && channel <= 1.0, "mid gray {mid:?}");
        }
        // Black stays black, white stays white under pure exposure.
        assert!(data[0].iter().all(|v| *v < 0.001));
        assert!(data[data.len() - 1].iter().all(|v| *v > 0.999));
    }

    #[test]
    fn empty_stacks_and_bad_sizes_are_refused() {
        let registry = EffectRegistry::new();
        assert!(export_cube(&registry, &[], 33, Time::ZERO).is_err());
        assert!(export_cube(&registry, &[exposure_effect(0.5)], 1, Time::ZERO).is_err());
        assert!(export_cube(&registry, &[exposure_effect(0.5)], 999, Time::ZERO).is_err());
    }
}
