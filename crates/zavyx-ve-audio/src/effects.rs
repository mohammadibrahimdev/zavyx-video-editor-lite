//! Audio effects — pure-Rust DSP that operates on f32 sample slices in-place.
//!
//! Phase 2 effects:
//! - `FadeIn` / `FadeOut`: linear or equal-power gain ramp
//! - `Gain`: constant gain in dB
//! - `Normalize`: scale so peak hits a target dBFS
//! - `SimpleEQ`: 3-band (low/mid/high) biquad-ish shelving filter
//! - `Compressor`: simple feed-forward compressor with threshold/ratio/attack/release
//!
//! All effects are pure functions on `&mut [f32]` sample buffers (stereo interleaved
//! by default, but most accept mono too via the `channels` parameter).

use crate::Rational;

/// Apply a linear fade-in over the first `duration_samples` samples.
/// Scales gain from 0 to 1 over that range; samples after the fade are untouched.
pub fn fade_in(samples: &mut [f32], fade_samples: usize) {
    let n = fade_samples.min(samples.len());
    for i in 0..n {
        let gain = i as f32 / n as f32;
        samples[i] *= gain;
    }
}

/// Apply a linear fade-out over the last `duration_samples` samples.
/// Scales gain from 1 to 0 over that range.
pub fn fade_out(samples: &mut [f32], fade_samples: usize) {
    let total = samples.len();
    let n = fade_samples.min(total);
    if n == 0 { return; }
    let start = total - n;
    for i in 0..n {
        // At i=0: gain = 1.0 (start of fade)
        // At i=n-1: gain = 1/n (near zero, not exactly zero to avoid div by zero)
        // We use (i+1)/n so the last sample hits gain=0 exactly when i+1=n.
        let gain = 1.0 - ((i + 1) as f32 / n as f32);
        samples[start + i] *= gain;
    }
}

/// Apply equal-power fade-in (constant-power crossfade gain curve).
pub fn fade_in_equal_power(samples: &mut [f32], fade_samples: usize) {
    let n = fade_samples.min(samples.len());
    for i in 0..n {
        let t = i as f32 / n as f32;
        let gain = (t * std::f32::consts::FRAC_PI_2).sin();
        samples[i] *= gain;
    }
}

/// Apply equal-power fade-out.
pub fn fade_out_equal_power(samples: &mut [f32], fade_samples: usize) {
    let total = samples.len();
    let n = fade_samples.min(total);
    let start = total - n;
    for i in 0..n {
        let t = i as f32 / n as f32;
        let gain = ((1.0 - t) * std::f32::consts::FRAC_PI_2).sin();
        samples[start + i] *= gain;
    }
}

/// Convert a dB value to a linear gain factor.
pub fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// Convert a linear gain factor (>= 0) to dB.
pub fn gain_to_db(gain: f32) -> f32 {
    if gain <= 0.0 { f32::NEG_INFINITY } else { 20.0 * gain.log10() }
}

/// Apply a constant gain (linear) to all samples.
pub fn apply_gain(samples: &mut [f32], gain: f32) {
    if gain == 1.0 { return; }
    for s in samples.iter_mut() {
        *s *= gain;
    }
}

/// Apply a constant gain specified in dB.
pub fn apply_gain_db(samples: &mut [f32], db: f32) {
    apply_gain(samples, db_to_gain(db));
}

/// Find the peak absolute amplitude in a sample slice.
pub fn peak_amplitude(samples: &[f32]) -> f32 {
    samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max)
}

/// Find the RMS amplitude over a slice.
pub fn rms_amplitude(samples: &[f32]) -> f32 {
    if samples.is_empty() { return 0.0; }
    let sum_sq: f32 = samples.iter().map(|s| s * s).sum();
    (sum_sq / samples.len() as f32).sqrt()
}

/// Normalize so the peak hits `target_db` dBFS (default -1.0 dBFS).
/// No-op if the signal is silent.
pub fn normalize(samples: &mut [f32], target_db: f32) {
    let peak = peak_amplitude(samples);
    if peak < 1e-9 { return; }
    let target_gain = db_to_gain(target_db);
    let scale = target_gain / peak;
    apply_gain(samples, scale);
}

/// 3-band simple EQ using one-pole shelving filters.
/// Applies low-shelf at `low_freq_hz`, high-shelf at `high_freq_hz`, and a
/// peaking filter at `mid_freq_hz`. Gains are in dB.
pub struct SimpleEQ {
    pub sample_rate: u32,
    pub low_gain_db:  f32,
    pub mid_gain_db:  f32,
    pub high_gain_db: f32,
    pub low_freq_hz:  f32,   // default 200
    pub mid_freq_hz:  f32,   // default 1000
    pub high_freq_hz: f32,   // default 5000
    // Internal state for one-pole filters (mono; for stereo we process L/R separately)
    low_state_l: f32, low_state_r: f32,
    high_state_l: f32, high_state_r: f32,
}

impl SimpleEQ {
    pub fn new(sample_rate: u32) -> Self {
        SimpleEQ {
            sample_rate,
            low_gain_db: 0.0, mid_gain_db: 0.0, high_gain_db: 0.0,
            low_freq_hz: 200.0, mid_freq_hz: 1000.0, high_freq_hz: 5000.0,
            low_state_l: 0.0, low_state_r: 0.0,
            high_state_l: 0.0, high_state_r: 0.0,
        }
    }

    /// Process a stereo interleaved buffer in-place.
    pub fn process_stereo(&mut self, samples: &mut [f32]) {
        let dt = 1.0 / self.sample_rate as f32;
        // One-pole low-shelf coefficient.
        let low_alpha = 1.0 - (-2.0 * std::f32::consts::PI * self.low_freq_hz * dt).exp();
        let high_alpha = 1.0 - (-2.0 * std::f32::consts::PI * self.high_freq_hz * dt).exp();
        let low_gain = db_to_gain(self.low_gain_db);
        let high_gain = db_to_gain(self.high_gain_db);
        let mid_gain = db_to_gain(self.mid_gain_db);
        for chunk in samples.chunks_exact_mut(2) {
            let (l, r) = (chunk[0], chunk[1]);
            // Low-shelf: extract low-pass component, scale by low_gain.
            self.low_state_l = self.low_state_l + low_alpha * (l - self.low_state_l);
            self.low_state_r = self.low_state_r + low_alpha * (r - self.low_state_r);
            // High-shelf: extract high-pass component.
            self.high_state_l = self.high_state_l + high_alpha * (l - self.high_state_l);
            self.high_state_r = self.high_state_r + high_alpha * (r - self.high_state_r);
            let lp_l = self.low_state_l; let lp_r = self.low_state_r;
            let hp_l = l - self.high_state_l; let hp_r = r - self.high_state_r;
            let mid_l = l - lp_l - hp_l; let mid_r = r - lp_r - hp_r;
            // Mix: low*low_gain + mid*mid_gain + high*high_gain
            chunk[0] = lp_l * low_gain + mid_l * mid_gain + hp_l * high_gain;
            chunk[1] = lp_r * low_gain + mid_r * mid_gain + hp_r * high_gain;
        }
    }
}

/// Simple feed-forward compressor.
pub struct Compressor {
    pub sample_rate: u32,
    pub threshold_db: f32,  // default -24
    pub ratio: f32,         // default 4 (4:1)
    pub attack_ms: f32,     // default 10
    pub release_ms: f32,   // default 100
    pub makeup_db: f32,     // default 0
    envelope: f32,
}

impl Compressor {
    pub fn new(sample_rate: u32) -> Self {
        Compressor {
            sample_rate,
            threshold_db: -24.0, ratio: 4.0,
            attack_ms: 10.0, release_ms: 100.0, makeup_db: 0.0,
            envelope: 1.0,  // start with unity gain (no reduction)
        }
    }

    /// Process a stereo interleaved buffer in-place.
    pub fn process_stereo(&mut self, samples: &mut [f32]) {
        let attack_coef = (-1.0 / (self.attack_ms * 0.001 * self.sample_rate as f32)).exp();
        let release_coef = (-1.0 / (self.release_ms * 0.001 * self.sample_rate as f32)).exp();
        let makeup = db_to_gain(self.makeup_db);
        for chunk in samples.chunks_exact_mut(2) {
            let l = chunk[0]; let r = chunk[1];
            let peak = l.abs().max(r.abs());
            let db_in = if peak < 1e-9 { f32::NEG_INFINITY } else { 20.0 * peak.log10() };
            // Gain computer: above threshold, gain reduction = (db_in - threshold) * (1 - 1/ratio)
            let gain_reduction_db = if db_in > self.threshold_db {
                (db_in - self.threshold_db) * (1.0 - 1.0 / self.ratio)
            } else { 0.0 };
            let target_gain = db_to_gain(-gain_reduction_db);
            // Smooth envelope (attack if reducing, release if not)
            let coef = if target_gain < self.envelope { attack_coef } else { release_coef };
            self.envelope = self.envelope + (target_gain - self.envelope) * (1.0 - coef);
            // Apply
            chunk[0] = l * self.envelope * makeup;
            chunk[1] = r * self.envelope * makeup;
        }
    }
}

/// Convert a Rational time to a sample count at a given sample rate.
pub fn time_to_samples(time: Rational, sample_rate: u32) -> usize {
    let s = time.to_f64() * sample_rate as f64;
    s.round() as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fade_in_scales_from_zero_to_one() {
        let mut s = vec![1.0f32; 100];
        fade_in(&mut s, 10);
        // First sample should be 0.
        assert!((s[0] - 0.0).abs() < 1e-6, "first sample was {}", s[0]);
        // Sample 10 should be back to 1.0 (gain = 10/10 = 1.0).
        assert!((s[10] - 1.0).abs() < 1e-6);
        // Sample 11 onwards untouched (gain > 1.0? no — gain at i is i/n).
        assert_eq!(s[20], 1.0);
    }

    #[test]
    fn fade_out_scales_from_one_to_zero() {
        let mut s = vec![1.0f32; 100];
        fade_out(&mut s, 10);
        // Last sample should be 0 (gain = 1.0 - 10/10 = 0).
        assert!((s[99] - 0.0).abs() < 1e-6, "last sample was {}", s[99]);
        // First sample of the fade (index 90) should have gain = 1 - 1/10 = 0.9.
        assert!((s[90] - 0.9).abs() < 1e-3, "first fade sample was {}", s[90]);
    }

    #[test]
    fn fade_in_equal_power_uses_sin_curve() {
        let mut s = vec![1.0f32; 100];
        fade_in_equal_power(&mut s, 100);
        // At i=0, sin(0) = 0.
        assert!((s[0] - 0.0).abs() < 1e-6);
        // At i=50 (midpoint), sin(π/4) ≈ 0.707.
        assert!((s[50] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-3);
        // At i=99 (near end), sin(π/2) ≈ 1.0
        assert!((s[99] - 1.0).abs() < 0.01);
    }

    #[test]
    fn db_gain_conversions() {
        assert!((db_to_gain(0.0) - 1.0).abs() < 1e-6);
        assert!((db_to_gain(6.0) - 1.995).abs() < 0.01);  // +6dB ≈ 2x
        assert!((db_to_gain(-6.0) - 0.501).abs() < 0.01); // -6dB ≈ 0.5x
        assert_eq!(gain_to_db(0.0), f32::NEG_INFINITY);
        assert!((gain_to_db(1.0) - 0.0).abs() < 1e-6);
        assert!((gain_to_db(2.0) - 6.02).abs() < 0.01);
    }

    #[test]
    fn apply_gain_scales_samples() {
        let mut s = vec![0.5f32, 0.5, 0.5];
        apply_gain(&mut s, 2.0);
        for v in &s { assert!((v - 1.0).abs() < 1e-6); }
    }

    #[test]
    fn apply_gain_db_scales_samples() {
        let mut s = vec![0.5f32, 0.5, 0.5];
        apply_gain_db(&mut s, 6.0);  // +6dB ≈ 2x
        for v in &s { assert!((v - 1.0).abs() < 0.01); }
    }

    #[test]
    fn peak_amplitude_finds_max() {
        let s = vec![0.1f32, -0.5, 0.3, -0.8, 0.2];
        assert_eq!(peak_amplitude(&s), 0.8);
    }

    #[test]
    fn rms_amplitude_computes_root_mean_square() {
        let s = vec![1.0f32, -1.0, 1.0, -1.0];
        let r = rms_amplitude(&s);
        assert!((r - 1.0).abs() < 1e-6, "RMS of ±1 should be 1, got {}", r);
    }

    #[test]
    fn normalize_brings_peak_to_target() {
        let mut s = vec![0.1f32, -0.5, 0.3, -0.8, 0.2];
        normalize(&mut s, -1.0); // target -1 dBFS ≈ 0.891
        let peak = peak_amplitude(&s);
        let target = db_to_gain(-1.0);
        assert!((peak - target).abs() < 0.01, "peak {} should be ~{}", peak, target);
    }

    #[test]
    fn normalize_silent_signal_is_noop() {
        let mut s = vec![0.0f32; 10];
        normalize(&mut s, -1.0);
        for v in &s { assert_eq!(*v, 0.0); }
    }

    #[test]
    fn simple_eq_does_not_explode_on_silence() {
        let mut eq = SimpleEQ::new(48000);
        eq.low_gain_db = 6.0; eq.mid_gain_db = -3.0; eq.high_gain_db = 6.0;
        let mut s = vec![0.0f32; 100];
        eq.process_stereo(&mut s);
        for v in &s { assert_eq!(*v, 0.0); }
    }

    #[test]
    fn simple_eq_passes_signal_through() {
        let mut eq = SimpleEQ::new(48000);
        // All gains 0 dB → identity (with some filter smearing).
        let mut s = vec![0.5f32; 100];
        eq.process_stereo(&mut s);
        // First samples may have filter transient, but steady-state should be ~0.5.
        let last = s[s.len()-1];
        assert!(last.abs() > 0.4 && last.abs() < 0.6,
            "expected pass-through ~0.5, got {}", last);
    }

    #[test]
    fn compressor_reduces_loud_signals() {
        let mut comp = Compressor::new(48000);
        comp.threshold_db = -12.0; // anything above -12 dBFS gets compressed
        comp.ratio = 4.0;
        comp.attack_ms = 1.0;     // fast attack so the envelope tracks within our short buffer
        comp.release_ms = 50.0;
        // Signal at 0 dBFS (peak = 1.0) is +12 dB above threshold.
        // After enough samples the envelope settles to ~0.355 (gain for -9 dB reduction).
        let mut s = vec![1.0f32; 1000];
        comp.process_stereo(&mut s);
        // Look at the steady-state value (last 100 samples).
        let last_100 = &s[s.len()-100..];
        let peak = peak_amplitude(last_100);
        // The compressor should reduce the peak significantly.
        assert!(peak < 0.5, "expected reduced peak, got {}", peak);
    }

    #[test]
    fn compressor_leaves_quiet_signals_untouched() {
        let mut comp = Compressor::new(48000);
        comp.threshold_db = -12.0;
        // Signal at -24 dBFS (peak ≈ 0.063) is well below threshold.
        let mut s = vec![0.063f32; 8];
        comp.process_stereo(&mut s);
        let peak = peak_amplitude(&s);
        // With no makeup gain, peak should be unchanged (within envelope smoothing).
        assert!((peak - 0.063).abs() < 0.01, "expected ~0.063, got {}", peak);
    }

    #[test]
    fn time_to_samples_rounds_correctly() {
        let t = Rational::new(1, 2);  // 0.5s
        let n = time_to_samples(t, 48000);
        assert_eq!(n, 24_000);
    }
}
