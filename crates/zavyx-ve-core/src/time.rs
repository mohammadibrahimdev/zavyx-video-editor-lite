//! Rational time and frame-rate maths, exact (no float drift).
//!
//! All timeline positions are stored as exact rationals (`num/den`). Conversion
//! to/from frame indices and timecodes is provided here so the rest of the
//! engine never touches floats for time arithmetic.

use num_rational::Rational as NumRational;
use num_traits::{Zero, One, Signed};
use serde::{Serialize, Deserialize};

/// A rational time value, e.g. `5/1` seconds, `1001/30000` for NTSC.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct Rational(pub NumRational);

impl Rational {
    pub fn new(num: i64, den: i64) -> Self {
        // num-rational's default Ratio uses isize; convert i64 -> isize safely.
        // We assume timeline numerators/denominators fit in isize (typically
        // they're small numbers like 1001/30000).
        Self(NumRational::new(num as isize, den as isize))
    }

    pub const ZERO: Rational = Rational(NumRational::new_raw(0, 1));
    pub const ONE:  Rational = Rational(NumRational::new_raw(1, 1));

    pub fn from_seconds(s: f64) -> Self {
        let ms = (s * 1000.0).round() as i64;
        Rational::new(ms, 1000)
    }

    pub fn to_f64(self) -> f64 {
        *self.0.numer() as f64 / *self.0.denom() as f64
    }

    pub fn numer(&self) -> i64 { *self.0.numer() as i64 }
    pub fn denom(&self) -> i64 { *self.0.denom() as i64 }

    pub fn is_zero(&self) -> bool { self.0.is_zero() }
    pub fn is_positive(&self) -> bool { self.0.is_positive() }
    pub fn is_negative(&self) -> bool { self.0.is_negative() }
}

impl Default for Rational {
    fn default() -> Self { Rational::ZERO }
}

impl std::ops::Add for Rational {
    type Output = Rational;
    fn add(self, other: Self) -> Self { Rational(self.0 + other.0) }
}
impl std::ops::Sub for Rational {
    type Output = Rational;
    fn sub(self, other: Self) -> Self { Rational(self.0 - other.0) }
}
impl std::ops::Mul for Rational {
    type Output = Rational;
    fn mul(self, other: Self) -> Self { Rational(self.0 * other.0) }
}
impl std::ops::Div for Rational {
    type Output = Rational;
    fn div(self, other: Self) -> Self {
        assert!(!other.is_zero(), "Rational divide by zero");
        Rational(self.0 / other.0)
    }
}
impl std::ops::Neg for Rational {
    type Output = Rational;
    fn neg(self) -> Self { Rational(-self.0) }
}

impl Zero for Rational {
    fn zero() -> Self { Rational::ZERO }
    fn is_zero(&self) -> bool { self.0.is_zero() }
}
impl One for Rational {
    fn one() -> Self { Rational::ONE }
}

/// A frame rate, stored as a fraction (e.g. 30000/1001 for 29.97 NTSC).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FrameRate(pub Rational);

impl FrameRate {
    pub const FPS_24:    FrameRate = FrameRate(Rational(NumRational::new_raw(24, 1)));
    pub const FPS_25:    FrameRate = FrameRate(Rational(NumRational::new_raw(25, 1)));
    pub const FPS_30:    FrameRate = FrameRate(Rational(NumRational::new_raw(30, 1)));
    pub const FPS_29_97: FrameRate = FrameRate(Rational(NumRational::new_raw(30000, 1001)));
    pub const FPS_50:    FrameRate = FrameRate(Rational(NumRational::new_raw(50, 1)));
    pub const FPS_60:    FrameRate = FrameRate(Rational(NumRational::new_raw(60, 1)));
    pub const FPS_59_94: FrameRate = FrameRate(Rational(NumRational::new_raw(60000, 1001)));

    /// Convert a frame index to a time position: `frame_idx / fps`.
    pub fn frame_to_time(self, frame_idx: i64) -> Rational {
        // time = frame_idx * (1 / fps) = frame_idx * den / num
        Rational::new(frame_idx, 1) * Rational::new(self.0.denom() as i64, self.0.numer() as i64)
    }

    /// Convert a time position to a frame index (floor).
    pub fn time_to_frame(self, time: Rational) -> i64 {
        // frame = time * fps = time * num / den
        let r = time.0 * self.0.0;
        // floor division
        let n = *r.numer() as i64;
        let d = *r.denom() as i64;
        if d == 0 { 0 } else if (n >= 0) == (d >= 0) { n / d } else { -((-n) / d + if (-n) % d != 0 { 1 } else { 0 }) }
    }
}

/// A half-open time range `[start, end)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct TimeRange {
    pub start: Rational,
    pub end:   Rational,
}

impl TimeRange {
    pub fn new(start: Rational, end: Rational) -> Self {
        assert!(end >= start, "TimeRange end must be >= start");
        TimeRange { start, end }
    }
    pub fn duration(&self) -> Rational { self.end - self.start }
    pub fn contains(&self, t: Rational) -> bool { t >= self.start && t < self.end }
    pub fn overlaps(&self, other: TimeRange) -> bool {
        self.start < other.end && other.start < self.end
    }
    pub fn intersect(&self, other: TimeRange) -> Option<TimeRange> {
        let s = std::cmp::max(self.start, other.start);
        let e = std::cmp::min(self.end, other.end);
        if e > s { Some(TimeRange { start: s, end: e }) } else { None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rational_arithmetic_exact() {
        let a = Rational::new(1, 3);
        let b = Rational::new(2, 3);
        assert_eq!(a + b, Rational::new(1, 1));
        assert_eq!(b - a, Rational::new(1, 3));
        assert_eq!(a * b, Rational::new(2, 9));
        assert_eq!(a / b, Rational::new(1, 2));
    }

    #[test]
    fn no_drift_over_long_timeline() {
        // 1-hour timeline at 29.97 fps.
        // Each frame = 1001/30000 seconds.
        // 3600 seconds * 29.97 fps = 107,892.1 frames; we render 107,892 frames
        // which gives total = 107,892 * 1001 / 30000 = 107,999,892 / 30000
        //                    = 3599.9964 seconds (exact, no float drift).
        let frame_dur = Rational::new(1001, 30000);
        let total_frames = 107_892i64;
        let total: Rational = (0..total_frames)
            .map(|_| frame_dur)
            .fold(Rational::ZERO, |acc, x| acc + x);
        // Expected = 107999892/30000, simplified to 8999991/2500 = 3599.9964
        let expected = Rational::new(8_999_991, 2500);
        assert_eq!(total, expected,
            "Drift: got {} expected {}", total.to_f64(), expected.to_f64());
    }

    #[test]
    fn frame_round_trip_29_97() {
        let fps = FrameRate::FPS_29_97;
        for i in 0..1000i64 {
            let t = fps.frame_to_time(i);
            let back = fps.time_to_frame(t);
            assert_eq!(back, i, "frame {} round-trip failed", i);
        }
    }

    #[test]
    fn time_range_overlaps() {
        let a = TimeRange::new(Rational::new(0, 1), Rational::new(10, 1));
        let b = TimeRange::new(Rational::new(5, 1), Rational::new(15, 1));
        let c = TimeRange::new(Rational::new(10, 1), Rational::new(20, 1));
        assert!(a.overlaps(b));
        assert!(!a.overlaps(c));  // a.end == c.start, half-open
        assert_eq!(a.intersect(b), Some(TimeRange::new(Rational::new(5, 1), Rational::new(10, 1))));
        assert_eq!(a.intersect(c), None);
    }
}
