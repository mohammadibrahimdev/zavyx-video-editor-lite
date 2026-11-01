//! Project model — the root of every `.zvep` file.

use crate::time::{Rational, FrameRate};
use crate::id::{ProjectId, SeqId, MediaId};
use serde::{Serialize, Deserialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Aspect ratio preset for new projects.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AspectRatio {
    /// 16:9 landscape
    Landscape,
    /// 9:16 vertical (TikTok / Shorts / Reels)
    Vertical,
    /// 1:1 square (Instagram)
    Square,
    /// 4:5 portrait (Instagram feed)
    Portrait45,
    /// 21:9 cinematic
    Cinematic,
    /// User-defined
    Custom(u32, u32),
}

impl AspectRatio {
    pub fn dimensions(self, base: u32) -> (u32, u32) {
        match self {
            AspectRatio::Landscape  => (base * 16 / 9, base),
            AspectRatio::Vertical   => (base * 9 / 16, base),
            AspectRatio::Square     => (base, base),
            AspectRatio::Portrait45 => (base * 4 / 5, base),
            AspectRatio::Cinematic  => (base * 21 / 9, base),
            AspectRatio::Custom(w, h) => (w, h),
        }
    }
    /// For a target width, what's the height?
    pub fn dims_for_width(self, width: u32) -> (u32, u32) {
        let (w, h) = match self {
            AspectRatio::Landscape  => (16, 9),
            AspectRatio::Vertical   => (9, 16),
            AspectRatio::Square     => (1, 1),
            AspectRatio::Portrait45 => (4, 5),
            AspectRatio::Cinematic  => (21, 9),
            AspectRatio::Custom(w, h) => (w as u32, h as u32),
        };
        (width, width * h / w)
    }
}

/// Basic info about an imported media file (probed once on import).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MediaInfo {
    pub path: PathBuf,
    pub duration: Rational,
    pub width:  Option<u32>,
    pub height: Option<u32>,
    pub fps:    Option<FrameRate>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub sample_rate: Option<u32>,
    pub channels:    Option<u16>,
    pub has_audio: bool,
    pub has_video: bool,
    pub file_size: u64,
}

/// A reference from the project's media pool into a `MediaInfo`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MediaRef {
    pub id: MediaId,
    /// Path relative to the .zvep file's directory, or absolute if external.
    pub relative_path: PathBuf,
    pub display_name: String,
}

/// The media pool — every imported file plus its probed info.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MediaPool {
    pub items: BTreeMap<MediaId, (MediaRef, MediaInfo)>,
}

impl MediaPool {
    pub fn add(&mut self, mref: MediaRef, info: MediaInfo) -> MediaId {
        let id = mref.id;
        self.items.insert(id, (mref, info));
        id
    }
    pub fn get(&self, id: &MediaId) -> Option<&(MediaRef, MediaInfo)> {
        self.items.get(id)
    }
    pub fn remove(&mut self, id: &MediaId) -> Option<(MediaRef, MediaInfo)> {
        self.items.remove(id)
    }
    pub fn iter(&self) -> impl Iterator<Item = (&MediaId, &(MediaRef, MediaInfo))> {
        self.items.iter()
    }
}

/// User-configurable settings stored in the project file.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub time_base: FrameRate,
    pub aspect: AspectRatio,
    pub width:  u32,
    pub height: u32,
    pub sample_rate: u32,    // 48000 default
    pub audio_channels: u16, // 2 default
    pub working_color_space: String,
}

impl Default for Settings {
    fn default() -> Self {
        let (w, h) = AspectRatio::Landscape.dims_for_width(1920);
        Settings {
            time_base: FrameRate::FPS_30,
            aspect: AspectRatio::Landscape,
            width: w,
            height: h,
            sample_rate: 48_000,
            audio_channels: 2,
            working_color_space: "linear-rec709".into(),
        }
    }
}

/// The top-level project. Everything in a `.zvep` file is one of these.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Project {
    pub id: ProjectId,
    pub name: String,
    pub version: u32,             // schema version, currently 2
    pub settings: Settings,
    pub media_pool: MediaPool,
    pub sequences: BTreeMap<SeqId, crate::timeline::Sequence>,
    pub active_sequence: Option<SeqId>,
    /// Path the project was last saved to (None if unsaved).
    #[serde(skip)]
    pub last_save_path: Option<PathBuf>,
}

impl Project {
    pub fn new(name: impl Into<String>) -> Self {
        let settings = Settings::default();
        let mut sequences = BTreeMap::new();
        let seq = crate::timeline::Sequence::new("Sequence 1", settings.time_base);
        let seq_id = seq.id;
        sequences.insert(seq_id, seq);
        Project {
            id: ProjectId::new(),
            name: name.into(),
            version: crate::serde::SCHEMA_VERSION,
            settings,
            media_pool: MediaPool::default(),
            sequences,
            active_sequence: Some(seq_id),
            last_save_path: None,
        }
    }

    pub fn active_sequence(&self) -> Option<&crate::timeline::Sequence> {
        self.active_sequence.and_then(|id| self.sequences.get(&id))
    }

    pub fn active_sequence_mut(&mut self) -> Option<&mut crate::timeline::Sequence> {
        if let Some(id) = self.active_sequence {
            self.sequences.get_mut(&id)
        } else { None }
    }
}

impl Default for Project {
    fn default() -> Self { Project::new("Untitled Project") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_project_has_one_sequence() {
        let p = Project::new("Test");
        assert_eq!(p.name, "Test");
        assert_eq!(p.version, 2);
        assert!(p.active_sequence().is_some());
        assert_eq!(p.active_sequence().unwrap().name, "Sequence 1");
    }

    #[test]
    fn aspect_ratio_dims() {
        assert_eq!(AspectRatio::Landscape.dims_for_width(1920), (1920, 1080));
        assert_eq!(AspectRatio::Vertical.dims_for_width(1080), (1080, 1920));
        assert_eq!(AspectRatio::Square.dims_for_width(1080), (1080, 1080));
        assert_eq!(AspectRatio::Portrait45.dims_for_width(1080), (1080, 1350));
    }
}
