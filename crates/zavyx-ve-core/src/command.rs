//! Undo/redo command pattern + history stack.
//!
//! Every timeline edit is a `Command`. The `History` keeps bounded stacks of
//! executed commands (undo) and rolled-back ones (redo). On 32-bit Windows
//! the default cap keeps memory under control (~250 MB max for ~5 MB/command
//! × 50 commands).

use parking_lot::Mutex;
use crate::project::Project;

/// A reversible edit on the project.
pub trait Command: Send + Sync {
    fn execute(&mut self, proj: &mut Project);
    fn undo(&mut self, proj: &mut Project);
    fn description(&self) -> &str;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryAction {
    Undo,
    Redo,
    Execute,
}

/// Bounded undo/redo stack.
pub struct History {
    undo_stack: Vec<Box<dyn Command>>,
    redo_stack: Vec<Box<dyn Command>>,
    limit: usize,
    /// Mutex protects against concurrent mutation from worker threads.
    inner: Mutex<HistoryInner>,
}

#[derive(Default)]
struct HistoryInner {
    /// Saved index: how many commands on the undo stack were committed before
    /// the last save. `is_clean()` returns true when undo_stack.len() == saved.
    saved_at: usize,
}

impl History {
    pub fn new(limit: usize) -> Self {
        History {
            undo_stack: Vec::with_capacity(limit.min(64)),
            redo_stack: Vec::with_capacity(limit.min(64)),
            limit,
            inner: Mutex::new(HistoryInner::default()),
        }
    }

    /// Push and execute a command. Clears the redo stack.
    pub fn execute(&mut self, cmd: Box<dyn Command>, proj: &mut Project) {
        let mut cmd = cmd;
        cmd.execute(proj);
        self.redo_stack.clear();
        // Respect the limit: drop the oldest command from the undo stack.
        if self.undo_stack.len() >= self.limit {
            self.undo_stack.remove(0);
            // Adjust saved_at so the saved state stays consistent.
            let mut inner = self.inner.lock();
            inner.saved_at = inner.saved_at.saturating_sub(1);
        }
        self.undo_stack.push(cmd);
    }

    pub fn undo(&mut self, proj: &mut Project) -> bool {
        if let Some(mut cmd) = self.undo_stack.pop() {
            cmd.undo(proj);
            self.redo_stack.push(cmd);
            true
        } else { false }
    }

    pub fn redo(&mut self, proj: &mut Project) -> bool {
        if let Some(mut cmd) = self.redo_stack.pop() {
            cmd.execute(proj);
            self.undo_stack.push(cmd);
            true
        } else { false }
    }

    pub fn can_undo(&self) -> bool { !self.undo_stack.is_empty() }
    pub fn can_redo(&self) -> bool { !self.redo_stack.is_empty() }

    /// Number of undoable commands.
    pub fn undo_count(&self) -> usize { self.undo_stack.len() }
    pub fn redo_count(&self) -> usize { self.redo_stack.len() }

    /// Return human-readable descriptions of the undo stack (oldest first).
    pub fn undo_descriptions(&self) -> Vec<&str> {
        self.undo_stack.iter().map(|c| c.description()).collect()
    }
    pub fn redo_descriptions(&self) -> Vec<&str> {
        self.redo_stack.iter().map(|c| c.description()).collect()
    }

    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
        let mut inner = self.inner.lock();
        inner.saved_at = 0;
    }

    /// Mark the current state as saved (clean).
    pub fn mark_saved(&self) {
        let mut inner = self.inner.lock();
        inner.saved_at = self.undo_stack.len();
    }

    /// True when no unsaved edits remain.
    pub fn is_clean(&self) -> bool {
        let inner = self.inner.lock();
        inner.saved_at == self.undo_stack.len()
    }
}

impl Default for History {
    fn default() -> Self { History::new(50) }
}

// --- Concrete commands used by the GUI and tested in the test suite ---------

use crate::id::{TrackId, ClipId, MediaId};
use crate::time::Rational;
use crate::timeline::ClipBlock;

/// Insert a clip on a track.
pub struct InsertClipCommand {
    track_id: TrackId,
    clip: ClipBlock,
}

impl InsertClipCommand {
    pub fn new(track_id: TrackId, clip: ClipBlock) -> Self {
        InsertClipCommand { track_id, clip }
    }
}

impl Command for InsertClipCommand {
    fn execute(&mut self, proj: &mut Project) {
        if let Some(seq) = proj.active_sequence_mut() {
            if let Some(t) = seq.track_mut(self.track_id) {
                t.insert_clip(self.clip.clone());
            }
        }
    }
    fn undo(&mut self, proj: &mut Project) {
        if let Some(seq) = proj.active_sequence_mut() {
            if let Some(t) = seq.track_mut(self.track_id) {
                t.remove_clip(self.clip.id);
            }
        }
    }
    fn description(&self) -> &str { "Insert clip" }
}

/// Remove a clip from a track.
pub struct RemoveClipCommand {
    track_id: TrackId,
    clip_id: ClipId,
    /// Stashed clip for undo.
    stashed: Option<ClipBlock>,
}

impl RemoveClipCommand {
    pub fn new(track_id: TrackId, clip_id: ClipId) -> Self {
        RemoveClipCommand { track_id, clip_id, stashed: None }
    }
}

impl Command for RemoveClipCommand {
    fn execute(&mut self, proj: &mut Project) {
        if let Some(seq) = proj.active_sequence_mut() {
            if let Some(t) = seq.track_mut(self.track_id) {
                if let Some(c) = t.clip(self.clip_id).cloned() {
                    self.stashed = Some(c);
                    t.remove_clip(self.clip_id);
                }
            }
        }
    }
    fn undo(&mut self, proj: &mut Project) {
        if let Some(clip) = self.stashed.take() {
            if let Some(seq) = proj.active_sequence_mut() {
                if let Some(t) = seq.track_mut(self.track_id) {
                    t.insert_clip(clip);
                }
            }
        }
    }
    fn description(&self) -> &str { "Remove clip" }
}

/// Split a clip at a given time.
pub struct SplitClipCommand {
    track_id: TrackId,
    clip_id: ClipId,
    at: Rational,
    /// The id of the new (right) clip created by the split, recorded on execute
    /// so undo can remove it.
    new_clip_id: Option<ClipId>,
}

impl SplitClipCommand {
    pub fn new(track_id: TrackId, clip_id: ClipId, at: Rational) -> Self {
        SplitClipCommand { track_id, clip_id, at, new_clip_id: None }
    }
}

impl Command for SplitClipCommand {
    fn execute(&mut self, proj: &mut Project) {
        if let Some(seq) = proj.active_sequence_mut() {
            if let Some(t) = seq.track_mut(self.track_id) {
                self.new_clip_id = t.split_clip(self.clip_id, self.at);
            }
        }
    }
    fn undo(&mut self, proj: &mut Project) {
        if let Some(new_id) = self.new_clip_id.take() {
            if let Some(seq) = proj.active_sequence_mut() {
                if let Some(t) = seq.track_mut(self.track_id) {
                    // Remove the right half, leave the left half (original) intact.
                    t.remove_clip(new_id);
                }
            }
        }
    }
    fn description(&self) -> &str { "Split clip" }
}

/// Move a clip's start time on its track.
pub struct MoveClipCommand {
    track_id: TrackId,
    clip_id: ClipId,
    new_in: Rational,
    old_in: Option<Rational>,
}

impl MoveClipCommand {
    pub fn new(track_id: TrackId, clip_id: ClipId, new_in: Rational) -> Self {
        MoveClipCommand { track_id, clip_id, new_in, old_in: None }
    }
}

impl Command for MoveClipCommand {
    fn execute(&mut self, proj: &mut Project) {
        if let Some(seq) = proj.active_sequence_mut() {
            if let Some(t) = seq.track_mut(self.track_id) {
                if let Some(c) = t.clip(self.clip_id).cloned() {
                    self.old_in = Some(c.in_point);
                    t.move_clip(self.clip_id, self.new_in);
                }
            }
        }
    }
    fn undo(&mut self, proj: &mut Project) {
        if let Some(old) = self.old_in.take() {
            if let Some(seq) = proj.active_sequence_mut() {
                if let Some(t) = seq.track_mut(self.track_id) {
                    t.move_clip(self.clip_id, old);
                }
            }
        }
    }
    fn description(&self) -> &str { "Move clip" }
}

/// Update a clip's scalar property (speed, opacity, etc.) via a closure.
/// Stores the old value so undo can restore it.
pub struct UpdateClipPropertyCommand<F: Fn(&mut crate::timeline::ClipBlock) + Send + Sync + 'static> {
    track_id: TrackId,
    clip_id: ClipId,
    apply_fn: F,
    /// Stashed snapshot of the clip before the change (for undo).
    stashed: Option<crate::timeline::ClipBlock>,
    description: String,
}

impl<F: Fn(&mut crate::timeline::ClipBlock) + Send + Sync + 'static> UpdateClipPropertyCommand<F> {
    pub fn new(
        track_id: TrackId,
        clip_id: ClipId,
        apply_fn: F,
        description: impl Into<String>,
    ) -> Self {
        UpdateClipPropertyCommand {
            track_id, clip_id, apply_fn, stashed: None,
            description: description.into(),
        }
    }
}

impl<F: Fn(&mut crate::timeline::ClipBlock) + Send + Sync + 'static> Command for UpdateClipPropertyCommand<F> {
    fn execute(&mut self, proj: &mut Project) {
        if let Some(seq) = proj.active_sequence_mut() {
            if let Some(t) = seq.track_mut(self.track_id) {
                if let Some(clip) = t.clip_mut(self.clip_id) {
                    self.stashed = Some(clip.clone());
                    (self.apply_fn)(clip);
                }
            }
        }
    }
    fn undo(&mut self, proj: &mut Project) {
        if let Some(old) = self.stashed.take() {
            if let Some(seq) = proj.active_sequence_mut() {
                if let Some(t) = seq.track_mut(self.track_id) {
                    // Replace the clip with its pre-edit snapshot.
                    let idx = t.blocks.iter().position(|b| matches!(b, crate::timeline::Block::Clip(c) if c.id == self.clip_id));
                    if let Some(i) = idx {
                        t.blocks[i] = crate::timeline::Block::Clip(old);
                    }
                }
            }
        }
    }
    fn description(&self) -> &str { &self.description }
}

/// Any edit, stored as the whole active sequence before and after. Used by
/// the GUI for edits that touch several clips or tracks at once.
pub struct SequenceEditCommand {
    before: crate::timeline::Sequence,
    after: crate::timeline::Sequence,
    description: String,
}

impl SequenceEditCommand {
    pub fn new(
        before: crate::timeline::Sequence,
        after: crate::timeline::Sequence,
        description: impl Into<String>,
    ) -> Self {
        SequenceEditCommand { before, after, description: description.into() }
    }
}

impl Command for SequenceEditCommand {
    fn execute(&mut self, proj: &mut Project) {
        if let Some(seq) = proj.active_sequence_mut() {
            *seq = self.after.clone();
        }
    }
    fn undo(&mut self, proj: &mut Project) {
        if let Some(seq) = proj.active_sequence_mut() {
            *seq = self.before.clone();
        }
    }
    fn description(&self) -> &str { &self.description }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_proj() -> Project {
        // Project::new() already creates a sequence with V1 and A1 tracks.
        // We don't need to mess with the structure.
        Project::new("Test")
    }

    #[test]
    fn history_executes_and_undoes() {
        let mut p = fresh_proj();
        let mut h = History::new(50);

        let track_id = p.active_sequence().unwrap().tracks[0].id;
        let media_id = MediaId::new();
        let clip = ClipBlock::new(media_id, Rational::new(0,1), Rational::new(5,1), Rational::new(0,1));

        h.execute(Box::new(InsertClipCommand::new(track_id, clip)), &mut p);
        assert!(!h.is_clean());
        assert!(h.can_undo());
        assert_eq!(p.active_sequence().unwrap().tracks[0].blocks.len(), 1);

        assert!(h.undo(&mut p));
        assert_eq!(p.active_sequence().unwrap().tracks[0].blocks.len(), 0);
        assert!(h.can_redo());
    }

    #[test]
    fn history_redo_after_undo() {
        let mut p = fresh_proj();
        let mut h = History::new(50);
        let track_id = p.active_sequence().unwrap().tracks[0].id;
        let clip = ClipBlock::new(MediaId::new(), Rational::new(0,1), Rational::new(5,1), Rational::new(0,1));
        h.execute(Box::new(InsertClipCommand::new(track_id, clip)), &mut p);
        let _ = h.undo(&mut p);
        assert!(h.redo(&mut p));
        assert_eq!(p.active_sequence().unwrap().tracks[0].blocks.len(), 1);
    }

    #[test]
    fn history_clears_redo_on_new_execute() {
        let mut p = fresh_proj();
        let mut h = History::new(50);
        let track_id = p.active_sequence().unwrap().tracks[0].id;
        let clip1 = ClipBlock::new(MediaId::new(), Rational::new(0,1), Rational::new(5,1), Rational::new(0,1));
        let clip2 = ClipBlock::new(MediaId::new(), Rational::new(5,1), Rational::new(10,1), Rational::new(0,1));
        h.execute(Box::new(InsertClipCommand::new(track_id, clip1)), &mut p);
        h.execute(Box::new(InsertClipCommand::new(track_id, clip2)), &mut p);
        assert!(h.undo(&mut p));
        assert!(h.can_redo());
        // New command should clear redo
        let clip3 = ClipBlock::new(MediaId::new(), Rational::new(20,1), Rational::new(25,1), Rational::new(0,1));
        h.execute(Box::new(InsertClipCommand::new(track_id, clip3)), &mut p);
        assert!(!h.can_redo());
    }

    #[test]
    fn history_respects_limit() {
        let mut p = fresh_proj();
        let mut h = History::new(3);  // small limit
        let track_id = p.active_sequence().unwrap().tracks[0].id;
        for i in 0..10 {
            let clip = ClipBlock::new(
                MediaId::new(),
                Rational::new(i * 10, 1),
                Rational::new(i * 10 + 5, 1),
                Rational::new(0,1),
            );
            h.execute(Box::new(InsertClipCommand::new(track_id, clip)), &mut p);
        }
        // Limit was 3, so undo_count should be 3 (oldest dropped)
        assert_eq!(h.undo_count(), 3);
    }

    #[test]
    fn split_command_undo_removes_right_half() {
        let mut p = fresh_proj();
        let mut h = History::new(50);
        let track_id = p.active_sequence().unwrap().tracks[0].id;
        let clip = ClipBlock::new(MediaId::new(), Rational::new(0,1), Rational::new(10,1), Rational::new(0,1));
        let clip_id = clip.id;
        h.execute(Box::new(InsertClipCommand::new(track_id, clip)), &mut p);
        // Now split
        h.execute(Box::new(SplitClipCommand::new(track_id, clip_id, Rational::new(5,1))), &mut p);
        assert_eq!(p.active_sequence().unwrap().tracks[0].blocks.len(), 2);
        // Undo split
        assert!(h.undo(&mut p));
        assert_eq!(p.active_sequence().unwrap().tracks[0].blocks.len(), 1);
    }

    #[test]
    fn move_command_undo_restores_position() {
        let mut p = fresh_proj();
        let mut h = History::new(50);
        let track_id = p.active_sequence().unwrap().tracks[0].id;
        let clip = ClipBlock::new(MediaId::new(), Rational::new(0,1), Rational::new(5,1), Rational::new(0,1));
        let clip_id = clip.id;
        h.execute(Box::new(InsertClipCommand::new(track_id, clip)), &mut p);
        h.execute(Box::new(MoveClipCommand::new(track_id, clip_id, Rational::new(20,1))), &mut p);
        assert_eq!(p.active_sequence().unwrap().tracks[0].clip(clip_id).unwrap().in_point, Rational::new(20,1));
        h.undo(&mut p);
        assert_eq!(p.active_sequence().unwrap().tracks[0].clip(clip_id).unwrap().in_point, Rational::new(0,1));
    }

    #[test]
    fn mark_saved_and_is_clean() {
        let mut p = fresh_proj();
        let mut h = History::new(50);
        assert!(h.is_clean());
        let track_id = p.active_sequence().unwrap().tracks[0].id;
        let clip = ClipBlock::new(MediaId::new(), Rational::new(0,1), Rational::new(5,1), Rational::new(0,1));
        h.execute(Box::new(InsertClipCommand::new(track_id, clip)), &mut p);
        assert!(!h.is_clean());
        h.mark_saved();
        assert!(h.is_clean());
    }

    #[test]
    fn sequence_edit_undoes_and_redoes() {
        let mut p = fresh_proj();
        let before = p.active_sequence().unwrap().clone();
        let mut after = before.clone();
        after.name = "Edited".into();
        let mut h = History::default();
        h.execute(Box::new(SequenceEditCommand::new(before, after, "Rename")), &mut p);
        assert_eq!(p.active_sequence().unwrap().name, "Edited");
        h.undo(&mut p);
        assert_eq!(p.active_sequence().unwrap().name, "Sequence 1");
        h.redo(&mut p);
        assert_eq!(p.active_sequence().unwrap().name, "Edited");
    }
}

// Need Sequence and Track imports for the test helper.
use crate::timeline::Sequence;
