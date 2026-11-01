//! Picture-in-picture and split-screen layouts.
//!
//! These effects take a primary input (the clip they're applied to) and
//! composite secondary inputs (from other tracks) into a multi-pane
//! layout. In Phase 4 we implement them as standalone functions that
//! take pre-rendered source frames; Phase 5 will wire them into the
//! compositor's multi-track pull.

use std::collections::BTreeMap;
use zavyx_ve_core::effect::{Effect, FrameView, RenderCtx, ParamSchema, ParamDef, ParamType, ParamValue};

/// Split-screen layout presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitLayout {
    /// Two panes side-by-side (left + right).
    TwoHorizontal,
    /// Two panes stacked (top + bottom).
    TwoVertical,
    /// Three panes: one large left, two stacked right.
    ThreeLeft,
    /// Three panes side-by-side.
    ThreeStrip,
    /// 2×2 grid.
    FourGrid,
    /// 3×2 grid (6 panes).
    SixGrid,
}

impl SplitLayout {
    /// Returns the rectangle (x, y, w, h) for pane `index` given a frame size.
    /// All values are in [0, 1] of the frame dimensions.
    pub fn pane_rect(&self, index: usize, total_panes: usize) -> (f32, f32, f32, f32) {
        if index >= total_panes { return (0.0, 0.0, 0.0, 0.0); }
        match self {
            SplitLayout::TwoHorizontal => {
                [(0.0, 0.0, 0.5, 1.0), (0.5, 0.0, 0.5, 1.0)][index]
            }
            SplitLayout::TwoVertical => {
                [(0.0, 0.0, 1.0, 0.5), (0.0, 0.5, 1.0, 0.5)][index]
            }
            SplitLayout::ThreeLeft => {
                [(0.0, 0.0, 0.5, 1.0), (0.5, 0.0, 0.5, 0.5), (0.5, 0.5, 0.5, 0.5)][index]
            }
            SplitLayout::ThreeStrip => {
                [(0.0, 0.0, 1.0/3.0, 1.0), (1.0/3.0, 0.0, 1.0/3.0, 1.0), (2.0/3.0, 0.0, 1.0/3.0, 1.0)][index]
            }
            SplitLayout::FourGrid => {
                [(0.0, 0.0, 0.5, 0.5), (0.5, 0.0, 0.5, 0.5), (0.0, 0.5, 0.5, 0.5), (0.5, 0.5, 0.5, 0.5)][index]
            }
            SplitLayout::SixGrid => {
                [
                    (0.0, 0.0, 1.0/3.0, 0.5),
                    (1.0/3.0, 0.0, 1.0/3.0, 0.5),
                    (2.0/3.0, 0.0, 1.0/3.0, 0.5),
                    (0.0, 0.5, 1.0/3.0, 0.5),
                    (1.0/3.0, 0.5, 1.0/3.0, 0.5),
                    (2.0/3.0, 0.5, 1.0/3.0, 0.5),
                ][index]
            }
        }
    }

    pub fn pane_count(&self) -> usize {
        match self {
            SplitLayout::TwoHorizontal | SplitLayout::TwoVertical => 2,
            SplitLayout::ThreeLeft | SplitLayout::ThreeStrip => 3,
            SplitLayout::FourGrid => 4,
            SplitLayout::SixGrid => 6,
        }
    }
}

/// Picture-in-picture effect: composites a secondary frame (e.g. webcam)
/// into a smaller rectangle on top of the primary frame.
pub struct PipEffect;

impl PipEffect {
    pub fn new() -> Self { PipEffect }
}

impl Effect for PipEffect {
    fn id(&self) -> &str { "zavyx.pip" }
    fn name(&self) -> &str { "Picture-in-Picture" }
    fn param_schema(&self) -> ParamSchema {
        ParamSchema { params: vec![
            ParamDef { id: "x".into(), label: "X (0-1)".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 0.7 },
                keyframeable: true },
            ParamDef { id: "y".into(), label: "Y (0-1)".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 0.7 },
                keyframeable: true },
            ParamDef { id: "w".into(), label: "Width (0-1)".into(),
                ty: ParamType::Float { min: 0.05, max: 1.0, step: 0.001, default: 0.25 },
                keyframeable: true },
            ParamDef { id: "h".into(), label: "Height (0-1)".into(),
                ty: ParamType::Float { min: 0.05, max: 1.0, step: 0.001, default: 0.25 },
                keyframeable: true },
            ParamDef { id: "border".into(), label: "Border (px)".into(),
                ty: ParamType::Float { min: 0.0, max: 20.0, step: 1.0, default: 2.0 },
                keyframeable: false },
        ] }
    }
    fn process(&self, frame: &mut FrameView<'_>, _params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        // In a real pipeline, this effect would receive the secondary frame as
        // an extra input. For Phase 4 we leave the primary frame untouched and
        // rely on the compositor's multi-track pull to overlay secondary
        // sources via the existing alpha-over compositing in `lib.rs`.
        //
        // The x/y/w/h params are stored so the GUI / pipeline builder knows
        // where to place the secondary frame.
        let _ = frame;
    }
}

/// Split-screen effect: divides the frame into N panes. The effect itself
/// is a no-op on the primary frame (panes come from other tracks); the GUI
/// uses the `layout` param to know how to arrange them.
pub struct SplitScreenEffect;

impl SplitScreenEffect {
    pub fn new() -> Self { SplitScreenEffect }
}

impl Effect for SplitScreenEffect {
    fn id(&self) -> &str { "zavyx.split_screen" }
    fn name(&self) -> &str { "Split Screen" }
    fn param_schema(&self) -> ParamSchema {
        ParamSchema { params: vec![
            ParamDef { id: "layout".into(), label: "Layout".into(),
                ty: ParamType::Choice {
                    options: vec![
                        "two_horizontal".into(),
                        "two_vertical".into(),
                        "three_left".into(),
                        "three_strip".into(),
                        "four_grid".into(),
                        "six_grid".into(),
                    ],
                    default: "two_horizontal".into(),
                },
                keyframeable: false },
        ] }
    }
    fn process(&self, frame: &mut FrameView<'_>, _params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        // No-op: the layout is applied by the compositor when it pulls secondary
        // sources. The effect exists so the parameter is preserved in the
        // project file and so the GUI can render a layout preview.
        let _ = frame;
    }
}

/// Helper: composite one RGBA8 source into a sub-rectangle of a destination.
/// Uses nearest-neighbour scaling.
pub fn composite_pane(
    dst: &mut [u8],
    dst_w: u32,
    dst_h: u32,
    src: &[u8],
    src_w: u32,
    src_h: u32,
    pane_rect: (f32, f32, f32, f32),  // (x, y, w, h) in [0,1]
) {
    let (rx, ry, rw, rh) = pane_rect;
    let x0 = (rx * dst_w as f32) as u32;
    let y0 = (ry * dst_h as f32) as u32;
    let pw = (rw * dst_w as f32) as u32;
    let ph = (rh * dst_h as f32) as u32;
    for py in 0..ph {
        let dy = y0 + py;
        if dy >= dst_h { break; }
        // Map to source Y (nearest).
        let sy = (py as f32 * src_h as f32 / ph as f32) as u32;
        if sy >= src_h { continue; }
        for px in 0..pw {
            let dx = x0 + px;
            if dx >= dst_w { break; }
            let sx = (px as f32 * src_w as f32 / pw as f32) as u32;
            if sx >= src_w { continue; }
            let di = ((dy * dst_w + dx) * 4) as usize;
            let si = ((sy * src_w + sx) * 4) as usize;
            if di + 3 < dst.len() && si + 3 < src.len() {
                // Alpha-over composite.
                let sa = src[si+3] as f32 / 255.0;
                let da = dst[di+3] as f32 / 255.0;
                let out_a = sa + da * (1.0 - sa);
                if out_a > 0.0 {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use zavyx_ve_core::Rational;

    #[test]
    fn split_layout_two_horizontal_panes() {
        let l = SplitLayout::TwoHorizontal;
        assert_eq!(l.pane_count(), 2);
        let r0 = l.pane_rect(0, 2);
        let r1 = l.pane_rect(1, 2);
        assert_eq!(r0, (0.0, 0.0, 0.5, 1.0));
        assert_eq!(r1, (0.5, 0.0, 0.5, 1.0));
    }

    #[test]
    fn split_layout_four_grid_panes() {
        let l = SplitLayout::FourGrid;
        assert_eq!(l.pane_count(), 4);
        let r0 = l.pane_rect(0, 4);
        let r3 = l.pane_rect(3, 4);
        assert_eq!(r0, (0.0, 0.0, 0.5, 0.5));
        assert_eq!(r3, (0.5, 0.5, 0.5, 0.5));
    }

    #[test]
    fn split_layout_six_grid_panes() {
        let l = SplitLayout::SixGrid;
        assert_eq!(l.pane_count(), 6);
        let r5 = l.pane_rect(5, 6);
        // Third column, second row.
        assert!((r5.0 - 2.0/3.0).abs() < 1e-6);
        assert!((r5.1 - 0.5).abs() < 1e-6);
    }

    #[test]
    fn split_layout_out_of_bounds_returns_zero() {
        let l = SplitLayout::TwoHorizontal;
        let r = l.pane_rect(2, 2);
        assert_eq!(r, (0.0, 0.0, 0.0, 0.0));
    }

    #[test]
    fn composite_pane_writes_into_destination() {
        let mut dst = vec![0u8; 4*4*4];
        let src = vec![255u8; 2*2*4];  // 2x2 white
        composite_pane(&mut dst, 4, 4, &src, 2, 2, (0.0, 0.0, 0.5, 0.5));
        // Top-left 2x2 of dst should now be white.
        assert_eq!(dst[0], 255);
        assert_eq!(dst[(0 * 4 + 1) * 4], 255);
    }
}
