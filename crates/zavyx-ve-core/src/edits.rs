//! Advanced edit operations: ripple, roll, slip, slide, ripple-delete, close-gaps.
//!
//! These wrap the basic `Track::insert_clip` / `split_clip` / `move_clip` /
//! `remove_clip` primitives with the multi-clip side effects that real NLEs
//! perform when the user does a ripple trim, roll trim, etc.
//!
//! Reference (per Olive / Kdenlive / Premiere conventions):
//!
//! - **Ripple trim**: change a clip's in/out point; downstream clips shift to
//!   fill (or open) the gap. The total sequence duration changes.
//! - **Roll trim**: change the boundary between two adjacent clips; the right
//!   clip's in-point shifts by the same delta as the left clip's out-point.
//!   Total duration unchanged.
//! - **Slip**: move a clip's source `media_in` without changing its timeline
//!   position or duration. The clip shows earlier or later content from the
//!   same source.
//! - **Slide**: move a clip left/right on the timeline; the adjacent clips'
//!   in/out points adjust to keep the timeline gap-free.
//! - **Ripple delete**: remove a clip and shift downstream clips left to
//!   close the gap.
//! - **Close gaps**: remove all gap blocks (or empty regions) on a track.

use crate::id::{TrackId, ClipId};
use crate::time::Rational;
use crate::project::Project;
use crate::timeline::{Sequence, Track, Block, ClipBlock};

/// Result of an advanced edit operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditResult {
    /// The edit succeeded; the named clips were modified.
    Ok { modified: Vec<ClipId> },
    /// The edit was a no-op (e.g. delta is zero, or clip not found).
    NoOp,
    /// The edit could not be performed (e.g. clips not adjacent, would
    /// produce a negative duration).
    Failed(String),
}

// =========================================================================
// Ripple trim — change a clip's out-point; shift downstream clips left
// (if shortened) or right (if lengthened).
// =========================================================================

/// Ripple-trim the **end** of `clip_id` by `delta` (positive = extend, negative
/// = shorten). All clips on the same track whose `in_point >= clip.out_point`
/// shift by `delta`. Other tracks are untouched.
pub fn ripple_trim_end(
    seq: &mut Sequence,
    track_id: TrackId,
    clip_id: ClipId,
    delta: Rational,
) -> EditResult {
    if delta.is_zero() { return EditResult::NoOp; }
    let track = match seq.track_mut(track_id) { Some(t) => t, None => return EditResult::Failed("track not found".into()) };
    let (old_out, old_in) = match track.clip(clip_id) {
        Some(c) => (c.out_point, c.in_point),
        None => return EditResult::Failed("clip not found".into()),
    };
    let new_out = old_out + delta;
    if new_out <= old_in { return EditResult::Failed("ripple would invert clip".into()); }
    // Update the target clip's out_point.
    if let Some(c) = track.clip_mut(clip_id) { c.out_point = new_out; }
    // Shift all clips with in_point >= old_out by `delta`.
    let mut modified = vec![clip_id];
    if let Some(t) = seq.track_mut(track_id) {
        for b in &mut t.blocks {
            if let Block::Clip(c) = b {
                if c.id != clip_id && c.in_point >= old_out {
                    c.in_point = c.in_point + delta;
                    c.out_point = c.out_point + delta;
                    modified.push(c.id);
                }
            }
            // Gap blocks also shift
            if let Block::Gap(g) = b {
                if g.in_point >= old_out {
                    g.in_point = g.in_point + delta;
                    g.out_point = g.out_point + delta;
                }
            }
        }
        t.sort_blocks();
    }
    EditResult::Ok { modified }
}

/// Ripple-trim the **start** of `clip_id` by `delta` (positive = extend left,
/// negative = shorten from start). All downstream clips shift accordingly.
pub fn ripple_trim_start(
    seq: &mut Sequence,
    track_id: TrackId,
    clip_id: ClipId,
    delta: Rational,
) -> EditResult {
    if delta.is_zero() { return EditResult::NoOp; }
    let track = match seq.track_mut(track_id) { Some(t) => t, None => return EditResult::Failed("track not found".into()) };
    let (old_in, old_out, media_in) = match track.clip(clip_id) {
        Some(c) => (c.in_point, c.out_point, c.media_in),
        None => return EditResult::Failed("clip not found".into()),
    };
    let new_in = old_in + delta;
    if new_in >= old_out { return EditResult::Failed("ripple would invert clip".into()); }
    if new_in < Rational::ZERO { return EditResult::Failed("ripple would push clip before t=0".into()); }
    if let Some(c) = track.clip_mut(clip_id) {
        c.in_point = new_in;
        c.media_in = media_in + delta;
    }
    let mut modified = vec![clip_id];
    if let Some(t) = seq.track_mut(track_id) {
        for b in &mut t.blocks {
            if let Block::Clip(c) = b {
                if c.id != clip_id && c.in_point >= old_in {
                    c.in_point = c.in_point + delta;
                    c.out_point = c.out_point + delta;
                    modified.push(c.id);
                }
            }
            if let Block::Gap(g) = b {
                if g.in_point >= old_in {
                    g.in_point = g.in_point + delta;
                    g.out_point = g.out_point + delta;
                }
            }
        }
        t.sort_blocks();
    }
    EditResult::Ok { modified }
}

// =========================================================================
// Roll trim — change the boundary between two adjacent clips. Left clip's
// out_point shifts by `delta`; right clip's in_point also shifts by `delta`.
// Total duration unchanged.
// =========================================================================

pub fn roll_trim(
    seq: &mut Sequence,
    track_id: TrackId,
    left_clip_id: ClipId,
    delta: Rational,
) -> EditResult {
    if delta.is_zero() { return EditResult::NoOp; }
    let track = match seq.track_mut(track_id) { Some(t) => t, None => return EditResult::Failed("track not found".into()) };
    // Find the left clip and the clip immediately after it.
    let left_idx = track.blocks.iter().position(|b| matches!(b, Block::Clip(c) if c.id == left_clip_id));
    let left_idx = match left_idx { Some(i) => i, None => return EditResult::Failed("left clip not found".into()) };
    if left_idx + 1 >= track.blocks.len() {
        return EditResult::Failed("no clip after the left clip; use ripple instead".into());
    }
    let left_out = match &track.blocks[left_idx] {
        Block::Clip(c) => c.out_point,
        _ => return EditResult::Failed("left block is not a clip".into()),
    };
    // Get the right clip's id and in_point.
    let (right_id, right_in) = match &track.blocks[left_idx + 1] {
        Block::Clip(c) => (c.id, c.in_point),
        _ => return EditResult::Failed("right block is not a clip; cannot roll".into()),
    };
    if left_out != right_in {
        return EditResult::Failed("clips are not adjacent (gap between them); cannot roll".into());
    }
    let left_in = match &track.blocks[left_idx] {
        Block::Clip(c) => c.in_point,
        _ => unreachable!(),
    };
    let new_boundary = left_out + delta;
    if new_boundary <= left_in { return EditResult::Failed("roll would invert left clip".into()); }
    // Find right clip's out_point.
    let right_out = match &track.blocks[left_idx + 1] {
        Block::Clip(c) => c.out_point,
        _ => unreachable!(),
    };
    if new_boundary >= right_out { return EditResult::Failed("roll would invert right clip".into()); }
    // Apply: left.out = new_boundary, right.in = new_boundary, right.media_in += delta
    let right_media_in = match &track.blocks[left_idx + 1] {
        Block::Clip(c) => c.media_in,
        _ => unreachable!(),
    };
    if let Block::Clip(c) = &mut track.blocks[left_idx] {
        c.out_point = new_boundary;
    }
    if let Block::Clip(c) = &mut track.blocks[left_idx + 1] {
        c.in_point = new_boundary;
        c.media_in = right_media_in + delta;
    }
    EditResult::Ok { modified: vec![left_clip_id, right_id] }
}

// =========================================================================
// Slip — change a clip's media_in without changing its timeline position or
// duration. The clip shows earlier/later content from the source.
// =========================================================================

pub fn slip_clip(
    seq: &mut Sequence,
    track_id: TrackId,
    clip_id: ClipId,
    delta: Rational,
) -> EditResult {
    if delta.is_zero() { return EditResult::NoOp; }
    let track = match seq.track_mut(track_id) { Some(t) => t, None => return EditResult::Failed("track not found".into()) };
    if let Some(c) = track.clip_mut(clip_id) {
        c.media_in = c.media_in + delta;
        EditResult::Ok { modified: vec![clip_id] }
    } else {
        EditResult::Failed("clip not found".into())
    }
}

// =========================================================================
// Slide — move a clip left/right; adjacent clips' in/out adjust to keep the
// track gap-free.
// =========================================================================

pub fn slide_clip(
    seq: &mut Sequence,
    track_id: TrackId,
    clip_id: ClipId,
    delta: Rational,
) -> EditResult {
    if delta.is_zero() { return EditResult::NoOp; }
    let track = match seq.track_mut(track_id) { Some(t) => t, None => return EditResult::Failed("track not found".into()) };
    let idx = track.blocks.iter().position(|b| matches!(b, Block::Clip(c) if c.id == clip_id));
    let idx = match idx { Some(i) => i, None => return EditResult::Failed("clip not found".into()) };

    // The clip's duration stays the same. The clip shifts by delta.
    // The left neighbour's out_point extends/shrinks by delta.
    // The right neighbour's in_point extends/shrinks by -delta (it gains on one side).
    let (clip_in, clip_out, clip_dur) = match &track.blocks[idx] {
        Block::Clip(c) => (c.in_point, c.out_point, c.out_point - c.in_point),
        _ => return EditResult::Failed("block is not a clip".into()),
    };
    let new_in = clip_in + delta;
    let new_out = clip_out + delta;
    if new_in < Rational::ZERO { return EditResult::Failed("slide would push clip before t=0".into()); }

    // Check that the left neighbour has room to extend / shrink.
    if idx > 0 {
        let left_in = match &track.blocks[idx - 1] {
            Block::Clip(c) => Some(c.in_point),
            Block::Gap(g)  => Some(g.in_point),
            _ => None,
        };
        if let Some(li) = left_in {
            if new_in < li {
                return EditResult::Failed("slide would overlap left neighbour".into());
            }
        }
    }

    // Apply.
    let mut modified = vec![clip_id];
    if idx > 0 {
        match &mut track.blocks[idx - 1] {
            Block::Clip(c) => { c.out_point = new_in; modified.push(c.id); }
            Block::Gap(g)  => { g.out_point = new_in; }
            _ => {}
        }
    }
    if let Block::Clip(c) = &mut track.blocks[idx] {
        c.in_point = new_in;
        c.out_point = new_out;
    }
    if idx + 1 < track.blocks.len() {
        match &mut track.blocks[idx + 1] {
            Block::Clip(c) => { c.in_point = new_out; modified.push(c.id); }
            Block::Gap(g)  => { g.in_point = new_out; }
            _ => {}
        }
    }
    let _ = clip_dur;
    EditResult::Ok { modified }
}

// =========================================================================
// Ripple delete — remove a clip; shift downstream clips left to close gap.
// =========================================================================

pub fn ripple_delete(
    seq: &mut Sequence,
    track_id: TrackId,
    clip_id: ClipId,
) -> EditResult {
    let track = match seq.track_mut(track_id) { Some(t) => t, None => return EditResult::Failed("track not found".into()) };
    let (in_pt, out_pt) = match track.clip(clip_id) {
        Some(c) => (c.in_point, c.out_point),
        None => return EditResult::Failed("clip not found".into()),
    };
    let delta = in_pt - out_pt; // negative (we're shifting left)
    // Remove the clip.
    track.remove_clip(clip_id);
    // Shift everything with in_point >= out_pt by `delta` (negative).
    if let Some(t) = seq.track_mut(track_id) {
        for b in &mut t.blocks {
            match b {
                Block::Clip(c) if c.in_point >= out_pt => {
                    c.in_point = c.in_point + delta;
                    c.out_point = c.out_point + delta;
                }
                Block::Gap(g) if g.in_point >= out_pt => {
                    g.in_point = g.in_point + delta;
                    g.out_point = g.out_point + delta;
                }
                _ => {}
            }
        }
        t.sort_blocks();
    }
    EditResult::Ok { modified: vec![clip_id] }
}

// =========================================================================
// Close gaps — remove all Gap blocks on a track and shift downstream clips left.
// =========================================================================

pub fn close_gaps(seq: &mut Sequence, track_id: TrackId) -> EditResult {
    let track = match seq.track_mut(track_id) { Some(t) => t, None => return EditResult::Failed("track not found".into()) };
    // Snapshot the blocks' positions, then rebuild without gaps, recomputing in/out.
    let mut new_blocks: Vec<Block> = Vec::new();
    let mut cursor = Rational::ZERO;
    let mut modified = Vec::new();
    for b in track.blocks.drain(..) {
        match b {
            Block::Clip(mut c) => {
                let dur = c.out_point - c.in_point;
                let old_in = c.in_point;
                if old_in != cursor {
                    c.in_point = cursor;
                    c.out_point = cursor + dur;
                    modified.push(c.id);
                }
                cursor = c.out_point;
                new_blocks.push(Block::Clip(c));
            }
            Block::Gap(_) => {
                // skip — closes the gap
            }
        }
    }
    track.blocks = new_blocks;
    if modified.is_empty() { EditResult::NoOp } else { EditResult::Ok { modified } }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::MediaId;
    use crate::time::{Rational, FrameRate};

    fn t(n: i64, d: i64) -> Rational { Rational::new(n, d) }
    fn mid() -> MediaId { MediaId::new() }

    fn seq_with_clip(clip: ClipBlock) -> (Sequence, TrackId, ClipId) {
        let mut s = Sequence::new("Test", FrameRate::FPS_30);
        let tid = s.tracks[0].id;
        let cid = clip.id;
        s.tracks[0].insert_clip(clip);
        (s, tid, cid)
    }

    fn add_clip_after(seq: &mut Sequence, track_id: TrackId, in_pt: Rational, dur: Rational) -> ClipId {
        let c = ClipBlock::new(mid(), in_pt, in_pt + dur, Rational::ZERO);
        let id = c.id;
        seq.track_mut(track_id).unwrap().insert_clip(c);
        id
    }

    #[test]
    fn ripple_trim_end_shortens_and_shifts_downstream() {
        let clip = ClipBlock::new(mid(), t(0,1), t(10,1), t(0,1));
        let (mut s, tid, cid) = seq_with_clip(clip);
        let cid2 = add_clip_after(&mut s, tid, t(10,1), t(5,1));
        // Ripple trim the first clip's end by -2s (shorten).
        let res = ripple_trim_end(&mut s, tid, cid, t(-2,1));
        assert!(matches!(res, EditResult::Ok { .. }));
        let c1 = s.track(tid).unwrap().clip(cid).unwrap();
        let c2 = s.track(tid).unwrap().clip(cid2).unwrap();
        assert_eq!(c1.out_point, t(8,1));
        assert_eq!(c2.in_point, t(8,1));  // shifted left by 2
        assert_eq!(c2.out_point, t(13,1));
    }

    #[test]
    fn ripple_trim_end_extends_and_shifts_downstream() {
        let clip = ClipBlock::new(mid(), t(0,1), t(10,1), t(0,1));
        let (mut s, tid, cid) = seq_with_clip(clip);
        let cid2 = add_clip_after(&mut s, tid, t(10,1), t(5,1));
        // Extend by 3s
        let res = ripple_trim_end(&mut s, tid, cid, t(3,1));
        assert!(matches!(res, EditResult::Ok { .. }));
        let c2 = s.track(tid).unwrap().clip(cid2).unwrap();
        assert_eq!(c2.in_point, t(13,1));
        assert_eq!(c2.out_point, t(18,1));
    }

    #[test]
    fn ripple_trim_end_rejects_inversion() {
        let clip = ClipBlock::new(mid(), t(0,1), t(10,1), t(0,1));
        let (mut s, tid, cid) = seq_with_clip(clip);
        let res = ripple_trim_end(&mut s, tid, cid, t(-15,1));
        assert!(matches!(res, EditResult::Failed(_)));
    }

    #[test]
    fn roll_trim_moves_boundary() {
        // Two adjacent clips: [0,10] and [10,20].
        let mut s = Sequence::new("Test", FrameRate::FPS_30);
        let tid = s.tracks[0].id;
        let c1 = ClipBlock::new(mid(), t(0,1), t(10,1), t(0,1));
        let c1_id = c1.id;
        s.tracks[0].insert_clip(c1);
        let c2 = ClipBlock::new(mid(), t(10,1), t(20,1), t(0,1));
        s.tracks[0].insert_clip(c2);
        // Roll by +2: c1.out = 12, c2.in = 12.
        let res = roll_trim(&mut s, tid, c1_id, t(2,1));
        assert!(matches!(res, EditResult::Ok { .. }));
        let c1 = s.track(tid).unwrap().clip(c1_id).unwrap();
        // Find c2 by position.
        let c2 = s.track(tid).unwrap().block_at(t(15,1)).unwrap().as_clip().unwrap();
        assert_eq!(c1.out_point, t(12,1));
        assert_eq!(c2.in_point, t(12,1));
        assert_eq!(c2.out_point, t(20,1));  // unchanged
    }

    #[test]
    fn roll_trim_rejects_non_adjacent() {
        let mut s = Sequence::new("Test", FrameRate::FPS_30);
        let tid = s.tracks[0].id;
        let c1 = ClipBlock::new(mid(), t(0,1), t(5,1), t(0,1));
        let c1_id = c1.id;
        s.tracks[0].insert_clip(c1);
        let c2 = ClipBlock::new(mid(), t(10,1), t(15,1), t(0,1));
        s.tracks[0].insert_clip(c2);
        let res = roll_trim(&mut s, tid, c1_id, t(2,1));
        assert!(matches!(res, EditResult::Failed(_)));
    }

    #[test]
    fn slip_clip_shifts_media_in() {
        let clip = ClipBlock::new(mid(), t(0,1), t(10,1), t(100,1));
        let (mut s, tid, cid) = seq_with_clip(clip);
        let res = slip_clip(&mut s, tid, cid, t(5,1));
        assert!(matches!(res, EditResult::Ok { .. }));
        let c = s.track(tid).unwrap().clip(cid).unwrap();
        assert_eq!(c.media_in, t(105,1));
        // Timeline position unchanged.
        assert_eq!(c.in_point, t(0,1));
        assert_eq!(c.out_point, t(10,1));
    }

    #[test]
    fn slide_clip_shifts_clip_and_neighbours() {
        // Layout: [c1: 0..5] [c2: 5..15] [c3: 15..20]
        let mut s = Sequence::new("Test", FrameRate::FPS_30);
        let tid = s.tracks[0].id;
        let c1 = ClipBlock::new(mid(), t(0,1), t(5,1), t(0,1));
        s.tracks[0].insert_clip(c1);
        let c2 = ClipBlock::new(mid(), t(5,1), t(15,1), t(0,1));
        let c2_id = c2.id;
        s.tracks[0].insert_clip(c2);
        let c3 = ClipBlock::new(mid(), t(15,1), t(20,1), t(0,1));
        s.tracks[0].insert_clip(c3);
        // Slide c2 right by 2s.
        let res = slide_clip(&mut s, tid, c2_id, t(2,1));
        assert!(matches!(res, EditResult::Ok { .. }));
        let c2 = s.track(tid).unwrap().clip(c2_id).unwrap();
        assert_eq!(c2.in_point, t(7,1));
        assert_eq!(c2.out_point, t(17,1));
        // c1 extended by 2 (out = 7), c3 shrunk by 2 (in = 17).
    }

    #[test]
    fn ripple_delete_removes_and_shifts() {
        // Layout: [c1: 0..5] [c2: 5..15] [c3: 15..20]
        let mut s = Sequence::new("Test", FrameRate::FPS_30);
        let tid = s.tracks[0].id;
        let c1 = ClipBlock::new(mid(), t(0,1), t(5,1), t(0,1));
        s.tracks[0].insert_clip(c1);
        let c2 = ClipBlock::new(mid(), t(5,1), t(15,1), t(0,1));
        let c2_id = c2.id;
        s.tracks[0].insert_clip(c2);
        let c3 = ClipBlock::new(mid(), t(15,1), t(20,1), t(0,1));
        let c3_id = c3.id;
        s.tracks[0].insert_clip(c3);
        // Delete c2 (10s clip).
        let res = ripple_delete(&mut s, tid, c2_id);
        assert!(matches!(res, EditResult::Ok { .. }));
        // c3 should now start at 5 (shifted left by 10s).
        let c3 = s.track(tid).unwrap().clip(c3_id).unwrap();
        assert_eq!(c3.in_point, t(5,1));
        assert_eq!(c3.out_point, t(10,1));
    }

    #[test]
    fn close_gaps_removes_gap_blocks() {
        let mut s = Sequence::new("Test", FrameRate::FPS_30);
        let tid = s.tracks[0].id;
        let c1 = ClipBlock::new(mid(), t(0,1), t(5,1), t(0,1));
        let c1_id = c1.id;
        s.tracks[0].insert_clip(c1);
        let c2 = ClipBlock::new(mid(), t(15,1), t(20,1), t(0,1));
        let c2_id = c2.id;
        s.tracks[0].insert_clip(c2);
        // There's a 10s gap between c1 and c2.
        let res = close_gaps(&mut s, tid);
        assert!(matches!(res, EditResult::Ok { .. }));
        // c1 stays at 0..5; c2 should now start at 5.
        let c2 = s.track(tid).unwrap().clip(c2_id).unwrap();
        assert_eq!(c2.in_point, t(5,1));
        assert_eq!(c2.out_point, t(10,1));
        // c1 unchanged.
        let c1 = s.track(tid).unwrap().clip(c1_id).unwrap();
        assert_eq!(c1.in_point, t(0,1));
        assert_eq!(c1.out_point, t(5,1));
    }
}
