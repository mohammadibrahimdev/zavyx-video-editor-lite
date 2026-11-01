//! Built-in transitions.
//!
//! A transition blends two clips' frames by an alpha in [0, 1].
//! - alpha = 0: fully `from` clip
//! - alpha = 1: fully `to` clip
//!
//! Phase 2 ships 10 transitions:
//! 1. Dissolve (cross-fade)
//! 2. Dip to Black (fade through black)
//! 3. Dip to White (fade through white)
//! 4. Wipe Left
//! 5. Wipe Right
//! 6. Wipe Up
//! 7. Wipe Down
//! 8. Slide (to clip slides in over from clip)
//! 9. Push (to clip pushes from clip off-screen)
//! 10. Zoom (to clip zooms in from center)
//!
//! Phase 3 adds 20 more transitions in the `extra` submodule.

pub mod extra;

use std::collections::BTreeMap;
use zavyx_ve_core::effect::{Transition, FrameView, FrameViewRef, RenderCtx, ParamSchema, ParamDef, ParamType, ParamValue};

/// Helper: lerp a u8 by an f32 weight in [0, 1].
fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    let a = a as f32; let b = b as f32;
    (a + (b - a) * t).round().clamp(0.0, 255.0) as u8
}

/// Dissolve: linear cross-fade between from and to.
pub struct Dissolve;
impl Dissolve {
    pub fn new() -> Self { Dissolve }
}
impl Transition for Dissolve {
    fn id(&self) -> &str { "zavyx.transition.dissolve" }
    fn name(&self) -> &str { "Dissolve" }
    fn param_schema(&self) -> ParamSchema { ParamSchema::default() }
    fn process(&self, from: &mut FrameView<'_>, to: FrameViewRef<'_>, alpha: f64,
               _params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        let a = alpha as f32;
        for (dst, src) in from.pixels.chunks_exact_mut(4).zip(to.pixels.chunks_exact(4)) {
            for c in 0..4 {
                dst[c] = lerp_u8(dst[c], src[c], a);
            }
        }
    }
}

/// Dip to Black: from → black → to.
pub struct DipToBlack;
impl DipToBlack {
    pub fn new() -> Self { DipToBlack }
}
impl Transition for DipToBlack {
    fn id(&self) -> &str { "zavyx.transition.dip_to_black" }
    fn name(&self) -> &str { "Dip to Black" }
    fn param_schema(&self) -> ParamSchema { ParamSchema::default() }
    fn process(&self, from: &mut FrameView<'_>, to: FrameViewRef<'_>, alpha: f64,
               _params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        // First half (0..0.5): fade from → black
        // Second half (0.5..1): fade black → to
        if alpha < 0.5 {
            let t = (alpha * 2.0) as f32; // 0..1
            for px in from.pixels.chunks_exact_mut(4) {
                for c in 0..3 { px[c] = lerp_u8(px[c], 0, t); }
                // alpha fades to 0 then we'll re-fade in second half (but to keep
                // alpha-over working, we keep source alpha as lerp to 0 at midpoint).
                px[3] = lerp_u8(px[3], 0, t);
            }
        } else {
            let t = ((alpha - 0.5) * 2.0) as f32; // 0..1
            for (dst, src) in from.pixels.chunks_exact_mut(4).zip(to.pixels.chunks_exact(4)) {
                for c in 0..3 { dst[c] = lerp_u8(0, src[c], t); }
                dst[3] = lerp_u8(0, src[3], t);
            }
        }
    }
}

/// Dip to White: from → white → to.
pub struct DipToWhite;
impl DipToWhite {
    pub fn new() -> Self { DipToWhite }
}
impl Transition for DipToWhite {
    fn id(&self) -> &str { "zavyx.transition.dip_to_white" }
    fn name(&self) -> &str { "Dip to White" }
    fn param_schema(&self) -> ParamSchema { ParamSchema::default() }
    fn process(&self, from: &mut FrameView<'_>, to: FrameViewRef<'_>, alpha: f64,
               _params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        if alpha < 0.5 {
            let t = (alpha * 2.0) as f32;
            for px in from.pixels.chunks_exact_mut(4) {
                for c in 0..3 { px[c] = lerp_u8(px[c], 255, t); }
                px[3] = lerp_u8(px[3], 255, t);
            }
        } else {
            let t = ((alpha - 0.5) * 2.0) as f32;
            for (dst, src) in from.pixels.chunks_exact_mut(4).zip(to.pixels.chunks_exact(4)) {
                for c in 0..3 { dst[c] = lerp_u8(255, src[c], t); }
                dst[3] = lerp_u8(255, src[3], t);
            }
        }
    }
}

/// Generic directional wipe. `dir` = (dx, dy) unit vector in the wipe direction.
/// The wipe boundary advances along `dir` as alpha goes 0 → 1.
fn wipe(dir: (f32, f32), from: &mut FrameView<'_>, to: FrameViewRef<'_>, alpha: f64) {
    let w = from.width as f32;
    let h = from.height as f32;
    let (dx, dy) = dir;
    let norm = (dx * dx + dy * dy).sqrt();
    let (ux, uy) = (dx / norm, dy / norm);
    // Project each pixel onto the unit vector; the result is in [0, max_proj].
    // alpha=0 → boundary at 0 (no to visible); alpha=1 → boundary at max_proj (all to).
    let max_proj = (w * ux.abs() + h * uy.abs()).abs();
    let boundary = (alpha as f32) * max_proj;
    for y in 0..from.height {
        for x in 0..from.width {
            let px_x = x as f32;
            let px_y = y as f32;
            // Project pixel position onto the unit vector (centered at 0,0).
            let proj = (px_x - 0.0) * ux + (px_y - 0.0) * uy;
            // Use the projected value; pixels with proj <= boundary get `to`.
            let use_to = proj <= boundary;
            let i = ((y * from.width + x) * 4) as usize;
            if use_to {
                let ti = ((y * to.width + x) * 4) as usize;
                if i + 3 < from.pixels.len() && ti + 3 < to.pixels.len() {
                    from.pixels[i]   = to.pixels[ti];
                    from.pixels[i+1] = to.pixels[ti+1];
                    from.pixels[i+2] = to.pixels[ti+2];
                    from.pixels[i+3] = to.pixels[ti+3];
                }
            }
            // else: leave the from pixel intact.
        }
    }
}

pub struct WipeLeft;
impl WipeLeft {
    pub fn new() -> Self { WipeLeft }
}
impl Transition for WipeLeft {
    fn id(&self) -> &str { "zavyx.transition.wipe_left" }
    fn name(&self) -> &str { "Wipe Left" }
    fn param_schema(&self) -> ParamSchema { ParamSchema::default() }
    fn process(&self, from: &mut FrameView<'_>, to: FrameViewRef<'_>, alpha: f64,
               _params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        // Wipe from right to left: the `to` clip grows from the right edge.
        wipe((-1.0, 0.0), from, to, alpha);
    }
}

pub struct WipeRight;
impl WipeRight {
    pub fn new() -> Self { WipeRight }
}
impl Transition for WipeRight {
    fn id(&self) -> &str { "zavyx.transition.wipe_right" }
    fn name(&self) -> &str { "Wipe Right" }
    fn param_schema(&self) -> ParamSchema { ParamSchema::default() }
    fn process(&self, from: &mut FrameView<'_>, to: FrameViewRef<'_>, alpha: f64,
               _params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        wipe((1.0, 0.0), from, to, alpha);
    }
}

pub struct WipeUp;
impl WipeUp {
    pub fn new() -> Self { WipeUp }
}
impl Transition for WipeUp {
    fn id(&self) -> &str { "zavyx.transition.wipe_up" }
    fn name(&self) -> &str { "Wipe Up" }
    fn param_schema(&self) -> ParamSchema { ParamSchema::default() }
    fn process(&self, from: &mut FrameView<'_>, to: FrameViewRef<'_>, alpha: f64,
               _params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        wipe((0.0, -1.0), from, to, alpha);
    }
}

pub struct WipeDown;
impl WipeDown {
    pub fn new() -> Self { WipeDown }
}
impl Transition for WipeDown {
    fn id(&self) -> &str { "zavyx.transition.wipe_down" }
    fn name(&self) -> &str { "Wipe Down" }
    fn param_schema(&self) -> ParamSchema { ParamSchema::default() }
    fn process(&self, from: &mut FrameView<'_>, to: FrameViewRef<'_>, alpha: f64,
               _params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        wipe((0.0, 1.0), from, to, alpha);
    }
}

/// Slide: the `to` clip slides in from the right edge over the `from` clip.
pub struct Slide;
impl Slide {
    pub fn new() -> Self { Slide }
}
impl Transition for Slide {
    fn id(&self) -> &str { "zavyx.transition.slide" }
    fn name(&self) -> &str { "Slide" }
    fn param_schema(&self) -> ParamSchema { ParamSchema::default() }
    fn process(&self, from: &mut FrameView<'_>, to: FrameViewRef<'_>, alpha: f64,
               _params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        // At alpha=0, the to clip is fully off-screen right. At alpha=1, it covers
        // the entire frame.
        let w = from.width;
        let h = from.height;
        let offset = ((1.0 - alpha) * w as f64).round() as i32;
        // For each pixel in `from`, look up the corresponding source pixel.
        let from_buf = from.pixels.to_vec();
        for y in 0..h {
            for x in 0..w {
                let src_x = x as i32 - offset;
                let di = ((y * w + x) * 4) as usize;
                if src_x < 0 || src_x >= w as i32 {
                    // Out of to-clip range: keep from clip
                    let fi = ((y * w + x) * 4) as usize;
                    from.pixels[di..di+4].copy_from_slice(&from_buf[fi..fi+4]);
                } else {
                    let ti = ((y * w + src_x as u32) * 4) as usize;
                    from.pixels[di]   = to.pixels[ti];
                    from.pixels[di+1] = to.pixels[ti+1];
                    from.pixels[di+2] = to.pixels[ti+2];
                    from.pixels[di+3] = to.pixels[ti+3];
                }
            }
        }
    }
}

/// Push: like Slide, but the `from` clip also slides out the left edge as `to`
/// slides in from the right.
pub struct Push;
impl Push {
    pub fn new() -> Self { Push }
}
impl Transition for Push {
    fn id(&self) -> &str { "zavyx.transition.push" }
    fn name(&self) -> &str { "Push" }
    fn param_schema(&self) -> ParamSchema { ParamSchema::default() }
    fn process(&self, from: &mut FrameView<'_>, to: FrameViewRef<'_>, alpha: f64,
               _params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        let w = from.width;
        let h = from.height;
        let offset = (alpha * w as f64).round() as i32;
        let from_buf = from.pixels.to_vec();
        for y in 0..h {
            for x in 0..w {
                // The "from" content is shifted left by `offset`.
                let from_src_x = x as i32 + offset;
                // The "to" content is shifted left by `offset - w` (i.e. starts at right edge and moves left).
                let to_src_x = x as i32 + offset - w as i32;
                let di = ((y * w + x) * 4) as usize;
                if to_src_x >= 0 && to_src_x < w as i32 {
                    // Use to clip's pixel
                    let ti = ((y * w + to_src_x as u32) * 4) as usize;
                    from.pixels[di]   = to.pixels[ti];
                    from.pixels[di+1] = to.pixels[ti+1];
                    from.pixels[di+2] = to.pixels[ti+2];
                    from.pixels[di+3] = to.pixels[ti+3];
                } else if from_src_x >= 0 && from_src_x < w as i32 {
                    let fi = ((y * w + from_src_x as u32) * 4) as usize;
                    from.pixels[di..di+4].copy_from_slice(&from_buf[fi..fi+4]);
                } else {
                    // Both out of range: black
                    from.pixels[di]   = 0;
                    from.pixels[di+1] = 0;
                    from.pixels[di+2] = 0;
                    from.pixels[di+3] = 255;
                }
            }
        }
    }
}

/// Zoom: the `to` clip zooms in from the center, starting small and growing
/// to fill the frame at alpha=1.
pub struct Zoom;
impl Zoom {
    pub fn new() -> Self { Zoom }
}
impl Transition for Zoom {
    fn id(&self) -> &str { "zavyx.transition.zoom" }
    fn name(&self) -> &str { "Zoom" }
    fn param_schema(&self) -> ParamSchema { ParamSchema::default() }
    fn process(&self, from: &mut FrameView<'_>, to: FrameViewRef<'_>, alpha: f64,
               _params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        // At alpha=0, to clip is invisible. As alpha→1, to clip grows from
        // a tiny dot in the center to full size. From clip remains visible where
        // to clip hasn't covered yet.
        let w = from.width;
        let h = from.height;
        let cx = w as f32 / 2.0;
        let cy = h as f32 / 2.0;
        let scale = alpha as f32; // 0..1, where 1 = full size
        if scale <= 0.0 { return; }
        let half_w = (w as f32 * scale) / 2.0;
        let half_h = (h as f32 * scale) / 2.0;
        let x0 = (cx - half_w).max(0.0) as u32;
        let x1 = ((cx + half_w).min(w as f32)) as u32;
        let y0 = (cy - half_h).max(0.0) as u32;
        let y1 = ((cy + half_h).min(h as f32)) as u32;
        for y in y0..y1 {
            for x in x0..x1 {
                // Map from-frame pixel back to to-frame pixel (nearest neighbor).
                let to_x = ((x as f32 - cx) / scale + cx) as u32;
                let to_y = ((y as f32 - cy) / scale + cy) as u32;
                if to_x < w && to_y < h {
                    let di = ((y * w + x) * 4) as usize;
                    let ti = ((to_y * w + to_x) * 4) as usize;
                    if di + 3 < from.pixels.len() && ti + 3 < to.pixels.len() {
                        from.pixels[di]   = to.pixels[ti];
                        from.pixels[di+1] = to.pixels[ti+1];
                        from.pixels[di+2] = to.pixels[ti+2];
                        from.pixels[di+3] = to.pixels[ti+3];
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

    fn make_pair(w: u32, h: u32, from_color: [u8; 4], to_color: [u8; 4])
        -> (Vec<u8>, Vec<u8>, FrameView<'static>, FrameView<'static>)
    {
        // Can't make 'static FrameView without unsafe; use a different pattern.
        let _ = (w, h, from_color, to_color);
        unimplemented!()
    }

    #[test]
    fn dissolve_blends_at_half() {
        let dissolve = Dissolve::new();
        let mut from = vec![0u8, 0, 0, 255];
        let to       = vec![255u8, 255, 255, 255];
        let mut from_view = FrameView {
            width: 1, height: 1, pixels: &mut from,
            time: Rational::ZERO,
        };
        let to_view = FrameViewRef {
            width: 1, height: 1, pixels: &to,
            time: Rational::ZERO,
        };
        let params = BTreeMap::new();
        let ctx = RenderCtx { working_color_space: "linear-rec709" };
        dissolve.process(&mut from_view, to_view, 0.5, &params, &ctx);
        eprintln!("after process: from = {:?}", from);
        eprintln!("after process: to   = {:?}", to);
        // At alpha=0.5: lerp(0, 255, 0.5) = 128 for R/G/B
        // Alpha: lerp(255, 255, 0.5) = 255 (unchanged since both opaque)
        assert_eq!(from[0], 128);
        assert_eq!(from[1], 128);
        assert_eq!(from[2], 128);
        assert_eq!(from[3], 255);  // alpha preserved
    }

    #[test]
    fn dip_to_black_at_midpoint_is_black() {
        let dip = DipToBlack::new();
        let mut from = vec![255u8, 100, 50, 255];
        let to       = vec![10u8, 20, 30, 255];
        let mut from_view = FrameView {
            width: 1, height: 1, pixels: &mut from,
            time: Rational::ZERO,
        };
        let to_view = FrameViewRef {
            width: 1, height: 1, pixels: &to,
            time: Rational::ZERO,
        };
        let params = BTreeMap::new();
        let ctx = RenderCtx { working_color_space: "linear-rec709" };
        dip.process(&mut from_view, to_view, 0.5, &params, &ctx);
        // At alpha=0.5, both halves reach their midpoint = black.
        assert_eq!(from[0], 0);
        assert_eq!(from[1], 0);
        assert_eq!(from[2], 0);
        assert_eq!(from[3], 0);
    }

    #[test]
    fn dip_to_white_at_midpoint_is_white() {
        let dip = DipToWhite::new();
        let mut from = vec![0u8, 50, 100, 255];
        let to       = vec![200u8, 100, 50, 255];
        let mut from_view = FrameView {
            width: 1, height: 1, pixels: &mut from,
            time: Rational::ZERO,
        };
        let to_view = FrameViewRef {
            width: 1, height: 1, pixels: &to,
            time: Rational::ZERO,
        };
        let params = BTreeMap::new();
        let ctx = RenderCtx { working_color_space: "linear-rec709" };
        dip.process(&mut from_view, to_view, 0.5, &params, &ctx);
        assert_eq!(from[0], 255);
        assert_eq!(from[1], 255);
        assert_eq!(from[2], 255);
    }

    #[test]
    fn wipe_right_at_full_alpha_covers_to() {
        let wipe = WipeRight::new();
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
        wipe.process(&mut from_view, to_view, 1.0, &params, &ctx);
        // At alpha=1, the entire frame should be `to` (white).
        for px in from.chunks_exact(4) {
            assert_eq!(px[0], 255, "expected full cover, got {}", px[0]);
        }
    }

    #[test]
    fn zoom_at_full_alpha_fills_with_to() {
        let zoom = Zoom::new();
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
        zoom.process(&mut from_view, to_view, 1.0, &params, &ctx);
        for px in from.chunks_exact(4) {
            assert_eq!(px[0], 255);
        }
    }
}
