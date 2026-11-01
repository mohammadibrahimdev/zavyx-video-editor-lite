//! Timeline audio mixing, shared by live playback and export.
//!
//! Every audible clip (clips on unmuted audio tracks, and video clips whose
//! file has sound) streams its audio from ffmpeg. Per clip we apply speed
//! (via ffmpeg's `atempo`), volume and fade in/out from the clip's
//! `zavyx.audio` settings, then sum into a stereo buffer.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use zavyx_ve_core::{ClipBlock, ClipId, MediaId, MediaInfo, Rational, Sequence, TrackKind};
use zavyx_ve_media::cli::AudioReader;

/// Effect id holding a clip's audio settings (volume, fade_in, fade_out).
pub const AUDIO_SETTINGS: &str = "zavyx.audio";

/// What the playback and export threads need from the project.
#[derive(Clone)]
pub struct Snapshot {
    pub seq: Sequence,
    pub media: HashMap<MediaId, MediaInfo>,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
}

impl Snapshot {
    pub fn duration(&self) -> f64 {
        self.seq.duration().to_f64()
    }

    pub fn path(&self, id: MediaId) -> Option<PathBuf> {
        self.media.get(&id).map(|m| m.path.clone())
    }
}

/// Volume and fades of a clip, read from its `zavyx.audio` effect.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioSettings {
    pub volume: f32,
    pub fade_in: f64,
    pub fade_out: f64,
}

impl AudioSettings {
    pub fn of(clip: &ClipBlock) -> Self {
        let fx = clip.effects.iter().find(|e| e.effect_id == AUDIO_SETTINGS);
        let get = |k: &str, d: f64| fx.and_then(|e| e.param(k, Rational::ZERO)).unwrap_or(d);
        AudioSettings {
            volume: get("volume", 1.0) as f32,
            fade_in: get("fade_in", 0.0),
            fade_out: get("fade_out", 0.0),
        }
    }

    /// Gain at `t` seconds into a clip that lasts `len` seconds.
    pub fn gain_at(&self, t: f64, len: f64) -> f32 {
        let mut g = self.volume;
        if self.fade_in > 0.0 && t < self.fade_in {
            g *= (t / self.fade_in).clamp(0.0, 1.0) as f32;
        }
        if self.fade_out > 0.0 && len - t < self.fade_out {
            g *= ((len - t) / self.fade_out).clamp(0.0, 1.0) as f32;
        }
        g
    }
}

/// Clips that should be heard, with the file they come from.
fn audible(snap: &Snapshot) -> Vec<(&ClipBlock, PathBuf)> {
    let any_solo = snap.seq.tracks.iter().any(|t| t.solo);
    let mut out = Vec::new();
    for track in &snap.seq.tracks {
        if track.muted || (any_solo && !track.solo) || track.kind == TrackKind::Subtitle {
            continue;
        }
        for clip in track.blocks.iter().filter_map(|b| b.as_clip()) {
            let Some(info) = snap.media.get(&clip.media_id) else { continue };
            if !info.has_audio || clip.reverse || AudioSettings::of(clip).volume <= 0.0 {
                continue;
            }
            out.push((clip, info.path.clone()));
        }
    }
    out
}

struct Stream {
    reader: AudioReader,
    /// Timeline sample position the next read corresponds to.
    next: i64,
}

/// Mixes the timeline into interleaved stereo f32 at `rate`.
pub struct Mixer {
    pub rate: u32,
    streams: HashMap<ClipId, Stream>,
    scratch: Vec<f32>,
}

impl Mixer {
    pub fn new(rate: u32) -> Self {
        Mixer { rate, streams: HashMap::new(), scratch: Vec::new() }
    }

    /// Forget all open streams (after a seek or an edit).
    pub fn reset(&mut self) {
        self.streams.clear();
    }

    /// Fill `out` (stereo frames) with the mix starting at sample `pos`.
    pub fn mix(&mut self, snap: &Snapshot, pos: i64, out: &mut [f32]) {
        out.fill(0.0);
        let frames = (out.len() / 2) as i64;
        let rate = self.rate as f64;
        let block_start = pos as f64 / rate;
        let block_end = (pos + frames) as f64 / rate;
        let clips = audible(snap);
        let live: Vec<ClipId> = clips.iter().map(|(c, _)| c.id).collect();
        self.streams.retain(|id, _| live.contains(id));

        for (clip, path) in clips {
            let (cin, cout) = (clip.in_point.to_f64(), clip.out_point.to_f64());
            if cout <= block_start || cin >= block_end {
                continue;
            }
            let first = ((cin * rate).round() as i64).max(pos);
            let last = ((cout * rate).round() as i64).min(pos + frames);
            if last <= first {
                continue;
            }
            let need_open = self.streams.get(&clip.id).map(|s| s.next != first).unwrap_or(true);
            if need_open {
                let media_t = clip.timeline_to_media_time(Rational::from_seconds(first as f64 / rate)).to_f64();
                match open_reader(&path, media_t.max(0.0), self.rate, clip.speed) {
                    Some(reader) => { self.streams.insert(clip.id, Stream { reader, next: first }); }
                    None => continue,
                }
            }
            let stream = self.streams.get_mut(&clip.id).unwrap();
            let n = (last - first) as usize;
            self.scratch.resize(n * 2, 0.0);
            stream.reader.read(&mut self.scratch);
            stream.next = last;

            let settings = AudioSettings::of(clip);
            let len = cout - cin;
            let offset = (first - pos) as usize;
            for i in 0..n {
                let t = (first + i as i64) as f64 / rate - cin;
                let g = settings.gain_at(t, len);
                out[(offset + i) * 2] += self.scratch[i * 2] * g;
                out[(offset + i) * 2 + 1] += self.scratch[i * 2 + 1] * g;
            }
        }
        for s in out.iter_mut() {
            *s = s.clamp(-1.0, 1.0);
        }
    }
}

fn open_reader(path: &std::path::Path, start: f64, rate: u32, speed: f64) -> Option<AudioReader> {
    let speed = if speed > 0.0 { speed } else { 1.0 };
    AudioReader::open_with_speed(path, start, rate, 2, speed).ok()
}

/// Build a snapshot of the project for the worker threads.
pub fn snapshot(project: &zavyx_ve_core::Project) -> Arc<Snapshot> {
    let seq = project.active_sequence().cloned()
        .unwrap_or_else(|| Sequence::new("Sequence 1", project.settings.time_base));
    let media = project.media_pool.items.iter()
        .map(|(id, (_, info))| (*id, info.clone()))
        .collect();
    Arc::new(Snapshot {
        seq,
        media,
        width: project.settings.width,
        height: project.settings.height,
        fps: project.settings.time_base.0.to_f64(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fades_shape_the_gain() {
        let s = AudioSettings { volume: 0.5, fade_in: 1.0, fade_out: 2.0 };
        assert_eq!(s.gain_at(0.0, 10.0), 0.0);
        assert_eq!(s.gain_at(0.5, 10.0), 0.25);
        assert_eq!(s.gain_at(5.0, 10.0), 0.5);
        assert_eq!(s.gain_at(9.0, 10.0), 0.25);
    }

    #[test]
    fn muted_track_and_silent_media_are_skipped() {
        let mut seq = Sequence::new("s", zavyx_ve_core::FrameRate::FPS_30);
        let (loud, silent) = (MediaId::new(), MediaId::new());
        seq.tracks[0].insert_clip(ClipBlock::new(loud, Rational::ZERO, Rational::ONE, Rational::ZERO));
        seq.tracks[0].insert_clip(ClipBlock::new(silent, Rational::ONE, Rational::new(2, 1), Rational::ZERO));
        let info = |audio| MediaInfo {
            path: "x.mp4".into(), duration: Rational::ONE, width: None, height: None, fps: None,
            video_codec: None, audio_codec: None, sample_rate: None, channels: None,
            has_audio: audio, has_video: true, file_size: 0,
        };
        let media = [(loud, info(true)), (silent, info(false))].into_iter().collect();
        let mut snap = Snapshot { seq, media, width: 16, height: 9, fps: 30.0 };
        assert_eq!(audible(&snap).len(), 1);
        snap.seq.tracks[0].muted = true;
        assert!(audible(&snap).is_empty());
    }
}
