//! Keyframes and easing curves.
//!
//! Every numeric parameter on a clip (transform, opacity, colour, etc.) can
//! have a keyframe track. The `KeyframeTrack<T>` interpolates between keyframes
//! using the chosen easing function.

use crate::time::Rational;
use serde::{Serialize, Deserialize};

/// Easing functions applied to keyframe interpolation in normalized `[0,1]`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Easing {
    Linear,
    /// Cubic ease in: t^3
    EaseIn,
    /// Cubic ease out: 1 - (1-t)^3
    EaseOut,
    /// Cubic ease in/out: piecewise cubic
    EaseInOut,
    /// Hold (step): value jumps at the keyframe
    Hold,
    /// Cubic bezier with two control points (x1, y1, x2, y2) in [0,1]^2.
    Bezier(f64, f64, f64, f64),
}

impl Easing {
    /// Apply easing to a normalized progress `t` in `[0,1]`.
    pub fn at(self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Easing::Linear    => t,
            Easing::EaseIn    => t * t * t,
            Easing::EaseOut   => 1.0 - (1.0 - t).powi(3),
            Easing::EaseInOut => {
                if t < 0.5 { 4.0 * t * t * t } else { 1.0 - ((-2.0 * t + 2.0).powi(3)) / 2.0 }
            }
            Easing::Hold      => if t < 1.0 { 0.0 } else { 1.0 },
            Easing::Bezier(x1, y1, x2, y2) => {
                // Simple cubic-bezier evaluation via Newton-Raphson.
                // Solve x(s) = t for s, then return y(s).
                let cx = 3.0 * x1;
                let bx = 3.0 * (x2 - x1) - cx;
                let ax = 1.0 - cx - bx;
                let cy = 3.0 * y1;
                let by = 3.0 * (y2 - y1) - cy;
                let ay = 1.0 - cy - by;
                let solve = |t_target: f64| -> f64 {
                    let mut s = t_target;
                    for _ in 0..8 {
                        let x = ((ax * s + bx) * s + cx) * s - t_target;
                        let d = (3.0 * ax * s + 2.0 * bx) * s + cx;
                        if d.abs() < 1e-9 { break; }
                        s -= x / d;
                    }
                    s
                };
                let s = solve(t);
                ((ay * s + by) * s + cy) * s
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Keyframe<T: Copy + PartialEq> {
    pub time: Rational,
    pub value: T,
    pub easing: Easing,
}

/// A sorted (by time) collection of keyframes for one parameter.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KeyframeTrack<T: Copy + PartialEq> {
    pub keyframes: Vec<Keyframe<T>>,
    pub default: T,
}

impl<T: Copy + PartialEq> KeyframeTrack<T> {
    pub fn new(default: T) -> Self {
        KeyframeTrack { keyframes: Vec::new(), default }
    }

    pub fn add(&mut self, kf: Keyframe<T>) {
        // Replace any existing keyframe at the same time, then insert and re-sort.
        self.keyframes.retain(|k| k.time != kf.time);
        self.keyframes.push(kf);
        self.keyframes.sort_by(|a, b| a.time.cmp(&b.time));
    }
    pub fn remove_at_time(&mut self, time: Rational) -> bool {
        let before = self.keyframes.len();
        self.keyframes.retain(|k| k.time != time);
        self.keyframes.len() < before
    }

    pub fn is_empty(&self) -> bool { self.keyframes.is_empty() }

    /// Returns the interpolated value at `time`. For numeric types we expect
    /// the caller to implement `lerp`. Generic types fall back to nearest.
    pub fn value_at<F: Fn(T, T, f64) -> T>(&self, time: Rational, lerp: F) -> T {
        if self.keyframes.is_empty() { return self.default; }
        let kfs = &self.keyframes;
        if time <= kfs[0].time { return kfs[0].value; }
        if time >= kfs.last().unwrap().time { return kfs.last().unwrap().value; }
        // Find the segment containing `time`.
        let idx = kfs.iter().position(|k| k.time > time).unwrap_or(kfs.len());
        let prev = &kfs[idx - 1];
        let next = &kfs[idx];
        let span = next.time - prev.time;
        if span.is_zero() { return next.value; }
        let local_t = (time - prev.time).to_f64() / span.to_f64();
        let eased = prev.easing.at(local_t);
        lerp(prev.value, next.value, eased)
    }
}

impl KeyframeTrack<f64> {
    pub fn value_at_f64(&self, time: Rational) -> f64 {
        self.value_at(time, |a, b, t| a + (b - a) * t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(n: i64, d: i64) -> Rational { Rational::new(n, d) }

    #[test]
    fn easing_linear_at_quarter() {
        assert!((Easing::Linear.at(0.25) - 0.25).abs() < 1e-9);
        assert!((Easing::Linear.at(0.5) - 0.5).abs() < 1e-9);
        assert!((Easing::Linear.at(0.75) - 0.75).abs() < 1e-9);
    }

    #[test]
    fn easing_ease_in_out_midpoint() {
        // Cubic ease in/out at t=0.5 should be exactly 0.5 (symmetric)
        assert!((Easing::EaseInOut.at(0.5) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn easing_hold_jumps_at_end() {
        assert_eq!(Easing::Hold.at(0.5), 0.0);
        assert_eq!(Easing::Hold.at(0.99), 0.0);
        assert_eq!(Easing::Hold.at(1.0), 1.0);
    }

    #[test]
    fn easing_bezier_linear_when_controls_on_diagonal() {
        // Bezier(0,0,1,1) == Linear
        for &x in &[0.1, 0.25, 0.5, 0.75, 0.9] {
            assert!((Easing::Bezier(0.0, 0.0, 1.0, 1.0).at(x) - x).abs() < 1e-6,
                "Bezier(0,0,1,1) at {} should equal {}", x, x);
        }
    }

    #[test]
    fn keyframe_track_returns_default_when_empty() {
        let trk: KeyframeTrack<f64> = KeyframeTrack::new(42.0);
        assert_eq!(trk.value_at_f64(t(5,1)), 42.0);
    }

    #[test]
    fn keyframe_track_clamps_outside_range() {
        let mut trk: KeyframeTrack<f64> = KeyframeTrack::new(0.0);
        trk.add(Keyframe { time: t(1,1), value: 10.0, easing: Easing::Linear });
        trk.add(Keyframe { time: t(5,1), value: 50.0, easing: Easing::Linear });
        assert_eq!(trk.value_at_f64(t(0,1)), 10.0);   // before first kf -> first value
        assert_eq!(trk.value_at_f64(t(6,1)), 50.0);   // after last  kf -> last value
    }

    #[test]
    fn keyframe_track_linear_interpolation() {
        let mut trk: KeyframeTrack<f64> = KeyframeTrack::new(0.0);
        trk.add(Keyframe { time: t(0,1), value: 0.0, easing: Easing::Linear });
        trk.add(Keyframe { time: t(10,1), value: 100.0, easing: Easing::Linear });
        assert!((trk.value_at_f64(t(5,1)) - 50.0).abs() < 1e-9);
        assert!((trk.value_at_f64(t(2,1)) - 20.0).abs() < 1e-9);
        assert!((trk.value_at_f64(t(7,1)) - 70.0).abs() < 1e-9);
    }

    #[test]
    fn keyframe_track_ease_in_out_midpoint() {
        let mut trk: KeyframeTrack<f64> = KeyframeTrack::new(0.0);
        trk.add(Keyframe { time: t(0,1), value: 0.0, easing: Easing::EaseInOut });
        trk.add(Keyframe { time: t(10,1), value: 100.0, easing: Easing::EaseInOut });
        // At midpoint with EaseInOut, eased value = 0.5, interpolated = 50.0
        assert!((trk.value_at_f64(t(5,1)) - 50.0).abs() < 1e-9);
    }

    #[test]
    fn keyframe_track_dedupes_by_time() {
        let mut trk: KeyframeTrack<f64> = KeyframeTrack::new(0.0);
        trk.add(Keyframe { time: t(5,1), value: 10.0, easing: Easing::Linear });
        trk.add(Keyframe { time: t(5,1), value: 99.0, easing: Easing::Linear });
        assert_eq!(trk.keyframes.len(), 1);
        assert_eq!(trk.keyframes[0].value, 99.0);  // last writer wins
    }
}
