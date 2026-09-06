//! Time: integer ticks and rational frame rates.
//!
//! All time in the model is `Time(i64)` — ticks at 120,000 ticks/second
//! (adopted from OpenCut's MediaTime; see ARCHITECTURE.md competitive
//! teardown). Rationale: f64 seconds drift across frame boundaries
//! (23.976fps = 0.04170833…s per frame); integer ticks make frame snapping
//! exact: 120000 / 24 = 5000, 120000 / 23.976×1001 = 5005 — both integers.
//! Same video on preview, export, and MCP render is bit-identical in time.

use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

use serde::{Deserialize, Serialize};

/// Ticks per second. Chosen so 24, 25, 30, 48, 50, 60 fps divide it evenly
/// and 23.976 (24000/1001) yields integer ticks-per-frame (5005).
pub const TICKS_PER_SEC: i64 = 120_000;

/// A moment in the timeline, in ticks. Transparent serde = a JSON number,
/// so the TS UI and MCP agents send plain integers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Time(pub i64);

impl Time {
    pub const ZERO: Time = Time(0);

    pub fn from_secs_f64(secs: f64) -> Time {
        Time((secs * TICKS_PER_SEC as f64).round() as i64)
    }

    pub fn as_secs_f64(self) -> f64 {
        self.0 as f64 / TICKS_PER_SEC as f64
    }

    /// Snap to the nearest frame boundary of `rate`.
    pub fn snap_to_frame(self, rate: FrameRate) -> Time {
        let tpf = rate.ticks_per_frame();
        if tpf == 0 {
            return self;
        }
        Time((self.0 / tpf) * tpf)
    }

    /// Convert this time to an SMPTE timecode string (e.g. `00:01:23:12`).
    pub fn to_timecode(&self, fps: FrameRate) -> String {
        let frame_num = fps.to_frame(*self);
        let nom_fps = (fps.num as f64 / fps.den as f64).round() as i64;
        let nom_fps = if nom_fps <= 0 { 1 } else { nom_fps };

        let is_neg = frame_num < 0;
        let abs_frames = frame_num.abs();

        let ff = abs_frames % nom_fps;
        let total_sec = abs_frames / nom_fps;
        let ss = total_sec % 60;
        let total_min = total_sec / 60;
        let mm = total_min % 60;
        let hh = total_min / 60;

        if is_neg {
            format!("-{:02}:{:02}:{:02}:{:02}", hh, mm, ss, ff)
        } else {
            format!("{:02}:{:02}:{:02}:{:02}", hh, mm, ss, ff)
        }
    }

    /// Parse an SMPTE timecode string (`HH:MM:SS:FF` or `HH:MM:SS;FF`) into `Time`.
    pub fn from_timecode(tc: &str, fps: FrameRate) -> Result<Self, TimecodeError> {
        let trimmed = tc.trim();
        let (is_neg, s) = if let Some(stripped) = trimmed.strip_prefix('-') {
            (true, stripped)
        } else {
            (false, trimmed)
        };

        let parts: Vec<&str> = s.split([':', ';', '.']).collect();
        if parts.len() != 4 {
            return Err(TimecodeError::InvalidFormat(tc.to_string()));
        }

        let hh: u32 = parts[0].parse()?;
        let mm: u32 = parts[1].parse()?;
        let ss: u32 = parts[2].parse()?;
        let ff: u32 = parts[3].parse()?;

        if mm >= 60 {
            return Err(TimecodeError::MinuteOutOfRange(mm));
        }
        if ss >= 60 {
            return Err(TimecodeError::SecondOutOfRange(ss));
        }

        let nom_fps = (fps.num as f64 / fps.den as f64).round() as u32;
        let nom_fps = if nom_fps == 0 { 1 } else { nom_fps };
        if ff >= nom_fps {
            return Err(TimecodeError::FrameOutOfRange(ff, nom_fps));
        }

        let total_frames =
            ((hh as i64 * 60 + mm as i64) * 60 + ss as i64) * nom_fps as i64 + ff as i64;
        let signed_frames = if is_neg { -total_frames } else { total_frames };

        Ok(fps.from_frame(signed_frames))
    }
}

impl Add for Time {
    type Output = Time;
    fn add(self, rhs: Time) -> Time {
        Time(self.0 + rhs.0)
    }
}

impl Add<&Time> for Time {
    type Output = Time;
    fn add(self, rhs: &Time) -> Time {
        Time(self.0 + rhs.0)
    }
}

impl Add<Time> for &Time {
    type Output = Time;
    fn add(self, rhs: Time) -> Time {
        Time(self.0 + rhs.0)
    }
}

impl Add<&Time> for &Time {
    type Output = Time;
    fn add(self, rhs: &Time) -> Time {
        Time(self.0 + rhs.0)
    }
}

impl Sub for Time {
    type Output = Time;
    fn sub(self, rhs: Time) -> Time {
        Time(self.0 - rhs.0)
    }
}

impl Sub<&Time> for Time {
    type Output = Time;
    fn sub(self, rhs: &Time) -> Time {
        Time(self.0 - rhs.0)
    }
}

impl Sub<Time> for &Time {
    type Output = Time;
    fn sub(self, rhs: Time) -> Time {
        Time(self.0 - rhs.0)
    }
}

impl Sub<&Time> for &Time {
    type Output = Time;
    fn sub(self, rhs: &Time) -> Time {
        Time(self.0 - rhs.0)
    }
}

impl Mul<i64> for Time {
    type Output = Time;
    fn mul(self, rhs: i64) -> Time {
        Time(self.0 * rhs)
    }
}

impl Mul<Time> for i64 {
    type Output = Time;
    fn mul(self, rhs: Time) -> Time {
        Time(self * rhs.0)
    }
}

impl Div<i64> for Time {
    type Output = Time;
    fn div(self, rhs: i64) -> Time {
        Time(self.0 / rhs)
    }
}

impl Neg for Time {
    type Output = Time;
    fn neg(self) -> Time {
        Time(-self.0)
    }
}

impl AddAssign for Time {
    fn add_assign(&mut self, rhs: Time) {
        self.0 += rhs.0;
    }
}

impl AddAssign<&Time> for Time {
    fn add_assign(&mut self, rhs: &Time) {
        self.0 += rhs.0;
    }
}

impl SubAssign for Time {
    fn sub_assign(&mut self, rhs: Time) {
        self.0 -= rhs.0;
    }
}

impl SubAssign<&Time> for Time {
    fn sub_assign(&mut self, rhs: &Time) {
        self.0 -= rhs.0;
    }
}

impl Sum for Time {
    fn sum<I: Iterator<Item = Time>>(iter: I) -> Self {
        iter.fold(Time::ZERO, |acc, x| acc + x)
    }
}

impl<'a> Sum<&'a Time> for Time {
    fn sum<I: Iterator<Item = &'a Time>>(iter: I) -> Self {
        iter.fold(Time::ZERO, |acc, x| acc + *x)
    }
}

impl fmt::Display for Time {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.3}s", self.as_secs_f64())
    }
}

/// Errors occurring during SMPTE timecode parsing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TimecodeError {
    #[error("invalid timecode format, expected HH:MM:SS:FF or HH:MM:SS;FF, got {0:?}")]
    InvalidFormat(String),
    #[error("invalid integer in timecode component: {0}")]
    ParseInt(#[from] std::num::ParseIntError),
    #[error("frame number {0} exceeds frame rate {1}")]
    FrameOutOfRange(u32, u32),
    #[error("second number {0} exceeds 59")]
    SecondOutOfRange(u32),
    #[error("minute number {0} exceeds 59")]
    MinuteOutOfRange(u32),
}

/// A rational frame rate: `num / den` frames per second.
/// 30fps = 30/1; NTSC 29.97 = 30000/1001; 23.976 = 24000/1001.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameRate {
    pub num: u32,
    pub den: u32,
}

impl FrameRate {
    pub const FPS_24: FrameRate = FrameRate { num: 24, den: 1 };
    pub const FPS_25: FrameRate = FrameRate { num: 25, den: 1 };
    pub const FPS_30: FrameRate = FrameRate { num: 30, den: 1 };
    pub const FPS_60: FrameRate = FrameRate { num: 60, den: 1 };
    pub const NTSC_FILM: FrameRate = FrameRate {
        num: 24000,
        den: 1001,
    };

    /// Exact ticks in one frame — always an integer by construction of
    /// TICKS_PER_SEC (documented invariants: 24k/1001 divides 120000·1001).
    pub fn ticks_per_frame(&self) -> i64 {
        if self.num == 0 {
            return 0;
        }
        (TICKS_PER_SEC * self.den as i64) / self.num as i64
    }

    pub fn as_f64(&self) -> f64 {
        if self.den == 0 {
            return 0.0;
        }
        self.num as f64 / self.den as f64
    }

    /// Convert a `Time` (in ticks) to a frame number at this frame rate.
    pub fn to_frame(&self, time: Time) -> i64 {
        let tpf = self.ticks_per_frame();
        if tpf == 0 {
            0
        } else {
            time.0 / tpf
        }
    }

    /// Convert a frame number to `Time` (in ticks) at this frame rate.
    pub fn from_frame(&self, frame_num: i64) -> Time {
        Time(frame_num * self.ticks_per_frame())
    }
}

impl fmt::Display for FrameRate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.den == 1 {
            write!(f, "{} fps", self.num)
        } else {
            let val = self.as_f64();
            let formatted = format!("{:.3}", val);
            let trimmed = formatted.trim_end_matches('0').trim_end_matches('.');
            write!(f, "{} fps", trimmed)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_rates_have_integer_ticks_per_frame() {
        for r in [
            FrameRate::FPS_24,
            FrameRate::FPS_25,
            FrameRate::FPS_30,
            FrameRate::FPS_60,
            FrameRate::NTSC_FILM,
        ] {
            let tpf = r.ticks_per_frame();
            assert!(tpf > 0, "{r:?}");
            assert_eq!((r.num as i64 * tpf) / r.den as i64, TICKS_PER_SEC, "{r:?}");
        }
    }

    #[test]
    fn ntsc_film_frame_is_exactly_5005_ticks() {
        assert_eq!(FrameRate::NTSC_FILM.ticks_per_frame(), 5005);
    }

    #[test]
    fn roundtrip_secs() {
        let t = Time::from_secs_f64(1.5);
        assert!((t.as_secs_f64() - 1.5).abs() < 1e-9);
    }

    #[test]
    fn time_arithmetic() {
        let t1 = Time(100);
        let t2 = Time(250);
        assert_eq!(t1 + t2, Time(350));
        assert_eq!(t2 - t1, Time(150));
        assert_eq!(t1 * 3, Time(300));
        assert_eq!(3 * t1, Time(300));
        assert_eq!(t2 / 5, Time(50));
        assert_eq!(-t1, Time(-100));

        let mut acc = Time(10);
        acc += Time(20);
        assert_eq!(acc, Time(30));
        acc -= Time(5);
        assert_eq!(acc, Time(25));

        let times = vec![Time(10), Time(20), Time(30)];
        let total: Time = times.into_iter().sum();
        assert_eq!(total, Time(60));
    }

    #[test]
    fn framerate_display_and_conversions() {
        assert_eq!(FrameRate::FPS_24.to_string(), "24 fps");
        assert_eq!(FrameRate::FPS_30.to_string(), "30 fps");
        assert_eq!(FrameRate::NTSC_FILM.to_string(), "23.976 fps");
        let ntsc_30 = FrameRate {
            num: 30000,
            den: 1001,
        };
        assert_eq!(ntsc_30.to_string(), "29.97 fps");

        let fps = FrameRate::FPS_24;
        let t = fps.from_frame(48);
        assert_eq!(t, Time(48 * 5000));
        assert_eq!(fps.to_frame(t), 48);
    }

    #[test]
    fn timecode_roundtrip() {
        let fps = FrameRate::FPS_24;
        let t = Time::from_timecode("01:02:03:04", fps).unwrap();
        assert_eq!(t.to_timecode(fps), "01:02:03:04");

        let t0 = Time(0);
        assert_eq!(t0.to_timecode(fps), "00:00:00:00");
        assert_eq!(Time::from_timecode("00:00:00:00", fps).unwrap(), t0);

        let err = Time::from_timecode("00:00:00:24", fps);
        assert!(matches!(err, Err(TimecodeError::FrameOutOfRange(24, 24))));

        let err2 = Time::from_timecode("00:65:00:00", fps);
        assert!(matches!(err2, Err(TimecodeError::MinuteOutOfRange(65))));
    }
}
