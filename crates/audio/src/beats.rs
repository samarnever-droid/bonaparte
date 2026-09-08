//! Beat detection: rhythm analysis for Beat Cut.
//!
//! Input is an onset envelope (RMS energy per time bucket, see
//! `DecodedAudio::onset_envelope`). Everything here is pure DSP — no
//! dependencies, no ML, fully deterministic: adaptive-threshold onset
//! picking, autocorrelation tempo estimation, phase-locked beat grid,
//! and downbeat (strong-beat) classification. This is what lets the
//! editor cut an entire video to a music track in one click.

/// A detected rhythm: tempo, beat times, and which beats are downbeats.
#[derive(Debug, Clone, PartialEq)]
pub struct BeatGrid {
    /// Estimated tempo in beats per minute (60–190).
    pub bpm: f64,
    /// Beat times in milliseconds, ascending, phase-locked to the onsets.
    pub beats_ms: Vec<f64>,
    /// Parallel to `beats_ms`: true on downbeats (the "1" of each bar).
    pub strong: Vec<bool>,
}

impl BeatGrid {
    /// Beat spacing in milliseconds (from the detected tempo).
    pub fn period_ms(&self) -> f64 {
        60_000.0 / self.bpm
    }
}

/// Default envelope resolution used across the beat pipeline.
pub const BUCKETS_PER_SEC: u32 = 200;

/// Detect the beat grid of an onset envelope.
///
/// `envelope` holds one RMS value per `buckets_per_sec` bucket. Fails
/// with a human error when the clip is too short or has no rhythmic
/// onsets — Beat Cut then tells the user instead of inventing a grid.
pub fn detect(envelope: &[f32], buckets_per_sec: u32) -> Result<BeatGrid, String> {
    let bps = buckets_per_sec.max(50) as f64;
    let seconds = envelope.len() as f64 / bps;
    if seconds < 4.0 {
        return Err("Beat detection needs at least 4 seconds of audio".into());
    }

    // 1) Onset strength: positive change of a short-window local average —
    //    robust to sustained loudness, fires on energy jumps (drums, cuts).
    let win = (bps / 5.0).round().max(1.0) as usize; // ~200 ms
    let mut strength = vec![0.0f32; envelope.len()];
    let mut rolling = 0.0f64;
    for i in 0..envelope.len() {
        let e = envelope[i] as f64;
        strength[i] = (e - rolling / win as f64).max(0.0) as f32;
        rolling += e;
        if i >= win {
            rolling -= envelope[i - win] as f64;
        }
    }
    let mean = strength.iter().sum::<f32>() / strength.len() as f32;
    let var = strength
        .iter()
        .map(|s| {
            let d = s - mean;
            d * d
        })
        .sum::<f32>()
        / strength.len() as f32;
    let floor = mean + 0.6 * var.sqrt();

    // 2) Peak picking with a 100 ms refractory gap.
    let gap = (bps / 10.0).round() as usize;
    let mut peaks: Vec<usize> = Vec::new();
    for i in 1..strength.len().saturating_sub(1) {
        if strength[i] < floor
            || strength[i] < strength[i - 1]
            || strength[i] < strength[i + 1].min(f32::MAX)
        {
            continue;
        }
        // Local max over ±1 plus refractory: keep the strongest in the gap.
        if let Some(&last) = peaks.last() {
            if i - last < gap {
                if strength[i] > strength[last] {
                    *peaks.last_mut().unwrap() = i;
                }
                continue;
            }
        }
        peaks.push(i);
    }
    if peaks.len() < 4 {
        return Err("No rhythmic onsets found in this audio".into());
    }

    // 3) Tempo: autocorrelation of the onset strength over 60–190 BPM.
    let min_lag = ((bps * 60.0) / 190.0).round() as usize;
    let max_lag = ((bps * 60.0) / 60.0).round() as usize;
    let mut best_lag = 0usize;
    let mut best_score = -1.0f64;
    for lag in min_lag..=max_lag.min(strength.len() - 1) {
        let mut sum = 0.0f64;
        let mut n = 0usize;
        for i in 0..strength.len() - lag {
            sum += strength[i] as f64 * strength[i + lag] as f64;
            n += 1;
        }
        let score = sum / n as f64;
        if score > best_score {
            best_score = score;
            best_lag = lag;
        }
    }
    if best_lag == 0 {
        return Err("Could not estimate a tempo".into());
    }
    // Fold the octave: a perfectly periodic train scores equally at 2× the
    // beat period (the measure level). Whenever the half-lag scores nearly
    // as well, prefer the faster, beat-level tempo.
    let lag_score = |lag: usize| -> f64 {
        let mut sum = 0.0f64;
        let mut n = 0usize;
        for i in 0..strength.len() - lag {
            sum += strength[i] as f64 * strength[i + lag] as f64;
            n += 1;
        }
        sum / n as f64
    };
    let top = lag_score(best_lag);
    while best_lag / 2 >= min_lag && lag_score(best_lag / 2) >= 0.85 * top {
        best_lag /= 2;
    }
    let period_buckets = best_lag as f64;
    let bpm = 60.0 * bps / period_buckets;

    // 4) Phase: slide one period of the grid over the peaks, keep the
    //    offset whose grid points land on the most onset energy.
    let period = period_buckets;
    let mut best_offset = 0.0f64;
    let mut best_energy = -1.0f64;
    let steps = period.ceil() as usize;
    for s in 0..steps {
        let offset = s as f64;
        let mut energy = 0.0f64;
        let mut t = offset;
        while (t as usize) < strength.len() {
            let idx = t.round() as usize;
            if idx < strength.len() {
                energy += strength[idx] as f64;
            }
            t += period;
        }
        if energy > best_energy {
            best_energy = energy;
            best_offset = offset;
        }
    }

    // 5) Emit the grid and mark downbeats: the strongest peak anchors bar 1.
    let mut beats_ms = Vec::new();
    let mut t = best_offset;
    while (t as usize) < strength.len() {
        beats_ms.push(t / bps * 1000.0);
        t += period;
    }
    // Strongest onset anchors the meter; exact ties go to the EARLIEST peak
    // (a uniform click train must anchor at its first hit, not its last).
    let mut anchor = peaks[0];
    let mut best = strength[peaks[0]];
    for &p in peaks.iter().skip(1) {
        if strength[p] > best {
            best = strength[p];
            anchor = p;
        }
    }
    let anchor_ms = anchor as f64 / bps * 1000.0;
    let anchor_beat = beats_ms
        .iter()
        .enumerate()
        .min_by(|a, b| {
            let da = (a.1 - anchor_ms).abs();
            let db = (b.1 - anchor_ms).abs();
            da.total_cmp(&db)
        })
        .map(|(i, _)| i)
        .unwrap_or(0);
    // Bars tile in 4/4 from the anchor beat.
    let strong = beats_ms
        .iter()
        .enumerate()
        .map(|(i, _)| i.abs_diff(anchor_beat) % 4 == 0)
        .collect();

    Ok(BeatGrid {
        bpm,
        beats_ms,
        strong,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthesize an onset envelope: rhythmic energy spikes at `bpm`.
    fn click_envelope(bpm: f64, seconds: f64, bps: u32) -> Vec<f32> {
        let n = (seconds * bps as f64) as usize;
        let mut env = vec![0.02f32; n]; // quiet bed
        let period = 60.0 * bps as f64 / bpm;
        let mut t = 0.0f64;
        while (t as usize) < n {
            let idx = t.round() as usize;
            for d in 0..3 {
                if idx + d < n {
                    env[idx + d] = 1.0 - d as f32 * 0.25;
                }
            }
            t += period;
        }
        env
    }

    #[test]
    fn finds_120_bpm_with_downbeats() {
        let env = click_envelope(120.0, 12.0, BUCKETS_PER_SEC);
        let grid = detect(&env, BUCKETS_PER_SEC).unwrap();
        assert!(
            (115.0..=125.0).contains(&grid.bpm),
            "bpm {} off target",
            grid.bpm
        );
        assert!(grid.beats_ms.len() >= 22, "should find most of 24 beats");
        let period = grid.period_ms();
        assert!((period - 500.0).abs() < 30.0, "period {period}");
        // Beats align with the synthesized click positions.
        for (i, b) in grid.beats_ms.iter().enumerate() {
            let nearest = (b / period).round() * period;
            assert!(
                (b - nearest).abs() < 40.0,
                "beat {i} at {b} misses the grid"
            );
        }
        // Downbeats are spaced exactly 4 apart, 4/4 tiling from the anchor.
        let strong_idx: Vec<usize> = grid
            .strong
            .iter()
            .enumerate()
            .filter(|(_, s)| **s)
            .map(|(i, _)| i)
            .collect();
        assert!(strong_idx.len() >= 5);
        for w in strong_idx.windows(2) {
            assert_eq!(w[1] - w[0], 4, "downbeats every 4th beat");
        }
    }

    #[test]
    fn finds_slow_90_bpm() {
        let env = click_envelope(90.0, 16.0, BUCKETS_PER_SEC);
        let grid = detect(&env, BUCKETS_PER_SEC).unwrap();
        assert!(
            (86.0..=94.0).contains(&grid.bpm),
            "bpm {} off target",
            grid.bpm
        );
        assert!((grid.period_ms() - 666.7).abs() < 45.0);
    }

    #[test]
    fn rejects_non_rhythmic_audio() {
        // Flat bed with no onsets — detection must say so, not invent cuts.
        let env = vec![0.3f32; BUCKETS_PER_SEC as usize * 8];
        assert!(detect(&env, BUCKETS_PER_SEC).is_err());
        // Too short is a clean error too.
        assert!(detect(
            &click_envelope(120.0, 2.0, BUCKETS_PER_SEC),
            BUCKETS_PER_SEC
        )
        .is_err());
    }
}
