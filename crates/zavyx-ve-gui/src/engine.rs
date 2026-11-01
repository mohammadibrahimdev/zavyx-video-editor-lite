//! Live preview engine: audio-driven playback and on-demand frames.
//!
//! - The cpal output callback pops mixed samples from a ring buffer; the
//!   samples it has played *are* the playback clock.
//! - An audio thread keeps the ring ~0.25 s ahead using the timeline `Mixer`.
//! - A video thread composes frames for the current clock time (while
//!   playing) or for the playhead (after a seek or an edit) at preview size.
//!
//! With no audio device the clock falls back to wall time.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::{unbounded, Receiver, Sender};
use parking_lot::Mutex;

use zavyx_ve_core::{ClipBlock, MediaId, Rational};
use zavyx_ve_media::cli::{fit_size, CliDecoder};
use zavyx_ve_media::{Decoder, Frame};
use zavyx_ve_render::Compositor;

use zavyx_ve_render::mixer::{Mixer, Snapshot};

enum Cmd {
    Project(Arc<Snapshot>),
    Seek(f64),
    Play,
    Pause,
    PreviewSize(u32, u32),
}

struct Shared {
    rate: AtomicU32,
    playing: AtomicBool,
    /// Clock in samples at `rate`.
    clock: AtomicI64,
    /// Bumped on every seek; stale audio is dropped.
    generation: AtomicU64,
    ring: Mutex<VecDeque<f32>>,
    frame: Mutex<Option<Frame>>,
    frame_serial: AtomicU64,
    /// Peak levels of the last played block, f32 bits (left, right).
    level_l: AtomicU32,
    level_r: AtomicU32,
    has_device: AtomicBool,
    reached_end: AtomicBool,
}

impl Shared {
    fn seconds(&self) -> f64 {
        self.clock.load(Ordering::Relaxed) as f64 / self.rate.load(Ordering::Relaxed) as f64
    }
}

pub struct Engine {
    shared: Arc<Shared>,
    video_tx: Sender<Cmd>,
    audio_tx: Sender<Cmd>,
    _stream: Option<cpal::Stream>,
    last_serial: u64,
}

impl Engine {
    pub fn new(repaint: egui::Context) -> Self {
        let shared = Arc::new(Shared {
            rate: AtomicU32::new(48_000),
            playing: AtomicBool::new(false),
            clock: AtomicI64::new(0),
            generation: AtomicU64::new(0),
            ring: Mutex::new(VecDeque::new()),
            frame: Mutex::new(None),
            frame_serial: AtomicU64::new(0),
            level_l: AtomicU32::new(0),
            level_r: AtomicU32::new(0),
            has_device: AtomicBool::new(false),
            reached_end: AtomicBool::new(false),
        });
        let stream = open_output(&shared);
        let (video_tx, video_rx) = unbounded();
        let (audio_tx, audio_rx) = unbounded();
        {
            let shared = shared.clone();
            std::thread::Builder::new().name("zavyx-video".into())
                .spawn(move || video_thread(shared, video_rx, repaint)).ok();
        }
        {
            let shared = shared.clone();
            std::thread::Builder::new().name("zavyx-audio".into())
                .spawn(move || audio_thread(shared, audio_rx)).ok();
        }
        Engine { shared, video_tx, audio_tx, _stream: stream, last_serial: 0 }
    }

    fn send(&self, make: impl Fn() -> Cmd) {
        let _ = self.video_tx.send(make());
        let _ = self.audio_tx.send(make());
    }

    pub fn set_project(&self, snap: Arc<Snapshot>) {
        self.send(|| Cmd::Project(snap.clone()));
    }

    pub fn seek(&self, seconds: f64) {
        let s = &self.shared;
        let rate = s.rate.load(Ordering::Relaxed) as f64;
        {
            let mut ring = s.ring.lock();
            ring.clear();
            s.generation.fetch_add(1, Ordering::SeqCst);
            s.clock.store((seconds.max(0.0) * rate).round() as i64, Ordering::SeqCst);
        }
        self.send(|| Cmd::Seek(seconds.max(0.0)));
    }

    pub fn play(&self) {
        self.shared.reached_end.store(false, Ordering::Relaxed);
        self.shared.playing.store(true, Ordering::SeqCst);
        self.send(|| Cmd::Play);
    }

    pub fn pause(&self) {
        self.shared.playing.store(false, Ordering::SeqCst);
        self.send(|| Cmd::Pause);
    }

    pub fn is_playing(&self) -> bool {
        self.shared.playing.load(Ordering::Relaxed)
    }

    /// True once after playback stopped by itself at the end.
    pub fn take_reached_end(&self) -> bool {
        self.shared.reached_end.swap(false, Ordering::Relaxed)
    }

    pub fn position(&self) -> f64 {
        self.shared.seconds()
    }

    pub fn set_preview_size(&self, w: u32, h: u32) {
        self.send(|| Cmd::PreviewSize(w, h));
    }

    /// The newest composed frame, if it changed since the last call.
    pub fn take_new_frame(&mut self) -> Option<Frame> {
        let serial = self.shared.frame_serial.load(Ordering::Acquire);
        if serial == self.last_serial {
            return None;
        }
        self.last_serial = serial;
        self.shared.frame.lock().clone()
    }

    /// Output peak levels (left, right), 0..1, while playing.
    pub fn levels(&self) -> (f32, f32) {
        if !self.is_playing() {
            return (0.0, 0.0);
        }
        (f32::from_bits(self.shared.level_l.load(Ordering::Relaxed)),
         f32::from_bits(self.shared.level_r.load(Ordering::Relaxed)))
    }

    pub fn has_audio_device(&self) -> bool {
        self.shared.has_device.load(Ordering::Relaxed)
    }
}

fn open_output(shared: &Arc<Shared>) -> Option<cpal::Stream> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    let device = cpal::default_host().default_output_device()?;
    let config = device.default_output_config().ok()?;
    let channels = config.channels() as usize;
    shared.rate.store(config.sample_rate().0, Ordering::Relaxed);
    let sh = shared.clone();
    let err = |e| log::warn!("audio output error: {e}");
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_output_stream(
            &config.config(),
            move |out: &mut [f32], _| fill_output(&sh, out, channels),
            err, None),
        cpal::SampleFormat::I16 => device.build_output_stream(
            &config.config(),
            move |out: &mut [i16], _| {
                let mut tmp = vec![0f32; out.len()];
                fill_output(&sh, &mut tmp, channels);
                for (o, s) in out.iter_mut().zip(tmp) { *o = (s * i16::MAX as f32) as i16; }
            },
            err, None),
        _ => return None,
    }.ok()?;
    stream.play().ok()?;
    shared.has_device.store(true, Ordering::Relaxed);
    Some(stream)
}

/// cpal callback: play buffered stereo samples and advance the clock.
fn fill_output(s: &Shared, out: &mut [f32], channels: usize) {
    out.fill(0.0);
    if !s.playing.load(Ordering::Relaxed) || channels == 0 {
        return;
    }
    let frames = out.len() / channels;
    let mut ring = s.ring.lock();
    let avail = (ring.len() / 2).min(frames);
    let (mut pl, mut pr) = (0f32, 0f32);
    for f in 0..avail {
        let l = ring.pop_front().unwrap_or(0.0);
        let r = ring.pop_front().unwrap_or(0.0);
        pl = pl.max(l.abs());
        pr = pr.max(r.abs());
        let o = &mut out[f * channels..(f + 1) * channels];
        if channels == 1 {
            o[0] = (l + r) * 0.5;
        } else {
            o[0] = l;
            o[1] = r;
        }
    }
    s.clock.fetch_add(avail as i64, Ordering::SeqCst);
    drop(ring);
    s.level_l.store(pl.to_bits(), Ordering::Relaxed);
    s.level_r.store(pr.to_bits(), Ordering::Relaxed);
}

fn audio_thread(s: Arc<Shared>, rx: Receiver<Cmd>) {
    let mut snap: Option<Arc<Snapshot>> = None;
    let mut mixer = Mixer::new(s.rate.load(Ordering::Relaxed));
    let mut write_pos: i64 = 0;
    let mut generation = u64::MAX;
    let block = 1024usize;
    let mut buf = vec![0f32; block * 2];
    loop {
        let playing = s.playing.load(Ordering::Relaxed) && s.has_device.load(Ordering::Relaxed);
        let cmd = if playing { rx.try_recv().ok() } else { rx.recv_timeout(Duration::from_millis(50)).ok() };
        match cmd {
            Some(Cmd::Project(p)) => { snap = Some(p); mixer.reset(); generation = u64::MAX; }
            Some(Cmd::Seek(_)) | Some(Cmd::Play) => { mixer.reset(); generation = u64::MAX; }
            Some(_) => {}
            None => {}
        }
        if !playing {
            continue;
        }
        let Some(snap) = snap.as_ref() else { std::thread::sleep(Duration::from_millis(5)); continue };
        let rate = s.rate.load(Ordering::Relaxed) as i64;
        let gen_now = s.generation.load(Ordering::SeqCst);
        if gen_now != generation {
            // Restart writing right where the clock is.
            let ring = s.ring.lock();
            write_pos = s.clock.load(Ordering::SeqCst) + (ring.len() / 2) as i64;
            generation = gen_now;
            drop(ring);
            mixer.reset();
        }
        let buffered = s.ring.lock().len() / 2;
        if buffered as i64 > rate / 4 {
            std::thread::sleep(Duration::from_millis(5));
            continue;
        }
        mixer.mix(snap, write_pos, &mut buf);
        let mut ring = s.ring.lock();
        if s.generation.load(Ordering::SeqCst) == generation {
            ring.extend(buf.iter().copied());
            write_pos += block as i64;
        }
    }
}

fn video_thread(s: Arc<Shared>, rx: Receiver<Cmd>, repaint: egui::Context) {
    zavyx_ve_render::register_builtins();
    let compositor = Compositor::new("linear-rec709");
    let mut snap: Option<Arc<Snapshot>> = None;
    let mut decoders: HashMap<(MediaId, u32, u32), CliDecoder> = HashMap::new();
    let mut box_size = (960u32, 540u32);
    let mut dirty = true;
    let mut last_t = f64::NAN;
    // Wall clock used when there is no audio device.
    let mut wall: Option<(Instant, f64)> = None;

    loop {
        let playing = s.playing.load(Ordering::Relaxed);
        let first = if playing || dirty {
            rx.try_recv().ok()
        } else {
            match rx.recv() { Ok(c) => Some(c), Err(_) => return }
        };
        for cmd in first.into_iter().chain(rx.try_iter()) {
            match cmd {
                Cmd::Project(p) => { snap = Some(p); dirty = true; }
                Cmd::Seek(_) => { dirty = true; wall = None; }
                Cmd::Play => { wall = None; }
                Cmd::Pause => { dirty = true; }
                Cmd::PreviewSize(w, h) => {
                    if (w, h) != box_size { box_size = (w, h); decoders.clear(); dirty = true; }
                }
            }
        }
        let Some(snap) = snap.as_ref() else { dirty = false; continue };
        let playing = s.playing.load(Ordering::Relaxed);

        if playing && !s.has_device.load(Ordering::Relaxed) {
            let (start, base) = *wall.get_or_insert((Instant::now(), s.seconds()));
            let t = base + start.elapsed().as_secs_f64();
            s.clock.store((t * s.rate.load(Ordering::Relaxed) as f64) as i64, Ordering::SeqCst);
        }
        let mut t = s.seconds();
        let end = snap.duration();
        if playing && t >= end {
            s.playing.store(false, Ordering::SeqCst);
            s.reached_end.store(true, Ordering::Relaxed);
            t = end;
            dirty = true;
        }

        let frame_len = 1.0 / snap.fps.max(1.0);
        let new_frame = (t / frame_len).floor() != (last_t / frame_len).floor();
        if dirty || (playing && new_frame) {
            let (w, h) = fit_size(snap.width, snap.height, box_size.0, box_size.1);
            let mut source = |clip: &ClipBlock, media_t: Rational, bw: u32, bh: u32| -> Option<Frame> {
                let info = snap.media.get(&clip.media_id)?;
                if !info.has_video {
                    return None;
                }
                let (fw, fh) = fit_size(info.width.unwrap_or(bw), info.height.unwrap_or(bh), bw, bh);
                let dec = decoders.entry((clip.media_id, fw, fh))
                    .or_insert_with(|| CliDecoder::new(&info.path, fw, fh, info.fps));
                dec.frame_at(media_t).ok()
            };
            let frame = compositor.compose(&snap.seq, Rational::from_seconds(t), w, h, &mut source);
            *s.frame.lock() = Some(frame);
            s.frame_serial.fetch_add(1, Ordering::Release);
            repaint.request_repaint();
            last_t = t;
            dirty = false;
        } else if playing {
            std::thread::sleep(Duration::from_millis(4));
        }
    }
}
