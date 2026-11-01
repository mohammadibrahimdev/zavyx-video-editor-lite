//! zavyx-ve-media — FFmpeg-backed decode/encode/probe/cache.
//!
//! Phase 1: trait definitions + stub implementations that compile without
//! linking FFmpeg. Phase 2 enables the `ffmpeg` feature and provides real
//! FFmpeg-backed implementations.

use std::path::{Path, PathBuf};
use zavyx_ve_core::{MediaInfo, MediaRef, MediaId, Rational};
use thiserror::Error;

pub mod cli;

#[derive(Debug, Error)]
pub enum MediaError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("probe failed: {0}")]
    Probe(String),
    #[error("decode failed: {0}")]
    Decode(String),
    #[error("encode failed: {0}")]
    Encode(String),
    #[error("ffmpeg not linked; build with --features ffmpeg")]
    NoFfmpeg,
    #[error("not found: {0}")]
    NotFound(PathBuf),
}

/// Probes a media file and returns its info.
pub trait Prober: Send + Sync {
    fn probe(&self, path: &Path) -> Result<MediaInfo, MediaError>;
}

/// Decodes media frames at a specific time. Implementations may keep an
/// internal cache to speed up back-to-back reads.
pub trait Decoder: Send {
    /// Seek to the given time on the source media. The next call to
    /// `next_frame` returns the frame at or just before this time.
    fn seek(&mut self, time: Rational) -> Result<(), MediaError>;
    /// Get the next decoded frame.
    fn next_frame(&mut self) -> Result<Frame, MediaError>;
    /// Get a frame at the specific time, using cache if available.
    fn frame_at(&mut self, time: Rational) -> Result<Frame, MediaError>;
}

/// A decoded frame. Phase 1 stub.
#[derive(Debug, Clone)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    /// RGBA8 pixel data, row-major, top-to-bottom.
    pub pixels: Vec<u8>,
    pub pts: Rational,
}

/// Encodes video frames to a file.
pub trait Encoder: Send {
    fn write_frame(&mut self, frame: &Frame) -> Result<(), MediaError>;
    fn finish(&mut self) -> Result<(), MediaError>;
}

/// Generates a small thumbnail for a media file at the given time.
pub trait Thumbnailer: Send + Sync {
    fn thumbnail(&self, media: &MediaRef, time: Rational) -> Result<Vec<u8>, MediaError>;
}

/// Generates audio waveform peaks (min, max) per channel.
pub trait Waveformer: Send + Sync {
    fn waveform(&self, media: &MediaRef) -> Result<Vec<(f32, f32)>, MediaError>;
}

// ============================================================================
// Stub implementations — work without FFmpeg linked. Used for tests, docs,
// and as a fallback when the `ffmpeg` feature is off.
// ============================================================================

/// A no-op prober that returns dummy media info. Useful for tests.
pub struct StubProber;

impl Prober for StubProber {
    fn probe(&self, path: &Path) -> Result<MediaInfo, MediaError> {
        Ok(MediaInfo {
            path: path.to_path_buf(),
            duration: Rational::new(0, 1),
            width: Some(1920),
            height: Some(1080),
            fps: Some(zavyx_ve_core::FrameRate::FPS_30),
            video_codec: Some("stub".into()),
            audio_codec: None,
            sample_rate: Some(48000),
            channels: Some(2),
            has_audio: false,
            has_video: true,
            file_size: 0,
        })
    }
}

/// A stub decoder that returns solid-color frames. Used for tests and
/// headless rendering when no FFmpeg is linked.
pub struct StubDecoder {
    width: u32,
    height: u32,
    color: [u8; 4],
    current_time: Rational,
}

impl StubDecoder {
    pub fn new(width: u32, height: u32, color: [u8; 4]) -> Self {
        StubDecoder { width, height, color, current_time: Rational::ZERO }
    }
}

impl Decoder for StubDecoder {
    fn seek(&mut self, time: Rational) -> Result<(), MediaError> {
        self.current_time = time;
        Ok(())
    }
    fn next_frame(&mut self) -> Result<Frame, MediaError> {
        let pixels = vec![self.color[0]; (self.width * self.height * 4) as usize];
        let mut alternating = pixels.clone();
        // Make a checkerboard so test images aren't solid
        for y in 0..self.height {
            for x in 0..self.width {
                let i = ((y * self.width + x) * 4) as usize;
                let even = ((x / 32 + y / 32) % 2) == 0;
                let c = if even { self.color } else {
                    [self.color[0]^0xff, self.color[1]^0xff, self.color[2]^0xff, 0xff]
                };
                alternating[i]   = c[0];
                alternating[i+1] = c[1];
                alternating[i+2] = c[2];
                alternating[i+3] = c[3];
            }
        }
        let t = self.current_time;
        self.current_time = self.current_time + Rational::new(1, 30);
        Ok(Frame { width: self.width, height: self.height, pixels: alternating, pts: t })
    }
    fn frame_at(&mut self, time: Rational) -> Result<Frame, MediaError> {
        self.seek(time)?;
        self.next_frame()
    }
}

/// A stub encoder that writes PNG files numbered sequentially to a directory.
/// Useful for headless render tests in CI without an FFmpeg link.
pub struct StubEncoder {
    out_dir: PathBuf,
    frame_idx: u64,
}

impl StubEncoder {
    pub fn new(out_dir: PathBuf) -> Self {
        std::fs::create_dir_all(&out_dir).ok();
        StubEncoder { out_dir, frame_idx: 0 }
    }
}

impl Encoder for StubEncoder {
    fn write_frame(&mut self, frame: &Frame) -> Result<(), MediaError> {
        let path = self.out_dir.join(format!("frame_{:06}.png", self.frame_idx));
        // Encode as PNG using a minimal hand-rolled encoder.
        write_png(&path, frame)?;
        self.frame_idx += 1;
        Ok(())
    }
    fn finish(&mut self) -> Result<(), MediaError> { Ok(()) }
}

/// Minimal PNG encoder (uncompressed IDAT via stored deflate) — enough for
/// tests. We avoid pulling in `image` crate in zavyx-ve-media so the GUI
/// crate can use a different image backend if needed.
fn write_png(path: &Path, frame: &Frame) -> Result<(), MediaError> {
    use std::io::Write;
    let mut f = std::fs::File::create(path)?;
    // PNG signature
    f.write_all(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A])?;
    // IHDR
    let w = frame.width;
    let h = frame.height;
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.push(8);  // bit depth
    ihdr.push(6);  // color type RGBA
    ihdr.push(0); ihdr.push(0); ihdr.push(0);
    write_chunk(&mut f, b"IHDR", &ihdr)?;
    // IDAT — stored deflate (no compression) so we don't need a zlib dep.
    // Each row gets a 0 filter byte prefix.
    let mut raw = Vec::with_capacity(((w * 4 + 1) * h) as usize);
    for y in 0..h {
        raw.push(0u8); // filter none
        let row_start = (y * w * 4) as usize;
        let row_end = row_start + (w as usize) * 4;
        raw.extend_from_slice(&frame.pixels[row_start..row_end]);
    }
    let mut idat = Vec::new();
    // zlib header: 0x78 0x01 (low compression)
    idat.extend_from_slice(&[0x78, 0x01]);
    // stored deflate block
    let n = raw.len();
    idat.push(0x01); // BFINAL=1, BTYPE=00 (stored)
    idat.extend_from_slice(&(n as u16).to_le_bytes());
    idat.extend_from_slice(&(!(n as u16)).to_le_bytes());
    idat.extend_from_slice(&raw);
    // adler32 (simplified: just use 1)
    let adler = adler32(&raw);
    idat.extend_from_slice(&adler.to_be_bytes());
    write_chunk(&mut f, b"IDAT", &idat)?;
    write_chunk(&mut f, b"IEND", &[])?;
    Ok(())
}

fn write_chunk(f: &mut std::fs::File, ty: &[u8; 4], data: &[u8]) -> Result<(), MediaError> {
    use std::io::Write;
    f.write_all(&(data.len() as u32).to_be_bytes())?;
    f.write_all(ty)?;
    f.write_all(data)?;
    let mut crc_input = Vec::with_capacity(4 + data.len());
    crc_input.extend_from_slice(ty);
    crc_input.extend_from_slice(data);
    let crc = crc32(&crc_input);
    f.write_all(&crc.to_be_bytes())?;
    Ok(())
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFFFFFF;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 { crc = (crc >> 1) ^ 0xEDB88320; } else { crc >>= 1; }
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &byte in data {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_decoder_returns_checkerboard() {
        let mut d = StubDecoder::new(64, 32, [0x00, 0x00, 0x00, 0xFF]);
        let f = d.next_frame().unwrap();
        assert_eq!(f.width, 64);
        assert_eq!(f.height, 32);
        assert_eq!(f.pixels.len(), 64 * 32 * 4);
        // Top-left pixel should be the color
        assert_eq!(&f.pixels[0..4], &[0x00, 0x00, 0x00, 0xFF]);
    }

    #[test]
    fn stub_encoder_writes_png() {
        let dir = std::env::temp_dir().join(format!("zavyx-test-{}", std::process::id()));
        let mut e = StubEncoder::new(dir.clone());
        let f = Frame { width: 4, height: 4, pixels: vec![0xFF; 4*4*4], pts: Rational::ZERO };
        e.write_frame(&f).unwrap();
        e.finish().unwrap();
        let p = dir.join("frame_000000.png");
        assert!(p.exists(), "PNG not written");
        // First 8 bytes are PNG signature
        let bytes = std::fs::read(&p).unwrap();
        assert_eq!(&bytes[0..8], &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

// =========================================================================
// Phase 3 — FFmpeg-backed implementations (behind the `ffmpeg` feature).
// =========================================================================
//
// These compile when the user enables the `ffmpeg` feature and provides
// `FFMPEG_DIR` (Windows) or system pkg-config (Linux/macOS). The trait
// interface stays the same as the stubs above, so the compositor and CLI
// can use either one transparently.

#[cfg(feature = "ffmpeg")]
pub mod ffmpeg_impl {
    use crate::{MediaError, MediaInfo, Frame, Prober, Decoder, Encoder};
    use zavyx_ve_core::{Rational, FrameRate};
    use std::path::Path;
    use ffmpeg_next as ff;

    /// Real FFmpeg-backed prober. Opens the file with avformat and extracts
    /// stream info.
    pub struct FfmpegProber;

    impl FfmpegProber {
        pub fn new() -> Self { FfmpegProber }
    }

    impl Prober for FfmpegProber {
        fn probe(&self, path: &Path) -> Result<MediaInfo, MediaError> {
            let path_str = path.to_str().ok_or_else(|| MediaError::Probe("path is not UTF-8".into()))?;
            let input = ff::format::input(&path_str).map_err(|e| MediaError::Probe(e.to_string()))?;
            let duration = Rational::new(
                (input.duration() as f64 / 1_000_000.0 * 1000.0) as i64,
                1000,
            );
            let mut width = None;
            let mut height = None;
            let mut fps = None;
            let mut video_codec = None;
            let mut audio_codec = None;
            let mut sample_rate = None;
            let mut channels = None;
            for stream in input.streams() {
                let p = stream.parameters();
                match p.medium() {
                    ff::media::Type::Video => {
                        width = Some(p.width());
                        height = Some(p.height());
                        if let Some(codec) = ff::decoder::find(p.id()) {
                            video_codec = Some(codec.name().to_string());
                        }
                        let fr = stream.avg_frame_rate();
                        if fr.numerator() != 0 && fr.denominator() != 0 {
                            fps = Some(FrameRate(Rational::new(fr.numerator() as i64, fr.denominator() as i64)));
                        }
                    }
                    ff::media::Type::Audio => {
                        if let Some(codec) = ff::decoder::find(p.id()) {
                            audio_codec = Some(codec.name().to_string());
                        }
                        sample_rate = Some(p.sample_rate());
                        channels = Some(p.channels());
                    }
                    _ => {}
                }
            }
            Ok(MediaInfo {
                path: path.to_path_buf(),
                duration,
                width, height, fps, video_codec, audio_codec,
                sample_rate, channels,
                has_audio: audio_codec.is_some(),
                has_video: video_codec.is_some(),
                file_size: std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
            })
        }
    }

    /// Real FFmpeg-backed decoder. Opens a media file, decodes frames on demand.
    pub struct FfmpegDecoder {
        ictx: ff::format::context::Input,
        video_stream_index: usize,
        decoder: ff::decoder::Video,
        scaler: ff::software::scaling::Context,
        current_pts: Rational,
        out_width: u32,
        out_height: u32,
    }

    impl FfmpegDecoder {
        pub fn open(path: &Path, out_width: u32, out_height: u32) -> Result<Self, MediaError> {
            let path_str = path.to_str().ok_or_else(|| MediaError::Decode("path is not UTF-8".into()))?;
            let ictx = ff::format::input(&path_str).map_err(|e| MediaError::Decode(e.to_string()))?;
            let stream = ictx.streams().best(ff::media::Type::Video)
                .ok_or_else(|| MediaError::Decode("no video stream".into()))?;
            let video_stream_index = stream.index();
            let mut decoder = stream.codec().decoder().video()
                .map_err(|e| MediaError::Decode(e.to_string()))?;
            decoder.set_width(out_width);
            decoder.set_height(out_height);
            let scaler = ff::software::scaling::context::Context::get(
                decoder.format(),
                decoder.width(),
                decoder.height(),
                ff::format::Pixel::RGBA,
                out_width,
                out_height,
                ff::software::scaling::flag::Flags::BILINEAR,
            ).map_err(|e| MediaError::Decode(e.to_string()))?;
            Ok(FfmpegDecoder {
                ictx, video_stream_index, decoder, scaler,
                current_pts: Rational::ZERO,
                out_width, out_height,
            })
        }
    }

    impl Decoder for FfmpegDecoder {
        fn seek(&mut self, time: Rational) -> Result<(), MediaError> {
            let target_ts = (time.to_f64() * 1_000_000.0) as i64;
            self.ictx.seek(target_ts, ..target_ts)
                .map_err(|e| MediaError::Decode(e.to_string()))?;
            self.current_pts = time;
            Ok(())
        }

        fn next_frame(&mut self) -> Result<Frame, MediaError> {
            let mut packet = ff::Packet::empty();
            let mut decoded = ff::frame::Video::empty();
            loop {
                match self.ictx.packets().next() {
                    Some(Ok((stream, p))) => {
                        if stream.index() != self.video_stream_index { continue; }
                        packet = p;
                    }
                    _ => return Err(MediaError::Decode("end of stream".into())),
                }
                self.decoder.send_packet(&packet).map_err(|e| MediaError::Decode(e.to_string()))?;
                while self.decoder.receive_frame(&mut decoded).is_ok() {
                    let mut rgb = ff::frame::Video::empty();
                    self.scaler.run(&decoded, &mut rgb).map_err(|e| MediaError::Decode(e.to_string()))?;
                    let pixels = rgb.data(0).to_vec();
                    let t = self.current_pts;
                    self.current_pts = self.current_pts + Rational::new(1, 30);
                    return Ok(Frame {
                        width: self.out_width,
                        height: self.out_height,
                        pixels,
                        pts: t,
                    });
                }
            }
        }

        fn frame_at(&mut self, time: Rational) -> Result<Frame, MediaError> {
            self.seek(time)?;
            self.next_frame()
        }
    }

    /// Real FFmpeg-backed encoder. Writes MP4 (H.264 + AAC) by default.
    pub struct FfmpegEncoder {
        octx: ff::format::context::Output,
        encoder: ff::encoder::Video,
        stream_index: usize,
        frame_idx: u64,
    }

    impl FfmpegEncoder {
        pub fn open(path: &Path, width: u32, height: u32, fps: u32) -> Result<Self, MediaError> {
            let path_str = path.to_str().ok_or_else(|| MediaError::Encode("path is not UTF-8".into()))?;
            let mut octx = ff::format::output(&path_str).map_err(|e| MediaError::Encode(e.to_string()))?;
            let codec = ff::encoder::find(ff::codec::Id::H264)
                .ok_or_else(|| MediaError::Encode("H.264 codec not found".into()))?;
            let mut encoder = ff::codec::context::Context::new_with_codec(codec)
                .encoder().video().map_err(|e| MediaError::Encode(e.to_string()))?;
            encoder.set_width(width);
            encoder.set_height(height);
            encoder.set_frame_rate(ff::Rational::from(fps as i32, 1));
            encoder.set_format(ff::format::Pixel::YUV420P);
            encoder.set_time_base(ff::Rational::from(1, fps as i32));
            encoder = encoder.open_as(ff::codec::Id::H264).map_err(|e| MediaError::Encode(e.to_string()))?;
            let stream = octx.add_stream(encoder).map_err(|e| MediaError::Encode(e.to_string()))?;
            let stream_index = stream.index();
            octx.write_header().map_err(|e| MediaError::Encode(e.to_string()))?;
            Ok(FfmpegEncoder { octx, encoder, stream_index, frame_idx: 0 })
        }
    }

    impl Encoder for FfmpegEncoder {
        fn write_frame(&mut self, frame: &Frame) -> Result<(), MediaError> {
            let mut yuv = ff::frame::Video::empty();
            ff::software::scaling::context::Context::get(
                ff::format::Pixel::RGBA, frame.width, frame.height,
                ff::format::Pixel::YUV420P, self.encoder.width(), self.encoder.height(),
                ff::software::scaling::flag::Flags::BILINEAR,
            ).map_err(|e| MediaError::Encode(e.to_string()))?
            .run(&ff::frame::Video::from_slice_rgba(&frame.pixels, frame.width, frame.height), &mut yuv)
            .map_err(|e| MediaError::Encode(e.to_string()))?;
            yuv.set_pts(Some(self.frame_idx as i64));
            self.frame_idx += 1;
            self.encoder.send_frame(&yuv).map_err(|e| MediaError::Encode(e.to_string()))?;
            let mut packet = ff::Packet::empty();
            while self.encoder.receive_packet(&mut packet).is_ok() {
                packet.set_stream(self.stream_index);
                packet.rescale_ts(ff::Rational::from(1, 30), self.octx.stream(self.stream_index).time_base());
                packet.write_interleaved(&mut self.octx).map_err(|e| MediaError::Encode(e.to_string()))?;
            }
            Ok(())
        }

        fn finish(&mut self) -> Result<(), MediaError> {
            self.encoder.send_eof().map_err(|e| MediaError::Encode(e.to_string()))?;
            let mut packet = ff::Packet::empty();
            while self.encoder.receive_packet(&mut packet).is_ok() {
                packet.set_stream(self.stream_index);
                packet.write_interleaved(&mut self.octx).map_err(|e| MediaError::Encode(e.to_string()))?;
            }
            self.octx.write_trailer().map_err(|e| MediaError::Encode(e.to_string()))?;
            Ok(())
        }
    }
}

// =========================================================================
// Phase 4 — Built-in recording (screen / webcam / voice-over).
// =========================================================================
//
// All three recording backends are trait-based; concrete implementations
// live in `zavyx-ve-gui` because they require platform-specific APIs
// (Windows Graphics Capture, Media Foundation, WASAPI). The traits are
// here so they can be tested with stubs.

// `Rational` is already imported at the top of this file.

/// A single recorded frame from any source.
#[derive(Clone, Debug)]
pub struct RecordedFrame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,  // RGBA8
    pub pts: Rational,
}

/// A recorded audio chunk (interleaved f32 stereo at `sample_rate`).
#[derive(Clone, Debug)]
pub struct RecordedAudio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
}

/// Source type for a recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordSource {
    /// Full screen capture.
    Screen,
    /// A specific window (by title).
    Window,
    /// Webcam (by device index or name).
    Webcam,
    /// Microphone / line-in.
    Microphone,
}

/// A recorder trait that produces frames or audio chunks.
pub trait Recorder: Send {
    /// Start recording. Returns immediately; frames arrive via `poll_frame()`.
    fn start(&mut self) -> Result<(), MediaError>;
    /// Stop recording. Blocks until all in-flight frames are flushed.
    fn stop(&mut self) -> Result<(), MediaError>;
    /// Poll for the next available frame (non-blocking). Returns None if no
    /// frame is ready.
    fn poll_frame(&mut self) -> Option<RecordedFrame>;
    /// Poll for the next audio chunk.
    fn poll_audio(&mut self) -> Option<RecordedAudio>;
    /// True if recording is in progress.
    fn is_recording(&self) -> bool;
}

/// Stub recorder that produces a single black frame per poll. Useful for
/// tests and as a fallback when no platform recorder is available.
pub struct StubRecorder {
    recording: bool,
    width: u32,
    height: u32,
    frame_count: u64,
}

impl StubRecorder {
    pub fn new(width: u32, height: u32) -> Self {
        StubRecorder { recording: false, width, height, frame_count: 0 }
    }
}

impl Recorder for StubRecorder {
    fn start(&mut self) -> Result<(), MediaError> {
        self.recording = true;
        self.frame_count = 0;
        Ok(())
    }
    fn stop(&mut self) -> Result<(), MediaError> {
        self.recording = false;
        Ok(())
    }
    fn poll_frame(&mut self) -> Option<RecordedFrame> {
        if !self.recording { return None; }
        self.frame_count += 1;
        Some(RecordedFrame {
            width: self.width,
            height: self.height,
            pixels: vec![0u8; (self.width * self.height * 4) as usize],
            pts: Rational::new(self.frame_count as i64, 30),
        })
    }
    fn poll_audio(&mut self) -> Option<RecordedAudio> {
        if !self.recording { return None; }
        Some(RecordedAudio {
            samples: vec![0.0f32; 4800],  // 100ms at 48kHz stereo
            sample_rate: 48_000,
            channels: 2,
        })
    }
    fn is_recording(&self) -> bool { self.recording }
}

#[cfg(test)]
mod recorder_tests {
    use super::*;

    #[test]
    fn stub_recorder_starts_and_stops() {
        let mut r = StubRecorder::new(320, 240);
        assert!(!r.is_recording());
        r.start().unwrap();
        assert!(r.is_recording());
        r.stop().unwrap();
        assert!(!r.is_recording());
    }

    #[test]
    fn stub_recorder_produces_frames_when_recording() {
        let mut r = StubRecorder::new(320, 240);
        r.start().unwrap();
        let f = r.poll_frame();
        assert!(f.is_some());
        let f = f.unwrap();
        assert_eq!(f.width, 320);
        assert_eq!(f.height, 240);
        assert_eq!(f.pixels.len(), 320 * 240 * 4);
    }

    #[test]
    fn stub_recorder_no_frames_when_stopped() {
        let mut r = StubRecorder::new(320, 240);
        let f = r.poll_frame();
        assert!(f.is_none());
    }
}
