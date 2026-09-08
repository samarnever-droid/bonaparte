//! Procedural sound design: synthesized impact/whoosh/riser one-shots.
//!
//! The Foley arm of Kinetic ⚡ — editors drop a beat grid on the timeline
//! and these fills land on the downbeats without touching a sample pack.
//! Everything is pure, seeded, deterministic DSP at the engine's 48 kHz;
//! the same input bytes always produce the same audio.

/// Deterministic xorshift64* — no external RNG, stable across platforms.
struct Rng(u64);
impl Rng {
    fn next_f32(&mut self) -> f32 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        let x = self.0.wrapping_mul(0x2545_F491_4F6C_DD1D);
        ((x >> 40) as f32 / (1u32 << 24) as f32) * 2.0 - 1.0
    }
}

/// Cinematic impact: pitch-dropping body + noise transient, 0.45 s.
pub fn impact(rate: u32) -> Vec<f32> {
    let n = rate as usize * 45 / 100;
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / rate as f32;
        // Body: exponential sweep 95 → 42 Hz with exp decay.
        let sweep = 42.0 + 53.0 * (-t * 14.0).exp();
        let phase = 2.0 * std::f32::consts::PI * sweep * t;
        let body = phase.sin() * (-t * 9.5).exp();
        // Transient: the first 8 ms are filtered noise.
        let transient = if t < 0.008 {
            rng.next_f32() * (1.0 - t / 0.008)
        } else {
            0.0
        };
        let s = body * 0.92 + transient * 0.5;
        out.push(s.clamp(-1.0, 1.0).tanh()); // soft clip keeps it fat, not digital
    }
    out
}

/// Transition whoosh: band-shaped noise swell, 0.30 s.
pub fn whoosh(rate: u32) -> Vec<f32> {
    let n = rate as usize * 3 / 10;
    let mut rng = Rng(0xC0FF_EE12_3456_789A);
    let mut out = Vec::with_capacity(n);
    let mut lp = 0.0f32;
    for i in 0..n {
        let t = i as f32 / n as f32; // 0..1
        let noise = rng.next_f32();
        // Rising one-pole cutoff: air opens as the whoosh flies by.
        let cutoff = 0.04 + 0.42 * t * t;
        lp += cutoff * (noise - lp);
        // Bell envelope: swell in, duck out — the classic whoosh shape.
        let bell = (std::f32::consts::PI * t).sin().powi(2);
        out.push((lp * 2.4 * bell).clamp(-1.0, 1.0));
    }
    out
}

/// Riser into a drop: rising filtered noise with a hard stop, 1.6 s.
/// Place it so it ends exactly ON the drop/downbeat.
pub fn riser(rate: u32) -> Vec<f32> {
    let n = rate as usize * 8 / 5;
    let mut rng = Rng(0xDEAD_BEEF_CAFE_0001);
    let mut out = Vec::with_capacity(n);
    let mut lp = 0.0f32;
    let mut bp = 0.0f32;
    for i in 0..n {
        let t = i as f32 / n as f32;
        let noise = rng.next_f32();
        lp += (0.02 + 0.5 * t * t) * (noise - lp); // opens up
        bp = bp + 0.3 * (lp - bp); // band shape for tension
        let amp = t * t; // exponential-feel rise
        out.push((bp * 3.2 * amp).clamp(-1.0, 1.0));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|s| s * s).sum::<f32>() / x.len().max(1) as f32).sqrt()
    }

    #[test]
    fn shots_have_right_length_and_stay_bounded() {
        for (name, x) in [
            ("impact", impact(48_000)),
            ("whoosh", whoosh(48_000)),
            ("riser", riser(48_000)),
        ] {
            assert!(!x.is_empty(), "{name} empty");
            assert!(
                x.iter().all(|s| s.is_finite() && s.abs() <= 1.0),
                "{name} unbounded"
            );
        }
        assert_eq!(impact(48_000).len(), 48_000 * 45 / 100);
        assert_eq!(whoosh(48_000).len(), 48_000 * 3 / 10);
        assert_eq!(riser(48_000).len(), 48_000 * 8 / 5);
    }

    #[test]
    fn impact_decays_and_hits_hard() {
        let x = impact(48_000);
        let head = rms(&x[..8000]);
        let tail = rms(&x[x.len() - 8000..]);
        assert!(head > 0.08, "impact must start loud: {head}");
        assert!(tail < 0.05, "impact must decay: {tail}");
        assert!(head > tail * 4.0);
    }

    #[test]
    fn whoosh_swells_and_ducks() {
        let x = whoosh(48_000);
        let q = x.len() / 4;
        let a = rms(&x[..q]);
        let mid = rms(&x[q..3 * q]);
        let z = rms(&x[3 * q..]);
        assert!(mid > a * 1.4, "whoosh swells: {mid} vs {a}");
        assert!(mid > z * 1.4, "whoosh ducks out: {mid} vs {z}");
    }

    #[test]
    fn riser_builds_and_is_deterministic() {
        let a = riser(48_000);
        let b = riser(48_000);
        assert_eq!(a, b, "synth must be deterministic");
        let q = a.len() / 4;
        let start = rms(&a[..q]);
        let end = rms(&a[3 * q..]);
        assert!(end > start * 2.5, "riser builds: {end} vs {start}");
    }
}
