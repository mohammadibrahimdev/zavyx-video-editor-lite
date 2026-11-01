//! Timeline model: `Sequence` ⊃ `Track[]` ⊃ `Block[]`.
//!
//! Design notes (informed by studying Olive and MLT architectures):
//!
//! - Blocks are *non-overlapping* on a single track. A `Block` is either a
//!   `Clip`, a `Gap`, or (later) a `Transition`. Two-track transitions live as
//!   a separate `Transition` block referencing the two adjacent clips — same
//!   pattern Kdenlive uses for MLT.
//! - All time is `Rational` (exact). No drift over long timelines.
//! - The list of blocks is kept *sorted and contiguous* by `in_point` on the
//!   track. Editing operations enforce this invariant.

use crate::time::{Rational, FrameRate, TimeRange};
use crate::id::{SeqId, TrackId, ClipId, MediaId, EffectId, MarkerId};
use crate::effect::EffectInstance;
use serde::{Serialize, Deserialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackKind { Video, Audio, Subtitle }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Marker {
    pub id: MarkerId,
    pub time: Rational,
    pub note: String,
    pub color: u32,
}

/// A clip references a media source, has a `media_in` offset, an `in_point`
/// and `out_point` on the timeline, speed, reverse flag, and an effect chain.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClipBlock {
    pub id: ClipId,
    pub media_id: MediaId,
    /// Time on the source media that maps to `in_point` on the timeline.
    pub media_in: Rational,
    /// Start time on the timeline.
    pub in_point: Rational,
    /// End time on the timeline (exclusive).
    pub out_point: Rational,
    pub speed: f64,         // 1.0 = normal; 0.5 = half speed; 2.0 = double
    pub reverse: bool,
    /// Optional speed ramping curve: when present, `speed` is the average and
    /// the actual per-frame speed is interpolated from these (timeline_time, speed)
    /// pairs. Speed = 0 at any point freezes that section.
    pub speed_curve: Vec<(Rational, f64)>,
    /// If Some(t), the clip is frozen at media time `t` for its entire duration.
    /// This is a convenience wrapper over speed=0.
    pub freeze_at: Option<Rational>,
    pub effects: Vec<EffectInstance>,
    pub locked: bool,
    pub label: String,
}

impl ClipBlock {
    pub fn new(media_id: MediaId, in_point: Rational, out_point: Rational, media_in: Rational) -> Self {
        let id = ClipId::new();
        let label = format!("Clip {}", &id.to_string()[..12]);
        ClipBlock {
            id, media_id, media_in, in_point, out_point,
            speed: 1.0, reverse: false,
            speed_curve: Vec::new(),
            freeze_at: None,
            effects: Vec::new(),
            locked: false, label,
        }
    }
    pub fn duration(&self) -> Rational { self.out_point - self.in_point }

    /// Effective speed at a given timeline time. Considers `freeze_at` (returns 0),
    /// then the `speed_curve` (interpolated), then falls back to the constant `speed`.
    pub fn speed_at(&self, timeline_time: Rational) -> f64 {
        if self.freeze_at.is_some() { return 0.0; }
        if self.speed_curve.is_empty() { return self.speed; }
        // Interpolate within speed_curve.
        let curve = &self.speed_curve;
        // Clamp outside [first, last] to the endpoints.
        if timeline_time <= curve[0].0 { return curve[0].1; }
        if timeline_time >= curve[curve.len()-1].0 { return curve[curve.len()-1].1; }
        for window in curve.windows(2) {
            let (t0, s0) = window[0];
            let (t1, s1) = window[1];
            if timeline_time >= t0 && timeline_time <= t1 {
                let span = t1 - t0;
                if span.is_zero() { return s1; }
                let local = (timeline_time - t0).to_f64() / span.to_f64();
                return s0 + (s1 - s0) * local;
            }
        }
        self.speed
    }

    pub fn media_duration(&self) -> Rational {
        // If frozen, no media is consumed.
        if self.freeze_at.is_some() { return Rational::ZERO; }
        // If a speed curve is set, integrate speed over the timeline duration.
        if !self.speed_curve.is_empty() {
            let d = self.duration().to_f64();
            // Sample 100 points along the curve and integrate (trapezoidal).
            let n = 100usize;
            let h = d / n as f64;
            let mut sum = 0.0;
            for i in 0..n {
                let t0 = self.in_point.to_f64() + (i as f64) * h;
                let t1 = self.in_point.to_f64() + ((i+1) as f64) * h;
                let s0 = self.speed_at(Rational::from_seconds(t0));
                let s1 = self.speed_at(Rational::from_seconds(t1));
                sum += (s0 + s1) * 0.5 * h;
            }
            return Rational::new((sum * 1000.0).round() as i64, 1000);
        }
        // Constant speed: media consumed = timeline_duration * speed
        let d = self.duration();
        Rational::new((d.to_f64() * self.speed * 1000.0).round() as i64, 1000)
    }
    pub fn timeline_to_media_time(&self, t: Rational) -> Rational {
        // Frozen: always returns the freeze_at time regardless of timeline time.
        if let Some(freeze_t) = self.freeze_at {
            return freeze_t;
        }
        let delta = t - self.in_point;
        // For constant speed, simple multiplication.
        if self.speed_curve.is_empty() {
            let media_delta = if self.speed == 1.0 {
                delta
            } else {
                Rational::new((delta.to_f64() * self.speed * 1000.0).round() as i64, 1000)
            };
            return if self.reverse {
                self.media_in - media_delta
            } else {
                self.media_in + media_delta
            };
        }
        // For speed curve, integrate speed from in_point to t.
        let dt = delta.to_f64();
        let mut sum = 0.0;
        let n = 100usize;
        let h = dt / n as f64;
        for i in 0..n {
            let t0 = self.in_point.to_f64() + (i as f64) * h;
            let t1 = self.in_point.to_f64() + ((i+1) as f64) * h;
            let s0 = self.speed_at(Rational::from_seconds(t0));
            let s1 = self.speed_at(Rational::from_seconds(t1));
            sum += (s0 + s1) * 0.5 * h;
        }
        let media_delta = Rational::new((sum * 1000.0).round() as i64, 1000);
        if self.reverse { self.media_in - media_delta } else { self.media_in + media_delta }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GapBlock {
    pub in_point: Rational,
    pub out_point: Rational,
}
impl GapBlock {
    pub fn new(in_point: Rational, out_point: Rational) -> Self {
        GapBlock { in_point, out_point }
    }
    pub fn duration(&self) -> Rational { self.out_point - self.in_point }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Block {
    Clip(ClipBlock),
    Gap(GapBlock),
}

impl Block {
    pub fn in_point(&self) -> Rational {
        match self { Block::Clip(c) => c.in_point, Block::Gap(g) => g.in_point }
    }
    pub fn out_point(&self) -> Rational {
        match self { Block::Clip(c) => c.out_point, Block::Gap(g) => g.out_point }
    }
    pub fn duration(&self) -> Rational { self.out_point() - self.in_point() }
    pub fn as_clip(&self) -> Option<&ClipBlock> {
        match self { Block::Clip(c) => Some(c), _ => None }
    }
    pub fn as_clip_mut(&mut self) -> Option<&mut ClipBlock> {
        match self { Block::Clip(c) => Some(c), _ => None }
    }
    pub fn is_gap(&self) -> bool { matches!(self, Block::Gap(_)) }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub id: TrackId,
    pub kind: TrackKind,
    pub name: String,
    pub blocks: Vec<Block>,
    pub height: u32,
    pub locked: bool,
    pub hidden: bool,
    pub muted: bool,
    pub solo: bool,
}

impl Track {
    pub fn new(name: impl Into<String>, kind: TrackKind) -> Self {
        Track {
            id: TrackId::new(),
            kind, name: name.into(), blocks: Vec::new(),
            height: 80, locked: false, hidden: false, muted: false, solo: false,
        }
    }

    pub fn duration(&self) -> Rational {
        self.blocks.iter().map(|b| b.out_point()).max().unwrap_or(Rational::ZERO)
    }

    /// Make `[start, end)` empty (overwrite edit): clips inside are removed,
    /// clips crossing an edge are trimmed, and a clip spanning the whole range
    /// is split around it. Clips listed in `keep` are left alone.
    pub fn clear_range(&mut self, start: Rational, end: Rational, keep: &[ClipId]) {
        if end <= start {
            return;
        }
        let hit: Vec<ClipBlock> = self.blocks.iter()
            .filter_map(|b| b.as_clip())
            .filter(|c| !keep.contains(&c.id) && c.in_point < end && c.out_point > start)
            .cloned()
            .collect();
        for c in hit {
            if c.in_point < start && c.out_point > end {
                self.split_clip(c.id, end);
            }
            if c.in_point >= start && c.out_point <= end {
                self.remove_clip(c.id);
            } else if c.in_point < start {
                if let Some(cm) = self.clip_mut(c.id) { cm.out_point = start; }
            } else if let Some(cm) = self.clip_mut(c.id) {
                cm.media_in = cm.timeline_to_media_time(end);
                cm.in_point = end;
            }
        }
        // A clip split above leaves a right part starting at `end`: keep it.
        self.sort_blocks();
    }

    /// Returns the block active at the given time, if any.
    pub fn block_at(&self, t: Rational) -> Option<&Block> {
        self.blocks.iter().find(|b| t >= b.in_point() && t < b.out_point())
    }

    /// Returns the index of the block active at the given time, if any.
    pub fn block_index_at(&self, t: Rational) -> Option<usize> {
        self.blocks.iter().position(|b| t >= b.in_point() && t < b.out_point())
    }

    /// Find the first block whose range contains the time, as a mutable ref.
    pub fn block_at_mut(&mut self, t: Rational) -> Option<&mut Block> {
        self.blocks.iter_mut().find(|b| t >= b.in_point() && t < b.out_point())
    }

    /// Insert a clip. The caller is responsible for handling overlaps via
    /// ripple/roll operations — this just appends in sorted order and re-sorts.
    /// Returns the clip's id on success.
    pub fn insert_clip(&mut self, mut clip: ClipBlock) -> ClipId {
        // Ensure in_point < out_point
        if clip.out_point <= clip.in_point {
            clip.out_point = clip.in_point + Rational::new(1, 30);
        }
        let id = clip.id;
        self.blocks.push(Block::Clip(clip));
        self.sort_blocks();
        id
    }

    pub fn sort_blocks(&mut self) {
        self.blocks.sort_by_key(|b| b.in_point());
    }

    /// Remove a clip by id. Returns true if removed.
    pub fn remove_clip(&mut self, id: ClipId) -> bool {
        let before = self.blocks.len();
        self.blocks.retain(|b| !matches!(b, Block::Clip(c) if c.id == id));
        self.blocks.len() < before
    }

    /// Find a clip by id.
    pub fn clip(&self, id: ClipId) -> Option<&ClipBlock> {
        self.blocks.iter().find_map(|b| b.as_clip().filter(|c| c.id == id))
    }
    pub fn clip_mut(&mut self, id: ClipId) -> Option<&mut ClipBlock> {
        self.blocks.iter_mut().find_map(|b| b.as_clip_mut().filter(|c| c.id == id))
    }

    /// Split the clip at `id` at the given timeline time. The original clip
    /// keeps the left half; a new clip is created for the right half.
    /// Returns the new clip's id, or None if the split time is outside the clip.
    pub fn split_clip(&mut self, id: ClipId, at: Rational) -> Option<ClipId> {
        // Find the clip's index and copy its data before mutation.
        let (clip_idx, clip) = self.blocks.iter()
            .enumerate()
            .find_map(|(i, b)| match b {
                Block::Clip(c) if c.id == id => Some((i, c.clone())),
                _ => None,
            })?;
        if at <= clip.in_point || at >= clip.out_point {
            return None;
        }
        // Left half keeps the original id and trimmed out_point.
        let left = ClipBlock {
            out_point: at,
            ..clip.clone()
        };
        // Right half gets a new id and adjusted media_in.
        let right = ClipBlock {
            id: ClipId::new(),
            in_point: at,
            out_point: clip.out_point,
            media_in: clip.timeline_to_media_time(at),
            ..clip
        };
        let right_id = right.id;
        // Replace the old block with the two halves.
        self.blocks.remove(clip_idx);
        self.blocks.push(Block::Clip(left));
        self.blocks.push(Block::Clip(right));
        self.sort_blocks();
        Some(right_id)
    }

    /// Move a clip to a new in_point, optionally to a different track.
    /// Adjusts media_in if the clip is moved but the source media stays the same.
    pub fn move_clip(&mut self, id: ClipId, new_in: Rational) -> bool {
        let clip = match self.blocks.iter().find_map(|b| match b {
            Block::Clip(c) if c.id == id => Some(c.clone()),
            _ => None,
        }) {
            Some(c) => c,
            None => return false,
        };
        let duration = clip.duration();
        let new_out = new_in + duration;
        let updated = ClipBlock { in_point: new_in, out_point: new_out, ..clip };
        // Replace
        let idx = self.blocks.iter().position(|b| matches!(b, Block::Clip(c) if c.id == id));
        if let Some(i) = idx {
            self.blocks[i] = Block::Clip(updated);
            self.sort_blocks();
            true
        } else { false }
    }

    /// Trim the start of a clip. `delta` is the change to `in_point`
    /// (positive = trim later, negative = extend earlier).
    /// Also adjusts `media_in` so the visible content stays continuous.
    pub fn trim_start(&mut self, id: ClipId, delta: Rational) -> bool {
        let clip = match self.clip_mut(id) { Some(c) => c, None => return false };
        let new_in = clip.in_point + delta;
        if new_in >= clip.out_point { return false; }
        clip.in_point = new_in;
        clip.media_in = clip.media_in + delta;  // for speed=1; speed≠1 handled in timeline_to_media_time
        true
    }

    /// Trim the end of a clip. `delta` is the change to `out_point`.
    pub fn trim_end(&mut self, id: ClipId, delta: Rational) -> bool {
        let clip = match self.clip_mut(id) { Some(c) => c, None => return false };
        let new_out = clip.out_point + delta;
        if new_out <= clip.in_point { return false; }
        clip.out_point = new_out;
        true
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Sequence {
    pub id: SeqId,
    pub name: String,
    pub time_base: FrameRate,
    pub tracks: Vec<Track>,
    pub markers: BTreeMap<MarkerId, Marker>,
}

impl Sequence {
    pub fn new(name: impl Into<String>, time_base: FrameRate) -> Self {
        Sequence {
            id: SeqId::new(),
            name: name.into(),
            time_base,
            tracks: vec![
                Track::new("V1", TrackKind::Video),
                Track::new("A1", TrackKind::Audio),
            ],
            markers: BTreeMap::new(),
        }
    }

    pub fn duration(&self) -> Rational {
        self.tracks.iter()
            .map(|t| t.duration())
            .max()
            .unwrap_or(Rational::ZERO)
    }

    pub fn add_track(&mut self, kind: TrackKind) -> TrackId {
        let n = self.tracks.iter().filter(|t| t.kind == kind).count() + 1;
        let prefix = match kind { TrackKind::Video => "V", TrackKind::Audio => "A", TrackKind::Subtitle => "S" };
        let mut t = Track::new(format!("{}{}", prefix, n), kind);
        let id = t.id;
        self.tracks.push(t);
        id
    }

    pub fn track(&self, id: TrackId) -> Option<&Track> {
        self.tracks.iter().find(|t| t.id == id)
    }
    pub fn track_mut(&mut self, id: TrackId) -> Option<&mut Track> {
        self.tracks.iter_mut().find(|t| t.id == id)
    }

    pub fn add_marker(&mut self, time: Rational, note: impl Into<String>) -> MarkerId {
        let m = Marker { id: MarkerId::new(), time, note: note.into(), color: 0xFFE08200 };
        let id = m.id;
        self.markers.insert(id, m);
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(num: i64, den: i64) -> Rational { Rational::new(num, den) }
    fn mid() -> MediaId { MediaId::new() }

    #[test]
    fn clear_range_trims_splits_and_removes() {
        let mut trk = Track::new("V1", TrackKind::Video);
        let a = trk.insert_clip(ClipBlock::new(mid(), t(0,1), t(4,1), t(0,1)));
        let b = trk.insert_clip(ClipBlock::new(mid(), t(5,1), t(6,1), t(0,1)));
        let c = trk.insert_clip(ClipBlock::new(mid(), t(7,1), t(10,1), t(0,1)));
        trk.clear_range(t(3,1), t(8,1), &[]);
        assert_eq!(trk.clip(a).unwrap().out_point, t(3,1));
        assert!(trk.clip(b).is_none());
        let cc = trk.clip(c).unwrap();
        assert_eq!((cc.in_point, cc.media_in), (t(8,1), t(1,1)));

        let mut trk = Track::new("V1", TrackKind::Video);
        let big = trk.insert_clip(ClipBlock::new(mid(), t(0,1), t(10,1), t(0,1)));
        trk.clear_range(t(2,1), t(3,1), &[]);
        assert_eq!(trk.clip(big).unwrap().out_point, t(2,1));
        let right = trk.blocks.iter().filter_map(|b| b.as_clip()).find(|c| c.id != big).unwrap();
        assert_eq!((right.in_point, right.out_point, right.media_in), (t(3,1), t(10,1), t(3,1)));
    }

    #[test]
    fn insert_and_locate_clip() {
        let mut trk = Track::new("V1", TrackKind::Video);
        let id = trk.insert_clip(ClipBlock::new(mid(), t(0,1), t(5,1), t(0,1)));
        assert!(trk.clip(id).is_some());
        assert_eq!(trk.block_at(t(2,1)).unwrap().as_clip().unwrap().id, id);
        assert!(trk.block_at(t(5,1)).is_none());  // half-open
    }

    #[test]
    fn split_clip_yields_two_contiguous_clips() {
        let mut trk = Track::new("V1", TrackKind::Video);
        let id = trk.insert_clip(ClipBlock::new(mid(), t(0,1), t(10,1), t(0,1)));
        let right = trk.split_clip(id, t(4,1)).expect("split");
        let left = trk.clip(id).unwrap();
        let right_clip = trk.clip(right).unwrap();
        assert_eq!(left.out_point, t(4,1));
        assert_eq!(right_clip.in_point, t(4,1));
        assert_eq!(right_clip.out_point, t(10,1));
        assert_eq!(right_clip.media_in, t(4,1));
    }

    #[test]
    fn split_outside_clip_returns_none() {
        let mut trk = Track::new("V1", TrackKind::Video);
        let id = trk.insert_clip(ClipBlock::new(mid(), t(0,1), t(5,1), t(0,1)));
        assert!(trk.split_clip(id, t(10,1)).is_none());
        assert!(trk.split_clip(id, t(0,1)).is_none());
        assert!(trk.split_clip(id, t(5,1)).is_none());
    }

    #[test]
    fn move_clip_changes_in_and_out() {
        let mut trk = Track::new("V1", TrackKind::Video);
        let id = trk.insert_clip(ClipBlock::new(mid(), t(0,1), t(5,1), t(0,1)));
        assert!(trk.move_clip(id, t(10,1)));
        let c = trk.clip(id).unwrap();
        assert_eq!(c.in_point, t(10,1));
        assert_eq!(c.out_point, t(15,1));
    }

    #[test]
    fn trim_start_adjusts_media_in() {
        let mut trk = Track::new("V1", TrackKind::Video);
        let id = trk.insert_clip(ClipBlock::new(mid(), t(0,1), t(10,1), t(0,1)));
        assert!(trk.trim_start(id, t(2,1)));
        let c = trk.clip(id).unwrap();
        assert_eq!(c.in_point, t(2,1));
        assert_eq!(c.media_in, t(2,1));
    }

    #[test]
    fn trim_end_changes_out_point() {
        let mut trk = Track::new("V1", TrackKind::Video);
        let id = trk.insert_clip(ClipBlock::new(mid(), t(0,1), t(10,1), t(0,1)));
        assert!(trk.trim_end(id, t(3,1)));
        let c = trk.clip(id).unwrap();
        assert_eq!(c.out_point, t(13,1));
    }

    #[test]
    fn remove_clip() {
        let mut trk = Track::new("V1", TrackKind::Video);
        let id = trk.insert_clip(ClipBlock::new(mid(), t(0,1), t(5,1), t(0,1)));
        assert!(trk.remove_clip(id));
        assert!(trk.clip(id).is_none());
    }

    #[test]
    fn timeline_to_media_time_forward() {
        let c = ClipBlock::new(mid(), t(10,1), t(20,1), t(100,1));
        assert_eq!(c.timeline_to_media_time(t(10,1)), t(100,1));
        assert_eq!(c.timeline_to_media_time(t(15,1)), t(105,1));
        assert_eq!(c.timeline_to_media_time(t(20,1)), t(110,1));
    }

    #[test]
    fn timeline_to_media_time_half_speed() {
        let mut c = ClipBlock::new(mid(), t(10,1), t(20,1), t(100,1));
        c.speed = 0.5;
        // At timeline 10s, media should be 100s.
        assert_eq!(c.timeline_to_media_time(t(10,1)), t(100,1));
        // At timeline 20s, only 5s of media played (half speed), so media=105s
        assert_eq!(c.timeline_to_media_time(t(20,1)), t(105,1));
    }

    #[test]
    fn timeline_to_media_time_reverse() {
        let mut c = ClipBlock::new(mid(), t(10,1), t(20,1), t(100,1));
        c.reverse = true;
        assert_eq!(c.timeline_to_media_time(t(10,1)), t(100,1));
        assert_eq!(c.timeline_to_media_time(t(15,1)), t(95,1));
        assert_eq!(c.timeline_to_media_time(t(20,1)), t(90,1));
    }

    #[test]
    fn freeze_at_returns_frozen_time_for_all_timeline_positions() {
        let mut c = ClipBlock::new(mid(), t(0,1), t(10,1), t(0,1));
        c.freeze_at = Some(t(50,1));
        // Regardless of timeline position, media time should be 50s.
        assert_eq!(c.timeline_to_media_time(t(0,1)), t(50,1));
        assert_eq!(c.timeline_to_media_time(t(5,1)), t(50,1));
        assert_eq!(c.timeline_to_media_time(t(10,1)), t(50,1));
        // media_duration should be 0 when frozen.
        assert_eq!(c.media_duration(), Rational::ZERO);
        // speed_at returns 0 when frozen.
        assert_eq!(c.speed_at(t(5,1)), 0.0);
    }

    #[test]
    fn speed_curve_interpolates_linearly() {
        let mut c = ClipBlock::new(mid(), t(0,1), t(10,1), t(0,1));
        // Speed ramps from 1.0 at t=0 to 3.0 at t=10.
        c.speed_curve = vec![
            (t(0,1), 1.0),
            (t(10,1), 3.0),
        ];
        // At t=5, interpolated speed = 2.0.
        let s = c.speed_at(t(5,1));
        assert!((s - 2.0).abs() < 1e-6, "expected speed 2.0 at midpoint, got {}", s);
    }

    #[test]
    fn speed_curve_clamps_outside_range() {
        let mut c = ClipBlock::new(mid(), t(0,1), t(10,1), t(0,1));
        c.speed_curve = vec![
            (t(2,1), 0.5),
            (t(8,1), 1.5),
        ];
        // Before the curve starts: clamp to first value (0.5).
        assert!((c.speed_at(t(0,1)) - 0.5).abs() < 1e-6);
        // After the curve ends: clamp to last value (1.5).
        assert!((c.speed_at(t(10,1)) - 1.5).abs() < 1e-6);
    }

    #[test]
    fn speed_curve_freeze_at_zero_speed_point() {
        let mut c = ClipBlock::new(mid(), t(0,1), t(10,1), t(0,1));
        // Curve hits speed=0 at midpoint — simulates a freeze-frame hold.
        c.speed_curve = vec![
            (t(0,1), 1.0),
            (t(5,1), 0.0),
            (t(10,1), 1.0),
        ];
        // At t=5 (frozen point), speed = 0.
        assert!((c.speed_at(t(5,1)) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn speed_curve_integrates_for_media_duration() {
        let mut c = ClipBlock::new(mid(), t(0,1), t(10,1), t(0,1));
        // Constant 2x speed across the whole clip via speed_curve.
        c.speed_curve = vec![
            (t(0,1), 2.0),
            (t(10,1), 2.0),
        ];
        // Media consumed = 10s timeline × 2.0 = 20s.
        let md = c.media_duration();
        let expected = Rational::new(20_000, 1000); // 20s = 20000/1000
        assert!((md.to_f64() - expected.to_f64()).abs() < 0.1,
            "expected ~20s media, got {} ({})", md.to_f64(), expected.to_f64());
    }

    #[test]
    fn sequence_duration_is_max_track_duration() {
        let mut s = Sequence::new("Test", FrameRate::FPS_30);
        let _ = s.tracks[0].insert_clip(ClipBlock::new(mid(), t(0,1), t(10,1), t(0,1)));
        let _ = s.tracks[1].insert_clip(ClipBlock::new(mid(), t(0,1), t(15,1), t(0,1)));
        assert_eq!(s.duration(), t(15,1));
    }

    #[test]
    fn add_track_increments_name() {
        let mut s = Sequence::new("Test", FrameRate::FPS_30);
        // Starts with V1 and A1
        assert_eq!(s.tracks.len(), 2);
        let _ = s.add_track(TrackKind::Video);
        assert_eq!(s.tracks[2].name, "V2");
        let _ = s.add_track(TrackKind::Audio);
        assert_eq!(s.tracks[3].name, "A2");
    }
}
