//! zavyx-ve-render — compositor and effect graph (GPU + CPU).
//!
//! Phase 2: CPU-only compositor + Transform/Opacity + 12 colour effects +
//! 10 transitions. Phase 3+: wgpu GPU path behind a feature flag.

use std::collections::BTreeMap;
use zavyx_ve_core::{
    Project, Sequence, Rational, FrameRate, EffectInstance, ParamValue,
    effect::{Effect, FrameView, FrameViewRef, RenderCtx, ParamSchema, ParamDef, ParamType},
    timeline::{ClipBlock, TrackKind},
};
use zavyx_ve_media::Frame;
use thiserror::Error;

pub mod effects;
pub mod transitions;
pub mod text;
pub mod masks;
pub mod pip;
pub mod tracking;
pub mod templates;
pub mod mixer;
pub mod export;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("media not found")]
    MediaNotFound,
    #[error("decoder error: {0}")]
    Decoder(String),
    #[error("compose error: {0}")]
    Compose(String),
}

/// Phase 5: GPU renderer selection. The CPU path is the default and always
/// available. The GPU path (wgpu) is feature-gated and only used when the
/// user enables hardware acceleration in settings AND the host supports it.
///
/// The GPU path stub lives here; the actual WGSL shader port is deferred to
/// v1.1 (see HANDOFF.md).
pub enum Renderer {
    /// Pure-Rust CPU compositor (always available, default).
    Cpu,
    /// GPU compositor via wgpu (feature-gated, requires `wgpu` feature).
    #[cfg(feature = "wgpu")]
    Gpu(GpuCtx),
}

/// GPU context placeholder. When the `wgpu` feature is enabled, this holds
/// the actual `wgpu::Device` + `wgpu::Queue` + a shader cache.
#[cfg(feature = "wgpu")]
pub struct GpuCtx {
    /// The wgpu device handle. Stored as `Option` because initialization may
    /// fail on hosts without D3D12/Vulkan/Metal support.
    pub device: Option<wgpu::Device>,
    pub queue: Option<wgpu::Queue>,
    /// Shader cache: WGSL source → compiled pipeline.
    pub shader_cache: std::collections::HashMap<u64, wgpu::RenderPipeline>,
}

#[cfg(feature = "wgpu")]
impl GpuCtx {
    /// Attempt to initialize a GPU context. Returns Err if no suitable
    /// adapter is found (e.g. on old 32-bit Windows with DirectX 11 only).
    pub async fn new() -> Result<Self, RenderError> {
        let instance = wgpu::Instance::default();
        let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }).await
            .ok_or_else(|| RenderError::Compose("no wgpu adapter found".into()))?;
        let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor {
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            label: None,
            memory_hints: wgpu::MemoryHints::Performance,
        }, None).await
            .map_err(|e| RenderError::Compose(format!("wgpu device: {}", e)))?;
        Ok(GpuCtx {
            device: Some(device),
            queue: Some(queue),
            shader_cache: std::collections::HashMap::new(),
        })
    }
}

impl Default for Renderer {
    fn default() -> Self { Renderer::Cpu }
}

/// Supplies a source frame for a clip: `(clip, media time, box width, box
/// height)` → a frame that fits inside the box (keeping its aspect), or
/// `None` when the clip has no picture (audio, missing file).
pub type FrameSource<'a> = dyn FnMut(&ClipBlock, Rational, u32, u32) -> Option<Frame> + 'a;

/// Effect id of the built-in title generator.
pub const TITLE_EFFECT: &str = "zavyx.title";
/// Prefix of every transition id. A clip carries its incoming transition as
/// an `EffectInstance` with this prefix and a `duration` param (seconds).
pub const TRANSITION_PREFIX: &str = "zavyx.transition.";

/// Compositor walks the sequence's tracks at a given time, pulls frames from
/// the caller's frame source, runs effects and transitions, and composites
/// the video tracks bottom-up (V1 first, higher tracks on top) over black.
pub struct Compositor {
    working_color_space: String,
}

impl Compositor {
    pub fn new(working_color_space: impl Into<String>) -> Self {
        Compositor { working_color_space: working_color_space.into() }
    }

    /// Compose one frame using a decoder per media id (kept for the CLI).
    pub fn compose_frame(
        &self,
        seq: &Sequence,
        time: Rational,
        width: u32,
        height: u32,
        decoder_provider: &mut dyn FnMut(zavyx_ve_core::MediaId) -> Option<Box<dyn zavyx_ve_media::Decoder>>,
    ) -> Result<Frame, RenderError> {
        let mut source = |clip: &ClipBlock, media_time: Rational, _w: u32, _h: u32| {
            decoder_provider(clip.media_id)?.frame_at(media_time).ok()
        };
        Ok(self.compose(seq, time, width, height, &mut source))
    }

    /// Compose one opaque RGBA frame of the sequence at `time`.
    pub fn compose(
        &self,
        seq: &Sequence,
        time: Rational,
        width: u32,
        height: u32,
        source: &mut FrameSource<'_>,
    ) -> Frame {
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        for track in seq.tracks.iter().filter(|t| t.kind == TrackKind::Video && !t.hidden) {
            if let Some(layer) = self.track_layer(track, time, width, height, source) {
                composite_over(&mut pixels, width, height, &layer, width, height);
            }
        }
        // Flatten over black so exports and previews are opaque.
        for px in pixels.chunks_exact_mut(4) {
            let a = px[3] as u32;
            for c in 0..3 {
                px[c] = (px[c] as u32 * a / 255) as u8;
            }
            px[3] = 255;
        }
        Frame { width, height, pixels, pts: time }
    }

    fn track_layer(
        &self,
        track: &zavyx_ve_core::Track,
        time: Rational,
        w: u32,
        h: u32,
        source: &mut FrameSource<'_>,
    ) -> Option<Vec<u8>> {
        let clip = track.block_at(time)?.as_clip()?;
        let mut layer = self.clip_layer(clip, time, w, h, source);

        let Some(tr) = clip.effects.iter()
            .find(|e| e.enabled && e.effect_id.starts_with(TRANSITION_PREFIX)) else {
            return layer;
        };
        let duration = tr.param("duration", time).unwrap_or(1.0).max(0.05);
        let progress = (time - clip.in_point).to_f64() / duration;
        if !(0.0..1.0).contains(&progress) {
            return layer;
        }
        let transition = lookup_transition(&tr.effect_id)?;
        // The outgoing picture is the previous clip's last frame (or nothing).
        let edge = Rational::new(1, 1000);
        let prev = track.blocks.iter()
            .filter_map(|b| b.as_clip())
            .find(|c| c.id != clip.id && (c.out_point - clip.in_point).to_f64().abs() < 0.01);
        let mut from = prev
            .and_then(|p| self.clip_layer(p, p.out_point - edge, w, h, source))
            .unwrap_or_else(|| vec![0u8; (w * h * 4) as usize]);
        let to = layer.take().unwrap_or_else(|| vec![0u8; (w * h * 4) as usize]);
        let ctx = RenderCtx { working_color_space: &self.working_color_space };
        let mut view = FrameView { width: w, height: h, pixels: &mut from, time };
        let to_ref = FrameViewRef { width: w, height: h, pixels: &to, time };
        transition.process(&mut view, to_ref, progress, &tr.params, &ctx);
        Some(from)
    }

    /// One clip's picture at `time` on a transparent canvas, effects applied.
    fn clip_layer(
        &self,
        clip: &ClipBlock,
        time: Rational,
        w: u32,
        h: u32,
        source: &mut FrameSource<'_>,
    ) -> Option<Vec<u8>> {
        let mut layer = vec![0u8; (w * h * 4) as usize];
        match source(clip, clip.timeline_to_media_time(time), w, h) {
            Some(src) => place_fit(&mut layer, w, h, &src),
            None if clip.effects.iter().any(|e| e.effect_id == TITLE_EFFECT) => {}
            None => return None,
        }
        // Effects (and their keyframes) run on clip-local time.
        let local = time - clip.in_point;
        let ctx = RenderCtx { working_color_space: &self.working_color_space };
        for fx in clip.effects.iter().filter(|e| e.enabled && !e.effect_id.starts_with(TRANSITION_PREFIX)) {
            if let Some(effect) = lookup_effect(&fx.effect_id) {
                let mut view = FrameView { width: w, height: h, pixels: &mut layer, time: local };
                effect.process(&mut view, &fx.params, &ctx);
            }
        }
        Some(layer)
    }
}

/// Copy `src` into the centre of a `w`×`h` canvas, scaled to fit (nearest
/// neighbour; sources normally arrive already at the fitted size).
fn place_fit(dst: &mut [u8], w: u32, h: u32, src: &Frame) {
    if src.width == 0 || src.height == 0 {
        return;
    }
    let scale = (w as f64 / src.width as f64).min(h as f64 / src.height as f64);
    let fw = ((src.width as f64 * scale).round() as u32).clamp(1, w);
    let fh = ((src.height as f64 * scale).round() as u32).clamp(1, h);
    let (ox, oy) = ((w - fw) / 2, (h - fh) / 2);
    let same = fw == src.width && fh == src.height;
    for y in 0..fh {
        let sy = if same { y } else { (y as u64 * src.height as u64 / fh as u64) as u32 };
        let drow = (((oy + y) * w + ox) * 4) as usize;
        if same {
            let srow = (sy * src.width * 4) as usize;
            let n = (fw * 4) as usize;
            dst[drow..drow + n].copy_from_slice(&src.pixels[srow..srow + n]);
        } else {
            for x in 0..fw {
                let sx = (x as u64 * src.width as u64 / fw as u64) as u32;
                let si = ((sy * src.width + sx) * 4) as usize;
                let di = drow + (x * 4) as usize;
                dst[di..di + 4].copy_from_slice(&src.pixels[si..si + 4]);
            }
        }
    }
}

/// Every registered effect, sorted by display name.
pub fn all_effects() -> Vec<Arc<dyn Effect>> {
    let mut v: Vec<_> = REGISTRY.lock().values().cloned().collect();
    v.sort_by(|a, b| a.name().cmp(b.name()));
    v
}

/// Every registered transition, sorted by display name.
pub fn all_transitions() -> Vec<Arc<dyn zavyx_ve_core::effect::Transition>> {
    let mut v: Vec<_> = TRANSITION_REGISTRY.lock().values().cloned().collect();
    v.sort_by(|a, b| a.name().cmp(b.name()));
    v
}

/// Simple alpha-over compositing.
fn composite_over(dst: &mut [u8], dw: u32, dh: u32, src: &[u8], sw: u32, sh: u32) {
    let dw = dw as usize; let dh = dh as usize;
    let sw = sw as usize; let sh = sh as usize;
    if dw == sw && dh == sh {
        for i in 0..(dw * dh) {
            let sa = src[i*4+3] as f32 / 255.0;
            if sa == 0.0 { continue; }
            let da = dst[i*4+3] as f32 / 255.0;
            let out_a = sa + da * (1.0 - sa);
            if out_a == 0.0 { continue; }
            for c in 0..3 {
                let s = src[i*4+c] as f32;
                let d = dst[i*4+c] as f32;
                dst[i*4+c] = ((s * sa + d * da * (1.0 - sa)) / out_a) as u8;
            }
            dst[i*4+3] = (out_a * 255.0) as u8;
        }
    } else {
        for y in 0..dh {
            let sy = y * sh / dh;
            for x in 0..dw {
                let sx = x * sw / dw;
                let di = (y * dw + x) * 4;
                let si = (sy * sw + sx) * 4;
                let sa = src[si+3] as f32 / 255.0;
                if sa == 0.0 { continue; }
                let da = dst[di+3] as f32 / 255.0;
                let out_a = sa + da * (1.0 - sa);
                if out_a == 0.0 { continue; }
                for c in 0..3 {
                    let s = src[si+c] as f32;
                    let d = dst[di+c] as f32;
                    dst[di+c] = ((s * sa + d * da * (1.0 - sa)) / out_a) as u8;
                }
                dst[di+3] = (out_a * 255.0) as u8;
            }
        }
    }
}

// --- Effect registry --------------------------------------------------------

use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

type Registry = HashMap<String, Arc<dyn Effect>>;
type TransitionRegistry = HashMap<String, Arc<dyn zavyx_ve_core::effect::Transition>>;

static REGISTRY: LazyLock<Mutex<Registry>> =
    LazyLock::new(|| parking_lot::const_mutex(HashMap::new()));

static TRANSITION_REGISTRY: LazyLock<Mutex<TransitionRegistry>> =
    LazyLock::new(|| parking_lot::const_mutex(HashMap::new()));

/// Register a built-in effect so the compositor can find it by id.
pub fn register_effect(effect: Arc<dyn Effect>) {
    let id = effect.id().to_string();
    REGISTRY.lock().insert(id, effect);
}

/// Register a built-in transition by id.
pub fn register_transition(transition: Arc<dyn zavyx_ve_core::effect::Transition>) {
    let id = transition.id().to_string();
    TRANSITION_REGISTRY.lock().insert(id, transition);
}

/// Look up a registered effect by id.
pub fn lookup_effect(id: &str) -> Option<Arc<dyn Effect>> {
    REGISTRY.lock().get(id).cloned()
}

/// Look up a registered transition by id.
pub fn lookup_transition(id: &str) -> Option<Arc<dyn zavyx_ve_core::effect::Transition>> {
    TRANSITION_REGISTRY.lock().get(id).cloned()
}

/// Register all built-in effects + transitions (Phase 1 + Phase 2 + Phase 3).
pub fn register_builtins() {
    // Phase 1
    register_effect(Arc::new(effects::TransformEffect::new()));
    register_effect(Arc::new(effects::OpacityEffect::new()));
    register_effect(Arc::new(text::TitleEffect::new(text::default_font_path())));

    // Phase 2 — colour effects
    register_effect(Arc::new(effects::ExposureEffect::new()));
    register_effect(Arc::new(effects::ContrastEffect::new()));
    register_effect(Arc::new(effects::HighlightsEffect::new()));
    register_effect(Arc::new(effects::ShadowsEffect::new()));
    register_effect(Arc::new(effects::WhitesEffect::new()));
    register_effect(Arc::new(effects::BlacksEffect::new()));
    register_effect(Arc::new(effects::SaturationEffect::new()));
    register_effect(Arc::new(effects::VibranceEffect::new()));
    register_effect(Arc::new(effects::TemperatureEffect::new()));
    register_effect(Arc::new(effects::TintEffect::new()));
    register_effect(Arc::new(effects::SharpenEffect::new()));
    register_effect(Arc::new(effects::VignetteEffect::new()));

    // Phase 2 — transitions (10)
    register_transition(Arc::new(transitions::Dissolve::new()));
    register_transition(Arc::new(transitions::DipToBlack::new()));
    register_transition(Arc::new(transitions::DipToWhite::new()));
    register_transition(Arc::new(transitions::WipeLeft::new()));
    register_transition(Arc::new(transitions::WipeRight::new()));
    register_transition(Arc::new(transitions::WipeUp::new()));
    register_transition(Arc::new(transitions::WipeDown::new()));
    register_transition(Arc::new(transitions::Slide::new()));
    register_transition(Arc::new(transitions::Push::new()));
    register_transition(Arc::new(transitions::Zoom::new()));

    // Phase 3 — 28 additional effects
    register_effect(Arc::new(effects::extra::GaussianBlurEffect::new()));
    register_effect(Arc::new(effects::extra::BoxBlurEffect::new()));
    register_effect(Arc::new(effects::extra::MotionBlurEffect::new()));
    register_effect(Arc::new(effects::extra::RadialBlurEffect::new()));
    register_effect(Arc::new(effects::extra::MosaicEffect::new()));
    register_effect(Arc::new(effects::extra::PosterizeEffect::new()));
    register_effect(Arc::new(effects::extra::InvertEffect::new()));
    register_effect(Arc::new(effects::extra::MirrorEffect::new()));
    register_effect(Arc::new(effects::extra::FlipEffect::new()));
    register_effect(Arc::new(effects::extra::RgbSplitEffect::new()));
    register_effect(Arc::new(effects::extra::GlowEffect::new()));
    register_effect(Arc::new(effects::extra::BloomEffect::new()));
    register_effect(Arc::new(effects::extra::OldFilmEffect::new()));
    register_effect(Arc::new(effects::extra::VhsEffect::new()));
    register_effect(Arc::new(effects::extra::GlitchEffect::new()));
    register_effect(Arc::new(effects::extra::ScanlinesEffect::new()));
    register_effect(Arc::new(effects::extra::CrtEffect::new()));
    register_effect(Arc::new(effects::extra::EdgeDetectEffect::new()));
    register_effect(Arc::new(effects::extra::BlackAndWhiteEffect::new()));
    register_effect(Arc::new(effects::extra::SepiaEffect::new()));
    register_effect(Arc::new(effects::extra::DuotoneEffect::new()));
    register_effect(Arc::new(effects::extra::ShakeEffect::new()));
    register_effect(Arc::new(effects::extra::ZoomPunchEffect::new()));
    register_effect(Arc::new(effects::extra::ChromaticAberrationEffect::new()));
    register_effect(Arc::new(effects::extra::LensCorrectionEffect::new()));
    register_effect(Arc::new(effects::extra::FilmGrainEffect::new()));
    register_effect(Arc::new(effects::extra::DejitterEffect::new()));
    register_effect(Arc::new(effects::extra::HalftoneEffect::new()));
    register_effect(Arc::new(effects::extra::DeinterlaceEffect::new()));

    // Phase 3 — masks + keying
    register_effect(Arc::new(masks::RectangleMaskEffect::new()));
    register_effect(Arc::new(masks::EllipseMaskEffect::new()));
    register_effect(Arc::new(masks::ChromaKeyEffect::new()));
    register_effect(Arc::new(masks::LumaKeyEffect::new()));
    // LutEffect is parameterized by the LUT, so it's not auto-registered.
    // Users create instances via LutEffect::new(Lut::parse_cube(text)?) and
    // register them via register_effect().

    // Phase 4 — PIP / split-screen
    register_effect(Arc::new(pip::PipEffect::new()));
    register_effect(Arc::new(pip::SplitScreenEffect::new()));

    // Phase 3 — 20 additional transitions
    register_transition(Arc::new(transitions::extra::Spin::new()));
    register_transition(Arc::new(transitions::extra::BlurTransition::new()));
    register_transition(Arc::new(transitions::extra::GlitchTransition::new()));
    register_transition(Arc::new(transitions::extra::WipeCircle::new()));
    register_transition(Arc::new(transitions::extra::WipeSquare::new()));
    register_transition(Arc::new(transitions::extra::WipeDiamond::new()));
    register_transition(Arc::new(transitions::extra::WipeStar::new()));
    register_transition(Arc::new(transitions::extra::WipeHeart::new()));
    register_transition(Arc::new(transitions::extra::PushLeft::new()));
    register_transition(Arc::new(transitions::extra::PushRight::new()));
    register_transition(Arc::new(transitions::extra::PushUp::new()));
    register_transition(Arc::new(transitions::extra::PushDown::new()));
    register_transition(Arc::new(transitions::extra::SlideLeft::new()));
    register_transition(Arc::new(transitions::extra::SlideRight::new()));
    register_transition(Arc::new(transitions::extra::Squeeze::new()));
    register_transition(Arc::new(transitions::extra::Flash::new()));
    register_transition(Arc::new(transitions::extra::IrisOpen::new()));
    register_transition(Arc::new(transitions::extra::IrisClose::new()));
    register_transition(Arc::new(transitions::extra::PageCurl::new()));
    register_transition(Arc::new(transitions::extra::CrossZoom::new()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opacity_reduces_alpha() {
        register_builtins();
        let opacity = effects::OpacityEffect::new();
        let mut pixels = vec![255u8; 4*4*4];
        let mut view = FrameView {
            width: 4, height: 4, pixels: &mut pixels,
            time: Rational::ZERO,
        };
        let mut params = BTreeMap::new();
        params.insert("opacity".into(), ParamValue::Const(0.5));
        let ctx = RenderCtx { working_color_space: "linear-rec709" };
        opacity.process(&mut view, &params, &ctx);
        assert_eq!(pixels[3], 127);
    }

    fn solid(w: u32, h: u32, rgba: [u8; 4]) -> Frame {
        Frame { width: w, height: h, pixels: rgba.repeat((w * h) as usize), pts: Rational::ZERO }
    }

    /// Sequence with clips A [0,2) and B [2,4) on V1, coloured by media id.
    fn two_clip_seq() -> (Sequence, zavyx_ve_core::MediaId, zavyx_ve_core::MediaId) {
        let mut seq = Sequence::new("s", FrameRate::FPS_30);
        let (a, b) = (zavyx_ve_core::MediaId::new(), zavyx_ve_core::MediaId::new());
        let v1 = &mut seq.tracks[0];
        v1.insert_clip(ClipBlock::new(a, Rational::ZERO, Rational::new(2, 1), Rational::ZERO));
        v1.insert_clip(ClipBlock::new(b, Rational::new(2, 1), Rational::new(4, 1), Rational::ZERO));
        (seq, a, b)
    }

    #[test]
    fn compose_letterboxes_and_flattens_over_black() {
        register_builtins();
        let (seq, _, _) = two_clip_seq();
        let c = Compositor::new("linear-rec709");
        // A 4x2 source in an 4x4 canvas: rows 1-2 filled, rows 0 and 3 black.
        let mut src = |_: &ClipBlock, _: Rational, _: u32, _: u32| Some(solid(4, 2, [200, 0, 0, 255]));
        let f = c.compose(&seq, Rational::ONE, 4, 4, &mut src);
        assert_eq!(&f.pixels[0..4], &[0, 0, 0, 255]);
        assert_eq!(&f.pixels[16..20], &[200, 0, 0, 255]);
    }

    #[test]
    fn dissolve_blends_previous_clip_into_next() {
        register_builtins();
        let (mut seq, a, _) = two_clip_seq();
        let clip_b = seq.tracks[0].blocks.iter().filter_map(|b| b.as_clip()).nth(1).unwrap().id;
        let mut tr = EffectInstance::new("zavyx.transition.dissolve");
        tr.set_const("duration", 1.0);
        seq.tracks[0].clip_mut(clip_b).unwrap().effects.push(tr);
        let c = Compositor::new("linear-rec709");
        let mut src = |clip: &ClipBlock, _: Rational, w: u32, h: u32| {
            Some(if clip.media_id == a { solid(w, h, [200, 0, 0, 255]) } else { solid(w, h, [0, 0, 200, 255]) })
        };
        let mid = c.compose(&seq, Rational::new(5, 2), 2, 2, &mut src);
        assert!((90..=110).contains(&mid.pixels[0]) && (90..=110).contains(&mid.pixels[2]));
        let after = c.compose(&seq, Rational::new(7, 2), 2, 2, &mut src);
        assert_eq!(&after.pixels[0..3], &[0, 0, 200]);
    }

    #[test]
    fn higher_video_track_is_on_top() {
        register_builtins();
        let (mut seq, a, _) = two_clip_seq();
        let v2 = seq.add_track(TrackKind::Video);
        let top = zavyx_ve_core::MediaId::new();
        seq.track_mut(v2).unwrap()
            .insert_clip(ClipBlock::new(top, Rational::ZERO, Rational::new(2, 1), Rational::ZERO));
        let c = Compositor::new("linear-rec709");
        let mut src = |clip: &ClipBlock, _: Rational, w: u32, h: u32| {
            Some(if clip.media_id == a { solid(w, h, [200, 0, 0, 255]) } else { solid(w, h, [0, 200, 0, 255]) })
        };
        let f = c.compose(&seq, Rational::ONE, 2, 2, &mut src);
        assert_eq!(&f.pixels[0..3], &[0, 200, 0]);
    }

    #[test]
    fn transform_shifts_pixels() {
        let transform = effects::TransformEffect::new();
        let mut pixels = vec![0u8; 4*4*4];
        pixels[0] = 255; pixels[1] = 0; pixels[2] = 0; pixels[3] = 255;
        let mut view = FrameView {
            width: 4, height: 4, pixels: &mut pixels,
            time: Rational::ZERO,
        };
        let mut params = BTreeMap::new();
        params.insert("x".into(), ParamValue::Const(0.25));
        params.insert("y".into(), ParamValue::Const(0.25));
        params.insert("scale".into(), ParamValue::Const(1.0));
        let ctx = RenderCtx { working_color_space: "linear-rec709" };
        transform.process(&mut view, &params, &ctx);
        let i = ((1*4 + 1) * 4) as usize;
        assert_eq!(pixels[i], 255);
        assert_eq!(pixels[i+1], 0);
        assert_eq!(pixels[i+3], 255);
        assert_eq!(pixels[0], 0);
    }

    #[test]
    fn registry_has_all_phase2_effects() {
        register_builtins();
        let ids = [
            "zavyx.transform", "zavyx.opacity",
            "zavyx.exposure", "zavyx.contrast",
            "zavyx.highlights", "zavyx.shadows", "zavyx.whites", "zavyx.blacks",
            "zavyx.saturation", "zavyx.vibrance",
            "zavyx.temperature", "zavyx.tint",
            "zavyx.sharpen", "zavyx.vignette",
        ];
        for id in ids {
            assert!(lookup_effect(id).is_some(), "missing effect {}", id);
        }
    }

    #[test]
    fn registry_has_all_phase3_effects() {
        register_builtins();
        let ids = [
            "zavyx.gaussian_blur", "zavyx.box_blur", "zavyx.motion_blur", "zavyx.radial_blur",
            "zavyx.mosaic", "zavyx.posterize", "zavyx.invert",
            "zavyx.mirror", "zavyx.flip", "zavyx.rgb_split",
            "zavyx.glow", "zavyx.bloom", "zavyx.old_film", "zavyx.vhs",
            "zavyx.glitch", "zavyx.scanlines", "zavyx.crt", "zavyx.edge_detect",
            "zavyx.black_and_white", "zavyx.sepia", "zavyx.duotone",
            "zavyx.shake", "zavyx.zoom_punch",
            "zavyx.chromatic_aberration", "zavyx.lens_correction",
            "zavyx.film_grain", "zavyx.dejitter", "zavyx.halftone", "zavyx.deinterlace",
            "zavyx.mask.rectangle", "zavyx.mask.ellipse",
            "zavyx.chroma_key", "zavyx.luma_key",
        ];
        for id in ids {
            assert!(lookup_effect(id).is_some(), "missing Phase 3 effect {}", id);
        }
    }

    #[test]
    fn registry_has_all_phase3_transitions() {
        register_builtins();
        let ids = [
            "zavyx.transition.spin", "zavyx.transition.blur", "zavyx.transition.glitch",
            "zavyx.transition.wipe_circle", "zavyx.transition.wipe_square",
            "zavyx.transition.wipe_diamond", "zavyx.transition.wipe_star", "zavyx.transition.wipe_heart",
            "zavyx.transition.push_left", "zavyx.transition.push_right",
            "zavyx.transition.push_up", "zavyx.transition.push_down",
            "zavyx.transition.slide_left", "zavyx.transition.slide_right",
            "zavyx.transition.squeeze", "zavyx.transition.flash",
            "zavyx.transition.iris_open", "zavyx.transition.iris_close",
            "zavyx.transition.page_curl", "zavyx.transition.cross_zoom",
        ];
        for id in ids {
            assert!(lookup_transition(id).is_some(), "missing Phase 3 transition {}", id);
        }
    }

    #[test]
    fn registry_has_all_phase2_transitions() {
        register_builtins();
        let ids = [
            "zavyx.transition.dissolve",
            "zavyx.transition.dip_to_black",
            "zavyx.transition.dip_to_white",
            "zavyx.transition.wipe_left",
            "zavyx.transition.wipe_right",
            "zavyx.transition.wipe_up",
            "zavyx.transition.wipe_down",
            "zavyx.transition.slide",
            "zavyx.transition.push",
            "zavyx.transition.zoom",
        ];
        for id in ids {
            assert!(lookup_transition(id).is_some(), "missing transition {}", id);
        }
    }
}
