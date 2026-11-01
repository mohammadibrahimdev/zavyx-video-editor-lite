//! Snapping and marker helpers.
//!
//! Snapping finds the nearest "interesting" time within a tolerance — used
//! when dragging clips or moving the playhead so they align to clip edges,
//! markers, or the playhead itself.

use crate::time::Rational;
use crate::id::{MarkerId, TrackId};
use crate::timeline::{Sequence, Marker};
use std::collections::BTreeSet;

/// Find the nearest snap point to `target` within `tolerance` seconds.
///
/// Snap candidates include:
/// - Every clip's in_point and out_point on every visible track
/// - Every marker's time
/// - The playhead position (if supplied)
///
/// Returns `Some(time)` if a snap point exists within `tolerance`, else `None`.
pub fn find_snap(
    seq: &Sequence,
    target: Rational,
    tolerance: Rational,
    playhead: Option<Rational>,
) -> Option<Rational> {
    let mut candidates: Vec<Rational> = Vec::new();
    for track in &seq.tracks {
        if track.hidden { continue; }
        for b in &track.blocks {
            candidates.push(b.in_point());
            candidates.push(b.out_point());
        }
    }
    for m in seq.markers.values() {
        candidates.push(m.time);
    }
    if let Some(p) = playhead { candidates.push(p); }
    // Find the closest candidate within tolerance.
    let mut best: Option<(Rational, Rational)> = None; // (distance, time)
    for c in candidates {
        let dist = if c > target { c - target } else { target - c };
        if dist > tolerance { continue; }
        match best {
            None => best = Some((dist, c)),
            Some((bd, _)) if dist < bd => best = Some((dist, c)),
            _ => {}
        }
    }
    best.map(|(_, t)| t)
}

/// Find the nearest snap point, considering only the active track list.
/// Useful when you want snapping scoped to one track (e.g. trimming one clip).
pub fn find_snap_on_track(
    seq: &Sequence,
    track_id: TrackId,
    target: Rational,
    tolerance: Rational,
) -> Option<Rational> {
    let track = seq.tracks.iter().find(|t| t.id == track_id)?;
    let mut candidates: Vec<Rational> = Vec::new();
    for b in &track.blocks {
        candidates.push(b.in_point());
        candidates.push(b.out_point());
    }
    let mut best: Option<(Rational, Rational)> = None;
    for c in candidates {
        let dist = if c > target { c - target } else { target - c };
        if dist > tolerance { continue; }
        match best {
            None => best = Some((dist, c)),
            Some((bd, _)) if dist < bd => best = Some((dist, c)),
            _ => {}
        }
    }
    best.map(|(_, t)| t)
}

/// Add a marker to the sequence at the given time. Returns the new marker's id.
pub fn add_marker(seq: &mut Sequence, time: Rational, note: impl Into<String>) -> MarkerId {
    seq.add_marker(time, note)
}

/// Remove a marker by id. Returns true if it was present.
pub fn remove_marker(seq: &mut Sequence, id: MarkerId) -> bool {
    seq.markers.remove(&id).is_some()
}

/// List all markers, sorted by time.
pub fn markers_sorted(seq: &Sequence) -> Vec<&Marker> {
    let mut v: Vec<&Marker> = seq.markers.values().collect();
    v.sort_by_key(|m| m.time);
    v
}

/// Move a marker to a new time.
pub fn move_marker(seq: &mut Sequence, id: MarkerId, new_time: Rational) -> bool {
    if let Some(m) = seq.markers.get_mut(&id) {
        m.time = new_time;
        true
    } else { false }
}

/// Edit a marker's note text.
pub fn edit_marker_note(seq: &mut Sequence, id: MarkerId, note: impl Into<String>) -> bool {
    if let Some(m) = seq.markers.get_mut(&id) {
        m.note = note.into();
        true
    } else { false }
}

/// Enumerate every distinct "snap point" in the sequence — useful for
/// visualizing snap guides on the timeline ruler.
pub fn all_snap_points(seq: &Sequence) -> BTreeSet<Rational> {
    let mut out: BTreeSet<Rational> = BTreeSet::new();
    for track in &seq.tracks {
        if track.hidden { continue; }
        for b in &track.blocks {
            out.insert(b.in_point());
            out.insert(b.out_point());
        }
    }
    for m in seq.markers.values() {
        out.insert(m.time);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::MediaId;
    use crate::timeline::ClipBlock;
    use crate::time::FrameRate;

    fn t(n: i64, d: i64) -> Rational { Rational::new(n, d) }

    #[test]
    fn find_snap_picks_nearest_clip_edge() {
        let mut seq = Sequence::new("Test", FrameRate::FPS_30);
        let _ = seq.tracks[0].insert_clip(ClipBlock::new(MediaId::new(), t(0,1), t(10,1), t(0,1)));
        // Target = 9.9s, tolerance = 0.5s. Nearest edge is 10s.
        let snap = find_snap(&seq, t(99,10), t(5,10), None);
        assert_eq!(snap, Some(t(10,1)));
    }

    #[test]
    fn find_snap_returns_none_when_outside_tolerance() {
        let mut seq = Sequence::new("Test", FrameRate::FPS_30);
        let _ = seq.tracks[0].insert_clip(ClipBlock::new(MediaId::new(), t(0,1), t(10,1), t(0,1)));
        let snap = find_snap(&seq, t(50,1), t(1,1), None);
        assert_eq!(snap, None);
    }

    #[test]
    fn find_snap_includes_playhead() {
        let mut seq = Sequence::new("Test", FrameRate::FPS_30);
        let _ = seq.tracks[0].insert_clip(ClipBlock::new(MediaId::new(), t(0,1), t(10,1), t(0,1)));
        // Playhead at 5.5s. Target = 5.4s, tolerance = 0.5s.
        let snap = find_snap(&seq, t(54,10), t(5,10), Some(t(11,2)));
        assert_eq!(snap, Some(t(11,2)));  // 5.5s = 11/2
    }

    #[test]
    fn add_and_remove_marker() {
        let mut seq = Sequence::new("Test", FrameRate::FPS_30);
        let id = add_marker(&mut seq, t(5,1), "Intro");
        assert_eq!(seq.markers.len(), 1);
        assert_eq!(seq.markers[&id].note, "Intro");
        assert!(remove_marker(&mut seq, id));
        assert_eq!(seq.markers.len(), 0);
    }

    #[test]
    fn move_marker_updates_time() {
        let mut seq = Sequence::new("Test", FrameRate::FPS_30);
        let id = add_marker(&mut seq, t(5,1), "Marker");
        assert!(move_marker(&mut seq, id, t(10,1)));
        assert_eq!(seq.markers[&id].time, t(10,1));
    }

    #[test]
    fn edit_marker_note_updates_text() {
        let mut seq = Sequence::new("Test", FrameRate::FPS_30);
        let id = add_marker(&mut seq, t(5,1), "Old");
        assert!(edit_marker_note(&mut seq, id, "New"));
        assert_eq!(seq.markers[&id].note, "New");
    }

    #[test]
    fn markers_sorted_returns_in_time_order() {
        let mut seq = Sequence::new("Test", FrameRate::FPS_30);
        let _ = add_marker(&mut seq, t(10,1), "C");
        let _ = add_marker(&mut seq, t(5,1), "A");
        let _ = add_marker(&mut seq, t(7,1), "B");
        let v = markers_sorted(&seq);
        assert_eq!(v[0].note, "A");
        assert_eq!(v[1].note, "B");
        assert_eq!(v[2].note, "C");
    }

    #[test]
    fn all_snap_points_includes_clip_edges_and_markers() {
        let mut seq = Sequence::new("Test", FrameRate::FPS_30);
        let _ = seq.tracks[0].insert_clip(ClipBlock::new(MediaId::new(), t(0,1), t(10,1), t(0,1)));
        let _ = add_marker(&mut seq, t(5,1), "M");
        let pts = all_snap_points(&seq);
        assert!(pts.contains(&t(0,1)));
        assert!(pts.contains(&t(5,1)));
        assert!(pts.contains(&t(10,1)));
    }

    #[test]
    fn find_snap_on_track_scopes_to_one_track() {
        let mut seq = Sequence::new("Test", FrameRate::FPS_30);
        let tid = seq.tracks[0].id;
        let _ = seq.tracks[0].insert_clip(ClipBlock::new(MediaId::new(), t(0,1), t(10,1), t(0,1)));
        let _ = seq.tracks[1].insert_clip(ClipBlock::new(MediaId::new(), t(15,1), t(25,1), t(0,1)));
        // Searching on track 0 with target 14s, tol 2s should NOT find track 1's edge at 15s.
        let snap = find_snap_on_track(&seq, tid, t(14,1), t(2,1));
        assert_eq!(snap, None);  // 10s and 0s are too far
    }
}
