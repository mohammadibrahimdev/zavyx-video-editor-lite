//! Phase 3 transitions: 20 additional transitions beyond the Phase 2 set.
//!
//! Categories:
//! - Shape wipes: Circle, Square, Diamond, Star, Heart
//! - Directional slides/pushes: PushLeft/Right/Up/Down, SlideLeft/Right
//! - Stylized: Spin, Blur, Glitch, Flash, CrossZoom
//! - Iris: IrisOpen, IrisClose
//! - Page: PageCurl
//! - Squeeze

use std::collections::BTreeMap;
use zavyx_ve_core::effect::{Transition, FrameView, FrameViewRef, RenderCtx, ParamSchema, ParamDef, ParamType, ParamValue};

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    let a = a as f32; let b = b as f32;
    (a + (b - a) * t).round().clamp(0.0, 255.0) as u8
}

// =========================================================================
// SPIN — `to` clip rotates in from a tiny angle to 0.
// =========================================================================

pub struct Spin;
impl Spin { pub fn new() -> Self { Spin } }
impl Transition for Spin {
    fn id(&self) -> &str { "zavyx.transition.spin" }
    fn name(&self) -> &str { "Spin" }
    fn param_schema(&self) -> ParamSchema { ParamSchema::default() }
    fn process(&self, from: &mut FrameView<'_>, to: FrameViewRef<'_>, alpha: f64,
               _params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        let w = from.width as f32;
        let h = from.height as f32;
        let cx = w / 2.0;
        let cy = h / 2.0;
        let scale = (alpha as f32).max(0.001);
        let angle = (1.0 - alpha as f32) * std::f32::consts::PI;  // 180deg at start, 0 at end
        let cos_a = angle.cos();
        let sin_a = angle.sin();
        let src_from = from.pixels.to_vec();
        for y in 0..from.height {
            for x in 0..from.width {
                let dx = x as f32 - cx;
                let dy = y as f32 - cy;
                // Rotate by -angle and scale by 1/scale (inverse transform).
                let nx = (dx * cos_a + dy * sin_a) / scale + cx;
                let ny = (-dx * sin_a + dy * cos_a) / scale + cy;
                let di = ((y * from.width + x) * 4) as usize;
                if nx >= 0.0 && nx < w && ny >= 0.0 && ny < h {
                    let tx = nx as u32;
                    let ty = ny as u32;
                    let ti = ((ty * to.width + tx) * 4) as usize;
                    // Blend with from based on alpha.
                    let blend = alpha as f32;
                    for c in 0..4 {
                        from.pixels[di+c] = lerp_u8(src_from[di+c], to.pixels[ti+c], blend);
                    }
                } else {
                    // Outside to-clip's rotated rect: keep from.
                    // (already there — from.pixels preserves original)
                }
            }
        }
    }
}

// =========================================================================
// BLUR TRANSITION — dissolve through a blur.
// =========================================================================

pub struct BlurTransition;
impl BlurTransition { pub fn new() -> Self { BlurTransition } }
impl Transition for BlurTransition {
    fn id(&self) -> &str { "zavyx.transition.blur" }
    fn name(&self) -> &str { "Blur" }
    fn param_schema(&self) -> ParamSchema { ParamSchema::default() }
    fn process(&self, from: &mut FrameView<'_>, to: FrameViewRef<'_>, alpha: f64,
               _params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        // Phase 3 simple version: cross-fade with progressively less blur.
        // We use a box blur approximation by averaging neighbors.
        let w = from.width as usize;
        let h = from.height as usize;
        let radius = ((1.0 - alpha.abs()) * 5.0) as usize;
        // Blend from -> to by alpha, then apply box blur on the result.
        let a = alpha as f32;
        for i in 0..(w*h) {
            for c in 0..4 {
                let v = lerp_u8(from.pixels[i*4+c], to.pixels[i*4+c], a);
                from.pixels[i*4+c] = v;
            }
        }
        // Box blur with `radius`.
        if radius > 0 {
            let src = from.pixels.to_vec();
            for y in 0..h {
                for x in 0..w {
                    let mut sum = [0u32; 4]; let mut count = 0u32;
                    for dy in -(radius as i32)..=(radius as i32) {
                        let ny = (y as i32 + dy).max(0).min(h as i32 - 1) as usize;
                        for dx in -(radius as i32)..=(radius as i32) {
                            let nx = (x as i32 + dx).max(0).min(w as i32 - 1) as usize;
                            let i = (ny * w + nx) * 4;
                            sum[0] += src[i] as u32; sum[1] += src[i+1] as u32; sum[2] += src[i+2] as u32; sum[3] += src[i+3] as u32;
                            count += 1;
                        }
                    }
                    let di = (y * w + x) * 4;
                    from.pixels[di]   = (sum[0] / count) as u8;
                    from.pixels[di+1] = (sum[1] / count) as u8;
                    from.pixels[di+2] = (sum[2] / count) as u8;
                    from.pixels[di+3] = (sum[3] / count) as u8;
                }
            }
        }
    }
}

// =========================================================================
// GLITCH TRANSITION — random row shifts + RGB split.
// =========================================================================

pub struct GlitchTransition;
impl GlitchTransition { pub fn new() -> Self { GlitchTransition } }
impl Transition for GlitchTransition {
    fn id(&self) -> &str { "zavyx.transition.glitch" }
    fn name(&self) -> &str { "Glitch" }
    fn param_schema(&self) -> ParamSchema { ParamSchema::default() }
    fn process(&self, from: &mut FrameView<'_>, to: FrameViewRef<'_>, alpha: f64,
               _params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        let w = from.width as usize;
        let h = from.height as usize;
        let a = alpha as f32;
        // Cross-fade.
        for i in 0..(w*h) {
            for c in 0..4 {
                from.pixels[i*4+c] = lerp_u8(from.pixels[i*4+c], to.pixels[i*4+c], a);
            }
        }
        // RGB split scaling with alpha.
        let offset = (a * 8.0) as i32;
        let src = from.pixels.to_vec();
        for y in 0..h {
            for x in 0..w {
                let di = (y * w + x) * 4;
                let r_x = (x as i32 - offset).max(0).min(w as i32 - 1) as usize;
                let b_x = (x as i32 + offset).max(0).min(w as i32 - 1) as usize;
                let r_i = (y * w + r_x) * 4;
                let b_i = (y * w + b_x) * 4;
                from.pixels[di]   = src[r_i];
                from.pixels[di+2] = src[b_i+2];
            }
        }
    }
}

// =========================================================================
// SHAPE WIPES — wipe by various shapes
// =========================================================================

/// Wipe in a growing circle from the center.
pub struct WipeCircle;
impl WipeCircle { pub fn new() -> Self { WipeCircle } }
impl Transition for WipeCircle {
    fn id(&self) -> &str { "zavyx.transition.wipe_circle" }
    fn name(&self) -> &str { "Wipe Circle" }
    fn param_schema(&self) -> ParamSchema { ParamSchema::default() }
    fn process(&self, from: &mut FrameView<'_>, to: FrameViewRef<'_>, alpha: f64,
               _params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        let w = from.width as f32;
        let h = from.height as f32;
        let cx = w / 2.0;
        let cy = h / 2.0;
        let max_d = (cx * cx + cy * cy).sqrt();
        let radius = (alpha as f32) * max_d;
        for y in 0..from.height {
            for x in 0..from.width {
                let dx = x as f32 - cx;
                let dy = y as f32 - cy;
                let d = (dx * dx + dy * dy).sqrt();
                let di = ((y * from.width + x) * 4) as usize;
                if d <= radius {
                    let ti = ((y * to.width + x) * 4) as usize;
                    from.pixels[di..di+4].copy_from_slice(&to.pixels[ti..ti+4]);
                }
            }
        }
    }
}

/// Wipe in a growing square from the center.
pub struct WipeSquare;
impl WipeSquare { pub fn new() -> Self { WipeSquare } }
impl Transition for WipeSquare {
    fn id(&self) -> &str { "zavyx.transition.wipe_square" }
    fn name(&self) -> &str { "Wipe Square" }
    fn param_schema(&self) -> ParamSchema { ParamSchema::default() }
    fn process(&self, from: &mut FrameView<'_>, to: FrameViewRef<'_>, alpha: f64,
               _params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        let w = from.width as f32;
        let h = from.height as f32;
        let cx = w / 2.0;
        let cy = h / 2.0;
        let half = (alpha as f32) * (w.max(h) / 2.0);
        for y in 0..from.height {
            for x in 0..from.width {
                let dx = (x as f32 - cx).abs();
                let dy = (y as f32 - cy).abs();
                let di = ((y * from.width + x) * 4) as usize;
                if dx <= half && dy <= half {
                    let ti = ((y * to.width + x) * 4) as usize;
                    from.pixels[di..di+4].copy_from_slice(&to.pixels[ti..ti+4]);
                }
            }
        }
    }
}

/// Wipe in a growing diamond from the center.
pub struct WipeDiamond;
impl WipeDiamond { pub fn new() -> Self { WipeDiamond } }
impl Transition for WipeDiamond {
    fn id(&self) -> &str { "zavyx.transition.wipe_diamond" }
    fn name(&self) -> &str { "Wipe Diamond" }
    fn param_schema(&self) -> ParamSchema { ParamSchema::default() }
    fn process(&self, from: &mut FrameView<'_>, to: FrameViewRef<'_>, alpha: f64,
               _params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        let w = from.width as f32;
        let h = from.height as f32;
        let cx = w / 2.0;
        let cy = h / 2.0;
        let max_d = (cx + cy).abs();
        let boundary = (alpha as f32) * max_d;
        for y in 0..from.height {
            for x in 0..from.width {
                let dx = (x as f32 - cx).abs();
                let dy = (y as f32 - cy).abs();
                let di = ((y * from.width + x) * 4) as usize;
                if dx + dy <= boundary {
                    let ti = ((y * to.width + x) * 4) as usize;
                    from.pixels[di..di+4].copy_from_slice(&to.pixels[ti..ti+4]);
                }
            }
        }
    }
}

/// Wipe in a growing 5-pointed star.
pub struct WipeStar;
impl WipeStar { pub fn new() -> Self { WipeStar } }
impl Transition for WipeStar {
    fn id(&self) -> &str { "zavyx.transition.wipe_star" }
    fn name(&self) -> &str { "Wipe Star" }
    fn param_schema(&self) -> ParamSchema { ParamSchema::default() }
    fn process(&self, from: &mut FrameView<'_>, to: FrameViewRef<'_>, alpha: f64,
               _params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        // Simplified: use a polar-coordinate check on a 5-pointed star.
        let w = from.width as f32;
        let h = from.height as f32;
        let cx = w / 2.0;
        let cy = h / 2.0;
        let max_d = (cx * cx + cy * cy).sqrt();
        let scale = (alpha as f32) * 1.5;  // star grows with alpha
        for y in 0..from.height {
            for x in 0..from.width {
                let dx = x as f32 - cx;
                let dy = y as f32 - cy;
                let r = (dx * dx + dy * dy).sqrt() / max_d;
                let theta = dy.atan2(dx);
                // Star radius varies with angle: 5 spikes.
                let star_r = 0.5 + 0.5 * (5.0 * theta).cos().abs();
                let di = ((y * from.width + x) * 4) as usize;
                if r * scale <= star_r {
                    let ti = ((y * to.width + x) * 4) as usize;
                    from.pixels[di..di+4].copy_from_slice(&to.pixels[ti..ti+4]);
                }
            }
        }
    }
}

/// Wipe in a heart shape.
pub struct WipeHeart;
impl WipeHeart { pub fn new() -> Self { WipeHeart } }
impl Transition for WipeHeart {
    fn id(&self) -> &str { "zavyx.transition.wipe_heart" }
    fn name(&self) -> &str { "Wipe Heart" }
    fn param_schema(&self) -> ParamSchema { ParamSchema::default() }
    fn process(&self, from: &mut FrameView<'_>, to: FrameViewRef<'_>, alpha: f64,
               _params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        // Heart curve: x = 16 sin^3(t), y = -(13cos(t) - 5cos(2t) - 2cos(3t) - cos(4t))
        let w = from.width as f32;
        let h = from.height as f32;
        let cx = w / 2.0;
        let cy = h / 2.0;
        let scale = (alpha as f32) * (w.min(h) / 40.0);
        for y in 0..from.height {
            for x in 0..from.width {
                let dx = (x as f32 - cx) / scale;
                let dy = -(y as f32 - cy) / scale;  // flip Y (heart points down)
                // Implicit heart equation: (x^2 + y^2 - 1)^3 - x^2*y^3 <= 0
                let x2 = dx * dx;
                let y2 = dy * dy;
                let lhs = (x2 + y2 - 1.0).powi(3) - x2 * dy * dy * dy;
                let di = ((y * from.width + x) * 4) as usize;
                if lhs <= 0.0 {
                    let ti = ((y * to.width + x) * 4) as usize;
                    from.pixels[di..di+4].copy_from_slice(&to.pixels[ti..ti+4]);
                }
            }
        }
    }
}

// =========================================================================
// DIRECTIONAL PUSHES
// =========================================================================

fn push_dir(from: &mut FrameView<'_>, to: FrameViewRef<'_>, alpha: f64, dx: i32, dy: i32) {
    let w = from.width as i32;
    let h = from.height as i32;
    let offset_x = (alpha * w as f64) as i32 * dx.signum();
    let offset_y = (alpha * h as f64) as i32 * dy.signum();
    let from_buf = from.pixels.to_vec();
    for y in 0..h {
        for x in 0..w {
            let from_x = x + offset_x * dx.signum().max(0) - (if dx < 0 { offset_x } else { 0 });
            let from_y = y + offset_y * dy.signum().max(0) - (if dy < 0 { offset_y } else { 0 });
            let to_x = x - (w - offset_x).max(0) * dx.signum().max(0) + (if dx > 0 { w - offset_x } else { 0 });
            let to_y = y - (h - offset_y).max(0) * dy.signum().max(0) + (if dy > 0 { h - offset_y } else { 0 });
            // Simpler approach: at each pixel, if x < offset use `to` clip's pixel from `x + (w - offset)`,
            // else use `from` clip's pixel from `x - offset`.
            let _ = (from_x, from_y, to_x, to_y);
            let di = ((y as u32 * from.width + x as u32) * 4) as usize;
            let use_to_x = x - (w - offset_x);
            let use_to_y = y - (h - offset_y);
            if dx != 0 && dy == 0 {
                if use_to_x >= 0 && use_to_x < w {
                    let ti = ((y as u32 * to.width + use_to_x as u32) * 4) as usize;
                    from.pixels[di..di+4].copy_from_slice(&to.pixels[ti..ti+4]);
                } else {
                    let fx = x - offset_x;
                    if fx >= 0 && fx < w {
                        let fi = ((y as u32 * from.width + fx as u32) * 4) as usize;
                        from.pixels[di..di+4].copy_from_slice(&from_buf[fi..fi+4]);
                    } else {
                        from.pixels[di..di+4].copy_from_slice(&[0,0,0,255]);
                    }
                }
            } else if dy != 0 && dx == 0 {
                if use_to_y >= 0 && use_to_y < h {
                    let ti = ((use_to_y as u32 * to.width + x as u32) * 4) as usize;
                    from.pixels[di..di+4].copy_from_slice(&to.pixels[ti..ti+4]);
                } else {
                    let fy = y - offset_y;
                    if fy >= 0 && fy < h {
                        let fi = ((fy as u32 * from.width + x as u32) * 4) as usize;
                        from.pixels[di..di+4].copy_from_slice(&from_buf[fi..fi+4]);
                    } else {
                        from.pixels[di..di+4].copy_from_slice(&[0,0,0,255]);
                    }
                }
            }
        }
    }
}

pub struct PushLeft;
impl PushLeft { pub fn new() -> Self { PushLeft } }
impl Transition for PushLeft {
    fn id(&self) -> &str { "zavyx.transition.push_left" }
    fn name(&self) -> &str { "Push Left" }
    fn param_schema(&self) -> ParamSchema { ParamSchema::default() }
    fn process(&self, from: &mut FrameView<'_>, to: FrameViewRef<'_>, alpha: f64,
               p: &BTreeMap<String, ParamValue>, c: &RenderCtx<'_>) {
        push_dir(from, to, alpha, -1, 0);
    }
}

pub struct PushRight;
impl PushRight { pub fn new() -> Self { PushRight } }
impl Transition for PushRight {
    fn id(&self) -> &str { "zavyx.transition.push_right" }
    fn name(&self) -> &str { "Push Right" }
    fn param_schema(&self) -> ParamSchema { ParamSchema::default() }
    fn process(&self, from: &mut FrameView<'_>, to: FrameViewRef<'_>, alpha: f64,
               p: &BTreeMap<String, ParamValue>, c: &RenderCtx<'_>) {
        push_dir(from, to, alpha, 1, 0);
    }
}

pub struct PushUp;
impl PushUp { pub fn new() -> Self { PushUp } }
impl Transition for PushUp {
    fn id(&self) -> &str { "zavyx.transition.push_up" }
    fn name(&self) -> &str { "Push Up" }
    fn param_schema(&self) -> ParamSchema { ParamSchema::default() }
    fn process(&self, from: &mut FrameView<'_>, to: FrameViewRef<'_>, alpha: f64,
               p: &BTreeMap<String, ParamValue>, c: &RenderCtx<'_>) {
        push_dir(from, to, alpha, 0, -1);
    }
}

pub struct PushDown;
impl PushDown { pub fn new() -> Self { PushDown } }
impl Transition for PushDown {
    fn id(&self) -> &str { "zavyx.transition.push_down" }
    fn name(&self) -> &str { "Push Down" }
    fn param_schema(&self) -> ParamSchema { ParamSchema::default() }
    fn process(&self, from: &mut FrameView<'_>, to: FrameViewRef<'_>, alpha: f64,
               p: &BTreeMap<String, ParamValue>, c: &RenderCtx<'_>) {
        push_dir(from, to, alpha, 0, 1);
    }
}

// =========================================================================
// DIRECTIONAL SLIDES (to clip slides in over from)
// =========================================================================

fn slide_dir(from: &mut FrameView<'_>, to: FrameViewRef<'_>, alpha: f64, dx: i32, dy: i32) {
    let w = from.width as i32;
    let h = from.height as i32;
    let offset_x = ((1.0 - alpha) * w as f64) as i32 * dx.signum();
    let offset_y = ((1.0 - alpha) * h as f64) as i32 * dy.signum();
    let from_buf = from.pixels.to_vec();
    for y in 0..h {
        for x in 0..w {
            let di = ((y as u32 * from.width + x as u32) * 4) as usize;
            let to_x = x - offset_x;
            let to_y = y - offset_y;
            if to_x >= 0 && to_x < w && to_y >= 0 && to_y < h {
                let ti = ((to_y as u32 * to.width + to_x as u32) * 4) as usize;
                from.pixels[di..di+4].copy_from_slice(&to.pixels[ti..ti+4]);
            } else {
                // Out of to-clip range: keep from clip.
                let fi = ((y as u32 * from.width + x as u32) * 4) as usize;
                from.pixels[di..di+4].copy_from_slice(&from_buf[fi..fi+4]);
            }
        }
    }
}

pub struct SlideLeft;
impl SlideLeft { pub fn new() -> Self { SlideLeft } }
impl Transition for SlideLeft {
    fn id(&self) -> &str { "zavyx.transition.slide_left" }
    fn name(&self) -> &str { "Slide Left" }
    fn param_schema(&self) -> ParamSchema { ParamSchema::default() }
    fn process(&self, from: &mut FrameView<'_>, to: FrameViewRef<'_>, alpha: f64,
               p: &BTreeMap<String, ParamValue>, c: &RenderCtx<'_>) {
        slide_dir(from, to, alpha, -1, 0);
    }
}

pub struct SlideRight;
impl SlideRight { pub fn new() -> Self { SlideRight } }
impl Transition for SlideRight {
    fn id(&self) -> &str { "zavyx.transition.slide_right" }
    fn name(&self) -> &str { "Slide Right" }
    fn param_schema(&self) -> ParamSchema { ParamSchema::default() }
    fn process(&self, from: &mut FrameView<'_>, to: FrameViewRef<'_>, alpha: f64,
               p: &BTreeMap<String, ParamValue>, c: &RenderCtx<'_>) {
        slide_dir(from, to, alpha, 1, 0);
    }
}

// =========================================================================
// SQUEEZE — `to` clip compresses horizontally then expands to fill.
// =========================================================================

pub struct Squeeze;
impl Squeeze { pub fn new() -> Self { Squeeze } }
impl Transition for Squeeze {
    fn id(&self) -> &str { "zavyx.transition.squeeze" }
    fn name(&self) -> &str { "Squeeze" }
    fn param_schema(&self) -> ParamSchema { ParamSchema::default() }
    fn process(&self, from: &mut FrameView<'_>, to: FrameViewRef<'_>, alpha: f64,
               _params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        let w = from.width;
        let h = from.height;
        // At alpha=0: to clip is squeezed to width 0 (invisible).
        // At alpha=0.5: to clip is squeezed to half width centered.
        // At alpha=1: to clip is full width.
        let to_width = ((alpha * w as f64) as u32).max(1);
        let start_x = (w - to_width) / 2;
        let from_buf = from.pixels.to_vec();
        for y in 0..h {
            for x in 0..w {
                let di = ((y * w + x) * 4) as usize;
                if x >= start_x && x < start_x + to_width {
                    // Map x back to the to-clip's full width.
                    let tx = ((x - start_x) as f64 / to_width as f64 * w as f64) as u32;
                    if tx < to.width {
                        let ti = ((y * to.width + tx) * 4) as usize;
                        from.pixels[di..di+4].copy_from_slice(&to.pixels[ti..ti+4]);
                    }
                } else {
                    // Keep from clip pixel.
                    let fi = ((y * w + x) * 4) as usize;
                    from.pixels[di..di+4].copy_from_slice(&from_buf[fi..fi+4]);
                }
            }
        }
    }
}

// =========================================================================
// FLASH — quick white flash transition.
// =========================================================================

pub struct Flash;
impl Flash { pub fn new() -> Self { Flash } }
impl Transition for Flash {
    fn id(&self) -> &str { "zavyx.transition.flash" }
    fn name(&self) -> &str { "Flash" }
    fn param_schema(&self) -> ParamSchema { ParamSchema::default() }
    fn process(&self, from: &mut FrameView<'_>, to: FrameViewRef<'_>, alpha: f64,
               _params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        // Flash brightness peaks at alpha=0.5.
        let flash = (1.0 - (alpha - 0.5).abs() * 2.0).max(0.0) as f32;
        let a = alpha as f32;
        // Cross-fade from -> to blended with flash brightness.
        for i in 0..(from.width as usize * from.height as usize) {
            for c in 0..3 {
                let v = lerp_u8(from.pixels[i*4+c], to.pixels[i*4+c], a);
                from.pixels[i*4+c] = ((v as f32) * (1.0 - flash) + 255.0 * flash) as u8;
            }
        }
    }
}

// =========================================================================
// IRIS — circular reveal/hide
// =========================================================================

pub struct IrisOpen;
impl IrisOpen { pub fn new() -> Self { IrisOpen } }
impl Transition for IrisOpen {
    fn id(&self) -> &str { "zavyx.transition.iris_open" }
    fn name(&self) -> &str { "Iris Open" }
    fn param_schema(&self) -> ParamSchema { ParamSchema::default() }
    fn process(&self, from: &mut FrameView<'_>, to: FrameViewRef<'_>, alpha: f64,
               _params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        // Same as WipeCircle (the iris is conceptually a circle).
        let wipe = WipeCircle::new();
        wipe.process(from, to, alpha, &BTreeMap::new(), _ctx);
    }
}

pub struct IrisClose;
impl IrisClose { pub fn new() -> Self { IrisClose } }
impl Transition for IrisClose {
    fn id(&self) -> &str { "zavyx.transition.iris_close" }
    fn name(&self) -> &str { "Iris Close" }
    fn param_schema(&self) -> ParamSchema { ParamSchema::default() }
    fn process(&self, from: &mut FrameView<'_>, to: FrameViewRef<'_>, alpha: f64,
               _params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        // Reverse: at alpha=0 to is fully visible, at alpha=1 to is hidden.
        let wipe = WipeCircle::new();
        wipe.process(from, to, 1.0 - alpha, &BTreeMap::new(), _ctx);
    }
}

// =========================================================================
// PAGE CURL — `to` clip curls in from the top-left corner (simplified).
// =========================================================================

pub struct PageCurl;
impl PageCurl { pub fn new() -> Self { PageCurl } }
impl Transition for PageCurl {
    fn id(&self) -> &str { "zavyx.transition.page_curl" }
    fn name(&self) -> &str { "Page Curl" }
    fn param_schema(&self) -> ParamSchema { ParamSchema::default() }
    fn process(&self, from: &mut FrameView<'_>, to: FrameViewRef<'_>, alpha: f64,
               _params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        // Simplified: the to clip rolls in from the top-left as a diagonal wipe.
        let w = from.width as f32;
        let h = from.height as f32;
        let boundary = (alpha as f32) * (w + h);
        for y in 0..from.height {
            for x in 0..from.width {
                let di = ((y * from.width + x) * 4) as usize;
                let progress = (x as f32 + y as f32);
                if progress <= boundary {
                    let ti = ((y * to.width + x) * 4) as usize;
                    from.pixels[di..di+4].copy_from_slice(&to.pixels[ti..ti+4]);
                }
            }
        }
    }
}

// =========================================================================
// CROSS ZOOM — zoom in on `from` while zooming out on `to`.
// =========================================================================

pub struct CrossZoom;
impl CrossZoom { pub fn new() -> Self { CrossZoom } }
impl Transition for CrossZoom {
    fn id(&self) -> &str { "zavyx.transition.cross_zoom" }
    fn name(&self) -> &str { "Cross Zoom" }
    fn param_schema(&self) -> ParamSchema { ParamSchema::default() }
    fn process(&self, from: &mut FrameView<'_>, to: FrameViewRef<'_>, alpha: f64,
               _params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        let w = from.width as f32;
        let h = from.height as f32;
        let cx = w / 2.0;
        let cy = h / 2.0;
        let a = alpha as f32;
        // from zooms in (scale > 1) as alpha goes 0 -> 0.5; to zooms in as alpha goes 0.5 -> 1.
        let from_scale = if a < 0.5 { 1.0 + a * 2.0 } else { 2.0 };
        let to_scale = if a < 0.5 { 0.5 } else { 0.5 + (a - 0.5) * 2.0 * 0.5 + 0.5 };
        let from_buf = from.pixels.to_vec();
        for y in 0..from.height {
            for x in 0..from.width {
                let dx = x as f32 - cx;
                let dy = y as f32 - cy;
                let di = ((y * from.width + x) * 4) as usize;
                if a < 0.5 {
                    // Show from, zoomed.
                    let sx = (cx + dx / from_scale) as i32;
                    let sy = (cy + dy / from_scale) as i32;
                    if sx >= 0 && sx < w as i32 && sy >= 0 && sy < h as i32 {
                        let fi = ((sy as u32 * from.width + sx as u32) * 4) as usize;
                        from.pixels[di..di+4].copy_from_slice(&from_buf[fi..fi+4]);
                    } else {
                        from.pixels[di..di+4].copy_from_slice(&[0, 0, 0, 255]);
                    }
                } else {
                    // Show to, scaling up from 0.5 to 1.0.
                    let sx = (cx + dx / to_scale) as i32;
                    let sy = (cy + dy / to_scale) as i32;
                    if sx >= 0 && sx < w as i32 && sy >= 0 && sy < h as i32 {
                        let ti = ((sy as u32 * to.width + sx as u32) * 4) as usize;
                        from.pixels[di..di+4].copy_from_slice(&to.pixels[ti..ti+4]);
                    } else {
                        from.pixels[di..di+4].copy_from_slice(&[0, 0, 0, 255]);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zavyx_ve_core::Rational;

    #[test]
    fn spin_does_not_panic() {
        let spin = Spin::new();
        let mut from = vec![100u8; 4*4*4];
        let to = vec![200u8; 4*4*4];
        let mut from_view = FrameView {
            width: 4, height: 4, pixels: &mut from,
            time: Rational::ZERO,
        };
        let to_view = FrameViewRef {
            width: 4, height: 4, pixels: &to,
            time: Rational::ZERO,
        };
        let params = BTreeMap::new();
        let ctx = RenderCtx { working_color_space: "linear-rec709" };
        spin.process(&mut from_view, to_view, 0.5, &params, &ctx);
        // Should not panic.
    }

    #[test]
    fn wipe_circle_at_full_alpha_covers_to() {
        let wipe = WipeCircle::new();
        let mut from = vec![0u8; 8*8*4];
        let to = vec![255u8; 8*8*4];
        let mut from_view = FrameView {
            width: 8, height: 8, pixels: &mut from,
            time: Rational::ZERO,
        };
        let to_view = FrameViewRef {
            width: 8, height: 8, pixels: &to,
            time: Rational::ZERO,
        };
        let params = BTreeMap::new();
        let ctx = RenderCtx { working_color_space: "linear-rec709" };
        wipe.process(&mut from_view, to_view, 1.0, &params, &ctx);
        // At alpha=1, the circle radius equals the corner distance, so all pixels should be `to`.
        for px in from.chunks_exact(4) {
            assert_eq!(px[0], 255);
        }
    }

    #[test]
    fn flash_at_midpoint_is_white() {
        let flash = Flash::new();
        let mut from = vec![0u8; 4*4*4];
        let to = vec![0u8; 4*4*4];
        let mut from_view = FrameView {
            width: 4, height: 4, pixels: &mut from,
            time: Rational::ZERO,
        };
        let to_view = FrameViewRef {
            width: 4, height: 4, pixels: &to,
            time: Rational::ZERO,
        };
        let params = BTreeMap::new();
        let ctx = RenderCtx { working_color_space: "linear-rec709" };
        flash.process(&mut from_view, to_view, 0.5, &params, &ctx);
        // At alpha=0.5 the flash peaks at full white.
        for px in from.chunks_exact(4) {
            assert_eq!(px[0], 255, "expected white at flash midpoint");
        }
    }

    #[test]
    fn heart_wipe_at_full_alpha_covers_heart_region() {
        let heart = WipeHeart::new();
        let mut from = vec![0u8; 64*64*4];
        let to = vec![255u8; 64*64*4];
        let mut from_view = FrameView {
            width: 64, height: 64, pixels: &mut from,
            time: Rational::ZERO,
        };
        let to_view = FrameViewRef {
            width: 64, height: 64, pixels: &to,
            time: Rational::ZERO,
        };
        let params = BTreeMap::new();
        let ctx = RenderCtx { working_color_space: "linear-rec709" };
        heart.process(&mut from_view, to_view, 1.0, &params, &ctx);
        // At alpha=1 the heart should cover at least some pixels (heart shape
        // is smaller than the bounding rect, so >0 but <100%).
        let mut white_count = 0;
        for px in from.chunks_exact(4) {
            if px[0] == 255 { white_count += 1; }
        }
        assert!(white_count > 0, "heart should cover at least some pixels at full alpha, got {}/{}", white_count, 64*64);
    }

    #[test]
    fn page_curl_at_full_alpha_covers_to() {
        let pc = PageCurl::new();
        let mut from = vec![0u8; 4*4*4];
        let to = vec![255u8; 4*4*4];
        let mut from_view = FrameView {
            width: 4, height: 4, pixels: &mut from,
            time: Rational::ZERO,
        };
        let to_view = FrameViewRef {
            width: 4, height: 4, pixels: &to,
            time: Rational::ZERO,
        };
        let params = BTreeMap::new();
        let ctx = RenderCtx { working_color_space: "linear-rec709" };
        pc.process(&mut from_view, to_view, 1.0, &params, &ctx);
        for px in from.chunks_exact(4) {
            assert_eq!(px[0], 255);
        }
    }
}
