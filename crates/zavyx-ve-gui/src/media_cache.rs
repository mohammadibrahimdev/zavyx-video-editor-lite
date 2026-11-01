//! Background-made pictures for the UI: media posters, timeline thumbnail
//! strips, audio waveforms, and effect/transition preview cards.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use crossbeam_channel::{unbounded, Receiver, Sender};
use egui::{ColorImage, TextureHandle, TextureOptions};

use zavyx_ve_core::effect::{FrameView, FrameViewRef, ParamType, ParamValue, RenderCtx};
use zavyx_ve_core::{MediaId, MediaInfo, Rational};
use zavyx_ve_media::cli::{decode_frame, fit_size, waveform_peaks};
use zavyx_ve_media::Frame;

/// Waveform resolution (peaks per second of audio).
pub const WAVE_PER_SECOND: u32 = 50;
const STRIP_FRAMES: usize = 12;

enum Job {
    Media(MediaId, PathBuf, MediaInfo),
    Previews,
}

enum Done {
    Poster(MediaId, Frame),
    Strip(MediaId, Vec<(f64, Frame)>),
    Wave(MediaId, Vec<f32>),
    Failed(MediaId),
    Preview(String, Frame),
}

pub struct MediaCache {
    tx: Sender<Job>,
    rx: Receiver<Done>,
    pending: HashSet<MediaId>,
    pub posters: HashMap<MediaId, TextureHandle>,
    /// (media time, texture) pairs across the clip, for timeline tiles.
    pub strips: HashMap<MediaId, Vec<(f64, TextureHandle)>>,
    pub waves: HashMap<MediaId, Arc<Vec<f32>>>,
    pub previews: HashMap<String, TextureHandle>,
    previews_requested: bool,
}

impl MediaCache {
    pub fn new(repaint: egui::Context) -> Self {
        let (tx, job_rx) = unbounded::<Job>();
        let (done_tx, rx) = unbounded::<Done>();
        std::thread::Builder::new().name("zavyx-thumbs".into())
            .spawn(move || worker(job_rx, done_tx, repaint)).ok();
        MediaCache {
            tx, rx,
            pending: HashSet::new(),
            posters: HashMap::new(),
            strips: HashMap::new(),
            waves: HashMap::new(),
            previews: HashMap::new(),
            previews_requested: false,
        }
    }

    /// Ask for pictures of a media file (once).
    pub fn request(&mut self, id: MediaId, info: &MediaInfo) {
        if self.pending.insert(id) {
            let _ = self.tx.send(Job::Media(id, info.path.clone(), info.clone()));
        }
    }

    pub fn request_previews(&mut self) {
        if !self.previews_requested {
            self.previews_requested = true;
            let _ = self.tx.send(Job::Previews);
        }
    }

    /// Move finished pictures into GPU textures. Call once per UI frame.
    pub fn poll(&mut self, ctx: &egui::Context) {
        for done in self.rx.try_iter() {
            match done {
                Done::Poster(id, f) => {
                    self.posters.insert(id, texture(ctx, &format!("poster-{id:?}"), &f));
                }
                Done::Strip(id, frames) => {
                    let tex = frames.iter().enumerate()
                        .map(|(i, (t, f))| (*t, texture(ctx, &format!("strip-{id:?}-{i}"), f)))
                        .collect();
                    self.strips.insert(id, tex);
                }
                Done::Wave(id, peaks) => { self.waves.insert(id, Arc::new(peaks)); }
                Done::Failed(_) => {}
                Done::Preview(key, f) => {
                    self.previews.insert(key.clone(), texture(ctx, &key, &f));
                }
            }
        }
    }
}

fn texture(ctx: &egui::Context, name: &str, f: &Frame) -> TextureHandle {
    let img = ColorImage::from_rgba_unmultiplied([f.width as usize, f.height as usize], &f.pixels);
    ctx.load_texture(name, img, TextureOptions::LINEAR)
}

fn worker(rx: Receiver<Job>, tx: Sender<Done>, repaint: egui::Context) {
    for job in rx {
        match job {
            Job::Media(id, path, info) => {
                if info.has_video {
                    let (w, h) = fit_size(info.width.unwrap_or(16), info.height.unwrap_or(9), 240, 136);
                    let dur = info.duration.to_f64();
                    let at = if zavyx_ve_media::cli::is_still_image(&path) { 0.0 } else { (dur * 0.1).min(1.0) };
                    match decode_frame(&path, at, w, h) {
                        Ok(f) => { let _ = tx.send(Done::Poster(id, f)); }
                        Err(_) => { let _ = tx.send(Done::Failed(id)); }
                    }
                    repaint.request_repaint();
                    let (sw, sh) = fit_size(info.width.unwrap_or(16), info.height.unwrap_or(9), 128, 72);
                    let count = if zavyx_ve_media::cli::is_still_image(&path) { 1 } else { STRIP_FRAMES };
                    let strip: Vec<(f64, Frame)> = (0..count)
                        .filter_map(|i| {
                            let t = dur * i as f64 / count as f64;
                            decode_frame(&path, t, sw, sh).ok().map(|f| (t, f))
                        })
                        .collect();
                    let _ = tx.send(Done::Strip(id, strip));
                }
                if info.has_audio {
                    if let Ok(p) = waveform_peaks(&path, WAVE_PER_SECOND) {
                        let _ = tx.send(Done::Wave(id, p));
                    }
                }
                repaint.request_repaint();
            }
            Job::Previews => {
                make_previews(&tx);
                repaint.request_repaint();
            }
        }
    }
}

const PW: u32 = 192;
const PH: u32 = 108;

/// A small painted scene (sky, sun, hills) used to preview effects.
pub fn sample_scene(variant: u8) -> Frame {
    let mut px = vec![0u8; (PW * PH * 4) as usize];
    for y in 0..PH {
        for x in 0..PW {
            let fy = y as f32 / PH as f32;
            let fx = x as f32 / PW as f32;
            let (sky_top, sky_bot) = if variant == 0 {
                ([0x5B, 0x34, 0xE6], [0xFF, 0x9E, 0x7A])
            } else {
                ([0x12, 0x6E, 0xB8], [0x9F, 0xE2, 0xF5])
            };
            let mut c = [0f32; 3];
            for i in 0..3 {
                c[i] = sky_top[i] as f32 + (sky_bot[i] as f32 - sky_top[i] as f32) * fy;
            }
            let (sx, sy) = (fx - 0.7, fy - 0.38);
            if sx * sx * 3.2 + sy * sy < 0.018 {
                c = [255.0, 0xE6 as f32, 0x8A as f32];
            }
            let hill1 = 0.62 + 0.08 * (fx * 7.0).sin();
            let hill2 = 0.75 + 0.05 * (fx * 11.0 + 1.0).cos();
            if fy > hill1 {
                c = if variant == 0 { [0x2E as f32, 0x1B as f32, 0x5E as f32] } else { [0x1E as f32, 0x6B as f32, 0x3A as f32] };
            }
            if fy > hill2 {
                c = if variant == 0 { [0x17 as f32, 0x0D as f32, 0x33 as f32] } else { [0x13 as f32, 0x45 as f32, 0x25 as f32] };
            }
            let i = ((y * PW + x) * 4) as usize;
            px[i] = c[0] as u8;
            px[i + 1] = c[1] as u8;
            px[i + 2] = c[2] as u8;
            px[i + 3] = 255;
        }
    }
    Frame { width: PW, height: PH, pixels: px, pts: Rational::ZERO }
}

/// Parameter values that make an effect visibly change the sample.
pub fn showcase_params(schema: &zavyx_ve_core::ParamSchema) -> BTreeMap<String, ParamValue> {
    schema.params.iter().filter_map(|p| match &p.ty {
        ParamType::Float { min, max, default, .. } => {
            let v = if default.abs() > 1e-9 { *default } else { min + (max - min) * 0.75 };
            Some((p.id.clone(), ParamValue::Const(v)))
        }
        ParamType::Int { min, max, default } => {
            let v = if *default != 0 { *default } else { min + (max - min) * 3 / 4 };
            Some((p.id.clone(), ParamValue::Const(v as f64)))
        }
        ParamType::Bool { default } => Some((p.id.clone(), ParamValue::Const(*default as i32 as f64))),
        _ => None,
    }).collect()
}

fn make_previews(tx: &Sender<Done>) {
    zavyx_ve_render::register_builtins();
    let ctx = RenderCtx { working_color_space: "linear-rec709" };
    let base = sample_scene(0);
    for fx in zavyx_ve_render::all_effects() {
        if fx.id() == zavyx_ve_render::TITLE_EFFECT {
            continue;
        }
        let mut f = base.clone();
        let params = showcase_params(&fx.param_schema());
        let mut view = FrameView { width: f.width, height: f.height, pixels: &mut f.pixels, time: Rational::new(1, 2) };
        fx.process(&mut view, &params, &ctx);
        let _ = tx.send(Done::Preview(format!("fx:{}", fx.id()), f));
    }
    let other = sample_scene(1);
    for tr in zavyx_ve_render::all_transitions() {
        let mut from = base.clone();
        let mut view = FrameView { width: from.width, height: from.height, pixels: &mut from.pixels, time: Rational::ZERO };
        let to = FrameViewRef { width: other.width, height: other.height, pixels: &other.pixels, time: Rational::ZERO };
        tr.process(&mut view, to, 0.45, &BTreeMap::new(), &ctx);
        let _ = tx.send(Done::Preview(format!("tr:{}", tr.id()), from));
    }
}
