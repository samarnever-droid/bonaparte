//! Keyframe tracks and interpolation.
//!
//! Contract: evaluation is a pure function of (track, time). Same track +
//! same time = same value, always. This is what makes playback, export, and
//! the MCP server agree on every frame. Times are integer ticks (time.rs).

use serde::{Deserialize, Serialize};

use crate::time::Time;

/// A property value at a point in time.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PropValue {
    Scalar(f32),
    Vec2([f32; 2]),
}

impl PropValue {
    /// Which `ValueKind` this value belongs to, for type-checking `Op`s
    /// against `Property::value_kind`.
    pub fn kind(&self) -> crate::document::ValueKind {
        match self {
            PropValue::Scalar(_) => crate::document::ValueKind::Scalar,
            PropValue::Vec2(_) => crate::document::ValueKind::Vec2,
        }
    }

    /// Linear interpolation between two values of the same kind.
    pub fn lerp(a: &PropValue, b: &PropValue, t: f32) -> PropValue {
        match (a, b) {
            (PropValue::Scalar(x), PropValue::Scalar(y)) => PropValue::Scalar(x + (y - x) * t),
            (PropValue::Vec2(x), PropValue::Vec2(y)) => {
                PropValue::Vec2([x[0] + (y[0] - x[0]) * t, x[1] + (y[1] - x[1]) * t])
            }
            // Callers type-check via Property::value_kind before reaching here;
            // mixed kinds at this point would be a model bug.
            _ => *a,
        }
    }
}

/// Easing applied FROM a keyframe to the NEXT keyframe. Bezier handles are
/// cubic-bezier control points in segment-relative space: x in 0..1 is
/// normalized time between the two keys, y is normalized value progress —
/// the same convention as CSS `cubic-bezier` and the AE graph editor.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Easing {
    Linear,
    /// Hold the outgoing value for the entire segment, then step to the next
    /// key exactly at its time — the classic stepped/"Hold" keyframe.
    Hold,
    Bezier {
        p1: [f32; 2],
        p2: [f32; 2],
    },
}

impl Default for Easing {
    fn default() -> Self {
        // Ease-in-out by default: the "smooth" beginners expect from presets.
        Easing::Bezier {
            p1: [0.42, 0.0],
            p2: [0.58, 1.0],
        }
    }
}

impl Easing {
    /// Map normalized time `u` (0..1) to normalized progress (0..1).
    pub fn ease(&self, u: f32) -> f32 {
        let u = u.clamp(0.0, 1.0);
        match *self {
            Easing::Linear => u,
            Easing::Hold => {
                if u >= 1.0 {
                    1.0
                } else {
                    0.0
                }
            }
            Easing::Bezier { p1, p2 } => {
                // Solve x(u) = u_target for u by bisection, then return y(u).
                // Bisection (not Newton) is deliberate: it cannot diverge, so
                // evaluation stays deterministic and total for any handles.
                let x = |t: f32| bezier1(t, p1[0], p2[0]);
                let mut lo = 0.0f32;
                let mut hi = 1.0f32;
                for _ in 0..32 {
                    let mid = (lo + hi) * 0.5;
                    if x(mid) < u {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                let t = (lo + hi) * 0.5;
                bezier1(t, p1[1], p2[1])
            }
        }
    }
}

/// One axis of a cubic bezier: 3(1-t)²t·p1 + 3(1-t)t²·p2 + t³.
fn bezier1(t: f32, p1: f32, p2: f32) -> f32 {
    let it = 1.0 - t;
    3.0 * it * it * t * p1 + 3.0 * it * t * t * p2 + t * t * t
}

/// A single keyframe: a value at a time (ticks), plus the easing used from
/// here to the next key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Keyframe {
    pub time: Time,
    pub value: PropValue,
    pub easing: Easing,
}

/// An animated property: a time-sorted list of keyframes.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Track {
    /// Sorted ascending by `time`; no two keys share a time (set_key merges).
    pub keys: Vec<Keyframe>,
}

impl Track {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or replace a keyframe, keeping the list time-sorted. This is
    /// the only way keys enter a track — sorting is an invariant, not a chore.
    pub fn set_key(&mut self, key: Keyframe) {
        match self.keys.binary_search_by(|k| k.time.cmp(&key.time)) {
            Ok(i) => self.keys[i] = key,
            Err(i) => self.keys.insert(i, key),
        }
    }

    /// Remove the key at exactly `time`. Returns it so `Op`s can undo.
    pub fn remove_key(&mut self, time: Time) -> Option<Keyframe> {
        let i = self.keys.binary_search_by(|k| k.time.cmp(&time)).ok()?;
        Some(self.keys.remove(i))
    }

    /// Move a key from one time to another (replace-if-occupied).
    pub fn move_key(&mut self, from: Time, to: Time) -> Option<Keyframe> {
        let key = self.remove_key(from)?;
        self.set_key(Keyframe { time: to, ..key });
        Some(key)
    }

    /// Evaluate at `time`. `None` only when the track has no keys; outside
    /// the key range the first/last value holds.
    pub fn evaluate(&self, time: Time) -> Option<PropValue> {
        let keys = &self.keys;
        match keys.len() {
            0 => return None,
            1 => return Some(keys[0].value),
            _ => {}
        }
        if time <= keys[0].time {
            return Some(keys[0].value);
        }
        let last = keys.len() - 1;
        if time >= keys[last].time {
            return Some(keys[last].value);
        }
        // Find the segment [keys[i], keys[i+1]] containing `time`.
        let i = keys
            .partition_point(|key| key.time <= time)
            .saturating_sub(1);
        let (a, b) = (&keys[i], &keys[i + 1]);
        let span = (b.time.0 - a.time.0).max(1);
        let u = (time.0 - a.time.0) as f32 / span as f32;
        let s = a.easing.ease(u);
        Some(PropValue::lerp(&a.value, &b.value, s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::TICKS_PER_SEC;

    fn k(secs: i64, value: f32) -> Keyframe {
        Keyframe {
            time: Time(secs * TICKS_PER_SEC),
            value: PropValue::Scalar(value),
            easing: Easing::Linear,
        }
    }

    #[test]
    fn linear_interpolation() {
        let mut t = Track::new();
        t.set_key(k(0, 0.0));
        t.set_key(k(1, 100.0));
        assert_eq!(
            t.evaluate(Time(TICKS_PER_SEC / 2)),
            Some(PropValue::Scalar(50.0))
        );
        // Holds outside the key range.
        assert_eq!(
            t.evaluate(Time(-TICKS_PER_SEC)),
            Some(PropValue::Scalar(0.0))
        );
        assert_eq!(
            t.evaluate(Time(2 * TICKS_PER_SEC)),
            Some(PropValue::Scalar(100.0))
        );
    }

    #[test]
    fn keys_stay_sorted_after_out_of_order_inserts() {
        let mut t = Track::new();
        t.set_key(k(2, 20.0));
        t.set_key(k(0, 0.0));
        t.set_key(k(1, 10.0));
        t.set_key(k(1, 11.0)); // same time replaces
        assert_eq!(t.keys.len(), 3);
        assert!(t.keys.windows(2).all(|w| w[0].time < w[1].time));
        assert_eq!(t.evaluate(k(1, 0.0).time), Some(PropValue::Scalar(11.0)));
    }

    #[test]
    fn default_easing_is_symmetric_ease_in_out() {
        let e = Easing::default();
        assert!((e.ease(0.25) - (1.0 - e.ease(0.75))).abs() < 1e-6);
        assert!((e.ease(0.0) - 0.0).abs() < 1e-6);
        assert!((e.ease(1.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn bezier_easing_matches_known_css_curve() {
        // cubic-bezier(0.25, 0.1, 0.25, 1.0) at x=0.5 ≈ 0.8024
        let e = Easing::Bezier {
            p1: [0.25, 0.1],
            p2: [0.25, 1.0],
        };
        assert!((e.ease(0.5) - 0.8024).abs() < 1e-3);
    }

    #[test]
    fn vec2_lerp() {
        let a = PropValue::Vec2([0.0, 0.0]);
        let b = PropValue::Vec2([100.0, 200.0]);
        assert_eq!(PropValue::lerp(&a, &b, 0.25), PropValue::Vec2([25.0, 50.0]));
    }
}
