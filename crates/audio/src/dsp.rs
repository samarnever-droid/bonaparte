//! Stereo-linked native channel strips. No host/browser DSP substitutes.
use bonaparte_model::{AudioLimiter, AudioProcessing};
use std::collections::VecDeque;
#[derive(Clone)]
struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
    z: [[f64; 2]; 2],
}
impl Biquad {
    fn new(b: [f64; 3], a: [f64; 3]) -> Self {
        Self {
            b: b.map(|x| x / a[0]),
            a: [a[1] / a[0], a[2] / a[0]],
            z: [[0.0; 2]; 2],
        }
    }
    fn process(&mut self, x: [f64; 2]) -> [f64; 2] {
        let mut out = [0.0; 2];
        for c in 0..2 {
            let y = self.b[0] * x[c] + self.z[c][0];
            self.z[c][0] = self.b[1] * x[c] - self.a[0] * y + self.z[c][1];
            self.z[c][1] = self.b[2] * x[c] - self.a[1] * y;
            for z in &mut self.z[c] {
                if z.abs() < 1e-24 {
                    *z = 0.0;
                }
            }
            out[c] = y;
        }
        out
    }
}
fn trig(hz: f64, rate: u32) -> (f64, f64) {
    let w = 2.0 * std::f64::consts::PI * hz.min(rate as f64 * 0.45).max(1.0) / rate as f64;
    (w.cos(), w.sin())
}
fn highpass(hz: f64, rate: u32) -> Biquad {
    let (c, s) = trig(hz, rate);
    let a = s / (2.0 * 0.7071067811865476);
    Biquad::new(
        [(1.0 + c) / 2.0, -(1.0 + c), (1.0 + c) / 2.0],
        [1.0 + a, -2.0 * c, 1.0 - a],
    )
}
fn peak(hz: f64, db: f64, q: f64, rate: u32) -> Biquad {
    let (c, s) = trig(hz, rate);
    let a = 10.0f64.powf(db / 40.0);
    let alpha = s / (2.0 * q);
    Biquad::new(
        [1.0 + alpha * a, -2.0 * c, 1.0 - alpha * a],
        [1.0 + alpha / a, -2.0 * c, 1.0 - alpha / a],
    )
}
fn shelf(hz: f64, db: f64, high: bool, rate: u32) -> Biquad {
    let (c, s) = trig(hz, rate);
    let a = 10.0f64.powf(db / 40.0);
    let beta = s * 2.0f64.sqrt() * a.sqrt();
    if high {
        Biquad::new(
            [
                a * ((a + 1.0) + (a - 1.0) * c + beta),
                -2.0 * a * ((a - 1.0) + (a + 1.0) * c),
                a * ((a + 1.0) + (a - 1.0) * c - beta),
            ],
            [
                (a + 1.0) - (a - 1.0) * c + beta,
                2.0 * ((a - 1.0) - (a + 1.0) * c),
                (a + 1.0) - (a - 1.0) * c - beta,
            ],
        )
    } else {
        Biquad::new(
            [
                a * ((a + 1.0) - (a - 1.0) * c + beta),
                2.0 * a * ((a - 1.0) - (a + 1.0) * c),
                a * ((a + 1.0) - (a - 1.0) * c - beta),
            ],
            [
                (a + 1.0) + (a - 1.0) * c + beta,
                -2.0 * ((a - 1.0) + (a + 1.0) * c),
                (a + 1.0) + (a - 1.0) * c - beta,
            ],
        )
    }
}
#[derive(Clone)]
pub(crate) struct ChannelState {
    filters: Vec<Biquad>,
    settings: AudioProcessing,
    attack: f64,
    release: f64,
    pub reduction: f64,
}
impl ChannelState {
    pub fn new(settings: &AudioProcessing, rate: u32) -> Self {
        let mut filters = vec![];
        let e = &settings.eq;
        if e.enabled {
            if e.highpass_hz >= 20.0 {
                filters.push(highpass(e.highpass_hz, rate));
            }
            if e.low_db != 0.0 {
                filters.push(shelf(e.low_hz, e.low_db, false, rate));
            }
            if e.mid_db != 0.0 {
                filters.push(peak(e.mid_hz, e.mid_db, e.mid_q, rate));
            }
            if e.high_db != 0.0 {
                filters.push(shelf(e.high_hz, e.high_db, true, rate));
            }
        }
        let c = &settings.compressor;
        Self {
            filters,
            settings: settings.clone(),
            attack: (-1.0 / (c.attack_ms * 0.001 * rate as f64)).exp(),
            release: (-1.0 / (c.release_ms * 0.001 * rate as f64)).exp(),
            reduction: 0.0,
        }
    }
    pub fn process(&mut self, mut sample: [f64; 2]) -> [f64; 2] {
        for f in &mut self.filters {
            sample = f.process(sample);
        }
        let c = &self.settings.compressor;
        if c.enabled {
            let level = 20.0 * sample[0].abs().max(sample[1].abs()).max(1e-15).log10();
            let d = level - c.threshold_db;
            let slope = 1.0 - 1.0 / c.ratio;
            let target = if c.knee_db > 0.0 && d.abs() < c.knee_db / 2.0 {
                slope * (d + c.knee_db / 2.0).powi(2) / (2.0 * c.knee_db)
            } else if d > 0.0 {
                slope * d
            } else {
                0.0
            };
            let coefficient = if target > self.reduction {
                self.attack
            } else {
                self.release
            };
            self.reduction = target + coefficient * (self.reduction - target);
            let gain = crate::db_gain(c.makeup_db - self.reduction);
            sample[0] *= gain;
            sample[1] *= gain;
        }
        sample
    }
    pub fn bytes(&self) -> usize {
        std::mem::size_of::<Self>() + self.filters.len() * std::mem::size_of::<Biquad>()
    }
}
#[derive(Clone)]
pub(crate) struct Delay {
    data: VecDeque<[f64; 2]>,
}
impl Delay {
    pub fn new(frames: usize) -> Self {
        Self {
            data: vec![[0.0; 2]; frames].into(),
        }
    }
    pub fn process(&mut self, sample: [f64; 2]) -> [f64; 2] {
        if self.data.is_empty() {
            return sample;
        }
        self.data.push_back(sample);
        self.data.pop_front().unwrap()
    }
    pub fn bytes(&self) -> usize {
        self.data.len() * 16
    }
}
#[derive(Clone)]
pub(crate) struct Limiter {
    delay: Delay,
    peaks: VecDeque<(u64, f64)>,
    position: u64,
    lookahead: u64,
    ceiling: f64,
    release: f64,
    gain: f64,
    pub reduction: f64,
}
impl Limiter {
    pub fn latency(rate: u32) -> usize {
        (rate as f64 * 0.005).ceil() as usize
    }
    pub fn new(config: &AudioLimiter, rate: u32) -> Self {
        let lookahead = Self::latency(rate) as u64;
        Self {
            delay: Delay::new(lookahead as usize),
            peaks: VecDeque::new(),
            position: 0,
            lookahead,
            ceiling: crate::db_gain(config.ceiling_db),
            release: (-1.0 / (config.release_ms * 0.001 * rate as f64)).exp(),
            gain: 1.0,
            reduction: 0.0,
        }
    }
    pub fn process(&mut self, sample: [f64; 2]) -> [f64; 2] {
        let p = sample[0].abs().max(sample[1].abs());
        while self.peaks.back().is_some_and(|(_, v)| *v <= p) {
            self.peaks.pop_back();
        }
        self.peaks.push_back((self.position, p));
        let oldest = self.position.saturating_sub(self.lookahead);
        while self.peaks.front().is_some_and(|(i, _)| *i < oldest) {
            self.peaks.pop_front();
        }
        let peak = self.peaks.front().map(|(_, p)| *p).unwrap_or(0.0);
        let target = if peak > self.ceiling {
            self.ceiling / peak
        } else {
            1.0
        };
        self.gain = if target < self.gain {
            target
        } else {
            target + self.release * (self.gain - target)
        };
        self.reduction = -20.0 * self.gain.max(1e-15).log10();
        self.position += 1;
        let mut out = self.delay.process(sample);
        out[0] *= self.gain;
        out[1] *= self.gain;
        out
    }
    pub fn bytes(&self) -> usize {
        self.delay.bytes() + self.peaks.len() * 16 + std::mem::size_of::<Self>()
    }
}

pub fn eq_response(config: &bonaparte_model::AudioEq, rate: u32) -> Result<Vec<[f64; 2]>, String> {
    let settings = AudioProcessing {
        eq: config.clone(),
        compressor: Default::default(),
    };
    settings.validate()?;
    let state = ChannelState::new(&settings, rate);
    let mut curve = Vec::with_capacity(160);
    for i in 0..160 {
        let frequency = 20.0 * (1000.0f64).powf(i as f64 / 159.0);
        let w = 2.0 * std::f64::consts::PI * frequency.min(rate as f64 * 0.499) / rate as f64;
        let mut db = 0.0;
        for f in &state.filters {
            let nr = f.b[0] + f.b[1] * w.cos() + f.b[2] * (2.0 * w).cos();
            let ni = -f.b[1] * w.sin() - f.b[2] * (2.0 * w).sin();
            let dr = 1.0 + f.a[0] * w.cos() + f.a[1] * (2.0 * w).cos();
            let di = -f.a[0] * w.sin() - f.a[1] * (2.0 * w).sin();
            db += 10.0
                * ((nr * nr + ni * ni) / (dr * dr + di * di))
                    .max(1e-12)
                    .log10();
        }
        curve.push([frequency, db]);
    }
    Ok(curve)
}
