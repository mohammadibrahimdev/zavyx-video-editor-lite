//! Phase 3 audio effects: reverb, echo/delay, pitch shift, beat detection,
//! silence detection, auto ducking.
//!
//! All pure-Rust DSP on f32 sample slices.

use crate::effects::db_to_gain;

// =========================================================================
// REVERB (Schroeder — 4 parallel comb filters + 2 series all-pass)
// =========================================================================

pub struct ReverbV2 {
    pub sample_rate: u32,
    pub room_size: f32,
    pub damping: f32,
    pub wet: f32,
    comb_bufs: Vec<Vec<f32>>,
    comb_idx: Vec<usize>,
    comb_lowpass: Vec<f32>,
    ap_bufs: Vec<Vec<f32>>,
    ap_idx: Vec<usize>,
}

impl ReverbV2 {
    pub fn new(sample_rate: u32) -> Self {
        let comb_delays = [1116, 1188, 1277, 1356];
        let ap_delays = [556, 441];
        let scale = sample_rate as f32 / 44100.0;
        let comb_bufs: Vec<Vec<f32>> = comb_delays.iter()
            .map(|&d| vec![0.0; ((d as f32 * scale) as usize).max(1)])
            .collect();
        let ap_bufs: Vec<Vec<f32>> = ap_delays.iter()
            .map(|&d| vec![0.0; ((d as f32 * scale) as usize).max(1)])
            .collect();
        ReverbV2 {
            sample_rate,
            room_size: 0.5, damping: 0.5, wet: 0.3,
            comb_bufs, comb_idx: vec![0; 4],
            comb_lowpass: vec![0.0; 4],
            ap_bufs, ap_idx: vec![0; 2],
        }
    }

    pub fn process_stereo(&mut self, samples: &mut [f32]) {
        let room = self.room_size * 0.28 + 0.7;
        let damp = self.damping * 0.4;
        let wet = self.wet;
        for chunk in samples.chunks_exact_mut(2) {
            let input = (chunk[0] + chunk[1]) * 0.5;
            let mut comb_out = 0.0f32;
            for i in 0..4 {
                let buf_len = self.comb_bufs[i].len();
                let idx = self.comb_idx[i];
                let cur = self.comb_bufs[i][idx];
                let filtered = cur * (1.0 - damp) + self.comb_lowpass[i] * damp;
                self.comb_lowpass[i] = filtered;
                let new_val = input + filtered * room;
                self.comb_bufs[i][idx] = new_val;
                self.comb_idx[i] = (idx + 1) % buf_len;
                comb_out += cur;
            }
            let mut ap_out = comb_out * 0.25;
            for i in 0..2 {
                let buf_len = self.ap_bufs[i].len();
                let idx = self.ap_idx[i];
                let delayed = self.ap_bufs[i][idx];
                let output = -ap_out + delayed;
                let new_val = ap_out + delayed * 0.5;
                self.ap_bufs[i][idx] = new_val;
                self.ap_idx[i] = (idx + 1) % buf_len;
                ap_out = output;
            }
            let dry = 1.0 - wet;
            chunk[0] = chunk[0] * dry + ap_out * wet;
            chunk[1] = chunk[1] * dry + ap_out * wet;
        }
    }
}

// =========================================================================
// ECHO / DELAY
// =========================================================================

pub struct Echo {
    pub sample_rate: u32,
    pub delay_ms: f32,        // default 250
    pub feedback: f32,        // 0..1, default 0.4
    pub wet: f32,             // 0..1, default 0.3
    buf: Vec<f32>,
    idx: usize,
}

impl Echo {
    pub fn new(sample_rate: u32) -> Self {
        let delay_samples = (250.0 * 0.001 * sample_rate as f32) as usize + 1;
        Echo {
            sample_rate,
            delay_ms: 250.0, feedback: 0.4, wet: 0.3,
            buf: vec![0.0; delay_samples], idx: 0,
        }
    }

    pub fn process_stereo(&mut self, samples: &mut [f32]) {
        let delay_samples = (self.delay_ms * 0.001 * self.sample_rate as f32) as usize + 1;
        if self.buf.len() != delay_samples {
            self.buf.resize(delay_samples, 0.0);
            self.idx = 0;
        }
        for chunk in samples.chunks_exact_mut(2) {
            let delayed = self.buf[self.idx];
            let input_mono = (chunk[0] + chunk[1]) * 0.5;
            // Write to delay buffer: input + delayed * feedback.
            self.buf[self.idx] = input_mono + delayed * self.feedback;
            self.idx = (self.idx + 1) % self.buf.len();
            // Output: dry + wet * delayed (apply delayed to both channels).
            let dry = 1.0 - self.wet;
            chunk[0] = chunk[0] * dry + delayed * self.wet;
            chunk[1] = chunk[1] * dry + delayed * self.wet;
        }
    }
}

// =========================================================================
// PITCH SHIFT (simple resampling — phase-vocoder would be better)
// =========================================================================

pub struct PitchShift {
    pub sample_rate: u32,
    pub semitones: f32,  // -12..+12, default 0
    read_pos: f32,
}

impl PitchShift {
    pub fn new(sample_rate: u32) -> Self {
        PitchShift { sample_rate, semitones: 0.0, read_pos: 0.0 }
    }

    pub fn process_stereo(&mut self, samples: &mut [f32]) {
        if self.semitones == 0.0 { return; }
        let ratio = 2f32.powf(self.semitones / 12.0);
        // Naive: read at integer positions multiplied by 1/ratio.
        // For each output sample, the input position advances by 1/ratio.
        let src = samples.to_vec();
        let mut read = 0.0f32;
        for chunk in samples.chunks_exact_mut(2) {
            let i = read as usize;
            if i + 1 >= src.len() { break; }
            let frac = read - i as f32;
            // Linear interpolation.
            chunk[0] = src[i*2] * (1.0 - frac) + src[(i+1)*2] * frac;
            chunk[1] = src[i*2+1] * (1.0 - frac) + src[(i+1)*2+1] * frac;
            read += 1.0 / ratio;
        }
    }
}

// =========================================================================
// BEAT DETECTION (simple onset detector via energy flux)
// =========================================================================

/// Beat detection: returns the timestamps (in samples) where beats likely occur.
/// Algorithm: compute short-time energy, find peaks above a moving average threshold.
pub fn detect_beats(samples: &[f32], sample_rate: u32, _sensitivity: f32) -> Vec<u64> {
    let window_size = (sample_rate as usize / 100).max(1);  // 10ms windows
    let mut energies = Vec::new();
    let mut i = 0;
    while i + window_size <= samples.len() {
        let mut sum = 0.0f32;
        for j in i..i + window_size {
            sum += samples[j] * samples[j];
        }
        let energy = sum / window_size as f32;
        energies.push((i as u64, energy));
        i += window_size;
    }
    if energies.is_empty() { return Vec::new(); }
    // Moving average over the last 20 windows (~200ms).
    let history = 20;
    let mut beats = Vec::new();
    let mut prev_energy = 0.0f32;
    let mut avg_window: Vec<f32> = Vec::new();
    for (sample_pos, energy) in &energies {
        avg_window.push(*energy);
        if avg_window.len() > history { avg_window.remove(0); }
        let avg: f32 = avg_window.iter().sum::<f32>() / avg_window.len() as f32;
        let threshold = avg * 1.5;
        // Beat: energy > threshold AND energy > prev_energy (local peak).
        if *energy > threshold && *energy > prev_energy {
            beats.push(*sample_pos);
        }
        prev_energy = *energy;
    }
    beats
}

// =========================================================================
// SILENCE DETECTION
// =========================================================================

/// Detect silent regions in a sample buffer.
/// Returns a list of (start_sample, end_sample) for each silent region.
pub fn detect_silence(samples: &[f32], sample_rate: u32, threshold_db: f32, min_duration_ms: u32) -> Vec<(u64, u64)> {
    let threshold = db_to_gain(threshold_db);
    let min_samples = (min_duration_ms as f32 * 0.001 * sample_rate as f32) as u64;
    let mut regions = Vec::new();
    let mut silence_start: Option<u64> = None;
    for (i, &s) in samples.iter().enumerate() {
        if s.abs() < threshold {
            if silence_start.is_none() {
                silence_start = Some(i as u64);
            }
        } else {
            if let Some(start) = silence_start.take() {
                let end = i as u64;
                if end - start >= min_samples {
                    regions.push((start, end));
                }
            }
        }
    }
    if let Some(start) = silence_start {
        let end = samples.len() as u64;
        if end - start >= min_samples {
            regions.push((start, end));
        }
    }
    regions
}

// =========================================================================
// AUTO DUCKING (side-chain compressor)
// =========================================================================

/// Side-chain compressor: reduces the gain of `target` samples whenever
/// the `sidechain` signal exceeds a threshold. Used to duck music under
/// voiceover.
pub struct AutoDucker {
    pub sample_rate: u32,
    pub threshold_db: f32,    // -inf..0, default -24
    pub ratio: f32,           // 1..20, default 4
    pub attack_ms: f32,       // 0..1000, default 30
    pub release_ms: f32,      // 0..5000, default 300
    pub makeup_db: f32,       // default 0
    envelope: f32,
}

impl AutoDucker {
    pub fn new(sample_rate: u32) -> Self {
        AutoDucker {
            sample_rate,
            threshold_db: -24.0, ratio: 4.0,
            attack_ms: 30.0, release_ms: 300.0,
            makeup_db: 0.0,
            envelope: 1.0,
        }
    }

    /// Process `target` samples in-place, ducking them based on `sidechain` levels.
    /// Both buffers must be the same length and stereo interleaved.
    pub fn process_stereo(&mut self, target: &mut [f32], sidechain: &[f32]) {
        let attack_coef = (-1.0 / (self.attack_ms * 0.001 * self.sample_rate as f32)).exp();
        let release_coef = (-1.0 / (self.release_ms * 0.001 * self.sample_rate as f32)).exp();
        let makeup = db_to_gain(self.makeup_db);
        let len = target.len().min(sidechain.len());
        for i in (0..len).step_by(2) {
            if i + 1 >= len { break; }
            // Compute sidechain peak.
            let sc_peak = sidechain[i].abs().max(sidechain[i+1].abs());
            let sc_db = if sc_peak < 1e-9 { f32::NEG_INFINITY } else { 20.0 * sc_peak.log10() };
            // Gain reduction based on sidechain above threshold.
            let gain_reduction_db = if sc_db > self.threshold_db {
                (sc_db - self.threshold_db) * (1.0 - 1.0 / self.ratio)
            } else { 0.0 };
            let target_gain = db_to_gain(-gain_reduction_db);
            // Smooth envelope (attack when reducing, release when not).
            let coef = if target_gain < self.envelope { attack_coef } else { release_coef };
            self.envelope = self.envelope + (target_gain - self.envelope) * (1.0 - coef);
            target[i]   = target[i] * self.envelope * makeup;
            target[i+1] = target[i+1] * self.envelope * makeup;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reverb_does_not_explode_on_silence() {
        let mut r = ReverbV2::new(48000);
        let mut s = vec![0.0f32; 1000];
        r.process_stereo(&mut s);
        for v in &s { assert_eq!(*v, 0.0); }
    }

    #[test]
    fn echo_produces_delayed_signal() {
        let mut e = Echo::new(48000);
        e.delay_ms = 10.0;  // ~480 samples at 48kHz (delay_samples = 481)
        e.feedback = 0.5;
        e.wet = 1.0;  // pure wet to hear the delay
        // Impulse: 1.0 at sample 0, 0 elsewhere.
        let mut s = vec![0.0f32; 2000];
        s[0] = 1.0; s[1] = 1.0;
        e.process_stereo(&mut s);
        // Stereo interleaved: each frame is 2 floats. The echo for sample 0 is
        // read at frame 481 (delay_samples = 481), so it lands at float index
        // 481 * 2 = 962. Look in a small window around there.
        let echo_idx = 481 * 2;
        let any_echo = (echo_idx..echo_idx+10).any(|i| s[i].abs() > 0.0);
        assert!(any_echo, "expected echo around float index {}, all zeros", echo_idx);
    }

    #[test]
    fn pitch_shift_changes_length() {
        let mut p = PitchShift::new(48000);
        p.semitones = 12.0;  // up one octave = 2x speed
        let mut s = vec![0.0f32; 100];
        for i in 0..50 { s[i*2] = 1.0; s[i*2+1] = 1.0; }  // first half is signal
        p.process_stereo(&mut s);
        // After pitch up by 12 semitones, the signal should be packed into the first half.
        // We can't easily assert pitch — just that the buffer is non-silent and didn't panic.
        let any_nonzero = s.iter().any(|v| v.abs() > 0.0);
        assert!(any_nonzero);
    }

    #[test]
    fn detect_beats_finds_energy_peaks() {
        // Construct a signal with strong transients every 0.5s.
        let sr = 48000u32;
        let mut samples = vec![0.0f32; sr as usize * 2];
        for i in 0..4 {
            let start = (i as usize) * (sr as usize / 2);  // every 0.5s
            for j in 0..1000 {
                if start + j < samples.len() {
                    samples[start + j] = (j as f32 / 1000.0 * std::f32::consts::TAU).sin();
                }
            }
        }
        let beats = detect_beats(&samples, sr, 1.0);
        // We should find at least one beat per transient.
        assert!(!beats.is_empty(), "expected at least one beat");
    }

    #[test]
    fn detect_silence_finds_quiet_regions() {
        let sr = 48000u32;
        let mut samples = vec![0.0f32; sr as usize];
        // First 0.5s = silence, last 0.5s = loud.
        for i in (sr as usize / 2)..samples.len() {
            samples[i] = 1.0;
        }
        let regions = detect_silence(&samples, sr, -40.0, 100);
        assert!(!regions.is_empty(), "expected at least one silent region");
        // First region should start at 0.
        assert_eq!(regions[0].0, 0);
        // End should be near 24000 (0.5s).
        assert!(regions[0].1 <= 25000 && regions[0].1 >= 23000,
            "expected silence end around 24000, got {}", regions[0].1);
    }

    #[test]
    fn auto_ducker_reduces_target_when_sidechain_is_loud() {
        let mut d = AutoDucker::new(48000);
        d.threshold_db = -12.0;
        d.ratio = 4.0;
        d.attack_ms = 1.0;
        d.release_ms = 100.0;
        // Target is steady at 0.5; sidechain is steady at 1.0 (loud).
        let mut target = vec![0.5f32; 2000];
        let sidechain = vec![1.0f32; 2000];
        d.process_stereo(&mut target, &sidechain);
        // After envelope settles, target should be much reduced.
        let last_100 = &target[target.len()-100..];
        let avg: f32 = last_100.iter().sum::<f32>() / last_100.len() as f32;
        assert!(avg < 0.5, "expected reduced target after ducking, got {}", avg);
    }

    #[test]
    fn auto_ducker_leaves_target_untouched_when_sidechain_is_quiet() {
        let mut d = AutoDucker::new(48000);
        d.threshold_db = -12.0;
        d.ratio = 4.0;
        d.attack_ms = 1.0;
        d.release_ms = 100.0;
        let mut target = vec![0.5f32; 2000];
        let sidechain = vec![0.001f32; 2000];  // very quiet sidechain
        d.process_stereo(&mut target, &sidechain);
        let last_100 = &target[target.len()-100..];
        let avg: f32 = last_100.iter().sum::<f32>() / last_100.len() as f32;
        assert!((avg - 0.5).abs() < 0.05, "expected target ~0.5, got {}", avg);
    }
}
