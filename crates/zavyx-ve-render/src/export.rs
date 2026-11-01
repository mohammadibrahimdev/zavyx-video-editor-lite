//! Export: render the timeline to a file with ffmpeg on a worker thread.
//! Shared by the app (`ZavyxVE.exe`) and the command line (`zavyx-ve-cli`).
//!
//! Pass 1 mixes the whole timeline's audio to a temporary raw file. Pass 2
//! composes every video frame at full size and pipes it to ffmpeg, which
//! encodes it together with that audio.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;

use zavyx_ve_core::{ClipBlock, MediaId, Rational};
use zavyx_ve_media::cli::{command, fit_size, has_encoder, CliDecoder};
use zavyx_ve_media::{Decoder, Frame};

use crate::mixer::{Mixer, Snapshot};
use crate::Compositor;

const AUDIO_RATE: u32 = 48_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format { Mp4, Mov, Mkv, Webm, Gif, Mp3, Wav, PngSequence }

impl Format {
    pub const ALL: [Format; 7] = [Format::Mp4, Format::Mov, Format::Mkv, Format::Webm, Format::Gif, Format::Mp3, Format::Wav];

    pub fn label(self) -> &'static str {
        match self {
            Format::Mp4 => "MP4", Format::Mov => "MOV", Format::Mkv => "MKV",
            Format::Webm => "WEBM", Format::Gif => "GIF", Format::Mp3 => "MP3", Format::Wav => "WAV",
            Format::PngSequence => "PNG",
        }
    }
    pub fn description(self) -> &'static str {
        match self {
            Format::Mp4 => "Plays everywhere: phones, YouTube, social media",
            Format::Mov => "QuickTime video, good for Apple devices",
            Format::Mkv => "Flexible container, good for archiving",
            Format::Webm => "Open web video (VP9 + Opus)",
            Format::Gif => "Silent looping animation",
            Format::Mp3 => "Audio only, small file",
            Format::Wav => "Audio only, uncompressed",
            Format::PngSequence => "One PNG image per frame, into a folder",
        }
    }
    pub fn extension(self) -> &'static str {
        match self {
            Format::Mp4 => "mp4", Format::Mov => "mov", Format::Mkv => "mkv", Format::Webm => "webm",
            Format::Gif => "gif", Format::Mp3 => "mp3", Format::Wav => "wav", Format::PngSequence => "png",
        }
    }
    pub fn audio_only(self) -> bool { matches!(self, Format::Mp3 | Format::Wav) }
    pub fn has_audio(self) -> bool { !matches!(self, Format::Gif | Format::PngSequence) }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quality { Low, Medium, High }

impl Quality {
    pub fn label(self) -> &'static str {
        match self { Quality::Low => "Smaller file", Quality::Medium => "Balanced", Quality::High => "Best quality" }
    }
}

#[derive(Clone, Debug)]
pub struct Settings {
    pub format: Format,
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub quality: Quality,
    /// Only export this part of the timeline (start, end) in seconds.
    pub range: Option<(f64, f64)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum State {
    Running(String),
    Done(PathBuf),
    Failed(String),
    Cancelled,
}

pub struct Job {
    /// Progress in thousandths.
    pub progress: Arc<AtomicU32>,
    pub cancel: Arc<AtomicBool>,
    pub state: Arc<Mutex<State>>,
}

impl Job {
    pub fn fraction(&self) -> f32 {
        self.progress.load(Ordering::Relaxed) as f32 / 1000.0
    }
    pub fn state(&self) -> State {
        self.state.lock().clone()
    }
}

/// Start exporting on a worker thread; `notify` is called on progress.
pub fn start(snap: Arc<Snapshot>, settings: Settings, notify: impl Fn() + Send + 'static) -> Job {
    let job = Job {
        progress: Arc::new(AtomicU32::new(0)),
        cancel: Arc::new(AtomicBool::new(false)),
        state: Arc::new(Mutex::new(State::Running("Preparing".into()))),
    };
    let (progress, cancel, state) = (job.progress.clone(), job.cancel.clone(), job.state.clone());
    std::thread::Builder::new().name("zavyx-export".into()).spawn(move || {
        let result = run(&snap, &settings, &progress, &cancel, &state, &notify);
        *state.lock() = match result {
            Ok(()) if cancel.load(Ordering::Relaxed) => {
                if settings.format != Format::PngSequence {
                    let _ = std::fs::remove_file(&settings.path);
                }
                State::Cancelled
            }
            Ok(()) => State::Done(settings.path.clone()),
            Err(e) => State::Failed(e),
        };
        notify();
    }).ok();
    job
}

fn run(
    snap: &Snapshot,
    s: &Settings,
    progress: &AtomicU32,
    cancel: &AtomicBool,
    state: &Mutex<State>,
    notify: &dyn Fn(),
) -> Result<(), String> {
    let total = snap.duration();
    if total <= 0.0 {
        return Err("The timeline is empty. Add some media first.".into());
    }
    let (t0, t1) = s.range.map(|(a, b)| (a.clamp(0.0, total), b.clamp(0.0, total))).unwrap_or((0.0, total));
    if t1 <= t0 {
        return Err("The export range is empty.".into());
    }
    let duration = t1 - t0;
    let first_sample = (t0 * AUDIO_RATE as f64).round() as i64;
    let temp = std::env::temp_dir().join("zavyx-ve-export");
    std::fs::create_dir_all(&temp).map_err(|e| e.to_string())?;
    let audio_path = temp.join(format!("audio-{}.f32", std::process::id()));
    let log_path = temp.join("ffmpeg-export.log");

    // Pass 1: audio.
    let audio_share = if s.format.audio_only() { 0.9 } else { 0.15 };
    if s.format.has_audio() {
        *state.lock() = State::Running("Mixing audio".into());
        let mut mixer = Mixer::new(AUDIO_RATE);
        let mut file = std::io::BufWriter::new(std::fs::File::create(&audio_path).map_err(|e| e.to_string())?);
        let total = (duration * AUDIO_RATE as f64).ceil() as i64;
        let block = 4096i64;
        let mut buf = vec![0f32; (block * 2) as usize];
        let mut pos = 0i64;
        while pos < total {
            if cancel.load(Ordering::Relaxed) { return Ok(()); }
            let n = block.min(total - pos);
            buf.resize((n * 2) as usize, 0.0);
            mixer.mix(snap, first_sample + pos, &mut buf);
            let bytes: Vec<u8> = buf.iter().flat_map(|v| v.to_le_bytes()).collect();
            file.write_all(&bytes).map_err(|e| e.to_string())?;
            pos += n;
            progress.store((pos as f64 / total as f64 * audio_share * 1000.0) as u32, Ordering::Relaxed);
            notify();
        }
        file.flush().map_err(|e| e.to_string())?;
    }

    // Pass 2: encode.
    let mut cmd = command().map_err(|e| e.to_string())?;
    cmd.arg("-y");
    let (w, h) = (s.width & !1, s.height & !1);
    if !s.format.audio_only() {
        cmd.args(["-f", "rawvideo", "-pix_fmt", "rgba", "-s"]).arg(format!("{w}x{h}"))
            .arg("-r").arg(format!("{}", s.fps)).args(["-i", "-"]);
    }
    if s.format.has_audio() {
        cmd.args(["-f", "f32le", "-ar"]).arg(AUDIO_RATE.to_string()).args(["-ac", "2", "-i"]).arg(&audio_path);
    }
    cmd.args(codec_args(s.format, s.quality, s.fps));
    if s.format == Format::PngSequence {
        std::fs::create_dir_all(&s.path).map_err(|e| e.to_string())?;
        cmd.arg(s.path.join("frame_%06d.png"));
    } else {
        cmd.arg(&s.path);
    }
    let log = std::fs::File::create(&log_path).map_err(|e| e.to_string())?;
    cmd.stdin(if s.format.audio_only() { Stdio::null() } else { Stdio::piped() })
        .stdout(Stdio::null())
        .stderr(log);
    *state.lock() = State::Running(if s.format.audio_only() { "Encoding audio".into() } else { "Rendering video".into() });
    let mut child = cmd.spawn().map_err(|e| format!("Could not start ffmpeg: {e}"))?;

    if !s.format.audio_only() {
        crate::register_builtins();
        let compositor = Compositor::new("linear-rec709");
        let mut decoders: HashMap<(MediaId, u32, u32), CliDecoder> = HashMap::new();
        let mut stdin = child.stdin.take().ok_or("ffmpeg has no input pipe")?;
        let frames = (duration * s.fps).ceil().max(1.0) as u64;
        for i in 0..frames {
            if cancel.load(Ordering::Relaxed) {
                drop(stdin);
                let _ = child.kill();
                let _ = child.wait();
                let _ = std::fs::remove_file(&audio_path);
                return Ok(());
            }
            let t = t0 + i as f64 / s.fps;
            let mut source = |clip: &ClipBlock, media_t: Rational, bw: u32, bh: u32| -> Option<Frame> {
                let info = snap.media.get(&clip.media_id)?;
                if !info.has_video { return None; }
                let (fw, fh) = fit_size(info.width.unwrap_or(bw), info.height.unwrap_or(bh), bw, bh);
                decoders.entry((clip.media_id, fw, fh))
                    .or_insert_with(|| CliDecoder::new(&info.path, fw, fh, info.fps))
                    .frame_at(media_t).ok()
            };
            let frame = compositor.compose(&snap.seq, Rational::from_seconds(t), w, h, &mut source);
            if stdin.write_all(&frame.pixels).is_err() {
                break;
            }
            let done = audio_share + (1.0 - audio_share) * (i + 1) as f64 / frames as f64;
            progress.store((done * 1000.0) as u32, Ordering::Relaxed);
            notify();
        }
        drop(stdin);
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&audio_path);
    if !status.success() {
        return Err(format!("ffmpeg failed: {}", last_lines(&log_path)));
    }
    progress.store(1000, Ordering::Relaxed);
    Ok(())
}

/// One full-size frame of the timeline (for snapshots).
pub fn render_still(snap: &Snapshot, t: f64) -> Frame {
    crate::register_builtins();
    let compositor = Compositor::new("linear-rec709");
    let mut decoders: HashMap<(MediaId, u32, u32), CliDecoder> = HashMap::new();
    let mut source = |clip: &ClipBlock, media_t: Rational, bw: u32, bh: u32| -> Option<Frame> {
        let info = snap.media.get(&clip.media_id)?;
        if !info.has_video { return None; }
        let (fw, fh) = fit_size(info.width.unwrap_or(bw), info.height.unwrap_or(bh), bw, bh);
        decoders.entry((clip.media_id, fw, fh))
            .or_insert_with(|| CliDecoder::new(&info.path, fw, fh, info.fps))
            .frame_at(media_t).ok()
    };
    compositor.compose(&snap.seq, Rational::from_seconds(t), snap.width & !1, snap.height & !1, &mut source)
}

fn codec_args(format: Format, q: Quality, fps: f64) -> Vec<String> {
    let crf = |low: u32, mid: u32, high: u32| match q { Quality::Low => low, Quality::Medium => mid, Quality::High => high };
    let mut a: Vec<String> = Vec::new();
    let mut push = |items: &[&str]| a.extend(items.iter().map(|s| s.to_string()));
    match format {
        Format::Mp4 | Format::Mov | Format::Mkv => {
            if has_encoder("libx264") {
                push(&["-c:v", "libx264", "-preset", "medium", "-crf"]);
                a.push(crf(28, 22, 17).to_string());
            } else if has_encoder("h264_mf") {
                push(&["-c:v", "h264_mf", "-b:v"]);
                a.push(match q { Quality::Low => "3M", Quality::Medium => "8M", Quality::High => "16M" }.into());
            } else {
                push(&["-c:v", "mpeg4", "-q:v"]);
                a.push(crf(8, 4, 2).to_string());
            }
            let mut push = |items: &[&str]| a.extend(items.iter().map(|s| s.to_string()));
            push(&["-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "192k", "-shortest"]);
            if format != Format::Mkv {
                push(&["-movflags", "+faststart"]);
            }
        }
        Format::Webm => {
            push(&["-c:v", "libvpx-vp9", "-b:v", "0", "-row-mt", "1", "-deadline", "good", "-cpu-used", "4", "-crf"]);
            a.push(crf(38, 32, 24).to_string());
            let mut push = |items: &[&str]| a.extend(items.iter().map(|s| s.to_string()));
            push(&["-pix_fmt", "yuv420p", "-c:a", "libopus", "-b:a", "160k", "-shortest"]);
        }
        Format::Gif => {
            let gif_fps = fps.min(15.0);
            push(&["-vf"]);
            a.push(format!(
                "fps={gif_fps},scale='min(640,iw)':-2:flags=lanczos,split[a][b];[a]palettegen[p];[b][p]paletteuse"
            ));
            let mut push = |items: &[&str]| a.extend(items.iter().map(|s| s.to_string()));
            push(&["-loop", "0"]);
        }
        Format::Mp3 => push(&["-c:a", "libmp3lame", "-q:a", match q { Quality::Low => "6", Quality::Medium => "3", Quality::High => "0" }]),
        Format::Wav => push(&["-c:a", "pcm_s16le"]),
        Format::PngSequence => push(&["-c:v", "png"]),
    }
    a
}

fn last_lines(path: &Path) -> String {
    let mut text = String::new();
    if let Ok(mut f) = std::fs::File::open(path) {
        let _ = f.read_to_string(&mut text);
    }
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(3)..].join(" | ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gif_has_no_audio_and_audio_formats_skip_video() {
        assert!(!Format::Gif.has_audio());
        assert!(Format::Mp3.audio_only() && Format::Wav.audio_only());
        assert!(!Format::Mp4.audio_only());
    }

    /// Renders a real 2-second project (video + music + title + filter) to
    /// MP4 and WAV and checks the files with ffmpeg. Skipped without ffmpeg.
    #[test]
    fn exports_real_files() {
        use zavyx_ve_core::{ClipBlock, EffectInstance, Project, TrackKind};
        use zavyx_ve_media::Prober;
        if zavyx_ve_media::cli::ffmpeg_path().is_none() {
            eprintln!("Skipping: no ffmpeg.exe");
            return;
        }
        let dir = std::env::temp_dir().join(format!("zavyx-export-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let clip = dir.join("in.mp4");
        let ok = command().unwrap()
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc2=size=320x180:rate=25:duration=2",
                   "-f", "lavfi", "-i", "sine=frequency=330:duration=2", "-shortest", "-pix_fmt", "yuv420p"])
            .arg(&clip).status().unwrap().success();
        assert!(ok);
        let info = zavyx_ve_media::cli::CliProber.probe(&clip).unwrap();

        let mut p = Project::new("t");
        p.settings.width = 320;
        p.settings.height = 180;
        let id = zavyx_ve_core::MediaId::new();
        p.media_pool.add(zavyx_ve_core::MediaRef { id, relative_path: clip.clone(), display_name: "in".into() }, info);
        let seq = p.active_sequence_mut().unwrap();
        let mut c = ClipBlock::new(id, Rational::ZERO, Rational::new(2, 1), Rational::ZERO);
        c.effects.push(EffectInstance::new("zavyx.sepia"));
        seq.tracks[0].insert_clip(c);
        let v2 = seq.add_track(TrackKind::Video);
        let mut title = ClipBlock::new(zavyx_ve_core::MediaId::new(), Rational::ZERO, Rational::ONE, Rational::ZERO);
        let mut fx = EffectInstance::new(crate::TITLE_EFFECT);
        fx.set_text("text", "Hello");
        title.effects.push(fx);
        seq.track_mut(v2).unwrap().insert_clip(title);
        let snap = crate::mixer::snapshot(&p);

        for format in [Format::Mp4, Format::Wav] {
            let out = dir.join(format!("out.{}", format.extension()));
            let s = Settings { format, path: out.clone(), width: 320, height: 180, fps: 25.0, quality: Quality::Low, range: None };
            let (progress, cancel) = (AtomicU32::new(0), AtomicBool::new(false));
            let state = Mutex::new(State::Running(String::new()));
            run(&snap, &s, &progress, &cancel, &state, &|| {}).unwrap();
            assert_eq!(progress.load(Ordering::Relaxed), 1000);
            let got = zavyx_ve_media::cli::CliProber.probe(&out).unwrap();
            assert!((got.duration.to_f64() - 2.0).abs() < 0.2, "{format:?} duration {:?}", got.duration);
            assert!(got.has_audio);
            assert_eq!(got.has_video, format == Format::Mp4);
            if format == Format::Mp4 {
                assert_eq!((got.width, got.height), (Some(320), Some(180)));
            }
        }
        // A range of a PNG sequence: 0.4 s at 25 fps = 10 frames.
        let pngs = dir.join("frames");
        let s = Settings { format: Format::PngSequence, path: pngs.clone(), width: 160, height: 90, fps: 25.0,
                           quality: Quality::High, range: Some((0.5, 0.9)) };
        let (progress, cancel) = (AtomicU32::new(0), AtomicBool::new(false));
        run(&snap, &s, &progress, &cancel, &Mutex::new(State::Running(String::new())), &|| {}).unwrap();
        assert_eq!(std::fs::read_dir(&pngs).unwrap().count(), 10);

        let still = render_still(&snap, 0.5);
        assert_eq!((still.width, still.height), (320, 180));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mp4_args_pick_an_h264_encoder_and_aac() {
        let a = codec_args(Format::Mp4, Quality::Medium, 30.0).join(" ");
        assert!(a.contains("-c:a aac") && a.contains("yuv420p") && a.contains("+faststart"));
    }
}
