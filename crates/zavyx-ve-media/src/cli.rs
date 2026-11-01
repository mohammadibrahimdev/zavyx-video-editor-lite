//! Media I/O through a bundled `ffmpeg.exe` (run as a separate program).
//!
//! This is the backend the app uses by default. It needs no FFmpeg DLLs at
//! build time, works for both the 32-bit and 64-bit app (a 32-bit app can
//! run a 64-bit ffmpeg.exe on 64-bit Windows), and keeps FFmpeg out of our
//! own process. `ffmpeg.exe` is looked up next to the app first, then in a
//! `bin/` folder next to it (or above it, for dev builds), then on PATH.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::OnceLock;

use zavyx_ve_core::{FrameRate, MediaInfo, Rational};

use crate::{Decoder, Frame, MediaError, Prober};

/// Default length of a still image when it is placed on the timeline.
pub const STILL_IMAGE_SECONDS: i64 = 5;

const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "bmp", "webp", "tif", "tiff"];

/// Path of the ffmpeg.exe this app uses, if one was found.
pub fn ffmpeg_path() -> Option<PathBuf> {
    static PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
    PATH.get_or_init(find_ffmpeg).clone()
}

fn find_ffmpeg() -> Option<PathBuf> {
    let name = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent().map(Path::to_path_buf);
        // Next to the exe, then <dir>/bin, walking up so `target/release`
        // finds the repo's `bin/` during development.
        for depth in 0..5 {
            let Some(d) = dir.clone() else { break };
            if depth == 0 && d.join(name).is_file() {
                return Some(d.join(name));
            }
            if d.join("bin").join(name).is_file() {
                return Some(d.join("bin").join(name));
            }
            dir = d.parent().map(Path::to_path_buf);
        }
    }
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var)
        .map(|d| d.join(name))
        .find(|p| p.is_file())
}

/// True when the bundled ffmpeg has the named encoder (e.g. `libx264`).
pub fn has_encoder(name: &str) -> bool {
    static LIST: OnceLock<String> = OnceLock::new();
    let list = LIST.get_or_init(|| {
        ffmpeg().ok()
            .and_then(|mut c| c.arg("-encoders").stderr(Stdio::null()).output().ok())
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default()
    });
    list.lines().any(|l| l.split_whitespace().nth(1) == Some(name))
}

/// A `Command` for ffmpeg with no console window (for exports and tools).
pub fn command() -> Result<Command, MediaError> {
    ffmpeg()
}

/// A `Command` for ffmpeg with no console window and quiet logging.
fn ffmpeg() -> Result<Command, MediaError> {
    let path = ffmpeg_path().ok_or_else(|| {
        MediaError::Probe("ffmpeg.exe was not found next to the app".into())
    })?;
    let mut cmd = Command::new(path);
    cmd.args(["-hide_banner", "-nostdin"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    Ok(cmd)
}

/// True when the file is a still image (by extension).
pub fn is_still_image(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| IMAGE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Largest even-sized (w, h) with the source aspect that fits in the box.
pub fn fit_size(src_w: u32, src_h: u32, box_w: u32, box_h: u32) -> (u32, u32) {
    if src_w == 0 || src_h == 0 {
        return (box_w.max(2) & !1, box_h.max(2) & !1);
    }
    let scale = (box_w as f64 / src_w as f64).min(box_h as f64 / src_h as f64);
    let w = ((src_w as f64 * scale).round() as u32).max(2) & !1;
    let h = ((src_h as f64 * scale).round() as u32).max(2) & !1;
    (w, h)
}

// --- Probing ----------------------------------------------------------------

/// Probes files by reading the stream summary `ffmpeg -i` prints.
pub struct CliProber;

impl Prober for CliProber {
    fn probe(&self, path: &Path) -> Result<MediaInfo, MediaError> {
        if !path.is_file() {
            return Err(MediaError::NotFound(path.to_path_buf()));
        }
        let out = ffmpeg()?
            .arg("-i").arg(path)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()?;
        let text = String::from_utf8_lossy(&out.stderr);
        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        parse_probe(path, &text, size)
    }
}

/// Parse the stream summary that `ffmpeg -i <file>` writes to stderr.
pub fn parse_probe(path: &Path, text: &str, file_size: u64) -> Result<MediaInfo, MediaError> {
    if !text.contains("Input #0") {
        let reason = text.lines().last().unwrap_or("unknown format").trim().to_string();
        return Err(MediaError::Probe(reason));
    }
    let mut info = MediaInfo {
        path: path.to_path_buf(),
        duration: Rational::ZERO,
        width: None, height: None, fps: None,
        video_codec: None, audio_codec: None,
        sample_rate: None, channels: None,
        has_audio: false, has_video: false,
        file_size,
    };
    let mut rotated = false;
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Duration:") {
            let d = rest.split(',').next().unwrap_or("").trim();
            if let Some(secs) = parse_hms(d) {
                info.duration = Rational::new((secs * 1000.0).round() as i64, 1000);
            }
        } else if line.starts_with("Stream #") {
            if let Some(v) = after(line, "Video: ") {
                if line.contains("(attached pic)") || info.has_video {
                    continue;
                }
                info.has_video = true;
                info.video_codec = Some(first_word(v));
                for tok in v.split([',', ' ']) {
                    if let Some((w, h)) = tok.split_once('x') {
                        if let (Ok(w), Ok(h)) = (w.parse::<u32>(), h.parse::<u32>()) {
                            if w > 0 && h > 0 && info.width.is_none() {
                                info.width = Some(w);
                                info.height = Some(h);
                            }
                        }
                    }
                }
                info.fps = parse_fps(v);
            } else if let Some(a) = after(line, "Audio: ") {
                if info.has_audio {
                    continue;
                }
                info.has_audio = true;
                info.audio_codec = Some(first_word(a));
                for part in a.split(',').map(str::trim) {
                    if let Some(hz) = part.strip_suffix(" Hz") {
                        info.sample_rate = hz.trim().parse().ok();
                    } else if let Some(ch) = channel_count(part) {
                        info.channels = Some(ch);
                    }
                }
            }
        } else if line.contains("rotation of -90") || line.contains("rotation of 90")
            || line.contains("rotation of 270") || line.contains("rotation of -270")
        {
            rotated = true;
        }
    }
    if rotated {
        std::mem::swap(&mut info.width, &mut info.height);
    }
    if is_still_image(path) && info.has_video {
        info.duration = Rational::new(STILL_IMAGE_SECONDS, 1);
        info.fps = None;
    }
    if !info.has_video && !info.has_audio {
        return Err(MediaError::Probe("no video or audio stream".into()));
    }
    Ok(info)
}

fn after<'a>(s: &'a str, pat: &str) -> Option<&'a str> {
    s.find(pat).map(|i| &s[i + pat.len()..])
}

fn first_word(s: &str) -> String {
    s.split([' ', ',']).next().unwrap_or("").to_string()
}

fn parse_hms(s: &str) -> Option<f64> {
    let mut parts = s.split(':');
    let h: f64 = parts.next()?.parse().ok()?;
    let m: f64 = parts.next()?.parse().ok()?;
    let sec: f64 = parts.next()?.parse().ok()?;
    Some(h * 3600.0 + m * 60.0 + sec)
}

fn parse_fps(v: &str) -> Option<FrameRate> {
    let part = v.split(',').map(str::trim).find(|p| p.ends_with(" fps"))?;
    let n: f64 = part.trim_end_matches(" fps").trim().parse().ok()?;
    let rate = match n {
        x if (x - 29.97).abs() < 0.01 => FrameRate::FPS_29_97,
        x if (x - 59.94).abs() < 0.01 => FrameRate::FPS_59_94,
        x if (x - 23.976).abs() < 0.01 => FrameRate(Rational::new(24000, 1001)),
        x if x.fract().abs() < 1e-6 && x > 0.0 => FrameRate(Rational::new(x as i64, 1)),
        x if x > 0.0 => FrameRate(Rational::new((x * 1000.0).round() as i64, 1000)),
        _ => return None,
    };
    Some(rate)
}

fn channel_count(part: &str) -> Option<u16> {
    match part.split('(').next()?.trim() {
        "mono" => Some(1),
        "stereo" => Some(2),
        "2.1" | "3.0" => Some(3),
        "quad" | "4.0" => Some(4),
        "5.0" => Some(5),
        "5.1" => Some(6),
        "6.1" => Some(7),
        "7.1" => Some(8),
        other => other.strip_suffix(" channels").and_then(|n| n.parse().ok()),
    }
}

// --- Video ------------------------------------------------------------------

/// Decode one RGBA frame of `path` at `time` seconds, scaled to `w`×`h`.
pub fn decode_frame(path: &Path, time: f64, w: u32, h: u32) -> Result<Frame, MediaError> {
    let mut cmd = ffmpeg()?;
    if time > 0.0 && !is_still_image(path) {
        cmd.arg("-ss").arg(format!("{time:.3}"));
    }
    let out = cmd
        .arg("-i").arg(path)
        .args(["-frames:v", "1", "-an", "-sn", "-vf"])
        .arg(format!("scale={w}:{h}:flags=bilinear"))
        .args(["-f", "rawvideo", "-pix_fmt", "rgba", "-"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()?;
    let need = (w * h * 4) as usize;
    if out.stdout.len() < need {
        return Err(MediaError::Decode(format!("no frame at {time:.2}s")));
    }
    let mut pixels = out.stdout;
    pixels.truncate(need);
    Ok(Frame { width: w, height: h, pixels, pts: Rational::from_seconds(time) })
}

/// Streams frames from ffmpeg and seeks by restarting it. Reading forward
/// (normal playback) reuses the running process.
pub struct CliDecoder {
    path: PathBuf,
    width: u32,
    height: u32,
    fps: f64,
    still: bool,
    stream: Option<(Child, ChildStdout)>,
    /// Media time of the next frame the running process will deliver.
    next_pts: f64,
    last: Option<Frame>,
    ended: bool,
}

impl CliDecoder {
    /// Decoder that outputs frames of exactly `width`×`height`.
    pub fn new(path: &Path, width: u32, height: u32, fps: Option<FrameRate>) -> Self {
        CliDecoder {
            path: path.to_path_buf(),
            width,
            height,
            fps: fps.map(|f| f.0.to_f64()).filter(|f| *f > 0.0).unwrap_or(30.0),
            still: is_still_image(path),
            stream: None,
            next_pts: 0.0,
            last: None,
            ended: false,
        }
    }

    pub fn size(&self) -> (u32, u32) { (self.width, self.height) }

    fn frame_bytes(&self) -> usize { (self.width * self.height * 4) as usize }

    fn start(&mut self, time: f64) -> Result<(), MediaError> {
        self.stop();
        let mut cmd = ffmpeg()?;
        if time > 0.0 {
            cmd.arg("-ss").arg(format!("{time:.3}"));
        }
        let mut child = cmd
            .arg("-i").arg(&self.path)
            .args(["-an", "-sn", "-vf"])
            .arg(format!("fps={},scale={}:{}:flags=bilinear", self.fps, self.width, self.height))
            .args(["-f", "rawvideo", "-pix_fmt", "rgba", "-"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdout = child.stdout.take()
            .ok_or_else(|| MediaError::Decode("ffmpeg gave no output pipe".into()))?;
        self.stream = Some((child, stdout));
        self.next_pts = time;
        self.ended = false;
        Ok(())
    }

    fn stop(&mut self) {
        if let Some((mut child, _)) = self.stream.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    fn read_one(&mut self) -> Result<Option<Frame>, MediaError> {
        let need = self.frame_bytes();
        let Some((_, stdout)) = self.stream.as_mut() else { return Ok(None) };
        let mut pixels = vec![0u8; need];
        let mut got = 0;
        while got < need {
            let n = stdout.read(&mut pixels[got..])?;
            if n == 0 {
                self.ended = true;
                return Ok(None);
            }
            got += n;
        }
        let pts = self.next_pts;
        self.next_pts += 1.0 / self.fps;
        Ok(Some(Frame {
            width: self.width, height: self.height, pixels,
            pts: Rational::from_seconds(pts),
        }))
    }

    fn holds(&self, t: f64) -> bool {
        let step = 1.0 / self.fps;
        self.last.as_ref()
            .map(|f| { let p = f.pts.to_f64(); t >= p - 1e-6 && t < p + step - 1e-6 })
            .unwrap_or(false)
    }
}

impl Drop for CliDecoder {
    fn drop(&mut self) { self.stop(); }
}

impl Decoder for CliDecoder {
    fn seek(&mut self, time: Rational) -> Result<(), MediaError> {
        self.start(time.to_f64().max(0.0))
    }

    fn next_frame(&mut self) -> Result<Frame, MediaError> {
        if self.stream.is_none() {
            self.start(0.0)?;
        }
        match self.read_one()? {
            Some(f) => { self.last = Some(f.clone()); Ok(f) }
            None => self.last.clone().ok_or_else(|| MediaError::Decode("end of stream".into())),
        }
    }

    fn frame_at(&mut self, time: Rational) -> Result<Frame, MediaError> {
        let t = time.to_f64().max(0.0);
        if self.still {
            if self.last.is_none() {
                self.last = Some(decode_frame(&self.path, 0.0, self.width, self.height)?);
            }
            return Ok(self.last.clone().unwrap());
        }
        if self.holds(t) {
            return Ok(self.last.clone().unwrap());
        }
        let step = 1.0 / self.fps;
        // Restart unless the wanted frame is a short read ahead of the stream.
        let ahead = t - self.next_pts;
        if self.stream.is_none() || ahead < -step * 0.5 || ahead > 1.5 {
            if self.ended && self.last.is_some() && ahead >= -step * 0.5 {
                return Ok(self.last.clone().unwrap());
            }
            self.start(t)?;
        }
        loop {
            match self.read_one()? {
                Some(f) => {
                    let p = f.pts.to_f64();
                    self.last = Some(f);
                    if t < p + step - 1e-6 {
                        break;
                    }
                }
                None => break,
            }
        }
        self.last.clone().ok_or_else(|| MediaError::Decode(format!("no frame at {t:.2}s")))
    }
}

// --- Audio ------------------------------------------------------------------

/// Streams interleaved f32 samples of a file's audio from a start time.
pub struct AudioReader {
    stream: Option<(Child, ChildStdout)>,
    buf: Vec<u8>,
}

impl AudioReader {
    pub fn open(path: &Path, start_secs: f64, sample_rate: u32, channels: u16) -> Result<Self, MediaError> {
        Self::open_with_speed(path, start_secs, sample_rate, channels, 1.0)
    }

    /// Like `open`, played `speed` times faster (pitch kept, via `atempo`).
    pub fn open_with_speed(
        path: &Path, start_secs: f64, sample_rate: u32, channels: u16, speed: f64,
    ) -> Result<Self, MediaError> {
        let mut cmd = ffmpeg()?;
        if start_secs > 0.0 {
            cmd.arg("-ss").arg(format!("{start_secs:.3}"));
        }
        cmd.arg("-i").arg(path);
        if (speed - 1.0).abs() > 1e-3 {
            cmd.arg("-af").arg(atempo_chain(speed));
        }
        let mut child = cmd
            .args(["-vn", "-sn", "-f", "f32le", "-ac"])
            .arg(channels.to_string())
            .arg("-ar").arg(sample_rate.to_string())
            .arg("-")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdout = child.stdout.take()
            .ok_or_else(|| MediaError::Decode("ffmpeg gave no output pipe".into()))?;
        Ok(AudioReader { stream: Some((child, stdout)), buf: Vec::new() })
    }

    /// Fill `out` with the next samples; past the end it is filled with silence.
    /// Returns how many samples came from the file.
    pub fn read(&mut self, out: &mut [f32]) -> usize {
        let need = out.len() * 4;
        self.buf.resize(need, 0);
        let mut got = 0;
        if let Some((_, stdout)) = self.stream.as_mut() {
            while got < need {
                match stdout.read(&mut self.buf[got..]) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => got += n,
                }
            }
        }
        let samples = got / 4;
        for (i, o) in out.iter_mut().enumerate() {
            *o = if i < samples {
                f32::from_le_bytes([self.buf[i*4], self.buf[i*4+1], self.buf[i*4+2], self.buf[i*4+3]])
            } else {
                0.0
            };
        }
        samples
    }
}

impl Drop for AudioReader {
    fn drop(&mut self) {
        if let Some((mut child, _)) = self.stream.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// `atempo` only accepts 0.5–100 per stage, so chain stages for slow speeds.
pub fn atempo_chain(speed: f64) -> String {
    let mut s = speed.clamp(0.01, 100.0);
    let mut stages = Vec::new();
    while s < 0.5 {
        stages.push("atempo=0.5".to_string());
        s /= 0.5;
    }
    stages.push(format!("atempo={s:.4}"));
    stages.join(",")
}

/// Peak level (0..1) of a file's audio, `per_second` buckets per second.
pub fn waveform_peaks(path: &Path, per_second: u32) -> Result<Vec<f32>, MediaError> {
    let rate = 8000u32;
    let out = ffmpeg()?
        .arg("-i").arg(path)
        .args(["-vn", "-sn", "-f", "f32le", "-ac", "1", "-ar"])
        .arg(rate.to_string())
        .arg("-")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()?;
    let samples: Vec<f32> = out.stdout.chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();
    let bucket = (rate / per_second.max(1)).max(1) as usize;
    Ok(samples.chunks(bucket)
        .map(|c| c.iter().fold(0.0f32, |m, s| m.max(s.abs())).min(1.0))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MP4: &str = "Input #0, mov,mp4,m4a,3gp,3g2,mj2, from 'clip.mp4':
  Duration: 00:00:10.00, start: 0.000000, bitrate: 3196 kb/s
  Stream #0:0[0x1](und): Video: h264 (High) (avc1 / 0x31637661), yuv420p(progressive), 1280x720 [SAR 1:1 DAR 16:9], 3117 kb/s, 29.97 fps, 30 tbr, 15360 tbn (default)
  Stream #0:1[0x2](und): Audio: aac (LC) (mp4a / 0x6134706D), 44100 Hz, stereo, fltp, 69 kb/s (default)
At least one output file must be specified";

    #[test]
    fn parses_video_with_audio() {
        let i = parse_probe(Path::new("clip.mp4"), MP4, 10).unwrap();
        assert_eq!(i.duration, Rational::new(10, 1));
        assert_eq!((i.width, i.height), (Some(1280), Some(720)));
        assert_eq!(i.fps, Some(FrameRate::FPS_29_97));
        assert_eq!(i.video_codec.as_deref(), Some("h264"));
        assert!(i.has_audio && i.has_video);
        assert_eq!(i.sample_rate, Some(44100));
        assert_eq!(i.channels, Some(2));
    }

    #[test]
    fn parses_image_as_five_seconds() {
        let text = "Input #0, png_pipe, from 'a.png':
  Duration: N/A, bitrate: N/A
  Stream #0:0: Video: png, rgb24(pc, gbr/unknown/unknown), 800x600 [SAR 1:1 DAR 4:3], 25 fps, 25 tbr, 25 tbn";
        let i = parse_probe(Path::new("a.png"), text, 1).unwrap();
        assert_eq!(i.duration, Rational::new(STILL_IMAGE_SECONDS, 1));
        assert_eq!((i.width, i.height), (Some(800), Some(600)));
        assert!(!i.has_audio);
    }

    #[test]
    fn parses_audio_and_ignores_cover_art() {
        let text = "Input #0, mp3, from 'm.mp3':
  Duration: 00:03:05.50, start: 0.025057, bitrate: 64 kb/s
  Stream #0:0: Audio: mp3 (mp3float), 48000 Hz, mono, fltp, 64 kb/s
  Stream #0:1: Video: mjpeg (Baseline), yuvj420p(pc), 500x500, 90k tbr, 90k tbn (attached pic)";
        let i = parse_probe(Path::new("m.mp3"), text, 1).unwrap();
        assert!(!i.has_video);
        assert_eq!(i.channels, Some(1));
        assert_eq!(i.duration, Rational::new(185_500, 1000));
    }

    #[test]
    fn rotated_phone_video_swaps_size() {
        let text = "Input #0, mov, from 'p.mp4':
  Duration: 00:00:02.00, start: 0.000000, bitrate: 1 kb/s
  Stream #0:0: Video: h264, yuv420p, 1920x1080, 30 fps, 30 tbr
      Side data:
        displaymatrix: rotation of -90.00 degrees";
        let i = parse_probe(Path::new("p.mp4"), text, 1).unwrap();
        assert_eq!((i.width, i.height), (Some(1080), Some(1920)));
    }

    #[test]
    fn unreadable_file_is_an_error() {
        let text = "x.mp4: Invalid data found when processing input";
        assert!(parse_probe(Path::new("x.mp4"), text, 1).is_err());
    }

    /// End-to-end against the real ffmpeg.exe; skipped when none is found.
    #[test]
    fn real_ffmpeg_probe_decode_and_audio() {
        if ffmpeg_path().is_none() {
            eprintln!("Skipping: no ffmpeg.exe");
            return;
        }
        let dir = std::env::temp_dir().join(format!("zavyx-ve-cli-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let clip = dir.join("t.mp4");
        let ok = ffmpeg().unwrap()
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc2=size=320x240:rate=25:duration=2",
                   "-f", "lavfi", "-i", "sine=frequency=440:duration=2", "-shortest", "-pix_fmt", "yuv420p"])
            .arg(&clip)
            .status().unwrap().success();
        assert!(ok);

        let info = CliProber.probe(&clip).unwrap();
        assert_eq!((info.width, info.height), (Some(320), Some(240)));
        assert!(info.has_audio);
        assert!((info.duration.to_f64() - 2.0).abs() < 0.1);

        let mut d = CliDecoder::new(&clip, 160, 120, info.fps);
        let a = d.frame_at(Rational::new(1, 2)).unwrap();
        let b = d.frame_at(Rational::new(4, 5)).unwrap();
        assert_eq!(a.pixels.len(), 160 * 120 * 4);
        assert!(a.pixels != b.pixels, "testsrc2 changes every frame");
        let back = d.frame_at(Rational::new(1, 2)).unwrap();
        assert_eq!(back.pixels.len(), a.pixels.len());

        let mut r = AudioReader::open(&clip, 0.5, 48_000, 2).unwrap();
        let mut buf = vec![0.0f32; 4800];
        assert_eq!(r.read(&mut buf), 4800);
        assert!(buf.iter().any(|s| s.abs() > 0.01));

        let peaks = waveform_peaks(&clip, 50).unwrap();
        assert!(peaks.len() >= 95 && peaks.iter().any(|p| *p > 0.05));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn atempo_chains_for_slow_speeds() {
        assert_eq!(atempo_chain(2.0), "atempo=2.0000");
        assert_eq!(atempo_chain(0.25), "atempo=0.5,atempo=0.5000");
    }

    #[test]
    fn fit_keeps_aspect_and_even_sizes() {
        assert_eq!(fit_size(1920, 1080, 640, 640), (640, 360));
        assert_eq!(fit_size(1080, 1920, 640, 360), (202, 360));
        assert_eq!(fit_size(801, 601, 1000, 1000), (1000, 750));
    }
}
