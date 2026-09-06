//! Serializable channel-strip processing and acyclic mix-bus routing.
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AudioEq {
    pub enabled: bool,
    pub highpass_hz: f64,
    pub low_hz: f64,
    pub low_db: f64,
    pub mid_hz: f64,
    pub mid_db: f64,
    pub mid_q: f64,
    pub high_hz: f64,
    pub high_db: f64,
}
impl Default for AudioEq {
    fn default() -> Self {
        Self {
            enabled: false,
            highpass_hz: 0.0,
            low_hz: 120.0,
            low_db: 0.0,
            mid_hz: 1500.0,
            mid_db: 0.0,
            mid_q: 1.0,
            high_hz: 8000.0,
            high_db: 0.0,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AudioCompressor {
    pub enabled: bool,
    pub threshold_db: f64,
    pub ratio: f64,
    pub knee_db: f64,
    pub attack_ms: f64,
    pub release_ms: f64,
    pub makeup_db: f64,
}
impl Default for AudioCompressor {
    fn default() -> Self {
        Self {
            enabled: false,
            threshold_db: -18.0,
            ratio: 3.0,
            knee_db: 6.0,
            attack_ms: 10.0,
            release_ms: 120.0,
            makeup_db: 0.0,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct AudioProcessing {
    pub eq: AudioEq,
    pub compressor: AudioCompressor,
}
impl AudioProcessing {
    pub fn active(&self) -> bool {
        self.eq.enabled || self.compressor.enabled
    }
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AudioLimiter {
    pub enabled: bool,
    pub ceiling_db: f64,
    pub release_ms: f64,
}
impl Default for AudioLimiter {
    fn default() -> Self {
        Self {
            enabled: false,
            ceiling_db: -1.0,
            release_ms: 80.0,
        }
    }
}
impl AudioLimiter {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudioSend {
    pub bus: String,
    pub gain_db: f64,
    #[serde(default = "yes")]
    pub enabled: bool,
}
fn yes() -> bool {
    true
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudioBus {
    pub id: String,
    pub name: String,
    pub gain_db: f64,
    pub pan: f64,
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub sends: Vec<AudioSend>,
    #[serde(default)]
    pub processing: AudioProcessing,
}
impl AudioBus {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            gain_db: 0.0,
            pan: 0.0,
            muted: false,
            output: None,
            sends: vec![],
            processing: Default::default(),
        }
    }
}
fn range(v: f64, lo: f64, hi: f64) -> bool {
    v.is_finite() && (lo..=hi).contains(&v)
}
impl AudioProcessing {
    pub fn validate(&self) -> Result<(), String> {
        let e = &self.eq;
        let c = &self.compressor;
        if !range(e.highpass_hz, 0.0, 1000.0)
            || !range(e.low_hz, 20.0, 1000.0)
            || !range(e.mid_hz, 40.0, 16000.0)
            || !range(e.high_hz, 1000.0, 20000.0)
            || !range(e.low_db, -18.0, 18.0)
            || !range(e.mid_db, -18.0, 18.0)
            || !range(e.high_db, -18.0, 18.0)
            || !range(e.mid_q, 0.1, 12.0)
        {
            return Err("EQ frequency, gain or Q is outside the supported range".into());
        }
        if !range(c.threshold_db, -60.0, 0.0)
            || !range(c.ratio, 1.0, 20.0)
            || !range(c.knee_db, 0.0, 24.0)
            || !range(c.attack_ms, 0.1, 200.0)
            || !range(c.release_ms, 10.0, 2000.0)
            || !range(c.makeup_db, -12.0, 24.0)
        {
            return Err("Compressor parameters are outside the supported range".into());
        }
        Ok(())
    }
}
impl crate::AudioArrangement {
    pub fn validate_processing(&self) -> Result<(), String> {
        self.processing.validate()?;
        if !range(self.limiter.ceiling_db, -12.0, 0.0)
            || !range(self.limiter.release_ms, 10.0, 1000.0)
            || self.buses.len() > 16
        {
            return Err("Invalid master limiter or mix-bus count (maximum 16)".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        for bus in &self.buses {
            if bus.id.is_empty()
                || bus.id.len() > 80
                || !bus
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                || !ids.insert(bus.id.as_str())
                || bus.name.len() > 256
                || !range(bus.gain_db, -96.0, 24.0)
                || !range(bus.pan, -1.0, 1.0)
            {
                return Err("Invalid or duplicate mix bus".into());
            }
            bus.processing.validate()?;
        }
        let route = |output: &Option<String>, sends: &[AudioSend]| -> Result<(), String> {
            if output.as_ref().is_some_and(|id| !ids.contains(id.as_str()))
                || sends.len() > 4
                || sends
                    .iter()
                    .any(|s| !ids.contains(s.bus.as_str()) || !range(s.gain_db, -96.0, 6.0))
            {
                return Err(
                    "Audio output/send targets must exist; at most four sends per strip".into(),
                );
            }
            Ok(())
        };
        for track in &self.tracks {
            track.processing.validate()?;
            route(&track.output, &track.sends)?;
        }
        for bus in &self.buses {
            route(&bus.output, &bus.sends)?;
        }
        fn visit(
            id: &str,
            a: &crate::AudioArrangement,
            active: &mut Vec<String>,
            done: &mut std::collections::BTreeSet<String>,
        ) -> Result<(), String> {
            if active.iter().any(|x| x == id) {
                return Err("Audio bus routing contains a feedback cycle".into());
            }
            if done.contains(id) {
                return Ok(());
            }
            active.push(id.into());
            let b = a
                .buses
                .iter()
                .find(|b| b.id == id)
                .ok_or("Mix bus missing")?;
            if let Some(next) = &b.output {
                visit(next, a, active, done)?;
            }
            for s in &b.sends {
                if s.enabled {
                    visit(&s.bus, a, active, done)?;
                }
            }
            active.pop();
            done.insert(id.into());
            Ok(())
        }
        let mut done = std::collections::BTreeSet::new();
        for bus in &self.buses {
            visit(&bus.id, self, &mut vec![], &mut done)?;
        }
        Ok(())
    }
    pub fn has_processing(&self) -> bool {
        self.processing.active()
            || self.limiter.enabled
            || !self.buses.is_empty()
            || self.tracks.iter().any(|t| {
                t.processing.active() || t.output.is_some() || t.sends.iter().any(|s| s.enabled)
            })
    }
}
