//! App state and every editing action the UI can trigger.

use std::path::{Path, PathBuf};

use eframe::egui;
use zavyx_ve_core::command::SequenceEditCommand;
use zavyx_ve_core::{
    AspectRatio, ClipBlock, ClipId, EffectInstance, History, MediaId, MediaInfo, MediaRef,
    Project, Rational, Sequence, TrackId, TrackKind,
};
use zavyx_ve_media::cli::CliProber;
use zavyx_ve_media::Prober;

use crate::engine::Engine;
use crate::export;
use crate::media_cache::MediaCache;
use crate::settings::{self, Settings};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Screen { Home, Editor }

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LibTab { Media, Audio, Titles, Transitions, Effects, Filters }

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InspTab { Title, Video, Color, Audio, Speed, Effects }

/// Something dragged from the library onto the timeline.
#[derive(Clone, Debug)]
pub enum Drag {
    Media(MediaId),
    Effect(String),
    Transition(String),
    Title(String),
}

/// An in-progress move or trim of a timeline clip.
#[derive(Clone, Debug)]
pub struct ClipDrag {
    pub clip: ClipId,
    pub track: TrackId,
    pub mode: DragMode,
    /// Seconds between the clip start and the grab point.
    pub grab: f64,
    pub before: Sequence,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragMode { Move, TrimStart, TrimEnd }

pub struct Toast {
    pub text: String,
    pub error: bool,
    pub until: f64,
}

pub struct ExportDialog {
    pub format: export::Format,
    pub name: String,
    pub folder: PathBuf,
    pub height: u32,
    pub fps: f64,
    pub quality: export::Quality,
}

pub struct ZavyxApp {
    pub screen: Screen,
    pub settings: Settings,
    pub project: Project,
    pub history: History,
    pub engine: Engine,
    pub cache: MediaCache,
    pub egui_ctx: egui::Context,

    pub playhead: f64,
    pub selected: Option<ClipId>,
    pub lib_tab: LibTab,
    pub lib_category: usize,
    pub search: String,
    pub insp_tab: InspTab,
    pub zoom: f32,
    pub snap: bool,
    pub clip_drag: Option<ClipDrag>,
    pub scrubbing: bool,
    pub clipboard: Option<ClipBlock>,
    pub preview_texture: Option<egui::TextureHandle>,
    pub preview_scale: f32,
    pub preview_full: bool,
    pub toast: Option<Toast>,
    pub about_open: bool,
    pub shortcuts_open: bool,
    pub export_dialog: Option<ExportDialog>,
    pub export_job: Option<export::Job>,
    pub new_aspect: AspectRatio,
    pub confirm_close: bool,
    pub allow_close: bool,
    pub recovery: Option<PathBuf>,
    /// Sequence before a slider/text edit that is still in progress.
    pub live_before: Option<Sequence>,
    pub live_desc: String,
    pub pending_home: bool,
    /// Media pool changed since the last save (imports are not undo steps).
    pub pool_dirty: bool,
    pub timeline_scroll: f64,
    pub track_scroll: f32,
    pub lanes_x: (f32, f32),
    pub meter: (f32, f32),
    engine_dirty: bool,
    last_autosave: f64,
}

pub fn autosave_path() -> Option<PathBuf> {
    settings::data_dir().map(|d| d.join("autosave.zvep"))
}

impl ZavyxApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        crate::style::install(&cc.egui_ctx);
        egui_extras::install_image_loaders(&cc.egui_ctx);
        let mut settings = Settings::load();
        settings.prune_missing();
        let recovery = autosave_path().filter(|p| p.is_file());
        let mut cache = MediaCache::new(cc.egui_ctx.clone());
        cache.request_previews();
        let mut app = ZavyxApp {
            screen: Screen::Home,
            settings,
            project: Project::new("Untitled"),
            history: History::default(),
            engine: Engine::new(cc.egui_ctx.clone()),
            cache,
            egui_ctx: cc.egui_ctx.clone(),
            playhead: 0.0,
            selected: None,
            lib_tab: LibTab::Media,
            lib_category: 0,
            search: String::new(),
            insp_tab: InspTab::Video,
            zoom: 60.0,
            snap: true,
            clip_drag: None,
            scrubbing: false,
            clipboard: None,
            preview_texture: None,
            preview_scale: 1.0,
            preview_full: false,
            toast: None,
            about_open: false,
            shortcuts_open: false,
            export_dialog: None,
            export_job: None,
            new_aspect: AspectRatio::Landscape,
            confirm_close: false,
            allow_close: false,
            recovery,
            live_before: None,
            live_desc: String::new(),
            pending_home: false,
            pool_dirty: false,
            timeline_scroll: 0.0,
            track_scroll: 0.0,
            lanes_x: (0.0, 1.0),
            meter: (0.0, 0.0),
            engine_dirty: true,
            last_autosave: 0.0,
        };
        app.open_args(std::env::args_os().skip(1).map(PathBuf::from).collect());
        app.sync_engine();
        app
    }

    /// Files given on the command line ("Open with"): a project opens; media
    /// files start a new project with them on the timeline.
    fn open_args(&mut self, files: Vec<PathBuf>) {
        let files: Vec<PathBuf> = files.into_iter().filter(|p| p.is_file()).collect();
        if let Some(p) = files.iter().find(|p| p.extension().map(|e| e.eq_ignore_ascii_case("zvep")).unwrap_or(false)) {
            let p = p.clone();
            self.open_path(&p);
        } else if !files.is_empty() {
            self.new_project(AspectRatio::Landscape);
            self.import_files(&files);
            // Add in the order the files were given.
            let ids: Vec<MediaId> = files.iter()
                .filter_map(|f| self.project.media_pool.items.iter().find(|(_, (_, i))| &i.path == f).map(|(id, _)| *id))
                .collect();
            for id in ids {
                self.append_media(id);
            }
            self.history.clear();
            self.selected = None;
        }
    }

    // --- Messages -----------------------------------------------------------

    pub fn notify(&mut self, text: impl Into<String>) {
        let now = self.egui_ctx.input(|i| i.time);
        self.toast = Some(Toast { text: text.into(), error: false, until: now + 3.0 });
    }

    pub fn error(&mut self, text: impl Into<String>) {
        let now = self.egui_ctx.input(|i| i.time);
        self.toast = Some(Toast { text: text.into(), error: true, until: now + 6.0 });
    }

    // --- Project lifecycle ----------------------------------------------------

    pub fn is_dirty(&self) -> bool {
        !self.history.is_clean() || self.pool_dirty || self.live_before.is_some()
    }

    /// Timeline time under screen x (for drags).
    pub fn drag_time(&self, x: f32) -> f64 {
        self.timeline_scroll + ((x - self.lanes_x.0) / self.zoom) as f64
    }

    pub fn fps(&self) -> f64 {
        self.project.settings.time_base.0.to_f64()
    }

    pub fn seq(&self) -> &Sequence {
        self.project.active_sequence().expect("project always has a sequence")
    }

    pub fn duration(&self) -> f64 {
        self.seq().duration().to_f64()
    }

    pub fn new_project(&mut self, aspect: AspectRatio) {
        let mut p = Project::new("Untitled");
        let (w, h) = match aspect {
            AspectRatio::Vertical => (1080, 1920),
            AspectRatio::Square => (1080, 1080),
            AspectRatio::Portrait45 => (1080, 1350),
            AspectRatio::Cinematic => (2560, 1080),
            _ => (1920, 1080),
        };
        p.settings.aspect = aspect;
        p.settings.width = w;
        p.settings.height = h;
        self.load_project(p, None);
    }

    fn load_project(&mut self, p: Project, path: Option<PathBuf>) {
        self.engine.pause();
        self.project = p;
        self.project.last_save_path = path;
        self.history.clear();
        self.pool_dirty = false;
        self.selected = None;
        self.playhead = 0.0;
        self.timeline_scroll = 0.0;
        self.preview_texture = None;
        let media: Vec<(MediaId, MediaInfo)> = self.project.media_pool.items.iter()
            .map(|(id, (_, info))| (*id, info.clone())).collect();
        for (id, info) in media {
            self.cache.request(id, &info);
        }
        self.engine.seek(0.0);
        self.mark_changed();
        self.screen = Screen::Editor;
    }

    pub fn open_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new().add_filter("Zavyx VE Project", &["zvep"]).pick_file() {
            self.open_path(&p);
        }
    }

    pub fn open_path(&mut self, path: &Path) {
        match std::fs::read_to_string(path).map_err(|e| e.to_string())
            .and_then(|t| zavyx_ve_core::serde::load(&t).map_err(|e| e.to_string()))
        {
            Ok(mut p) => {
                relink_media(&mut p, path);
                let missing = p.media_pool.items.values().filter(|(_, i)| !i.path.is_file()).count();
                let name = p.name.clone();
                self.settings.touch_recent(path.to_path_buf(), name);
                self.settings.save();
                self.load_project(p, Some(path.to_path_buf()));
                if missing > 0 {
                    self.error(format!("{missing} media file(s) could not be found"));
                }
            }
            Err(e) => self.error(format!("Could not open project: {e}")),
        }
    }

    /// Save; asks for a file name the first time. Returns true when saved.
    pub fn save(&mut self) -> bool {
        match self.project.last_save_path.clone() {
            Some(p) => self.save_to(&p),
            None => self.save_as(),
        }
    }

    pub fn save_as(&mut self) -> bool {
        let name = format!("{}.zvep", self.project.name);
        match rfd::FileDialog::new().add_filter("Zavyx VE Project", &["zvep"]).set_file_name(name).save_file() {
            Some(p) => self.save_to(&p),
            None => false,
        }
    }

    fn save_to(&mut self, path: &Path) -> bool {
        self.commit_live();
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            self.project.name = stem.to_string();
        }
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        for (mref, info) in self.project.media_pool.items.values_mut() {
            mref.relative_path = info.path.strip_prefix(&dir).map(Path::to_path_buf).unwrap_or(info.path.clone());
        }
        let result = zavyx_ve_core::serde::save(&self.project).map_err(|e| e.to_string())
            .and_then(|json| std::fs::write(path, json).map_err(|e| e.to_string()));
        match result {
            Ok(()) => {
                self.project.last_save_path = Some(path.to_path_buf());
                self.history.mark_saved();
                self.pool_dirty = false;
                self.settings.touch_recent(path.to_path_buf(), self.project.name.clone());
                self.settings.save();
                if let Some(a) = autosave_path() {
                    let _ = std::fs::remove_file(a);
                }
                self.notify("Project saved");
                true
            }
            Err(e) => {
                self.error(format!("Could not save: {e}"));
                false
            }
        }
    }

    pub fn recover_autosave(&mut self) {
        if let Some(path) = self.recovery.take() {
            match std::fs::read_to_string(&path).map_err(|e| e.to_string())
                .and_then(|t| zavyx_ve_core::serde::load(&t).map_err(|e| e.to_string()))
            {
                Ok(p) => {
                    self.load_project(p, None);
                    self.pool_dirty = true;
                    self.notify("Recovered your unsaved project");
                }
                Err(e) => self.error(format!("Could not recover: {e}")),
            }
        }
    }

    pub fn discard_autosave(&mut self) {
        if let Some(p) = self.recovery.take() {
            let _ = std::fs::remove_file(p);
        }
    }

    pub fn maybe_autosave(&mut self, now: f64) {
        if now - self.last_autosave < 30.0 {
            return;
        }
        self.last_autosave = now;
        if self.screen != Screen::Editor || !self.is_dirty() {
            return;
        }
        if let (Some(path), Ok(json)) = (autosave_path(), zavyx_ve_core::serde::save(&self.project)) {
            let _ = std::fs::write(path, json);
        }
    }

    // --- Engine sync ------------------------------------------------------------

    /// Note that the timeline changed so the preview re-renders.
    pub fn mark_changed(&mut self) {
        self.engine_dirty = true;
    }

    pub fn sync_engine(&mut self) {
        if self.engine_dirty {
            self.engine_dirty = false;
            self.engine.set_project(crate::mixer::snapshot(&self.project));
        }
    }

    pub fn seek(&mut self, seconds: f64) {
        let t = seconds.clamp(0.0, self.duration().max(0.0));
        self.playhead = t;
        self.engine.seek(t);
    }

    pub fn toggle_play(&mut self) {
        if self.engine.is_playing() {
            self.engine.pause();
            self.playhead = self.engine.position();
        } else {
            if self.playhead >= self.duration() - 1e-3 {
                self.seek(0.0);
            }
            self.engine.play();
        }
    }

    pub fn step_frames(&mut self, n: i64) {
        self.engine.pause();
        let f = 1.0 / self.fps();
        let frame = (self.playhead / f).round() as i64 + n;
        self.seek(frame.max(0) as f64 * f);
    }

    // --- Editing ------------------------------------------------------------------

    /// Apply an edit to the active sequence as one undo step.
    pub fn edit(&mut self, desc: &str, f: impl FnOnce(&mut Sequence)) {
        self.commit_live();
        let before = self.seq().clone();
        let mut after = before.clone();
        f(&mut after);
        self.history.execute(Box::new(SequenceEditCommand::new(before, after, desc)), &mut self.project);
        self.mark_changed();
    }

    /// Change the sequence immediately (e.g. while a slider moves); the undo
    /// step is recorded by `commit_live` when the gesture ends.
    pub fn live_edit(&mut self, desc: &str, f: impl FnOnce(&mut Sequence)) {
        if self.live_before.is_none() {
            self.live_before = Some(self.seq().clone());
            self.live_desc = desc.to_string();
        }
        if let Some(seq) = self.project.active_sequence_mut() {
            f(seq);
        }
        self.mark_changed();
    }

    pub fn commit_live(&mut self) {
        if let Some(before) = self.live_before.take() {
            let after = self.seq().clone();
            let desc = std::mem::take(&mut self.live_desc);
            self.history.execute(Box::new(SequenceEditCommand::new(before, after, desc)), &mut self.project);
        }
    }

    pub fn undo(&mut self) {
        self.commit_live();
        if self.history.undo(&mut self.project) {
            self.after_history();
        }
    }

    pub fn redo(&mut self) {
        self.commit_live();
        if self.history.redo(&mut self.project) {
            self.after_history();
        }
    }

    fn after_history(&mut self) {
        if let Some(id) = self.selected {
            if self.find_clip(id).is_none() {
                self.selected = None;
            }
        }
        self.mark_changed();
    }

    pub fn find_clip(&self, id: ClipId) -> Option<(TrackId, ClipBlock)> {
        self.seq().tracks.iter().find_map(|t| t.clip(id).map(|c| (t.id, c.clone())))
    }

    pub fn selected_clip(&self) -> Option<(TrackId, ClipBlock)> {
        self.selected.and_then(|id| self.find_clip(id))
    }

    /// Edit one clip by id as a single undo step.
    pub fn edit_clip(&mut self, id: ClipId, desc: &str, f: impl FnOnce(&mut ClipBlock)) {
        self.edit(desc, |seq| {
            if let Some(c) = seq.tracks.iter_mut().find_map(|t| t.clip_mut(id)) {
                f(c);
            }
        });
    }

    pub fn live_edit_clip(&mut self, id: ClipId, desc: &str, f: impl FnOnce(&mut ClipBlock)) {
        self.live_edit(desc, |seq| {
            if let Some(c) = seq.tracks.iter_mut().find_map(|t| t.clip_mut(id)) {
                f(c);
            }
        });
    }

    // --- Media -------------------------------------------------------------------

    pub fn import_dialog(&mut self) {
        let files = rfd::FileDialog::new()
            .add_filter("Media", &["mp4", "mov", "mkv", "avi", "webm", "m4v", "wmv", "flv", "mts", "mp3", "wav",
                                   "aac", "m4a", "flac", "ogg", "opus", "wma", "png", "jpg", "jpeg", "bmp", "webp", "gif"])
            .add_filter("All files", &["*"])
            .pick_files();
        if let Some(files) = files {
            self.import_files(&files);
        }
    }

    pub fn import_files(&mut self, files: &[PathBuf]) {
        if zavyx_ve_media::cli::ffmpeg_path().is_none() {
            self.error("ffmpeg.exe is missing. Reinstall Zavyx VE to restore it.");
            return;
        }
        let (mut ok, mut failed) = (0, Vec::new());
        for path in files {
            if self.project.media_pool.items.values().any(|(_, i)| &i.path == path) {
                continue;
            }
            match CliProber.probe(path) {
                Ok(info) => {
                    let id = MediaId::new();
                    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("media").to_string();
                    self.cache.request(id, &info);
                    self.project.media_pool.add(MediaRef { id, relative_path: path.clone(), display_name: name }, info);
                    ok += 1;
                }
                Err(_) => failed.push(path.file_name().and_then(|s| s.to_str()).unwrap_or("?").to_string()),
            }
        }
        if ok > 0 {
            // Media pool changes are saved with the project but are not undo steps.
            self.pool_dirty = true;
            self.mark_changed();
            self.lib_tab = LibTab::Media;
            self.notify(format!("Imported {ok} file(s)"));
        }
        if !failed.is_empty() {
            self.error(format!("Could not read: {}", failed.join(", ")));
        }
    }

    pub fn media_info(&self, id: MediaId) -> Option<&MediaInfo> {
        self.project.media_pool.get(&id).map(|(_, i)| i)
    }

    pub fn media_name(&self, id: MediaId) -> String {
        self.project.media_pool.get(&id).map(|(r, _)| r.display_name.clone()).unwrap_or_default()
    }

    /// Clip for a media file at `start`, using the whole file.
    pub fn clip_for_media(&self, id: MediaId, start: f64) -> Option<ClipBlock> {
        let info = self.media_info(id)?;
        let dur = info.duration.to_f64().max(0.1);
        let start = Rational::from_seconds(start);
        let mut clip = ClipBlock::new(id, start, start + Rational::from_seconds(dur), Rational::ZERO);
        clip.label = self.media_name(id);
        Some(clip)
    }

    /// Track a media file goes on by default.
    fn default_track(&self, id: MediaId) -> Option<TrackId> {
        let audio_only = self.media_info(id).map(|i| !i.has_video).unwrap_or(false);
        let kind = if audio_only { TrackKind::Audio } else { TrackKind::Video };
        self.seq().tracks.iter().find(|t| t.kind == kind).map(|t| t.id)
    }

    /// Add a media file at the end of its main track.
    pub fn append_media(&mut self, id: MediaId) {
        let Some(track) = self.default_track(id) else { return };
        let end = self.seq().track(track).map(|t| t.duration().to_f64()).unwrap_or(0.0);
        self.place_media(id, track, end);
    }

    /// Drop a media file on `track` at `time` (overwriting what is there).
    pub fn place_media(&mut self, id: MediaId, track: TrackId, time: f64) {
        let Some(mut clip) = self.clip_for_media(id, time.max(0.0)) else { return };
        let audio_only = self.media_info(id).map(|i| !i.has_video).unwrap_or(false);
        let want = if audio_only { TrackKind::Audio } else { TrackKind::Video };
        let track = match self.seq().track(track) {
            Some(t) if t.kind == want => track,
            _ => match self.default_track(id) { Some(t) => t, None => return },
        };
        if self.seq().track(track).map(|t| t.locked).unwrap_or(false) {
            self.error("That track is locked");
            return;
        }
        let new_id = clip.id;
        clip.locked = false;
        self.edit("Add clip", |seq| {
            if let Some(t) = seq.track_mut(track) {
                t.clear_range(clip.in_point, clip.out_point, &[]);
                t.insert_clip(clip);
            }
        });
        self.selected = Some(new_id);
    }

    // --- Clip operations -----------------------------------------------------------

    pub fn split_at_playhead(&mut self) {
        let at = Rational::from_seconds(self.playhead);
        // Prefer the selected clip; otherwise split every clip under the playhead.
        let targets: Vec<(TrackId, ClipId)> = match self.selected_clip() {
            Some((t, c)) if c.in_point < at && c.out_point > at => vec![(t, c.id)],
            _ => self.seq().tracks.iter().filter(|t| !t.locked)
                .filter_map(|t| t.block_at(at).and_then(|b| b.as_clip())
                    .filter(|c| c.in_point < at).map(|c| (t.id, c.id)))
                .collect(),
        };
        if targets.is_empty() {
            self.notify("Move the playhead over a clip to split it");
            return;
        }
        self.edit("Split", |seq| {
            for (t, c) in &targets {
                if let Some(track) = seq.track_mut(*t) {
                    track.split_clip(*c, at);
                }
            }
        });
    }

    pub fn delete_selected(&mut self, ripple: bool) {
        let Some((track, clip)) = self.selected_clip() else { return };
        if self.seq().track(track).map(|t| t.locked).unwrap_or(false) {
            self.error("That track is locked");
            return;
        }
        self.edit(if ripple { "Ripple delete" } else { "Delete" }, |seq| {
            if let Some(t) = seq.track_mut(track) {
                t.remove_clip(clip.id);
                if ripple {
                    let gap = clip.duration();
                    for b in t.blocks.iter_mut().filter_map(|b| b.as_clip_mut()) {
                        if b.in_point >= clip.out_point {
                            b.in_point = b.in_point - gap;
                            b.out_point = b.out_point - gap;
                        }
                    }
                }
            }
        });
        self.selected = None;
    }

    pub fn copy_selected(&mut self) {
        if let Some((_, c)) = self.selected_clip() {
            self.clipboard = Some(c);
            self.notify("Clip copied");
        }
    }

    /// Paste the copied clip at the playhead on the selected clip's track (or its own kind).
    pub fn paste(&mut self) {
        let Some(mut clip) = self.clipboard.clone() else { return };
        let is_audio_media = self.media_info(clip.media_id).map(|i| !i.has_video).unwrap_or(false);
        let kind = if is_audio_media { TrackKind::Audio } else { TrackKind::Video };
        let track = self.selected_clip().map(|(t, _)| t)
            .filter(|t| self.seq().track(*t).map(|tr| tr.kind == kind).unwrap_or(false))
            .or_else(|| self.seq().tracks.iter().find(|t| t.kind == kind).map(|t| t.id));
        let Some(track) = track else { return };
        let dur = clip.duration();
        clip.id = ClipId::new();
        clip.in_point = Rational::from_seconds(self.playhead);
        clip.out_point = clip.in_point + dur;
        let id = clip.id;
        self.edit("Paste", |seq| {
            if let Some(t) = seq.track_mut(track) {
                t.clear_range(clip.in_point, clip.out_point, &[]);
                t.insert_clip(clip);
            }
        });
        self.selected = Some(id);
    }

    pub fn duplicate_selected(&mut self) {
        let Some((track, mut clip)) = self.selected_clip() else { return };
        let dur = clip.duration();
        clip.id = ClipId::new();
        clip.in_point = clip.out_point;
        clip.out_point = clip.in_point + dur;
        let id = clip.id;
        self.edit("Duplicate", |seq| {
            if let Some(t) = seq.track_mut(track) {
                t.clear_range(clip.in_point, clip.out_point, &[]);
                t.insert_clip(clip);
            }
        });
        self.selected = Some(id);
    }

    pub fn add_marker(&mut self) {
        let t = Rational::from_seconds(self.playhead);
        let n = self.seq().markers.len() + 1;
        self.edit("Add marker", |seq| { seq.add_marker(t, format!("Marker {n}")); });
    }

    pub fn add_track(&mut self, kind: TrackKind) {
        self.edit("Add track", |seq| { seq.add_track(kind); });
    }

    pub fn remove_track(&mut self, id: TrackId) {
        let kind = self.seq().track(id).map(|t| t.kind);
        let same = self.seq().tracks.iter().filter(|t| Some(t.kind) == kind).count();
        if same <= 1 {
            self.notify("The last video or audio track can't be removed");
            return;
        }
        self.edit("Remove track", |seq| seq.tracks.retain(|t| t.id != id));
    }

    // --- Titles, effects, transitions ------------------------------------------------

    /// Add a title from a template at the playhead on the top video track.
    pub fn add_title(&mut self, template_id: &str, at: Option<(TrackId, f64)>) {
        let templates = zavyx_ve_render::templates::all_templates();
        let Some(tpl) = templates.iter().find(|t| t.id == template_id) else { return };
        let mut fx = EffectInstance::new(zavyx_ve_render::TITLE_EFFECT);
        fx.params = tpl.params.clone();
        fx.set_text("text", tpl.name);
        let (track, start) = match at {
            Some((t, s)) if self.seq().track(t).map(|tr| tr.kind == TrackKind::Video).unwrap_or(false) => (Some(t), s),
            _ => (None, self.playhead),
        };
        let start = Rational::from_seconds(start.max(0.0));
        let mut clip = ClipBlock::new(MediaId::new(), start, start + Rational::new(5, 1), Rational::ZERO);
        clip.label = tpl.name.to_string();
        clip.effects.push(fx);
        let id = clip.id;
        self.edit("Add title", |seq| {
            // Titles go on the topmost video track; make one above V1 if needed.
            let track = track.or_else(|| {
                let videos: Vec<TrackId> = seq.tracks.iter().filter(|t| t.kind == TrackKind::Video).map(|t| t.id).collect();
                if videos.len() >= 2 { videos.last().copied() } else { Some(seq.add_track(TrackKind::Video)) }
            });
            if let Some(t) = track.and_then(|t| seq.track_mut(t)) {
                t.clear_range(clip.in_point, clip.out_point, &[]);
                t.insert_clip(clip);
            }
        });
        self.selected = Some(id);
        self.insp_tab = InspTab::Title;
    }

    pub fn apply_effect(&mut self, effect_id: &str, target: Option<ClipId>) {
        let Some(clip) = target.or(self.selected) else {
            self.notify("Select a clip on the timeline first");
            return;
        };
        let Some(effect) = zavyx_ve_render::lookup_effect(effect_id) else { return };
        let mut fx = EffectInstance::new(effect_id);
        fx.params = crate::media_cache::showcase_params(&effect.param_schema());
        let name = effect.name().to_string();
        self.edit_clip(clip, "Add effect", |c| c.effects.push(fx));
        self.selected = Some(clip);
        self.insp_tab = InspTab::Effects;
        self.notify(format!("Added {name}"));
    }

    /// Set a clip's incoming transition (replacing any existing one).
    pub fn apply_transition(&mut self, transition_id: &str, target: Option<ClipId>) {
        let Some(clip) = target.or(self.selected) else {
            self.notify("Select the clip the transition should lead into");
            return;
        };
        let Some((_, c)) = self.find_clip(clip) else { return };
        let dur = (c.duration().to_f64() / 2.0).min(1.0);
        let mut fx = EffectInstance::new(transition_id);
        fx.set_const("duration", dur);
        self.edit_clip(clip, "Add transition", |c| {
            c.effects.retain(|e| !e.effect_id.starts_with(zavyx_ve_render::TRANSITION_PREFIX));
            c.effects.push(fx);
        });
        self.selected = Some(clip);
        self.insp_tab = InspTab::Effects;
    }

    // --- Export ------------------------------------------------------------------------

    pub fn open_export(&mut self) {
        self.commit_live();
        if self.duration() <= 0.0 {
            self.notify("Add some media to the timeline before exporting");
            return;
        }
        self.engine.pause();
        let folder = self.settings.last_export_dir.clone()
            .or_else(|| std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join("Videos")))
            .unwrap_or_default();
        self.export_dialog = Some(ExportDialog {
            format: export::Format::Mp4,
            name: self.project.name.clone(),
            folder,
            height: self.project.settings.height,
            fps: self.fps(),
            quality: export::Quality::Medium,
        });
    }

    pub fn start_export(&mut self) {
        let Some(d) = self.export_dialog.as_ref() else { return };
        let s = &self.project.settings;
        let height = d.height.min(s.height.max(d.height));
        let width = ((s.width as f64 * height as f64 / s.height as f64).round() as u32) & !1;
        let path = d.folder.join(format!("{}.{}", sanitize(&d.name), d.format.extension()));
        let settings = export::Settings { format: d.format, path, width, height: height & !1, fps: d.fps, quality: d.quality, range: None };
        self.settings.last_export_dir = Some(d.folder.clone());
        self.settings.save();
        let snap = crate::mixer::snapshot(&self.project);
        let ctx = self.egui_ctx.clone();
        self.export_job = Some(export::start(snap, settings, move || ctx.request_repaint()));
    }
}

/// Fix media paths of a project that was moved: try the path stored relative
/// to the project file when the absolute path is gone.
fn relink_media(p: &mut Project, project_path: &Path) {
    let dir = project_path.parent().map(Path::to_path_buf).unwrap_or_default();
    for (mref, info) in p.media_pool.items.values_mut() {
        if !info.path.is_file() {
            let candidate = dir.join(&mref.relative_path);
            if candidate.is_file() {
                info.path = candidate;
            }
        }
    }
}

fn sanitize(name: &str) -> String {
    let s: String = name.chars().map(|c| if "<>:\"/\\|?*".contains(c) { '_' } else { c }).collect();
    let s = s.trim().to_string();
    if s.is_empty() { "Zavyx Video".into() } else { s }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_bad_characters() {
        assert_eq!(sanitize("My: Video?"), "My_ Video_");
        assert_eq!(sanitize("  "), "Zavyx Video");
    }

    #[test]
    fn relink_uses_relative_path_when_absolute_is_missing() {
        let dir = std::env::temp_dir().join(format!("zavyx-relink-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("clip.mp4"), b"x").unwrap();
        let mut p = Project::new("t");
        let id = MediaId::new();
        p.media_pool.add(
            MediaRef { id, relative_path: "clip.mp4".into(), display_name: "clip".into() },
            MediaInfo {
                path: "Z:/gone/clip.mp4".into(), duration: Rational::ONE, width: None, height: None,
                fps: None, video_codec: None, audio_codec: None, sample_rate: None, channels: None,
                has_audio: false, has_video: true, file_size: 1,
            },
        );
        relink_media(&mut p, &dir.join("project.zvep"));
        assert_eq!(p.media_pool.get(&id).unwrap().1.path, dir.join("clip.mp4"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
