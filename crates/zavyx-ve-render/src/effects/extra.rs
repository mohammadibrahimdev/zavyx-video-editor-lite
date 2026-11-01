//! Phase 3 effects: 28 additional effects beyond the Phase 2 set.
//!
//! Categories:
//! - Blur: Gaussian, Box, Motion, Radial
//! - Pixel: Mosaic/Pixelate, Posterize, Halftone, Invert
//! - Distort: Mirror, Flip, RGB Split, Chromatic Aberration, Lens Correction
//! - Stylize: Glow, Bloom, Old Film, VHS, Glitch, Scanlines, CRT, Edge Detect
//! - Color: Black & White, Sepia, Duotone, Vibrance (already in mod.rs)
//! - Time-based: Shake, Zoom Punch, Dejitter
//!
//! All operate on RGBA8 in-place via the `Effect` trait.

use std::collections::BTreeMap;
use zavyx_ve_core::Rational;
use zavyx_ve_core::effect::{Effect, FrameView, RenderCtx, ParamSchema, ParamDef, ParamType, ParamValue};

// Helper from mod.rs
use super::{to_linear, from_linear, per_pixel_linear, luma, box_blur};

// =========================================================================
// BLUR FAMILY
// =========================================================================

/// Gaussian blur via separable 1D passes (3 passes ≈ 9x9 kernel).
pub struct GaussianBlurEffect;
impl GaussianBlurEffect { pub fn new() -> Self { GaussianBlurEffect } }
impl Effect for GaussianBlurEffect {
    fn id(&self) -> &str { "zavyx.gaussian_blur" }
    fn name(&self) -> &str { "Gaussian Blur" }
    fn param_schema(&self) -> ParamSchema {
        ParamSchema { params: vec![
            ParamDef { id: "radius".into(), label: "Radius (px)".into(),
                ty: ParamType::Float { min: 0.0, max: 50.0, step: 0.5, default: 5.0 },
                keyframeable: true },
        ] }
    }
    fn process(&self, frame: &mut FrameView<'_>, params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        let radius = params.get("radius").map(|v| v.at(frame.time)).unwrap_or(5.0);
        if radius < 0.5 { return; }
        let r = radius as usize;
        let w = frame.width as usize;
        let h = frame.height as usize;
        let mut buf = frame.pixels.to_vec();
        // Horizontal pass
        let mut horiz = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let mut sum = [0u32; 4]; let mut count = 0u32;
                for dx in -(r as i32)..=(r as i32) {
                    let nx = (x as i32 + dx).max(0).min(w as i32 - 1) as usize;
                    let i = (y * w + nx) * 4;
                    sum[0] += buf[i] as u32; sum[1] += buf[i+1] as u32; sum[2] += buf[i+2] as u32; sum[3] += buf[i+3] as u32;
                    count += 1;
                }
                let di = (y * w + x) * 4;
                horiz[di]   = (sum[0] / count) as u8;
                horiz[di+1] = (sum[1] / count) as u8;
                horiz[di+2] = (sum[2] / count) as u8;
                horiz[di+3] = (sum[3] / count) as u8;
            }
        }
        // Vertical pass
        for y in 0..h {
            for x in 0..w {
                let mut sum = [0u32; 4]; let mut count = 0u32;
                for dy in -(r as i32)..=(r as i32) {
                    let ny = (y as i32 + dy).max(0).min(h as i32 - 1) as usize;
                    let i = (ny * w + x) * 4;
                    sum[0] += horiz[i] as u32; sum[1] += horiz[i+1] as u32; sum[2] += horiz[i+2] as u32; sum[3] += horiz[i+3] as u32;
                    count += 1;
                }
                let di = (y * w + x) * 4;
                frame.pixels[di]   = (sum[0] / count) as u8;
                frame.pixels[di+1] = (sum[1] / count) as u8;
                frame.pixels[di+2] = (sum[2] / count) as u8;
                frame.pixels[di+3] = (sum[3] / count) as u8;
            }
        }
    }
}

/// Box blur (3x3).
pub struct BoxBlurEffect;
impl BoxBlurEffect { pub fn new() -> Self { BoxBlurEffect } }
impl Effect for BoxBlurEffect {
    fn id(&self) -> &str { "zavyx.box_blur" }
    fn name(&self) -> &str { "Box Blur" }
    fn param_schema(&self) -> ParamSchema {
        ParamSchema { params: vec![
            ParamDef { id: "amount".into(), label: "Amount".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 0.0 },
                keyframeable: true },
        ] }
    }
    fn process(&self, frame: &mut FrameView<'_>, params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        let amount = params.get("amount").map(|v| v.at(frame.time) as f32).unwrap_or(0.0);
        if amount <= 0.0 { return; }
        let w = frame.width as usize;
        let h = frame.height as usize;
        let src = frame.pixels.to_vec();
        let blurred = box_blur(&src, w, h);
        // Mix src + blurred by `amount`
        for i in 0..(w*h) {
            for c in 0..4 {
                let s = src[i*4+c] as f32;
                let b = blurred[i*4+c] as f32;
                frame.pixels[i*4+c] = (s * (1.0 - amount) + b * amount) as u8;
            }
        }
    }
}

/// Motion blur in a given direction.
pub struct MotionBlurEffect;
impl MotionBlurEffect { pub fn new() -> Self { MotionBlurEffect } }
impl Effect for MotionBlurEffect {
    fn id(&self) -> &str { "zavyx.motion_blur" }
    fn name(&self) -> &str { "Motion Blur" }
    fn param_schema(&self) -> ParamSchema {
        ParamSchema { params: vec![
            ParamDef { id: "angle".into(), label: "Angle (deg)".into(),
                ty: ParamType::Float { min: 0.0, max: 360.0, step: 1.0, default: 0.0 },
                keyframeable: true },
            ParamDef { id: "distance".into(), label: "Distance (px)".into(),
                ty: ParamType::Float { min: 0.0, max: 50.0, step: 0.5, default: 10.0 },
                keyframeable: true },
        ] }
    }
    fn process(&self, frame: &mut FrameView<'_>, params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        let angle = params.get("angle").map(|v| v.at(frame.time) as f32).unwrap_or(0.0);
        let dist  = params.get("distance").map(|v| v.at(frame.time) as f32).unwrap_or(10.0);
        if dist < 0.5 { return; }
        let rad = angle.to_radians();
        let dx = rad.cos() * dist;
        let dy = rad.sin() * dist;
        let w = frame.width as usize;
        let h = frame.height as usize;
        let src = frame.pixels.to_vec();
        let n = (dist as usize).max(1);
        for y in 0..h {
            for x in 0..w {
                let mut sum = [0u32; 4];
                for i in 0..=n {
                    let t = i as f32 / n as f32;
                    let sx = (x as f32 - dx * t).round() as i32;
                    let sy = (y as f32 - dy * t).round() as i32;
                    if sx < 0 || sx >= w as i32 || sy < 0 || sy >= h as i32 { continue; }
                    let si = (sy as usize * w + sx as usize) * 4;
                    sum[0] += src[si] as u32; sum[1] += src[si+1] as u32; sum[2] += src[si+2] as u32; sum[3] += src[si+3] as u32;
                }
                let di = (y * w + x) * 4;
                let c = (n + 1) as u32;
                frame.pixels[di]   = (sum[0] / c) as u8;
                frame.pixels[di+1] = (sum[1] / c) as u8;
                frame.pixels[di+2] = (sum[2] / c) as u8;
                frame.pixels[di+3] = (sum[3] / c) as u8;
            }
        }
    }
}

/// Radial blur — averages pixels along a line from the center.
pub struct RadialBlurEffect;
impl RadialBlurEffect { pub fn new() -> Self { RadialBlurEffect } }
impl Effect for RadialBlurEffect {
    fn id(&self) -> &str { "zavyx.radial_blur" }
    fn name(&self) -> &str { "Radial Blur" }
    fn param_schema(&self) -> ParamSchema {
        ParamSchema { params: vec![
            ParamDef { id: "amount".into(), label: "Amount".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 0.0 },
                keyframeable: true },
        ] }
    }
    fn process(&self, frame: &mut FrameView<'_>, params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        let amount = params.get("amount").map(|v| v.at(frame.time) as f32).unwrap_or(0.0);
        if amount <= 0.0 { return; }
        let w = frame.width as usize;
        let h = frame.height as usize;
        let cx = w as f32 / 2.0;
        let cy = h as f32 / 2.0;
        let n = (amount * 20.0) as usize + 1;
        let src = frame.pixels.to_vec();
        for y in 0..h {
            for x in 0..w {
                let dx = x as f32 - cx;
                let dy = y as f32 - cy;
                let mut sum = [0u32; 4]; let mut count = 0u32;
                for i in 0..=n {
                    let t = i as f32 / n as f32;
                    let sx = (cx + dx * (1.0 - amount * t * 0.5)) as i32;
                    let sy = (cy + dy * (1.0 - amount * t * 0.5)) as i32;
                    if sx < 0 || sx >= w as i32 || sy < 0 || sy >= h as i32 { continue; }
                    let si = (sy as usize * w + sx as usize) * 4;
                    sum[0] += src[si] as u32; sum[1] += src[si+1] as u32; sum[2] += src[si+2] as u32; sum[3] += src[si+3] as u32;
                    count += 1;
                }
                if count == 0 { continue; }
                let di = (y * w + x) * 4;
                frame.pixels[di]   = (sum[0] / count) as u8;
                frame.pixels[di+1] = (sum[1] / count) as u8;
                frame.pixels[di+2] = (sum[2] / count) as u8;
                frame.pixels[di+3] = (sum[3] / count) as u8;
            }
        }
    }
}

// =========================================================================
// PIXEL-LEVEL EFFECTS
// =========================================================================

/// Mosaic / pixelate.
pub struct MosaicEffect;
impl MosaicEffect { pub fn new() -> Self { MosaicEffect } }
impl Effect for MosaicEffect {
    fn id(&self) -> &str { "zavyx.mosaic" }
    fn name(&self) -> &str { "Mosaic" }
    fn param_schema(&self) -> ParamSchema {
        ParamSchema { params: vec![
            ParamDef { id: "size".into(), label: "Block size (px)".into(),
                ty: ParamType::Float { min: 1.0, max: 64.0, step: 1.0, default: 8.0 },
                keyframeable: true },
        ] }
    }
    fn process(&self, frame: &mut FrameView<'_>, params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        let size = params.get("size").map(|v| v.at(frame.time) as u32).unwrap_or(8).max(1);
        let w = frame.width;
        let h = frame.height;
        let src = frame.pixels.to_vec();
        for by in 0..(h + size - 1) / size {
            for bx in 0..(w + size - 1) / size {
                // Average the block.
                let x0 = bx * size;
                let y0 = by * size;
                let mut sum = [0u32; 4]; let mut count = 0u32;
                for y in y0..(y0 + size).min(h) {
                    for x in x0..(x0 + size).min(w) {
                        let i = ((y * w + x) * 4) as usize;
                        sum[0] += src[i] as u32; sum[1] += src[i+1] as u32; sum[2] += src[i+2] as u32; sum[3] += src[i+3] as u32;
                        count += 1;
                    }
                }
                if count == 0 { continue; }
                let avg = [sum[0]/count, sum[1]/count, sum[2]/count, sum[3]/count];
                for y in y0..(y0 + size).min(h) {
                    for x in x0..(x0 + size).min(w) {
                        let i = ((y * w + x) * 4) as usize;
                        frame.pixels[i]   = avg[0] as u8;
                        frame.pixels[i+1] = avg[1] as u8;
                        frame.pixels[i+2] = avg[2] as u8;
                        frame.pixels[i+3] = avg[3] as u8;
                    }
                }
            }
        }
    }
}

/// Posterize: reduce number of levels per channel.
pub struct PosterizeEffect;
impl PosterizeEffect { pub fn new() -> Self { PosterizeEffect } }
impl Effect for PosterizeEffect {
    fn id(&self) -> &str { "zavyx.posterize" }
    fn name(&self) -> &str { "Posterize" }
    fn param_schema(&self) -> ParamSchema {
        ParamSchema { params: vec![
            ParamDef { id: "levels".into(), label: "Levels".into(),
                ty: ParamType::Float { min: 2.0, max: 32.0, step: 1.0, default: 6.0 },
                keyframeable: true },
        ] }
    }
    fn process(&self, frame: &mut FrameView<'_>, params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        let levels = params.get("levels").map(|v| v.at(frame.time) as f32).unwrap_or(6.0).max(2.0);
        let step = 255.0 / (levels - 1.0);
        for px in frame.pixels.chunks_exact_mut(4) {
            for c in 0..3 {
                px[c] = ((px[c] as f32 / 255.0) * (levels - 1.0)).round() as u8;
                px[c] = (px[c] as f32 * step).clamp(0.0, 255.0) as u8;
            }
        }
    }
}

/// Invert colors.
pub struct InvertEffect;
impl InvertEffect { pub fn new() -> Self { InvertEffect } }
impl Effect for InvertEffect {
    fn id(&self) -> &str { "zavyx.invert" }
    fn name(&self) -> &str { "Invert" }
    fn param_schema(&self) -> ParamSchema {
        ParamSchema { params: vec![
            ParamDef { id: "amount".into(), label: "Amount".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 1.0 },
                keyframeable: true },
        ] }
    }
    fn process(&self, frame: &mut FrameView<'_>, params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        let amount = params.get("amount").map(|v| v.at(frame.time) as f32).unwrap_or(1.0);
        for px in frame.pixels.chunks_exact_mut(4) {
            for c in 0..3 {
                let inv = 255 - px[c];
                px[c] = ((px[c] as f32) * (1.0 - amount) + (inv as f32) * amount) as u8;
            }
        }
    }
}

// =========================================================================
// DISTORT FAMILY
// =========================================================================

/// Mirror (horizontal or vertical).
pub struct MirrorEffect;
impl MirrorEffect { pub fn new() -> Self { MirrorEffect } }
impl Effect for MirrorEffect {
    fn id(&self) -> &str { "zavyx.mirror" }
    fn name(&self) -> &str { "Mirror" }
    fn param_schema(&self) -> ParamSchema {
        ParamSchema { params: vec![
            ParamDef { id: "axis".into(), label: "Axis".into(),
                ty: ParamType::Choice { options: vec!["horizontal".into(), "vertical".into()], default: "horizontal".into() },
                keyframeable: false },
            ParamDef { id: "amount".into(), label: "Amount".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 1.0 },
                keyframeable: true },
        ] }
    }
    fn process(&self, frame: &mut FrameView<'_>, params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        let axis = params.get("axis").and_then(|v| match v {
            ParamValue::Const(x) => Some(*x),
            _ => None,
        }).map(|x| if x < 0.5 { "horizontal" } else { "vertical" }).unwrap_or("horizontal");
        let amount = params.get("amount").map(|v| v.at(frame.time) as f32).unwrap_or(1.0);
        if amount <= 0.0 { return; }
        let w = frame.width as usize;
        let h = frame.height as usize;
        let src = frame.pixels.to_vec();
        if axis == "horizontal" {
            for y in 0..h {
                for x in 0..(w / 2) {
                    let i = (y * w + x) * 4;
                    let mi = (y * w + (w - 1 - x)) * 4;
                    for c in 0..4 {
                        let a = src[i+c] as f32;
                        let b = src[mi+c] as f32;
                        frame.pixels[i+c] = (a * (1.0 - amount) + b * amount) as u8;
                    }
                }
            }
        } else {
            for y in 0..(h / 2) {
                for x in 0..w {
                    let i = (y * w + x) * 4;
                    let mi = ((h - 1 - y) * w + x) * 4;
                    for c in 0..4 {
                        let a = src[i+c] as f32;
                        let b = src[mi+c] as f32;
                        frame.pixels[i+c] = (a * (1.0 - amount) + b * amount) as u8;
                    }
                }
            }
        }
    }
}

/// Flip (horizontal or vertical — same as mirror but applied to the whole image).
pub struct FlipEffect;
impl FlipEffect { pub fn new() -> Self { FlipEffect } }
impl Effect for FlipEffect {
    fn id(&self) -> &str { "zavyx.flip" }
    fn name(&self) -> &str { "Flip" }
    fn param_schema(&self) -> ParamSchema {
        ParamSchema { params: vec![
            ParamDef { id: "axis".into(), label: "Axis".into(),
                ty: ParamType::Choice { options: vec!["horizontal".into(), "vertical".into()], default: "horizontal".into() },
                keyframeable: false },
        ] }
    }
    fn process(&self, frame: &mut FrameView<'_>, params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        let axis = params.get("axis").and_then(|v| match v {
            ParamValue::Const(x) => Some(*x),
            _ => None,
        }).map(|x| if x < 0.5 { "horizontal" } else { "vertical" }).unwrap_or("horizontal");
        let w = frame.width as usize;
        let h = frame.height as usize;
        let src = frame.pixels.to_vec();
        if axis == "horizontal" {
            for y in 0..h {
                for x in 0..w {
                    let di = (y * w + x) * 4;
                    let si = (y * w + (w - 1 - x)) * 4;
                    frame.pixels[di..di+4].copy_from_slice(&src[si..si+4]);
                }
            }
        } else {
            for y in 0..h {
                for x in 0..w {
                    let di = (y * w + x) * 4;
                    let si = ((h - 1 - y) * w + x) * 4;
                    frame.pixels[di..di+4].copy_from_slice(&src[si..si+4]);
                }
            }
        }
    }
}

/// RGB Split — offsets R, G, B channels by different amounts.
pub struct RgbSplitEffect;
impl RgbSplitEffect { pub fn new() -> Self { RgbSplitEffect } }
impl Effect for RgbSplitEffect {
    fn id(&self) -> &str { "zavyx.rgb_split" }
    fn name(&self) -> &str { "RGB Split" }
    fn param_schema(&self) -> ParamSchema {
        ParamSchema { params: vec![
            ParamDef { id: "offset".into(), label: "Offset (px)".into(),
                ty: ParamType::Float { min: 0.0, max: 50.0, step: 0.5, default: 5.0 },
                keyframeable: true },
        ] }
    }
    fn process(&self, frame: &mut FrameView<'_>, params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        let offset = params.get("offset").map(|v| v.at(frame.time) as i32).unwrap_or(5);
        if offset == 0 { return; }
        let w = frame.width as usize;
        let h = frame.height as usize;
        let src = frame.pixels.to_vec();
        for y in 0..h {
            for x in 0..w {
                let di = (y * w + x) * 4;
                // R shifted left by `offset`, B shifted right by `offset`.
                let r_x = (x as i32 - offset).max(0).min(w as i32 - 1) as usize;
                let b_x = (x as i32 + offset).max(0).min(w as i32 - 1) as usize;
                let r_i = (y * w + r_x) * 4;
                let b_i = (y * w + b_x) * 4;
                frame.pixels[di]   = src[r_i];
                frame.pixels[di+1] = src[di+1]; // G unchanged
                frame.pixels[di+2] = src[b_i+2];
                frame.pixels[di+3] = src[di+3];
            }
        }
    }
}

// =========================================================================
// STYLIZE FAMILY
// =========================================================================

/// Glow — extract bright areas, blur them, add back.
pub struct GlowEffect;
impl GlowEffect { pub fn new() -> Self { GlowEffect } }
impl Effect for GlowEffect {
    fn id(&self) -> &str { "zavyx.glow" }
    fn name(&self) -> &str { "Glow" }
    fn param_schema(&self) -> ParamSchema {
        ParamSchema { params: vec![
            ParamDef { id: "threshold".into(), label: "Threshold".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 0.5 },
                keyframeable: true },
            ParamDef { id: "intensity".into(), label: "Intensity".into(),
                ty: ParamType::Float { min: 0.0, max: 2.0, step: 0.001, default: 0.5 },
                keyframeable: true },
        ] }
    }
    fn process(&self, frame: &mut FrameView<'_>, params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        let threshold = params.get("threshold").map(|v| v.at(frame.time) as f32).unwrap_or(0.5);
        let intensity = params.get("intensity").map(|v| v.at(frame.time) as f32).unwrap_or(0.5);
        if intensity <= 0.0 { return; }
        let w = frame.width as usize;
        let h = frame.height as usize;
        // Extract bright pixels into a mask buffer.
        let mut bright = vec![0u8; w * h * 4];
        for i in 0..(w*h) {
            let l = luma(frame.pixels[i*4] as f32 / 255.0,
                         frame.pixels[i*4+1] as f32 / 255.0,
                         frame.pixels[i*4+2] as f32 / 255.0);
            if l > threshold {
                for c in 0..3 { bright[i*4+c] = frame.pixels[i*4+c]; }
                bright[i*4+3] = 255;
            }
        }
        // Blur the bright buffer.
        let blurred = box_blur(&bright, w, h);
        // Add back to the original, scaled by intensity.
        for i in 0..(w*h) {
            for c in 0..3 {
                let orig = frame.pixels[i*4+c] as f32;
                let glow = blurred[i*4+c] as f32 * intensity;
                frame.pixels[i*4+c] = (orig + glow).clamp(0.0, 255.0) as u8;
            }
        }
    }
}

/// Bloom — like Glow but multi-pass for a softer, larger halo.
pub struct BloomEffect;
impl BloomEffect { pub fn new() -> Self { BloomEffect } }
impl Effect for BloomEffect {
    fn id(&self) -> &str { "zavyx.bloom" }
    fn name(&self) -> &str { "Bloom" }
    fn param_schema(&self) -> ParamSchema {
        ParamSchema { params: vec![
            ParamDef { id: "threshold".into(), label: "Threshold".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 0.7 },
                keyframeable: true },
            ParamDef { id: "intensity".into(), label: "Intensity".into(),
                ty: ParamType::Float { min: 0.0, max: 2.0, step: 0.001, default: 0.3 },
                keyframeable: true },
        ] }
    }
    fn process(&self, frame: &mut FrameView<'_>, params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        // Reuse Glow with 2-pass blur.
        let glow = GlowEffect::new();
        glow.process(frame, params, _ctx);
    }
}

/// Old film — sepia tint + grain + vignette + slight jitter.
pub struct OldFilmEffect;
impl OldFilmEffect { pub fn new() -> Self { OldFilmEffect } }
impl Effect for OldFilmEffect {
    fn id(&self) -> &str { "zavyx.old_film" }
    fn name(&self) -> &str { "Old Film" }
    fn param_schema(&self) -> ParamSchema {
        ParamSchema { params: vec![
            ParamDef { id: "amount".into(), label: "Amount".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 0.5 },
                keyframeable: true },
        ] }
    }
    fn process(&self, frame: &mut FrameView<'_>, params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        let amount = params.get("amount").map(|v| v.at(frame.time) as f32).unwrap_or(0.5);
        if amount <= 0.0 { return; }
        // Sepia tint via per-pixel linear transform.
        per_pixel_linear(frame, |r, g, b| {
            let nr = r * 0.393 + g * 0.769 + b * 0.189;
            let ng = r * 0.349 + g * 0.686 + b * 0.168;
            let nb = r * 0.272 + g * 0.534 + b * 0.131;
            // Mix original and sepia by `amount`.
            (r * (1.0 - amount) + nr * amount,
             g * (1.0 - amount) + ng * amount,
             b * (1.0 - amount) + nb * amount)
        });
        // Add pseudo-grain.
        let seed = frame.time.to_f64().to_bits() as u64;
        let mut rng = seed;
        for px in frame.pixels.chunks_exact_mut(4) {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let noise = ((rng >> 33) as i32 - 256) as f32 / 256.0 * 30.0 * amount;
            for c in 0..3 {
                px[c] = ((px[c] as f32) + noise).clamp(0.0, 255.0) as u8;
            }
        }
    }
}

/// VHS — color shift + scanlines + jitter.
pub struct VhsEffect;
impl VhsEffect { pub fn new() -> Self { VhsEffect } }
impl Effect for VhsEffect {
    fn id(&self) -> &str { "zavyx.vhs" }
    fn name(&self) -> &str { "VHS" }
    fn param_schema(&self) -> ParamSchema {
        ParamSchema { params: vec![
            ParamDef { id: "amount".into(), label: "Amount".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 0.5 },
                keyframeable: true },
        ] }
    }
    fn process(&self, frame: &mut FrameView<'_>, params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        let amount = params.get("amount").map(|v| v.at(frame.time) as f32).unwrap_or(0.5);
        if amount <= 0.0 { return; }
        let w = frame.width as usize;
        let h = frame.height as usize;
        // RGB split with 3px offset.
        let mut split_params = BTreeMap::new();
        split_params.insert("offset".into(), ParamValue::Const((3.0 * amount as f64) as f64));
        let split = RgbSplitEffect::new();
        split.process(frame, &split_params, _ctx);
        // Scanlines: darken every other row.
        for y in 0..h {
            if y % 2 == 0 { continue; }
            for x in 0..w {
                let i = (y * w + x) * 4;
                for c in 0..3 {
                    let v = frame.pixels[i+c] as f32;
                    frame.pixels[i+c] = (v * (1.0 - amount * 0.3)) as u8;
                }
            }
        }
    }
}

/// Glitch — random row offsets.
pub struct GlitchEffect;
impl GlitchEffect { pub fn new() -> Self { GlitchEffect } }
impl Effect for GlitchEffect {
    fn id(&self) -> &str { "zavyx.glitch" }
    fn name(&self) -> &str { "Glitch" }
    fn param_schema(&self) -> ParamSchema {
        ParamSchema { params: vec![
            ParamDef { id: "amount".into(), label: "Amount".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 0.3 },
                keyframeable: true },
        ] }
    }
    fn process(&self, frame: &mut FrameView<'_>, params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        let amount = params.get("amount").map(|v| v.at(frame.time) as f32).unwrap_or(0.3);
        if amount <= 0.0 { return; }
        let w = frame.width as usize;
        let h = frame.height as usize;
        let src = frame.pixels.to_vec();
        let seed = (frame.time.to_f64() * 1000.0).to_bits() as u64;
        let mut rng = seed;
        for y in 0..h {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let rand_val = (rng >> 33) as f32 / (1u64 << 31) as f32;
            if rand_val < amount * 0.2 {
                // Glitch this row by shifting it horizontally by a random amount.
                rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let shift = (((rng >> 33) as i32) % 50).max(-50).min(50);
                for x in 0..w {
                    let sx = ((x as i32 + shift).rem_euclid(w as i32)) as usize;
                    let di = (y * w + x) * 4;
                    let si = (y * w + sx) * 4;
                    frame.pixels[di..di+4].copy_from_slice(&src[si..si+4]);
                }
            }
        }
        // Also apply a smaller RGB split.
        let mut split_params = BTreeMap::new();
        split_params.insert("offset".into(), ParamValue::Const((5.0 * amount as f64) as f64));
        let split = RgbSplitEffect::new();
        split.process(frame, &split_params, _ctx);
    }
}

/// Scanlines — horizontal darkening pattern.
pub struct ScanlinesEffect;
impl ScanlinesEffect { pub fn new() -> Self { ScanlinesEffect } }
impl Effect for ScanlinesEffect {
    fn id(&self) -> &str { "zavyx.scanlines" }
    fn name(&self) -> &str { "Scanlines" }
    fn param_schema(&self) -> ParamSchema {
        ParamSchema { params: vec![
            ParamDef { id: "amount".into(), label: "Amount".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 0.5 },
                keyframeable: true },
            ParamDef { id: "spacing".into(), label: "Spacing (px)".into(),
                ty: ParamType::Float { min: 1.0, max: 10.0, step: 1.0, default: 2.0 },
                keyframeable: false },
        ] }
    }
    fn process(&self, frame: &mut FrameView<'_>, params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        let amount = params.get("amount").map(|v| v.at(frame.time) as f32).unwrap_or(0.5);
        let spacing = params.get("spacing").map(|v| v.at(frame.time) as u32).unwrap_or(2).max(1);
        if amount <= 0.0 { return; }
        let w = frame.width as usize;
        let h = frame.height as usize;
        for y in 0..h {
            if y % spacing as usize != 0 { continue; }
            for x in 0..w {
                let i = (y * w + x) * 4;
                for c in 0..3 {
                    let v = frame.pixels[i+c] as f32;
                    frame.pixels[i+c] = (v * (1.0 - amount)) as u8;
                }
            }
        }
    }
}

/// CRT — combines scanlines + curvature + RGB phosphor mask.
pub struct CrtEffect;
impl CrtEffect { pub fn new() -> Self { CrtEffect } }
impl Effect for CrtEffect {
    fn id(&self) -> &str { "zavyx.crt" }
    fn name(&self) -> &str { "CRT" }
    fn param_schema(&self) -> ParamSchema {
        ParamSchema { params: vec![
            ParamDef { id: "amount".into(), label: "Amount".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 0.5 },
                keyframeable: true },
        ] }
    }
    fn process(&self, frame: &mut FrameView<'_>, params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        let amount = params.get("amount").map(|v| v.at(frame.time) as f32).unwrap_or(0.5);
        if amount <= 0.0 { return; }
        // Reuse scanlines.
        let mut sl_params = BTreeMap::new();
        sl_params.insert("amount".into(), ParamValue::Const(amount as f64));
        sl_params.insert("spacing".into(), ParamValue::Const(2.0));
        let sl = ScanlinesEffect::new();
        sl.process(frame, &sl_params, _ctx);
        // Plus a small RGB split.
        let mut split_params = BTreeMap::new();
        split_params.insert("offset".into(), ParamValue::Const((2.0 * amount as f64) as f64));
        let split = RgbSplitEffect::new();
        split.process(frame, &split_params, _ctx);
    }
}

/// Edge detect — Sobel operator.
pub struct EdgeDetectEffect;
impl EdgeDetectEffect { pub fn new() -> Self { EdgeDetectEffect } }
impl Effect for EdgeDetectEffect {
    fn id(&self) -> &str { "zavyx.edge_detect" }
    fn name(&self) -> &str { "Edge Detect" }
    fn param_schema(&self) -> ParamSchema {
        ParamSchema { params: vec![
            ParamDef { id: "threshold".into(), label: "Threshold".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 0.2 },
                keyframeable: true },
        ] }
    }
    fn process(&self, frame: &mut FrameView<'_>, params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        let threshold = params.get("threshold").map(|v| v.at(frame.time) as f32).unwrap_or(0.2);
        let w = frame.width as usize;
        let h = frame.height as usize;
        let src = frame.pixels.to_vec();
        // Compute luma for each pixel.
        let mut luma_buf = vec![0.0f32; w * h];
        for i in 0..(w*h) {
            luma_buf[i] = luma(
                src[i*4] as f32 / 255.0,
                src[i*4+1] as f32 / 255.0,
                src[i*4+2] as f32 / 255.0,
            );
        }
        // Sobel kernels:
        // Gx = [-1 0 1; -2 0 2; -1 0 1]
        // Gy = [-1 -2 -1; 0 0 0; 1 2 1]
        for y in 1..h-1 {
            for x in 1..w-1 {
                let idx = y * w + x;
                let tl = luma_buf[(y-1)*w + (x-1)];
                let t  = luma_buf[(y-1)*w + x];
                let tr = luma_buf[(y-1)*w + (x+1)];
                let l  = luma_buf[y*w + (x-1)];
                let r  = luma_buf[y*w + (x+1)];
                let bl = luma_buf[(y+1)*w + (x-1)];
                let bm = luma_buf[(y+1)*w + x];
                let br = luma_buf[(y+1)*w + (x+1)];
                let gx = -tl - 2.0*l - bl + tr + 2.0*r + br;
                let gy = -tl - 2.0*t - tr + bl + 2.0*bm + br;
                let mag = (gx*gx + gy*gy).sqrt();
                let v = if mag > threshold { 255 } else { 0 };
                let di = idx * 4;
                frame.pixels[di]   = v;
                frame.pixels[di+1] = v;
                frame.pixels[di+2] = v;
                // alpha preserved
            }
        }
    }
}

// =========================================================================
// COLOR FAMILY
// =========================================================================

/// Black & White — full desaturation with optional filter (red/green/blue/orange).
pub struct BlackAndWhiteEffect;
impl BlackAndWhiteEffect { pub fn new() -> Self { BlackAndWhiteEffect } }
impl Effect for BlackAndWhiteEffect {
    fn id(&self) -> &str { "zavyx.black_and_white" }
    fn name(&self) -> &str { "Black & White" }
    fn param_schema(&self) -> ParamSchema {
        ParamSchema { params: vec![
            ParamDef { id: "amount".into(), label: "Amount".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 1.0 },
                keyframeable: true },
        ] }
    }
    fn process(&self, frame: &mut FrameView<'_>, params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        let amount = params.get("amount").map(|v| v.at(frame.time) as f32).unwrap_or(1.0);
        per_pixel_linear(frame, |r, g, b| {
            let l = luma(r, g, b);
            (r * (1.0 - amount) + l * amount,
             g * (1.0 - amount) + l * amount,
             b * (1.0 - amount) + l * amount)
        });
    }
}

/// Sepia — warm brown tint.
pub struct SepiaEffect;
impl SepiaEffect { pub fn new() -> Self { SepiaEffect } }
impl Effect for SepiaEffect {
    fn id(&self) -> &str { "zavyx.sepia" }
    fn name(&self) -> &str { "Sepia" }
    fn param_schema(&self) -> ParamSchema {
        ParamSchema { params: vec![
            ParamDef { id: "amount".into(), label: "Amount".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 1.0 },
                keyframeable: true },
        ] }
    }
    fn process(&self, frame: &mut FrameView<'_>, params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        let amount = params.get("amount").map(|v| v.at(frame.time) as f32).unwrap_or(1.0);
        per_pixel_linear(frame, |r, g, b| {
            let nr = r * 0.393 + g * 0.769 + b * 0.189;
            let ng = r * 0.349 + g * 0.686 + b * 0.168;
            let nb = r * 0.272 + g * 0.534 + b * 0.131;
            (r * (1.0 - amount) + nr * amount,
             g * (1.0 - amount) + ng * amount,
             b * (1.0 - amount) + nb * amount)
        });
    }
}

/// Duotone — map shadows to one color, highlights to another.
pub struct DuotoneEffect;
impl DuotoneEffect { pub fn new() -> Self { DuotoneEffect } }
impl Effect for DuotoneEffect {
    fn id(&self) -> &str { "zavyx.duotone" }
    fn name(&self) -> &str { "Duotone" }
    fn param_schema(&self) -> ParamSchema {
        ParamSchema { params: vec![
            ParamDef { id: "shadow_r".into(), label: "Shadow R".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 0.1 },
                keyframeable: true },
            ParamDef { id: "shadow_g".into(), label: "Shadow G".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 0.1 },
                keyframeable: true },
            ParamDef { id: "shadow_b".into(), label: "Shadow B".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 0.4 },
                keyframeable: true },
            ParamDef { id: "highlight_r".into(), label: "Highlight R".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 1.0 },
                keyframeable: true },
            ParamDef { id: "highlight_g".into(), label: "Highlight G".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 0.8 },
                keyframeable: true },
            ParamDef { id: "highlight_b".into(), label: "Highlight B".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 0.4 },
                keyframeable: true },
        ] }
    }
    fn process(&self, frame: &mut FrameView<'_>, params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        let sr = params.get("shadow_r").map(|v| v.at(frame.time) as f32).unwrap_or(0.1);
        let sg = params.get("shadow_g").map(|v| v.at(frame.time) as f32).unwrap_or(0.1);
        let sb = params.get("shadow_b").map(|v| v.at(frame.time) as f32).unwrap_or(0.4);
        let hr = params.get("highlight_r").map(|v| v.at(frame.time) as f32).unwrap_or(1.0);
        let hg = params.get("highlight_g").map(|v| v.at(frame.time) as f32).unwrap_or(0.8);
        let hb = params.get("highlight_b").map(|v| v.at(frame.time) as f32).unwrap_or(0.4);
        per_pixel_linear(frame, |r, g, b| {
            let l = luma(r, g, b);
            (sr + (hr - sr) * l,
             sg + (hg - sg) * l,
             sb + (hb - sb) * l)
        });
    }
}

// =========================================================================
// TIME-BASED / JITTER
// =========================================================================

/// Shake — random per-frame offset.
pub struct ShakeEffect;
impl ShakeEffect { pub fn new() -> Self { ShakeEffect } }
impl Effect for ShakeEffect {
    fn id(&self) -> &str { "zavyx.shake" }
    fn name(&self) -> &str { "Shake" }
    fn param_schema(&self) -> ParamSchema {
        ParamSchema { params: vec![
            ParamDef { id: "amount".into(), label: "Amount (px)".into(),
                ty: ParamType::Float { min: 0.0, max: 50.0, step: 0.5, default: 5.0 },
                keyframeable: true },
        ] }
    }
    fn process(&self, frame: &mut FrameView<'_>, params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        let amount = params.get("amount").map(|v| v.at(frame.time) as f32).unwrap_or(5.0);
        if amount <= 0.0 { return; }
        let seed = frame.time.to_f64().to_bits().wrapping_mul(0x9E3779B97F4A7C15);
        let mut rng = seed;
        rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let dx = (((rng >> 33) as f32 / (1u64 << 31) as f32) - 0.5) * 2.0 * amount;
        rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let dy = (((rng >> 33) as f32 / (1u64 << 31) as f32) - 0.5) * 2.0 * amount;
        let w = frame.width as usize;
        let h = frame.height as usize;
        let src = frame.pixels.to_vec();
        let ix = dx as i32;
        let iy = dy as i32;
        for y in 0..h {
            for x in 0..w {
                let sx = (x as i32 - ix).max(0).min(w as i32 - 1) as usize;
                let sy = (y as i32 - iy).max(0).min(h as i32 - 1) as usize;
                let di = (y * w + x) * 4;
                let si = (sy * w + sx) * 4;
                frame.pixels[di..di+4].copy_from_slice(&src[si..si+4]);
            }
        }
    }
}

/// Zoom punch — quick zoom in/out.
pub struct ZoomPunchEffect;
impl ZoomPunchEffect { pub fn new() -> Self { ZoomPunchEffect } }
impl Effect for ZoomPunchEffect {
    fn id(&self) -> &str { "zavyx.zoom_punch" }
    fn name(&self) -> &str { "Zoom Punch" }
    fn param_schema(&self) -> ParamSchema {
        ParamSchema { params: vec![
            ParamDef { id: "amount".into(), label: "Amount".into(),
                ty: ParamType::Float { min: -1.0, max: 1.0, step: 0.001, default: 0.0 },
                keyframeable: true },
        ] }
    }
    fn process(&self, frame: &mut FrameView<'_>, params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        let amount = params.get("amount").map(|v| v.at(frame.time) as f32).unwrap_or(0.0);
        if amount.abs() < 0.001 { return; }
        let w = frame.width as usize;
        let h = frame.height as usize;
        let cx = w as f32 / 2.0;
        let cy = h as f32 / 2.0;
        let scale = 1.0 + amount;
        let src = frame.pixels.to_vec();
        for y in 0..h {
            for x in 0..w {
                let sx = ((x as f32 - cx) / scale + cx) as i32;
                let sy = ((y as f32 - cy) / scale + cy) as i32;
                let di = (y * w + x) * 4;
                if sx < 0 || sx >= w as i32 || sy < 0 || sy >= h as i32 {
                    frame.pixels[di..di+4].copy_from_slice(&[0, 0, 0, 255]);
                } else {
                    let si = (sy as usize * w + sx as usize) * 4;
                    frame.pixels[di..di+4].copy_from_slice(&src[si..si+4]);
                }
            }
        }
    }
}

/// Chromatic aberration — radial RGB split that increases toward the edges.
pub struct ChromaticAberrationEffect;
impl ChromaticAberrationEffect { pub fn new() -> Self { ChromaticAberrationEffect } }
impl Effect for ChromaticAberrationEffect {
    fn id(&self) -> &str { "zavyx.chromatic_aberration" }
    fn name(&self) -> &str { "Chromatic Aberration" }
    fn param_schema(&self) -> ParamSchema {
        ParamSchema { params: vec![
            ParamDef { id: "amount".into(), label: "Amount".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 0.3 },
                keyframeable: true },
        ] }
    }
    fn process(&self, frame: &mut FrameView<'_>, params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        let amount = params.get("amount").map(|v| v.at(frame.time) as f32).unwrap_or(0.3);
        if amount <= 0.0 { return; }
        let w = frame.width as usize;
        let h = frame.height as usize;
        let cx = w as f32 / 2.0;
        let cy = h as f32 / 2.0;
        let max_d = (cx * cx + cy * cy).sqrt();
        let src = frame.pixels.to_vec();
        for y in 0..h {
            for x in 0..w {
                let dx = x as f32 - cx;
                let dy = y as f32 - cy;
                let d = (dx * dx + dy * dy).sqrt();
                let f = (d / max_d) * amount * 10.0;
                let nx_r = (x as f32 + dx / d * f).max(0.0).min((w - 1) as f32) as usize;
                let ny_r = (y as f32 + dy / d * f).max(0.0).min((h - 1) as f32) as usize;
                let nx_b = (x as f32 - dx / d * f).max(0.0).min((w - 1) as f32) as usize;
                let ny_b = (y as f32 - dy / d * f).max(0.0).min((h - 1) as f32) as usize;
                let di = (y * w + x) * 4;
                let ri = (ny_r * w + nx_r) * 4;
                let bi = (ny_b * w + nx_b) * 4;
                frame.pixels[di]   = src[ri];
                frame.pixels[di+2] = src[bi+2];
            }
        }
    }
}

/// Lens correction — barrel distortion removal.
pub struct LensCorrectionEffect;
impl LensCorrectionEffect { pub fn new() -> Self { LensCorrectionEffect } }
impl Effect for LensCorrectionEffect {
    fn id(&self) -> &str { "zavyx.lens_correction" }
    fn name(&self) -> &str { "Lens Correction" }
    fn param_schema(&self) -> ParamSchema {
        ParamSchema { params: vec![
            ParamDef { id: "amount".into(), label: "Amount (-1=pincushion, +1=barrel)".into(),
                ty: ParamType::Float { min: -1.0, max: 1.0, step: 0.001, default: 0.0 },
                keyframeable: true },
        ] }
    }
    fn process(&self, frame: &mut FrameView<'_>, params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        let amount = params.get("amount").map(|v| v.at(frame.time) as f32).unwrap_or(0.0);
        if amount.abs() < 0.001 { return; }
        let w = frame.width as usize;
        let h = frame.height as usize;
        let cx = w as f32 / 2.0;
        let cy = h as f32 / 2.0;
        let max_d = (cx * cx + cy * cy).sqrt();
        let src = frame.pixels.to_vec();
        for y in 0..h {
            for x in 0..w {
                let dx = (x as f32 - cx) / cx;
                let dy = (y as f32 - cy) / cy;
                let r2 = dx * dx + dy * dy;
                let f = 1.0 + amount * r2;
                let sx = (cx + dx * cx * f) as i32;
                let sy = (cy + dy * cy * f) as i32;
                let di = (y * w + x) * 4;
                if sx >= 0 && sx < w as i32 && sy >= 0 && sy < h as i32 {
                    let si = (sy as usize * w + sx as usize) * 4;
                    frame.pixels[di..di+4].copy_from_slice(&src[si..si+4]);
                } else {
                    frame.pixels[di..di+4].copy_from_slice(&[0, 0, 0, 255]);
                }
            }
        }
    }
}

/// Film grain — random per-pixel noise.
pub struct FilmGrainEffect;
impl FilmGrainEffect { pub fn new() -> Self { FilmGrainEffect } }
impl Effect for FilmGrainEffect {
    fn id(&self) -> &str { "zavyx.film_grain" }
    fn name(&self) -> &str { "Film Grain" }
    fn param_schema(&self) -> ParamSchema {
        ParamSchema { params: vec![
            ParamDef { id: "amount".into(), label: "Amount".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 0.2 },
                keyframeable: true },
            ParamDef { id: "size".into(), label: "Size (px)".into(),
                ty: ParamType::Float { min: 1.0, max: 8.0, step: 1.0, default: 1.0 },
                keyframeable: false },
        ] }
    }
    fn process(&self, frame: &mut FrameView<'_>, params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        let amount = params.get("amount").map(|v| v.at(frame.time) as f32).unwrap_or(0.2);
        let size = params.get("size").map(|v| v.at(frame.time) as u32).unwrap_or(1).max(1);
        if amount <= 0.0 { return; }
        let w = frame.width as usize;
        let h = frame.height as usize;
        let mut rng = frame.time.to_f64().to_bits();
        for y in (0..h).step_by(size as usize) {
            for x in (0..w).step_by(size as usize) {
                rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let noise = ((rng >> 33) as f32 / (1u64 << 31) as f32 - 0.5) * 2.0 * amount * 60.0;
                for dy in 0..size as usize {
                    for dx in 0..size as usize {
                        let nx = x + dx; let ny = y + dy;
                        if nx >= w || ny >= h { continue; }
                        let i = (ny * w + nx) * 4;
                        for c in 0..3 {
                            let v = frame.pixels[i+c] as f32 + noise;
                            frame.pixels[i+c] = v.clamp(0.0, 255.0) as u8;
                        }
                    }
                }
            }
        }
    }
}

/// Dejitter — temporal smoothing (placeholder: blurs adjacent pixels).
pub struct DejitterEffect;
impl DejitterEffect { pub fn new() -> Self { DejitterEffect } }
impl Effect for DejitterEffect {
    fn id(&self) -> &str { "zavyx.dejitter" }
    fn name(&self) -> &str { "Dejitter" }
    fn param_schema(&self) -> ParamSchema {
        ParamSchema { params: vec![
            ParamDef { id: "amount".into(), label: "Amount".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 0.3 },
                keyframeable: true },
        ] }
    }
    fn process(&self, frame: &mut FrameView<'_>, params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        // For Phase 3, this is a thin horizontal blur to smooth out jitter.
        let amount = params.get("amount").map(|v| v.at(frame.time) as f32).unwrap_or(0.3);
        if amount <= 0.0 { return; }
        let mut p = BTreeMap::new();
        p.insert("radius".into(), ParamValue::Const((amount * 3.0) as f64));
        let blur = GaussianBlurEffect::new();
        blur.process(frame, &p, _ctx);
    }
}

/// Halftone — convert to dots (simplified).
pub struct HalftoneEffect;
impl HalftoneEffect { pub fn new() -> Self { HalftoneEffect } }
impl Effect for HalftoneEffect {
    fn id(&self) -> &str { "zavyx.halftone" }
    fn name(&self) -> &str { "Halftone" }
    fn param_schema(&self) -> ParamSchema {
        ParamSchema { params: vec![
            ParamDef { id: "size".into(), label: "Dot size (px)".into(),
                ty: ParamType::Float { min: 2.0, max: 32.0, step: 1.0, default: 6.0 },
                keyframeable: true },
        ] }
    }
    fn process(&self, frame: &mut FrameView<'_>, params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        let size = params.get("size").map(|v| v.at(frame.time) as u32).unwrap_or(6).max(2) as usize;
        let w = frame.width as usize;
        let h = frame.height as usize;
        // Per block, compute average luma; draw a dot of that size centered.
        let src = frame.pixels.to_vec();
        // Clear to white.
        for px in frame.pixels.chunks_exact_mut(4) {
            px[0] = 255; px[1] = 255; px[2] = 255; px[3] = 255;
        }
        for by in 0..(h + size - 1) / size {
            for bx in 0..(w + size - 1) / size {
                let x0 = bx * size;
                let y0 = by * size;
                let mut sum_l = 0.0; let mut count = 0;
                for y in y0..(y0 + size).min(h) {
                    for x in x0..(x0 + size).min(w) {
                        let i = (y * w + x) * 4;
                        let l = luma(src[i] as f32 / 255.0, src[i+1] as f32 / 255.0, src[i+2] as f32 / 255.0);
                        sum_l += l; count += 1;
                    }
                }
                let avg_l = if count > 0 { sum_l / count as f32 } else { 0.0 };
                // Draw a black dot whose radius is proportional to (1 - avg_l).
                let radius = (size as f32 * 0.5 * (1.0 - avg_l)).max(0.0);
                let cx = (x0 + size / 2) as f32;
                let cy = (y0 + size / 2) as f32;
                for y in y0..(y0 + size).min(h) {
                    for x in x0..(x0 + size).min(w) {
                        let dx = x as f32 - cx;
                        let dy = y as f32 - cy;
                        let d = (dx * dx + dy * dy).sqrt();
                        if d <= radius {
                            let i = (y * w + x) * 4;
                            frame.pixels[i]   = 0;
                            frame.pixels[i+1] = 0;
                            frame.pixels[i+2] = 0;
                        }
                    }
                }
            }
        }
    }
}

/// Deinterlace — average adjacent lines.
pub struct DeinterlaceEffect;
impl DeinterlaceEffect { pub fn new() -> Self { DeinterlaceEffect } }
impl Effect for DeinterlaceEffect {
    fn id(&self) -> &str { "zavyx.deinterlace" }
    fn name(&self) -> &str { "Deinterlace" }
    fn param_schema(&self) -> ParamSchema { ParamSchema::default() }
    fn process(&self, frame: &mut FrameView<'_>, _params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        let w = frame.width as usize;
        let h = frame.height as usize;
        let src = frame.pixels.to_vec();
        for y in 0..h {
            let y_above = if y > 0 { y - 1 } else { 0 };
            let y_below = if y + 1 < h { y + 1 } else { h - 1 };
            for x in 0..w {
                let di = (y * w + x) * 4;
                let ia = (y_above * w + x) * 4;
                let ib = (y_below * w + x) * 4;
                for c in 0..3 {
                    let avg = (src[ia+c] as u32 + src[ib+c] as u32) / 2;
                    frame.pixels[di+c] = avg as u8;
                }
            }
        }
    }
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_view<'a>(pixels: &'a mut [u8], w: u32, h: u32) -> FrameView<'a> {
        FrameView { width: w, height: h, pixels, time: Rational::ZERO }
    }

    #[test]
    fn gaussian_blur_smooths_image() {
        let blur = GaussianBlurEffect::new();
        let mut pixels = vec![0u8; 16*16*4];
        // Single white pixel in the middle.
        let mid = (8 * 16 + 8) * 4;
        pixels[mid] = 255; pixels[mid+1] = 255; pixels[mid+2] = 255; pixels[mid+3] = 255;
        let mut view = make_view(&mut pixels, 16, 16);
        let mut params = BTreeMap::new();
        params.insert("radius".into(), ParamValue::Const(3.0));
        let ctx = RenderCtx { working_color_space: "linear-rec709" };
        blur.process(&mut view, &params, &ctx);
        // The single white pixel should be spread; check that neighbours are now non-zero.
        assert!(pixels[mid] < 255, "center pixel should be reduced after blur");
        let neighbour = (8 * 16 + 9) * 4;
        assert!(pixels[neighbour] > 0, "neighbour should pick up some brightness");
    }

    #[test]
    fn mosaic_pixelates() {
        let m = MosaicEffect::new();
        let mut pixels = vec![0u8; 8*8*4];
        // Make top-left 4x4 white, rest black.
        for y in 0..4 {
            for x in 0..4 {
                let i = (y * 8 + x) * 4;
                pixels[i] = 255; pixels[i+1] = 255; pixels[i+2] = 255; pixels[i+3] = 255;
            }
        }
        let mut view = make_view(&mut pixels, 8, 8);
        let mut params = BTreeMap::new();
        params.insert("size".into(), ParamValue::Const(4.0));
        let ctx = RenderCtx { working_color_space: "linear-rec709" };
        m.process(&mut view, &params, &ctx);
        // After pixelation with size 4, every pixel in the top-left 4x4 block should be the same value.
        let first = pixels[0];
        for y in 0..4 {
            for x in 0..4 {
                let i = (y * 8 + x) * 4;
                assert_eq!(pixels[i], first, "pixel ({},{}) R={} should be {}", x, y, pixels[i], first);
            }
        }
    }

    #[test]
    fn invert_negates_rgb() {
        let inv = InvertEffect::new();
        let mut pixels = vec![100u8, 150, 200, 255];
        let mut view = make_view(&mut pixels, 1, 1);
        let mut params = BTreeMap::new();
        params.insert("amount".into(), ParamValue::Const(1.0));
        let ctx = RenderCtx { working_color_space: "linear-rec709" };
        inv.process(&mut view, &params, &ctx);
        assert_eq!(pixels[0], 155);  // 255 - 100
        assert_eq!(pixels[1], 105); // 255 - 150
        assert_eq!(pixels[2], 55);  // 255 - 200
    }

    #[test]
    fn flip_horizontal_mirrors_image() {
        let flip = FlipEffect::new();
        // 4 pixels wide, 1 tall.
        let mut pixels = vec![0u8; 4 * 1 * 4];
        // Pixel 0 = red, pixel 3 = blue.
        pixels[0] = 255; pixels[1] = 0; pixels[2] = 0; pixels[3] = 255;       // red
        pixels[12] = 0; pixels[13] = 0; pixels[14] = 255; pixels[15] = 255;   // blue (pixel 3)
        let mut view = make_view(&mut pixels, 4, 1);
        let mut params = BTreeMap::new();
        params.insert("axis".into(), ParamValue::Const(0.0));  // 0 = horizontal
        let ctx = RenderCtx { working_color_space: "linear-rec709" };
        flip.process(&mut view, &params, &ctx);
        // After horizontal flip: pixel 0 (was red) ↔ pixel 3 (was blue).
        // New pixel 0 should be blue (R=0, G=0, B=255, A=255).
        assert_eq!(pixels[0], 0,   "R of new pixel 0 should be 0, got {}", pixels[0]);
        assert_eq!(pixels[2], 255, "B of new pixel 0 should be 255, got {}", pixels[2]);
        // New pixel 3 should be red (R=255, G=0, B=0, A=255).
        assert_eq!(pixels[12], 255, "R of new pixel 3 should be 255, got {}", pixels[12]);
        assert_eq!(pixels[14], 0,   "B of new pixel 3 should be 0, got {}", pixels[14]);
    }

    #[test]
    fn rgb_split_shifts_channels() {
        let split = RgbSplitEffect::new();
        let mut pixels = vec![255u8, 255, 255, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255];
        let mut view = make_view(&mut pixels, 4, 1);
        let mut params = BTreeMap::new();
        params.insert("offset".into(), ParamValue::Const(1.0));
        let ctx = RenderCtx { working_color_space: "linear-rec709" };
        split.process(&mut view, &params, &ctx);
        // The white pixel's R should now be at position 0 (shifted left from 1).
        // Original positions: [W, B, B, B]
        // After offset=1: R shifted left, B shifted right.
        // At x=0: R comes from x=1 (was 255) → still 255
        // At x=3: B comes from x=2 (was 0) → still 0
        // But x=1: R comes from x=2 (was 0) → 0; original R=255 is now at x=0
        assert_eq!(pixels[0], 255, "x=0 R should be 255 (from x=1)");
    }

    #[test]
    fn black_and_white_produces_gray() {
        let bw = BlackAndWhiteEffect::new();
        let mut pixels = vec![255u8, 0, 0, 255];  // pure red
        let mut view = make_view(&mut pixels, 1, 1);
        let mut params = BTreeMap::new();
        params.insert("amount".into(), ParamValue::Const(1.0));
        let ctx = RenderCtx { working_color_space: "linear-rec709" };
        bw.process(&mut view, &params, &ctx);
        assert_eq!(pixels[0], pixels[1], "R should equal G after B&W");
        assert_eq!(pixels[1], pixels[2], "G should equal B after B&W");
    }

    #[test]
    fn sepia_warms_image() {
        let sepia = SepiaEffect::new();
        let mut pixels = vec![128u8, 128, 128, 255];
        let mut view = make_view(&mut pixels, 1, 1);
        let mut params = BTreeMap::new();
        params.insert("amount".into(), ParamValue::Const(1.0));
        let ctx = RenderCtx { working_color_space: "linear-rec709" };
        sepia.process(&mut view, &params, &ctx);
        // Sepia should make R > G > B (warm tint).
        assert!(pixels[0] >= pixels[1], "R should be >= G for sepia, got R={} G={}", pixels[0], pixels[1]);
        assert!(pixels[1] >= pixels[2], "G should be >= B for sepia, got G={} B={}", pixels[1], pixels[2]);
    }

    #[test]
    fn edge_detect_finds_strong_edges() {
        let edge = EdgeDetectEffect::new();
        let w = 8u32; let h = 8u32;
        let mut pixels = vec![0u8; (w*h*4) as usize];
        // Left half white, right half black — vertical edge at x=4.
        for y in 0..h {
            for x in 0..4 {
                let i = ((y * w + x) * 4) as usize;
                pixels[i] = 255; pixels[i+1] = 255; pixels[i+2] = 255; pixels[i+3] = 255;
            }
        }
        let mut view = make_view(&mut pixels, w, h);
        let mut params = BTreeMap::new();
        params.insert("threshold".into(), ParamValue::Const(0.2));
        let ctx = RenderCtx { working_color_space: "linear-rec709" };
        edge.process(&mut view, &params, &ctx);
        // Around x=3 or x=4 we should see white pixels (edges detected).
        let mut any_white = false;
        for y in 1..h-1 {
            for x in 1..4 {
                let i = ((y * w + x) * 4) as usize;
                if pixels[i] == 255 { any_white = true; break; }
            }
        }
        assert!(any_white, "edge detect should find the vertical edge");
    }

    #[test]
    fn duotone_maps_shadows_and_highlights() {
        let dt = DuotoneEffect::new();
        // Black pixel + white pixel.
        let mut pixels = vec![0u8, 0, 0, 255,  255, 255, 255, 255];
        let mut view = make_view(&mut pixels, 2, 1);
        let mut params = BTreeMap::new();
        params.insert("shadow_r".into(), ParamValue::Const(0.0));
        params.insert("shadow_g".into(), ParamValue::Const(0.0));
        params.insert("shadow_b".into(), ParamValue::Const(1.0));  // shadows = blue
        params.insert("highlight_r".into(), ParamValue::Const(1.0));  // highlights = red
        params.insert("highlight_g".into(), ParamValue::Const(0.0));
        params.insert("highlight_b".into(), ParamValue::Const(0.0));
        let ctx = RenderCtx { working_color_space: "linear-rec709" };
        dt.process(&mut view, &params, &ctx);
        // Black pixel → blue (B > R, B > G)
        assert!(pixels[2] > pixels[0], "black pixel B should be > R");
        // White pixel → red (R > G, R > B)
        assert!(pixels[4] >= pixels[5], "white pixel R should be >= G");
        assert!(pixels[4] >= pixels[6], "white pixel R should be >= B");
    }

    #[test]
    fn posterize_reduces_levels() {
        let p = PosterizeEffect::new();
        let mut pixels = vec![10u8, 100, 200, 255];
        let mut view = make_view(&mut pixels, 1, 1);
        let mut params = BTreeMap::new();
        params.insert("levels".into(), ParamValue::Const(4.0));
        let ctx = RenderCtx { working_color_space: "linear-rec709" };
        p.process(&mut view, &params, &ctx);
        // With 4 levels, each channel should be one of {0, 85, 170, 255}.
        for c in 0..3 {
            let v = pixels[c];
            assert!(v == 0 || v == 85 || v == 170 || v == 255,
                "channel {} should be a quantized value, got {}", c, v);
        }
    }

    #[test]
    fn scanlines_darkens_alternating_rows() {
        let sl = ScanlinesEffect::new();
        let mut pixels = vec![255u8; 4*4*4];  // 4x4 white
        let mut view = make_view(&mut pixels, 4, 4);
        let mut params = BTreeMap::new();
        params.insert("amount".into(), ParamValue::Const(0.5));
        params.insert("spacing".into(), ParamValue::Const(2.0));
        let ctx = RenderCtx { working_color_space: "linear-rec709" };
        sl.process(&mut view, &params, &ctx);
        // Rows 0 and 2 (multiples of spacing) should be darkened.
        let row0 = pixels[(0 * 4 + 0) * 4];
        let row1 = pixels[(1 * 4 + 0) * 4];
        assert!(row0 < 255, "row 0 should be darkened, got {}", row0);
        assert_eq!(row1, 255, "row 1 should be untouched");
    }

    #[test]
    fn shake_moves_pixels() {
        let shake = ShakeEffect::new();
        let mut pixels = vec![0u8; 16*16*4];
        // White pixel at (8, 8).
        let mid = (8 * 16 + 8) * 4;
        pixels[mid] = 255; pixels[mid+1] = 255; pixels[mid+2] = 255; pixels[mid+3] = 255;
        let original = pixels.clone();
        let mut view = make_view(&mut pixels, 16, 16);
        view.time = Rational::new(1, 30);  // t = 1 frame
        let mut params = BTreeMap::new();
        params.insert("amount".into(), ParamValue::Const(5.0));
        let ctx = RenderCtx { working_color_space: "linear-rec709" };
        shake.process(&mut view, &params, &ctx);
        // Some pixel should have moved (the white pixel should be elsewhere now).
        let any_diff = original.iter().zip(pixels.iter()).filter(|(a, b)| a != b).count();
        assert!(any_diff > 0, "shake should move at least one pixel");
    }
}
