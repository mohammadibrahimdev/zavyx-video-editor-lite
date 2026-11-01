//! Playback clock + scheduler.
//!
//! Audio-driven playback: a cpal callback pulls N samples per call from a
//! `Mixer`, advancing the audio clock. A separate video scheduler thread
//! polls the audio clock and pulls compositor frames, pushing them to a
//! bounded preview queue.
//!
//! Phase 4: pure-Rust `Clock` + `PreviewQueue` + `PlayerController` that
//! can be tested without a real audio device. The `cpal_stream` field on
//! `Player` is feature-gated behind `playback`.

use std::sync::Arc;
use parking_lot::Mutex;
use crossbeam_channel::{bounded, Sender, Receiver};

use zavyx_ve_core::Rational;

/// Sample-accurate audio clock. The audio callback advances it; the video
/// scheduler reads it.
#[derive(Debug)]
pub struct Clock {
    inner: Mutex<ClockInner>,
}

#[derive(Debug, Clone, Copy)]
struct ClockInner {
    /// Current playback position in seconds.
    position: Rational,
    /// 1.0 = normal, 0.5 = half speed, 2.0 = double, 0.0 = paused.
    speed: f64,
    /// Whether playback is currently active.
    playing: bool,
    /// Sample rate the clock is being advanced at (used to convert sample
    /// deltas to seconds).
    sample_rate: u32,
}

impl Default for Clock {
    fn default() -> Self {
        Clock {
            inner: Mutex::new(ClockInner {
                position: Rational::ZERO,
                speed: 1.0,
                playing: false,
                sample_rate: 48_000,
            }),
        }
    }
}

impl Clock {
    pub fn new(sample_rate: u32) -> Self {
        Clock {
            inner: Mutex::new(ClockInner {
                position: Rational::ZERO,
                speed: 1.0,
                playing: false,
                sample_rate,
            }),
        }
    }

    /// Read the current playback position.
    pub fn position(&self) -> Rational {
        self.inner.lock().position
    }

    /// Advance the clock by N samples. Called from the audio callback.
    pub fn advance_samples(&self, n: u64) {
        let mut inner = self.inner.lock();
        if !inner.playing { return; }
        let dt = Rational::new(n as i64, inner.sample_rate as i64);
        let speed = Rational::new((inner.speed * 1000.0).round() as i64, 1000);
        let advance = dt * speed;
        inner.position = inner.position + advance;
    }

    /// Advance by a fractional second amount (for testing without sample math).
    pub fn advance_seconds(&self, secs: f64) {
        let mut inner = self.inner.lock();
        if !inner.playing { return; }
        let dt = Rational::from_seconds(secs);
        let speed = Rational::new((inner.speed * 1000.0).round() as i64, 1000);
        inner.position = inner.position + dt * speed;
    }

    pub fn set_speed(&self, speed: f64) {
        self.inner.lock().speed = speed;
    }

    pub fn speed(&self) -> f64 {
        self.inner.lock().speed
    }

    pub fn play(&self) {
        self.inner.lock().playing = true;
    }

    pub fn pause(&self) {
        self.inner.lock().playing = false;
    }

    pub fn is_playing(&self) -> bool {
        self.inner.lock().playing
    }

    pub fn seek(&self, position: Rational) {
        let mut inner = self.inner.lock();
        inner.position = position;
    }
}

/// A bounded preview queue of decoded RGBA frames. The scheduler pushes;
/// the GUI pulls the latest.
pub struct PreviewQueue {
    tx: Sender<PreviewFrame>,
    rx: Receiver<PreviewFrame>,
    capacity: usize,
    dropped_count: Arc<Mutex<u64>>,
}

/// One entry in the preview queue.
#[derive(Clone)]
pub struct PreviewFrame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    pub pts: Rational,
}

impl PreviewQueue {
    pub fn new(capacity: usize) -> Self {
        let (tx, rx) = bounded(capacity);
        PreviewQueue {
            tx, rx, capacity,
            dropped_count: Arc::new(Mutex::new(0)),
        }
    }

    /// Push a frame; if the queue is full, drop the oldest and increment
    /// the dropped counter.
    pub fn push(&self, frame: PreviewFrame) {
        if self.tx.is_full() {
            // Drain one to make room. Use try_recv.
            let _ = self.rx.try_recv();
            *self.dropped_count.lock() += 1;
        }
        let _ = self.tx.try_send(frame);
    }

    /// Pull the latest frame, dropping any older ones in the queue.
    pub fn latest(&self) -> Option<PreviewFrame> {
        let mut last = None;
        while let Ok(f) = self.rx.try_recv() {
            last = Some(f);
        }
        last
    }

    /// Pull one frame (FIFO) without draining.
    pub fn pop(&self) -> Option<PreviewFrame> {
        self.rx.try_recv().ok()
    }

    pub fn dropped_count(&self) -> u64 {
        *self.dropped_count.lock()
    }

    pub fn is_full(&self) -> bool {
        self.tx.is_full()
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn clear(&self) {
        while self.rx.try_recv().is_ok() {}
    }
}

/// The PlayerController owns the Clock and PreviewQueue. The GUI / CLI
/// drives it via `play()`, `pause()`, `seek()`, and reads frames via
/// `latest_frame()`.
pub struct PlayerController {
    pub clock: Arc<Clock>,
    pub queue: Arc<PreviewQueue>,
}

impl PlayerController {
    pub fn new(sample_rate: u32, queue_capacity: usize) -> Self {
        PlayerController {
            clock: Arc::new(Clock::new(sample_rate)),
            queue: Arc::new(PreviewQueue::new(queue_capacity)),
        }
    }

    pub fn play(&self) {
        self.clock.play();
    }

    pub fn pause(&self) {
        self.clock.pause();
    }

    pub fn seek(&self, t: Rational) {
        self.clock.seek(t);
        self.queue.clear();
    }

    pub fn position(&self) -> Rational {
        self.clock.position()
    }

    pub fn latest_frame(&self) -> Option<PreviewFrame> {
        self.queue.latest()
    }

    pub fn dropped_frames(&self) -> u64 {
        self.queue.dropped_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_starts_at_zero_and_paused() {
        let c = Clock::new(48000);
        assert_eq!(c.position(), Rational::ZERO);
        assert!(!c.is_playing());
    }

    #[test]
    fn clock_does_not_advance_when_paused() {
        let c = Clock::new(48000);
        c.advance_samples(480);  // 10ms at 48kHz
        assert_eq!(c.position(), Rational::ZERO);
    }

    #[test]
    fn clock_advances_when_playing() {
        let c = Clock::new(48000);
        c.play();
        c.advance_samples(480);  // 10ms at 48kHz
        let pos = c.position();
        assert!((pos.to_f64() - 0.01).abs() < 1e-6, "expected 0.01s, got {}", pos.to_f64());
    }

    #[test]
    fn clock_halves_at_half_speed() {
        let c = Clock::new(48000);
        c.set_speed(0.5);
        c.play();
        c.advance_samples(480);  // 10ms of audio at half speed = 5ms of timeline
        let pos = c.position();
        assert!((pos.to_f64() - 0.005).abs() < 1e-6, "expected 0.005s, got {}", pos.to_f64());
    }

    #[test]
    fn clock_seek_resets_position() {
        let c = Clock::new(48000);
        c.play();
        c.advance_samples(480);
        c.seek(Rational::new(5, 1));
        assert_eq!(c.position(), Rational::new(5, 1));
    }

    #[test]
    fn preview_queue_pushes_and_pops() {
        let q = PreviewQueue::new(3);
        q.push(PreviewFrame { width: 1, height: 1, pixels: vec![0; 4], pts: Rational::ZERO });
        q.push(PreviewFrame { width: 1, height: 1, pixels: vec![0; 4], pts: Rational::new(1, 30) });
        let f = q.pop();
        assert!(f.is_some());
        assert_eq!(f.unwrap().pts, Rational::ZERO);
    }

    #[test]
    fn preview_queue_latest_drops_older() {
        let q = PreviewQueue::new(5);
        for i in 0..3 {
            q.push(PreviewFrame {
                width: 1, height: 1, pixels: vec![0; 4],
                pts: Rational::new(i, 1),
            });
        }
        let latest = q.latest();
        assert!(latest.is_some());
        assert_eq!(latest.unwrap().pts, Rational::new(2, 1));
    }

    #[test]
    fn preview_queue_drops_oldest_when_full() {
        let q = PreviewQueue::new(2);
        q.push(PreviewFrame { width: 1, height: 1, pixels: vec![0; 4], pts: Rational::ZERO });
        q.push(PreviewFrame { width: 1, height: 1, pixels: vec![0; 4], pts: Rational::new(1, 1) });
        q.push(PreviewFrame { width: 1, height: 1, pixels: vec![0; 4], pts: Rational::new(2, 1) });
        assert!(q.dropped_count() >= 1, "expected at least 1 dropped, got {}", q.dropped_count());
    }

    #[test]
    fn player_controller_play_pauses() {
        let p = PlayerController::new(48000, 3);
        assert!(!p.clock.is_playing());
        p.play();
        assert!(p.clock.is_playing());
        p.pause();
        assert!(!p.clock.is_playing());
    }

    #[test]
    fn player_controller_seek_clears_queue() {
        let p = PlayerController::new(48000, 5);
        p.queue.push(PreviewFrame { width: 1, height: 1, pixels: vec![0; 4], pts: Rational::ZERO });
        assert_eq!(p.queue.pop().map(|_| 1).unwrap_or(0), 1);
        p.queue.push(PreviewFrame { width: 1, height: 1, pixels: vec![0; 4], pts: Rational::ZERO });
        p.seek(Rational::new(5, 1));
        assert!(p.queue.pop().is_none(), "queue should be empty after seek");
    }
}
