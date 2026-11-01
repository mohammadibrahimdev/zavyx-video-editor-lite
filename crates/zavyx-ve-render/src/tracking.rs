//! Motion tracking and video stabilization.
//!
//! Phase 4 implements:
//! - `MotionTracker` — Lucas-Kanade sparse optical flow tracking of a point
//!   across frames. Returns per-frame (dx, dy) displacement.
//! - `Stabilizer` — uses the tracker to compute per-frame shifts, then inverts
//!   them to stabilize the video (apply inverse shift).
//!
//! Both are pure-Rust and operate on RGBA8 frames.

use zavyx_ve_core::Rational;

/// A point to track, in pixel coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrackPoint {
    pub x: f32,
    pub y: f32,
}

/// Per-frame displacement of a tracked point relative to its initial position.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Displacement {
    pub dx: f32,
    pub dy: f32,
}

/// Lucas-Kanade sparse optical flow tracker.
///
/// Tracks a single point across a sequence of frames. The point's position
/// is updated after each `track()` call so successive calls follow the point.
pub struct MotionTracker {
    /// Current position of the tracked point.
    pub point: TrackPoint,
    /// Half-size of the search window (in pixels). The tracker searches a
    /// (2*window+1)² neighborhood.
    pub window: u32,
    /// Stored previous frame (grayscale luma) for differencing.
    prev_luma: Vec<f32>,
    prev_width: u32,
    prev_height: u32,
    /// History of displacements from frame to frame.
    pub displacements: Vec<Displacement>,
}

impl MotionTracker {
    pub fn new(point: TrackPoint, window: u32) -> Self {
        MotionTracker {
            point, window,
            prev_luma: Vec::new(),
            prev_width: 0, prev_height: 0,
            displacements: Vec::new(),
        }
    }

    /// Compute the luma (grayscale) buffer for a frame.
    fn luma_buffer(pixels: &[u8], w: u32, h: u32) -> Vec<f32> {
        let mut out = Vec::with_capacity((w * h) as usize);
        for px in pixels.chunks_exact(4) {
            let r = px[0] as f32 / 255.0;
            let g = px[1] as f32 / 255.0;
            let b = px[2] as f32 / 255.0;
            out.push(0.2126 * r + 0.7152 * g + 0.0722 * b);
        }
        let _ = h;
        out
    }

    /// Track the point from the previous frame to this frame. Updates
    /// `self.point` and appends to `self.displacements`.
    pub fn track(&mut self, pixels: &[u8], w: u32, h: u32) {
        let cur_luma = Self::luma_buffer(pixels, w, h);
        if self.prev_luma.is_empty() || self.prev_width != w || self.prev_height != h {
            // First frame: just store, no displacement.
            self.prev_luma = cur_luma;
            self.prev_width = w;
            self.prev_height = h;
            self.displacements.push(Displacement::default());
            return;
        }
        // Lucas-Kanade: minimize sum of squared differences between prev_patch
        // and cur_patch offset by (dx, dy).
        //
        // For simplicity we use a brute-force search over integer offsets in
        // [-window, +window] and pick the one with minimum SSD.
        let win = self.window as i32;
        let px = self.point.x as i32;
        let py = self.point.y as i32;
        let mut best_dx = 0i32;
        let mut best_dy = 0i32;
        let mut best_ssd = f32::MAX;
        for dy in -win..=win {
            for dx in -win..=win {
                let mut ssd = 0.0;
                let mut count = 0;
                let half = (self.window / 2) as i32;
                for sy in -half..=half {
                    for sx in -half..=half {
                        let prev_x = (px + sx).max(0).min(w as i32 - 1) as usize;
                        let prev_y = (py + sy).max(0).min(h as i32 - 1) as usize;
                        let cur_x = (px + sx + dx).max(0).min(w as i32 - 1) as usize;
                        let cur_y = (py + sy + dy).max(0).min(h as i32 - 1) as usize;
                        let prev_idx = prev_y * w as usize + prev_x;
                        let cur_idx = cur_y * w as usize + cur_x;
                        if prev_idx < self.prev_luma.len() && cur_idx < cur_luma.len() {
                            let diff = self.prev_luma[prev_idx] - cur_luma[cur_idx];
                            ssd += diff * diff;
                            count += 1;
                        }
                    }
                }
                if count > 0 {
                    ssd /= count as f32;
                    if ssd < best_ssd {
                        best_ssd = ssd;
                        best_dx = dx;
                        best_dy = dy;
                    }
                }
            }
        }
        // Update point.
        self.point.x += best_dx as f32;
        self.point.y += best_dy as f32;
        self.displacements.push(Displacement {
            dx: best_dx as f32,
            dy: best_dy as f32,
        });
        self.prev_luma = cur_luma;
    }
}

/// Video stabilizer: smooths out camera shake by tracking a point and
/// applying the inverse of its (smoothed) displacement.
pub struct Stabilizer {
    pub tracker: MotionTracker,
    /// Smoothing window (in frames). Larger = smoother but more lag.
    pub smoothing: u32,
    /// Accumulated displacement history (for smoothing).
    history: Vec<Displacement>,
}

impl Stabilizer {
    pub fn new(point: TrackPoint, window: u32, smoothing: u32) -> Self {
        Stabilizer {
            tracker: MotionTracker::new(point, window),
            smoothing,
            history: Vec::new(),
        }
    }

    /// Process one frame: track, then return the (dx, dy) shift to apply to
    /// stabilize the next frame. The GUI/renderer applies the shift via
    /// `TransformEffect` or by directly shifting pixels.
    pub fn process(&mut self, pixels: &[u8], w: u32, h: u32) -> Displacement {
        self.tracker.track(pixels, w, h);
        let last = *self.tracker.displacements.last().unwrap_or(&Displacement::default());
        self.history.push(last);
        // Keep only the last `smoothing` frames.
        if self.history.len() > self.smoothing as usize {
            self.history.remove(0);
        }
        // Compute the smoothed (average) displacement.
        let sum_dx: f32 = self.history.iter().map(|d| d.dx).sum();
        let sum_dy: f32 = self.history.iter().map(|d| d.dy).sum();
        let n = self.history.len() as f32;
        let avg_dx = sum_dx / n;
        let avg_dy = sum_dy / n;
        // Inverse shift: subtract the smoothed displacement from the latest.
        Displacement {
            dx: -(last.dx - avg_dx),
            dy: -(last.dy - avg_dy),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_frame(w: u32, h: u32, fill: u8) -> Vec<u8> {
        vec![fill; (w * h * 4) as usize]
    }

    #[test]
    fn tracker_first_frame_no_displacement() {
        let mut t = MotionTracker::new(TrackPoint { x: 4.0, y: 4.0 }, 2);
        let f = make_frame(8, 8, 128);
        t.track(&f, 8, 8);
        assert_eq!(t.displacements.len(), 1);
        assert_eq!(t.displacements[0], Displacement::default());
    }

    #[test]
    fn tracker_detects_horizontal_shift() {
        // Frame 0: white pixel at (4, 4).
        // Frame 1: white pixel at (5, 4) — shifted right by 1.
        let mut f0 = make_frame(16, 16, 0);
        let mut f1 = make_frame(16, 16, 0);
        let set = |f: &mut Vec<u8>, w: u32, x: u32, y: u32| {
            let i = ((y * w + x) * 4) as usize;
            f[i] = 255; f[i+1] = 255; f[i+2] = 255; f[i+3] = 255;
        };
        set(&mut f0, 16, 4, 4);
        set(&mut f1, 16, 5, 4);
        let mut t = MotionTracker::new(TrackPoint { x: 4.0, y: 4.0 }, 3);
        t.track(&f0, 16, 16);
        t.track(&f1, 16, 16);
        // After tracking, the displacement should be near (1, 0).
        let last = t.displacements.last().unwrap();
        assert!((last.dx - 1.0).abs() < 0.5, "expected dx ~1, got {}", last.dx);
    }

    #[test]
    fn stabilizer_smoothes_displacement() {
        let mut s = Stabilizer::new(TrackPoint { x: 4.0, y: 4.0 }, 3, 3);
        // Use frames with a single bright pixel so the tracker has a clear target.
        let mut f0 = make_frame(16, 16, 0);
        let i = ((4 * 16 + 4) * 4) as usize;
        f0[i] = 255; f0[i+1] = 255; f0[i+2] = 255; f0[i+3] = 255;
        s.process(&f0, 16, 16);
        s.process(&f0, 16, 16);
        // On identical frames, displacement should be (0, 0) since the best
        // match is at offset (0, 0).
        let last = *s.history.last().unwrap_or(&Displacement::default());
        assert!((last.dx).abs() < 0.5 && (last.dy).abs() < 0.5,
            "expected near-zero displacement on identical frames, got ({}, {})", last.dx, last.dy);
    }

    #[test]
    fn luma_buffer_correct_length() {
        let f = make_frame(4, 4, 128);
        let l = MotionTracker::luma_buffer(&f, 4, 4);
        assert_eq!(l.len(), 16);
    }
}
