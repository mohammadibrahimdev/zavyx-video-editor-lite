//! zavyx-ve-audio — mixer, playback, audio effects.
//!
//! Phase 1: pure-Rust mixer math (sum + clip + constant-power crossfade) +
//! simple cpal player stub. All mixing math is in pure Rust and fully
//! testable without a real audio device.
//!
//! Phase 2: adds `effects` module with FadeIn, FadeOut, Gain, Normalize,
//! SimpleEQ, Compressor — all pure-Rust DSP on f32 sample slices.

pub mod effects;
pub mod extra;
pub mod playback;

use zavyx_ve_core::Rational;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("playback not available: {0}")]
    Playback(String),
    #[error("mix error: {0}")]
    Mix(String),
}

/// Default sample rate (48 kHz).
pub const SAMPLE_RATE: u32 = 48_000;

/// Default channel count (stereo).
pub const CHANNELS: u16 = 2;

/// A mono or stereo sample slice, f32 in [-1, 1].
pub type Samples = Vec<f32>;

/// Track mixer state for one track. Phase 1: volume + pan only.
#[derive(Clone, Debug)]
pub struct TrackMixer {
    pub volume: f32,        // 0.0 (muted) to 2.0 (2x gain)
    pub pan: f32,           // -1.0 (left) to 1.0 (right)
    pub muted: bool,
    pub solo: bool,
}

impl Default for TrackMixer {
    fn default() -> Self { TrackMixer { volume: 1.0, pan: 0.0, muted: false, solo: false } }
}

impl TrackMixer {
    /// Apply volume + pan to a stereo slice in-place.
    pub fn apply(&self, samples: &mut [f32]) {
        if self.muted {
            for s in samples.iter_mut() { *s = 0.0; }
            return;
        }
        let vol = self.volume;
        // Equal-power pan: left_gain = cos((pan+1)/2 * π/2), right = sin(...)
        let angle = ((self.pan + 1.0) * 0.5) * std::f32::consts::FRAC_PI_2;
        let left_gain = vol * angle.cos();
        let right_gain = vol * angle.sin();
        for chunk in samples.chunks_exact_mut(2) {
            chunk[0] *= left_gain;
            chunk[1] *= right_gain;
        }
    }
}

/// Mixes multiple stereo sample slices into one output slice.
/// All slices must be the same length. Output is hard-clipped to [-1, 1].
pub fn mix_stereo(layers: &[&[f32]]) -> Samples {
    if layers.is_empty() { return Vec::new(); }
    let len = layers.iter().map(|l| l.len()).min().unwrap_or(0);
    let mut out = vec![0.0f32; len];
    for l in layers {
        for (i, &s) in l.iter().take(len).enumerate() {
            out[i] += s;
        }
    }
    // Hard clip to [-1, 1]
    for s in out.iter_mut() {
        if *s > 1.0 { *s = 1.0; }
        else if *s < -1.0 { *s = -1.0; }
    }
    out
}

/// Constant-power crossfade between two stereo slices.
/// `alpha` = 0 means all `a`, `alpha` = 1 means all `b`.
pub fn crossfade(a: &[f32], b: &[f32], alpha: f32) -> Samples {
    let len = a.len().min(b.len());
    let mut out = Vec::with_capacity(len);
    let a_gain = ((1.0 - alpha) * std::f32::consts::FRAC_PI_2).cos();
    let b_gain = (alpha * std::f32::consts::FRAC_PI_2).cos();
    for i in 0..len {
        out.push(a[i] * a_gain + b[i] * b_gain);
    }
    out
}

/// Convert a per-sample time (in samples) to a Rational time given a sample rate.
pub fn samples_to_time(samples: u64, sample_rate: u32) -> Rational {
    Rational::new(samples as i64, sample_rate as i64)
}

/// Convert a Rational time to a sample index at the given sample rate.
pub fn time_to_samples(time: Rational, sample_rate: u32) -> u64 {
    let r = time.0 * num_rational::Rational::new(sample_rate as isize, 1);
    let n = *r.numer() as i64;
    let d = *r.denom() as i64;
    if d == 0 { 0 } else { (n / d).max(0) as u64 }
}

/// Frame counter for a stereo mix at the project's sample rate.
pub struct Mixer {
    pub sample_rate: u32,
    pub channels: u16,
    pub tracks: Vec<TrackMixer>,
}

impl Mixer {
    pub fn new(num_tracks: usize) -> Self {
        Mixer {
            sample_rate: SAMPLE_RATE,
            channels: CHANNELS,
            tracks: (0..num_tracks).map(|_| TrackMixer::default()).collect(),
        }
    }

    /// Mix the given track sample layers into one stereo output.
    /// Each layer's samples are pre-shaped to stereo interleaved.
    pub fn mix(&self, layers: &[Samples]) -> Samples {
        let track_mixers: Vec<TrackMixer> = self.tracks.iter().cloned().collect();
        let mut processed: Vec<Samples> = Vec::new();
        let solo_active = track_mixers.iter().any(|t| t.solo);
        for (i, layer) in layers.iter().enumerate() {
            let mut buf = layer.clone();
            if let Some(m) = track_mixers.get(i) {
                let mut m = m.clone();
                if solo_active && !m.solo { m.muted = true; }
                m.apply(&mut buf);
            }
            processed.push(buf);
        }
        let refs: Vec<&[f32]> = processed.iter().map(|p| p.as_slice()).collect();
        mix_stereo(&refs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f32, b: f32) -> bool { (a - b).abs() < 1e-4 }

    #[test]
    fn mix_sums_two_layers() {
        let a: Vec<f32> = vec![0.2, 0.2, 0.2, 0.2];
        let b: Vec<f32> = vec![0.3, 0.3, 0.3, 0.3];
        let out = mix_stereo(&[&a, &b]);
        for &v in &out { assert!(approx_eq(v, 0.5)); }
    }

    #[test]
    fn mix_clips_to_one() {
        let a: Vec<f32> = vec![0.8; 4];
        let b: Vec<f32> = vec![0.8; 4];
        let out = mix_stereo(&[&a, &b]);
        for &v in &out { assert_eq!(v, 1.0); }
    }

    #[test]
    fn mute_zeros_track() {
        let m = TrackMixer { muted: true, ..Default::default() };
        let mut samples = vec![0.5, 0.5, 0.5, 0.5];
        m.apply(&mut samples);
        for &v in &samples { assert_eq!(v, 0.0); }
    }

    #[test]
    fn pan_attenuates_opposite_channel() {
        // Pan fully left: right channel should be near 0
        let m = TrackMixer { pan: -1.0, ..Default::default() };
        let mut samples = vec![1.0, 1.0];
        m.apply(&mut samples);
        assert!(samples[0] > 0.99);
        assert!(samples[1] < 0.01);
        // Pan fully right: left channel should be near 0
        let m = TrackMixer { pan: 1.0, ..Default::default() };
        let mut samples = vec![1.0, 1.0];
        m.apply(&mut samples);
        assert!(samples[0] < 0.01);
        assert!(samples[1] > 0.99);
    }

    #[test]
    fn crossfade_at_half_uses_equal_power() {
        let a = vec![1.0f32; 4];
        let b = vec![1.0f32; 4];
        let out = crossfade(&a, &b, 0.5);
        // Equal-power crossfade at alpha=0.5: cos(π/4)² ≈ 0.5
        // So each output = 1.0 * cos(π/4) + 1.0 * cos(π/4) ≈ √2 ≈ 1.414
        for &v in &out {
            assert!(approx_eq(v, std::f32::consts::SQRT_2));
        }
    }

    #[test]
    fn samples_time_round_trip() {
        let sr = 48_000;
        let t = Rational::new(1, 2); // 0.5 seconds
        let n = time_to_samples(t, sr);
        assert_eq!(n, 24_000);
        let back = samples_to_time(n, sr);
        assert_eq!(back, t);
    }
}
