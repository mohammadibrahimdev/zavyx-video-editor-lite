//! Masks, keying (chroma / luma), LUT import, and colour scopes.
//!
//! All pure Rust; CPU-only; no platform deps.

use std::collections::BTreeMap;
use zavyx_ve_core::effect::{Effect, FrameView, RenderCtx, ParamSchema, ParamDef, ParamType, ParamValue};

// =========================================================================
// MASKS — produce an alpha mask that multiplies the frame's alpha channel.
// =========================================================================

/// Rectangle mask. Pixels inside the rect keep their alpha; outside is alpha 0.
pub struct RectangleMaskEffect;
impl RectangleMaskEffect { pub fn new() -> Self { RectangleMaskEffect } }
impl Effect for RectangleMaskEffect {
    fn id(&self) -> &str { "zavyx.mask.rectangle" }
    fn name(&self) -> &str { "Mask: Rectangle" }
    fn param_schema(&self) -> ParamSchema {
        ParamSchema { params: vec![
            ParamDef { id: "x".into(), label: "X (0-1)".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 0.1 },
                keyframeable: true },
            ParamDef { id: "y".into(), label: "Y (0-1)".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 0.1 },
                keyframeable: true },
            ParamDef { id: "w".into(), label: "Width (0-1)".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 0.8 },
                keyframeable: true },
            ParamDef { id: "h".into(), label: "Height (0-1)".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 0.8 },
                keyframeable: true },
            ParamDef { id: "feather".into(), label: "Feather".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 0.0 },
                keyframeable: true },
            ParamDef { id: "invert".into(), label: "Invert".into(),
                ty: ParamType::Bool { default: false },
                keyframeable: false },
        ] }
    }
    fn process(&self, frame: &mut FrameView<'_>, params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        let x = params.get("x").map(|v| v.at(frame.time) as f32).unwrap_or(0.1);
        let y = params.get("y").map(|v| v.at(frame.time) as f32).unwrap_or(0.1);
        let w = params.get("w").map(|v| v.at(frame.time) as f32).unwrap_or(0.8);
        let h = params.get("h").map(|v| v.at(frame.time) as f32).unwrap_or(0.8);
        let feather = params.get("feather").map(|v| v.at(frame.time) as f32).unwrap_or(0.0);
        let invert = params.get("invert").map(|v| match v {
            ParamValue::Const(x) => *x >= 0.5, _ => false,
        }).unwrap_or(false);
        let fw = frame.width as f32;
        let fh = frame.height as f32;
        let x0 = x * fw;
        let y0 = y * fh;
        let x1 = (x + w) * fw;
        let y1 = (y + h) * fh;
        let feather_px = feather * (fw.min(fh)) * 0.1;
        for py in 0..frame.height {
            for px in 0..frame.width {
                let dx = (px as f32 - x0).max(0.0).min(x1 - px as f32);
                let dy = (py as f32 - y0).max(0.0).min(y1 - py as f32);
                // Distance to nearest edge (0 inside, > 0 inside, < 0 outside)
                let inside_x = (px as f32) >= (x0) && (px as f32) < (x1);
                let inside_y = (py as f32) >= (y0) && (py as f32) < (y1);
                let inside = inside_x && inside_y;
                let mask = if feather_px > 0.0 {
                    if inside {
                        // Distance to nearest edge:
                        let dist = (px as f32 - x0).min(x1 - px as f32).min(py as f32 - y0).min(y1 - py as f32);
                        (dist / feather_px).clamp(0.0, 1.0)
                    } else {
                        0.0
                    }
                } else {
                    if inside { 1.0 } else { 0.0 }
                };
                let final_mask = if invert { 1.0 - mask } else { mask };
                let i = ((py * frame.width + px) * 4) as usize;
                frame.pixels[i+3] = ((frame.pixels[i+3] as f32) * final_mask) as u8;
            }
        }
    }
}

/// Ellipse mask.
pub struct EllipseMaskEffect;
impl EllipseMaskEffect { pub fn new() -> Self { EllipseMaskEffect } }
impl Effect for EllipseMaskEffect {
    fn id(&self) -> &str { "zavyx.mask.ellipse" }
    fn name(&self) -> &str { "Mask: Ellipse" }
    fn param_schema(&self) -> ParamSchema {
        ParamSchema { params: vec![
            ParamDef { id: "cx".into(), label: "Center X (0-1)".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 0.5 },
                keyframeable: true },
            ParamDef { id: "cy".into(), label: "Center Y (0-1)".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 0.5 },
                keyframeable: true },
            ParamDef { id: "rx".into(), label: "Radius X (0-1)".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 0.4 },
                keyframeable: true },
            ParamDef { id: "ry".into(), label: "Radius Y (0-1)".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 0.4 },
                keyframeable: true },
            ParamDef { id: "feather".into(), label: "Feather".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 0.0 },
                keyframeable: true },
            ParamDef { id: "invert".into(), label: "Invert".into(),
                ty: ParamType::Bool { default: false },
                keyframeable: false },
        ] }
    }
    fn process(&self, frame: &mut FrameView<'_>, params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        let cx = params.get("cx").map(|v| v.at(frame.time) as f32).unwrap_or(0.5);
        let cy = params.get("cy").map(|v| v.at(frame.time) as f32).unwrap_or(0.5);
        let rx = params.get("rx").map(|v| v.at(frame.time) as f32).unwrap_or(0.4);
        let ry = params.get("ry").map(|v| v.at(frame.time) as f32).unwrap_or(0.4);
        let feather = params.get("feather").map(|v| v.at(frame.time) as f32).unwrap_or(0.0);
        let invert = params.get("invert").map(|v| match v {
            ParamValue::Const(x) => *x >= 0.5, _ => false,
        }).unwrap_or(false);
        let fw = frame.width as f32;
        let fh = frame.height as f32;
        let cx_px = cx * fw;
        let cy_px = cy * fh;
        let rx_px = rx * fw;
        let ry_px = ry * fh;
        let feather_px = feather * (rx_px.min(ry_px));
        for py in 0..frame.height {
            for px in 0..frame.width {
                let dx = (px as f32 - cx_px) / rx_px.max(1.0);
                let dy = (py as f32 - cy_px) / ry_px.max(1.0);
                let r = (dx * dx + dy * dy).sqrt();
                let mask = if feather_px > 0.0 {
                    if r <= 1.0 { ((1.0 - r) * rx_px.min(ry_px) / feather_px).clamp(0.0, 1.0) }
                    else { 0.0 }
                } else {
                    if r <= 1.0 { 1.0 } else { 0.0 }
                };
                let final_mask = if invert { 1.0 - mask } else { mask };
                let i = ((py * frame.width + px) * 4) as usize;
                frame.pixels[i+3] = ((frame.pixels[i+3] as f32) * final_mask) as u8;
            }
        }
    }
}

// =========================================================================
// CHROMA KEY + LUMA KEY
// =========================================================================

/// Chroma key: remove pixels close to a target color (green/blue screen).
pub struct ChromaKeyEffect;
impl ChromaKeyEffect { pub fn new() -> Self { ChromaKeyEffect } }
impl Effect for ChromaKeyEffect {
    fn id(&self) -> &str { "zavyx.chroma_key" }
    fn name(&self) -> &str { "Chroma Key" }
    fn param_schema(&self) -> ParamSchema {
        ParamSchema { params: vec![
            ParamDef { id: "target_r".into(), label: "Target R (0-1)".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 0.0 },
                keyframeable: true },
            ParamDef { id: "target_g".into(), label: "Target G (0-1)".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 1.0 },
                keyframeable: true },
            ParamDef { id: "target_b".into(), label: "Target B (0-1)".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 0.0 },
                keyframeable: true },
            ParamDef { id: "threshold".into(), label: "Threshold".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 0.4 },
                keyframeable: true },
            ParamDef { id: "smoothness".into(), label: "Smoothness".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 0.1 },
                keyframeable: true },
            ParamDef { id: "spill".into(), label: "Spill Suppression".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 0.0 },
                keyframeable: true },
        ] }
    }
    fn process(&self, frame: &mut FrameView<'_>, params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        let tr = params.get("target_r").map(|v| v.at(frame.time) as f32).unwrap_or(0.0);
        let tg = params.get("target_g").map(|v| v.at(frame.time) as f32).unwrap_or(1.0);
        let tb = params.get("target_b").map(|v| v.at(frame.time) as f32).unwrap_or(0.0);
        let threshold = params.get("threshold").map(|v| v.at(frame.time) as f32).unwrap_or(0.4);
        let smoothness = params.get("smoothness").map(|v| v.at(frame.time) as f32).unwrap_or(0.1);
        let spill = params.get("spill").map(|v| v.at(frame.time) as f32).unwrap_or(0.0);
        for px in frame.pixels.chunks_exact_mut(4) {
            let r = px[0] as f32 / 255.0;
            let g = px[1] as f32 / 255.0;
            let b = px[2] as f32 / 255.0;
            // Distance from target color in RGB.
            let dr = r - tr;
            let dg = g - tg;
            let db = b - tb;
            let dist = (dr*dr + dg*dg + db*db).sqrt();
            // Alpha based on distance.
            let alpha = if dist < threshold {
                0.0
            } else if dist < threshold + smoothness && smoothness > 0.0 {
                (dist - threshold) / smoothness
            } else {
                1.0
            };
            // Spill suppression: reduce the target channel's contribution.
            if spill > 0.0 {
                // Find dominant target channel and reduce it.
                let max_target = tr.max(tg).max(tb);
                if max_target == tg {
                    // Target is green — reduce green.
                    let target_g = (r + b) * 0.5;
                    px[1] = ((g * (1.0 - spill) + target_g * spill) * 255.0) as u8;
                } else if max_target == tb {
                    let target_b = (r + g) * 0.5;
                    px[2] = ((b * (1.0 - spill) + target_b * spill) * 255.0) as u8;
                } else {
                    let target_r = (g + b) * 0.5;
                    px[0] = ((r * (1.0 - spill) + target_r * spill) * 255.0) as u8;
                }
            }
            px[3] = ((px[3] as f32) * alpha) as u8;
        }
    }
}

/// Luma key: remove pixels based on luminance (e.g. white background).
pub struct LumaKeyEffect;
impl LumaKeyEffect { pub fn new() -> Self { LumaKeyEffect } }
impl Effect for LumaKeyEffect {
    fn id(&self) -> &str { "zavyx.luma_key" }
    fn name(&self) -> &str { "Luma Key" }
    fn param_schema(&self) -> ParamSchema {
        ParamSchema { params: vec![
            ParamDef { id: "threshold".into(), label: "Threshold".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 0.5 },
                keyframeable: true },
            ParamDef { id: "smoothness".into(), label: "Smoothness".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 0.1 },
                keyframeable: true },
            ParamDef { id: "mode".into(), label: "Mode".into(),
                ty: ParamType::Choice {
                    options: vec!["remove_bright".into(), "remove_dark".into()],
                    default: "remove_bright".into(),
                },
                keyframeable: false },
        ] }
    }
    fn process(&self, frame: &mut FrameView<'_>, params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        let threshold = params.get("threshold").map(|v| v.at(frame.time) as f32).unwrap_or(0.5);
        let smoothness = params.get("smoothness").map(|v| v.at(frame.time) as f32).unwrap_or(0.1);
        let mode = params.get("mode").and_then(|v| match v {
            ParamValue::Const(x) => Some(*x),
            _ => None,
        }).map(|x| if x < 0.5 { "remove_bright" } else { "remove_dark" }).unwrap_or("remove_bright");
        for px in frame.pixels.chunks_exact_mut(4) {
            let r = px[0] as f32 / 255.0;
            let g = px[1] as f32 / 255.0;
            let b = px[2] as f32 / 255.0;
            let l = 0.2126 * r + 0.7152 * g + 0.0722 * b;
            let alpha = if mode == "remove_bright" {
                if l > threshold + smoothness { 0.0 }
                else if l > threshold && smoothness > 0.0 { 1.0 - (l - threshold) / smoothness }
                else { 1.0 }
            } else {
                // remove_dark
                if l < threshold - smoothness { 0.0 }
                else if l < threshold && smoothness > 0.0 { (l - (threshold - smoothness)) / smoothness }
                else { 1.0 }
            };
            px[3] = ((px[3] as f32) * alpha) as u8;
        }
    }
}

// =========================================================================
// LUT IMPORT (.cube file format)
// =========================================================================

/// A parsed 3D LUT in Adobe .cube format.
#[derive(Debug, Clone)]
pub struct Lut {
    pub title: Option<String>,
    /// LUT_3D_SIZE: number of samples per axis (e.g. 32 → 32×32×32).
    pub size: u32,
    /// Linear RGB samples in [0, 1], indexed as data[r * size * size + g * size + b].
    pub data: Vec<[f32; 3]>,
}

impl Lut {
    /// Parse a .cube file. Errors return a descriptive message.
    pub fn parse_cube(text: &str) -> Result<Lut, String> {
        let mut title: Option<String> = None;
        let mut size: u32 = 0;
        let mut data: Vec<[f32; 3]> = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') { continue; }
            let upper = line.to_uppercase();
            if upper.starts_with("TITLE") {
                let after_eq = line.split_once('"').and_then(|(_, rest)| rest.split_once('"'));
                if let Some((t, _)) = after_eq {
                    title = Some(t.to_string());
                }
                continue;
            }
            if upper.starts_with("LUT_3D_SIZE") {
                let n: u32 = line.split_whitespace().nth(1).ok_or("LUT_3D_SIZE missing value")?.parse().map_err(|e: std::num::ParseIntError| e.to_string())?;
                size = n;
                continue;
            }
            if upper.starts_with("LUT_1D_SIZE") {
                return Err("1D LUTs are not supported".into());
            }
            if upper.starts_with("DOMAIN_MIN") || upper.starts_with("DOMAIN_MAX") {
                continue; // We always assume [0, 1] domain.
            }
            // Otherwise, expect 3 floats separated by whitespace.
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let r: f32 = parts[0].parse().map_err(|e: std::num::ParseFloatError| e.to_string())?;
                let g: f32 = parts[1].parse().map_err(|e: std::num::ParseFloatError| e.to_string())?;
                let b: f32 = parts[2].parse().map_err(|e: std::num::ParseFloatError| e.to_string())?;
                data.push([r, g, b]);
            }
        }
        if size == 0 { return Err("Missing LUT_3D_SIZE".into()); }
        let expected = (size * size * size) as usize;
        if data.len() != expected {
            return Err(format!("Expected {} LUT entries, got {}", expected, data.len()));
        }
        Ok(Lut { title, size, data })
    }

    /// Sample the LUT at the given (r, g, b) in [0, 1] using trilinear interpolation.
    pub fn sample(&self, r: f32, g: f32, b: f32) -> [f32; 3] {
        let s = self.size as f32;
        let r = r.clamp(0.0, 1.0) * (s - 1.0);
        let g = g.clamp(0.0, 1.0) * (s - 1.0);
        let b = b.clamp(0.0, 1.0) * (s - 1.0);
        let r0 = r.floor() as u32; let r1 = (r0 + 1).min(self.size - 1);
        let g0 = g.floor() as u32; let g1 = (g0 + 1).min(self.size - 1);
        let b0 = b.floor() as u32; let b1 = (b0 + 1).min(self.size - 1);
        let fr = r - r0 as f32;
        let fg = g - g0 as f32;
        let fb = b - b0 as f32;
        let idx = |rr: u32, gg: u32, bb: u32| -> usize {
            (rr * self.size * self.size + gg * self.size + bb) as usize
        };
        let c000 = self.data[idx(r0, g0, b0)];
        let c100 = self.data[idx(r1, g0, b0)];
        let c010 = self.data[idx(r0, g1, b0)];
        let c110 = self.data[idx(r1, g1, b0)];
        let c001 = self.data[idx(r0, g0, b1)];
        let c101 = self.data[idx(r1, g0, b1)];
        let c011 = self.data[idx(r0, g1, b1)];
        let c111 = self.data[idx(r1, g1, b1)];
        // Trilinear interp.
        let c00 = [
            c000[0] * (1.0 - fr) + c100[0] * fr,
            c000[1] * (1.0 - fr) + c100[1] * fr,
            c000[2] * (1.0 - fr) + c100[2] * fr,
        ];
        let c01 = [
            c001[0] * (1.0 - fr) + c101[0] * fr,
            c001[1] * (1.0 - fr) + c101[1] * fr,
            c001[2] * (1.0 - fr) + c101[2] * fr,
        ];
        let c10 = [
            c010[0] * (1.0 - fr) + c110[0] * fr,
            c010[1] * (1.0 - fr) + c110[1] * fr,
            c010[2] * (1.0 - fr) + c110[2] * fr,
        ];
        let c11 = [
            c011[0] * (1.0 - fr) + c111[0] * fr,
            c011[1] * (1.0 - fr) + c111[1] * fr,
            c011[2] * (1.0 - fr) + c111[2] * fr,
        ];
        let c0 = [
            c00[0] * (1.0 - fg) + c10[0] * fg,
            c00[1] * (1.0 - fg) + c10[1] * fg,
            c00[2] * (1.0 - fg) + c10[2] * fg,
        ];
        let c1 = [
            c01[0] * (1.0 - fg) + c11[0] * fg,
            c01[1] * (1.0 - fg) + c11[1] * fg,
            c01[2] * (1.0 - fg) + c11[2] * fg,
        ];
        [
            c0[0] * (1.0 - fb) + c1[0] * fb,
            c0[1] * (1.0 - fb) + c1[1] * fb,
            c0[2] * (1.0 - fb) + c1[2] * fb,
        ]
    }
}

/// Effect that applies a LUT to the frame. The LUT is owned by the effect.
pub struct LutEffect {
    pub lut: Lut,
}
impl LutEffect {
    pub fn new(lut: Lut) -> Self { LutEffect { lut } }
}
impl Effect for LutEffect {
    fn id(&self) -> &str { "zavyx.lut" }
    fn name(&self) -> &str { "LUT" }
    fn param_schema(&self) -> ParamSchema {
        ParamSchema { params: vec![
            ParamDef { id: "intensity".into(), label: "Intensity".into(),
                ty: ParamType::Float { min: 0.0, max: 1.0, step: 0.001, default: 1.0 },
                keyframeable: true },
        ] }
    }
    fn process(&self, frame: &mut FrameView<'_>, params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        let intensity = params.get("intensity").map(|v| v.at(frame.time) as f32).unwrap_or(1.0);
        if intensity <= 0.0 { return; }
        for px in frame.pixels.chunks_exact_mut(4) {
            let r = px[0] as f32 / 255.0;
            let g = px[1] as f32 / 255.0;
            let b = px[2] as f32 / 255.0;
            let lut_rgb = self.lut.sample(r, g, b);
            // Mix original and LUT-applied by intensity.
            let nr = r * (1.0 - intensity) + lut_rgb[0] * intensity;
            let ng = g * (1.0 - intensity) + lut_rgb[1] * intensity;
            let nb = b * (1.0 - intensity) + lut_rgb[2] * intensity;
            px[0] = (nr.clamp(0.0, 1.0) * 255.0) as u8;
            px[1] = (ng.clamp(0.0, 1.0) * 255.0) as u8;
            px[2] = (nb.clamp(0.0, 1.0) * 255.0) as u8;
        }
    }
}

// =========================================================================
// COLOUR SCOPES — pure compute functions returning data for the GUI.
// =========================================================================

/// Histogram: per-channel counts in 256 bins.
pub struct Histogram {
    pub r: [u32; 256],
    pub g: [u32; 256],
    pub b: [u32; 256],
}

impl Histogram {
    pub fn from_frame(frame: &FrameView<'_>) -> Self {
        let mut h = Histogram {
            r: [0; 256], g: [0; 256], b: [0; 256],
        };
        for px in frame.pixels.chunks_exact(4) {
            h.r[px[0] as usize] += 1;
            h.g[px[1] as usize] += 1;
            h.b[px[2] as usize] += 1;
        }
        h
    }
    /// Maximum count across all bins (for normalization in GUI).
    pub fn max_count(&self) -> u32 {
        let mr = self.r.iter().copied().max().unwrap_or(0);
        let mg = self.g.iter().copied().max().unwrap_or(0);
        let mb = self.b.iter().copied().max().unwrap_or(0);
        mr.max(mg).max(mb)
    }
}

/// Waveform scope: per-column luma histogram.
/// Returns a 2D array of `width` columns × 256 rows of u8 intensities.
pub fn waveform_scope(frame: &FrameView<'_>, target_width: u32) -> Vec<u8> {
    let w = frame.width;
    let h = frame.height;
    let mut out = vec![0u8; (target_width * 256) as usize];
    let scale_x = w as f32 / target_width as f32;
    for tx in 0..target_width {
        let x0 = (tx as f32 * scale_x) as u32;
        let x1 = (((tx as f32 + 1.0) * scale_x) as u32).min(w);
        // Count luma values in this column range.
        let mut counts = [0u32; 256];
        let mut total = 0u32;
        for y in 0..h {
            for x in x0..x1 {
                let i = ((y * w + x) * 4) as usize;
                let r = frame.pixels[i] as f32 / 255.0;
                let g = frame.pixels[i+1] as f32 / 255.0;
                let b = frame.pixels[i+2] as f32 / 255.0;
                let l = (0.2126 * r + 0.7152 * g + 0.0722 * b) * 255.0;
                counts[l as usize] += 1;
                total += 1;
            }
        }
        if total == 0 { continue; }
        // Normalize to 0-255.
        let max = counts.iter().copied().max().unwrap_or(1).max(1);
        for v in 0..256 {
            let intensity = (counts[v] as f32 / max as f32 * 255.0) as u8;
            out[((v as u32) * target_width + tx) as usize] = intensity;
        }
    }
    out
}

/// Vectorscope: 2D polar plot of Cb/Cr (or just B-Y/R-Y).
/// Returns a 256×256 grayscale image where bright = more pixels at that Cb/Cr.
pub fn vectorscope(frame: &FrameView<'_>) -> Vec<u8> {
    let mut counts = vec![0u32; 256 * 256];
    for px in frame.pixels.chunks_exact(4) {
        let r = px[0] as i32;
        let g = px[1] as i32;
        let b = px[2] as i32;
        // Y = 0.299R + 0.587G + 0.114B (BT.601)
        // R - Y, B - Y
        let y = (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32) as i32;
        let cb = (b - y).clamp(-128, 127) + 128;
        let cr = (r - y).clamp(-128, 127) + 128;
        // Map to 256×256 with cb on x, cr on y.
        let idx = (cr as usize * 256 + cb as usize) as usize;
        if idx < counts.len() {
            counts[idx] = counts[idx].saturating_add(1);
        }
    }
    let max = counts.iter().copied().max().unwrap_or(1).max(1);
    counts.iter().map(|c| (*c as f32 / max as f32 * 255.0) as u8).collect()
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use zavyx_ve_core::Rational;

    fn make_view<'a>(pixels: &'a mut [u8], w: u32, h: u32) -> FrameView<'a> {
        FrameView { width: w, height: h, pixels, time: Rational::ZERO }
    }

    #[test]
    fn rectangle_mask_zeros_outside() {
        let mask = RectangleMaskEffect::new();
        let mut pixels = vec![255u8; 8*8*4];  // 8x8 white opaque
        let mut view = make_view(&mut pixels, 8, 8);
        let mut params = BTreeMap::new();
        params.insert("x".into(), ParamValue::Const(0.25));  // x=2px
        params.insert("y".into(), ParamValue::Const(0.25));  // y=2px
        params.insert("w".into(), ParamValue::Const(0.5));   // w=4px (x in 2..6)
        params.insert("h".into(), ParamValue::Const(0.5));   // h=4px
        params.insert("feather".into(), ParamValue::Const(0.0));
        params.insert("invert".into(), ParamValue::Const(0.0));
        let ctx = RenderCtx { working_color_space: "linear-rec709" };
        mask.process(&mut view, &params, &ctx);
        // Pixel at (0, 0) should have alpha 0.
        assert_eq!(pixels[3], 0, "outside pixel should have alpha 0");
        // Pixel at (3, 3) should have alpha 255.
        let mid = ((3 * 8 + 3) * 4) as usize;
        assert_eq!(pixels[mid+3], 255, "inside pixel should have alpha 255");
    }

    #[test]
    fn ellipse_mask_circular() {
        let mask = EllipseMaskEffect::new();
        let mut pixels = vec![255u8; 16*16*4];
        let mut view = make_view(&mut pixels, 16, 16);
        let mut params = BTreeMap::new();
        params.insert("cx".into(), ParamValue::Const(0.5));
        params.insert("cy".into(), ParamValue::Const(0.5));
        params.insert("rx".into(), ParamValue::Const(0.5));
        params.insert("ry".into(), ParamValue::Const(0.5));
        params.insert("feather".into(), ParamValue::Const(0.0));
        params.insert("invert".into(), ParamValue::Const(0.0));
        let ctx = RenderCtx { working_color_space: "linear-rec709" };
        mask.process(&mut view, &params, &ctx);
        // Corner pixel (0, 0) should have alpha 0 (outside circle).
        assert_eq!(pixels[3], 0, "corner should be outside circle");
        // Center pixel (8, 8) should have alpha 255.
        let center = ((8 * 16 + 8) * 4) as usize;
        assert_eq!(pixels[center+3], 255, "center should be inside circle");
    }

    #[test]
    fn chroma_key_removes_green() {
        let ck = ChromaKeyEffect::new();
        let mut pixels = vec![
            0u8, 255, 0, 255,    // green
            255, 0, 0, 255,      // red
            0, 0, 255, 255,      // blue
            255, 255, 255, 255,  // white
        ];
        let mut view = make_view(&mut pixels, 4, 1);
        let mut params = BTreeMap::new();
        params.insert("target_r".into(), ParamValue::Const(0.0));
        params.insert("target_g".into(), ParamValue::Const(1.0));
        params.insert("target_b".into(), ParamValue::Const(0.0));
        params.insert("threshold".into(), ParamValue::Const(0.3));
        params.insert("smoothness".into(), ParamValue::Const(0.1));
        params.insert("spill".into(), ParamValue::Const(0.0));
        let ctx = RenderCtx { working_color_space: "linear-rec709" };
        ck.process(&mut view, &params, &ctx);
        // Green pixel should be transparent.
        assert_eq!(pixels[3], 0, "green pixel should be transparent");
        // Red pixel should remain opaque.
        assert_eq!(pixels[7], 255, "red pixel should remain opaque");
        // Blue pixel should remain opaque.
        assert_eq!(pixels[11], 255, "blue pixel should remain opaque");
        // White pixel should remain opaque.
        assert_eq!(pixels[15], 255, "white pixel should remain opaque");
    }

    #[test]
    fn luma_key_removes_bright() {
        let lk = LumaKeyEffect::new();
        let mut pixels = vec![
            0u8, 0, 0, 255,      // black (luma 0)
            255, 255, 255, 255,   // white (luma 1)
            128, 128, 128, 255,  // mid gray (luma 0.5)
        ];
        let mut view = make_view(&mut pixels, 3, 1);
        let mut params = BTreeMap::new();
        params.insert("threshold".into(), ParamValue::Const(0.7));
        params.insert("smoothness".into(), ParamValue::Const(0.1));
        params.insert("mode".into(), ParamValue::Const(0.0));  // remove_bright
        let ctx = RenderCtx { working_color_space: "linear-rec709" };
        lk.process(&mut view, &params, &ctx);
        // Black should remain opaque.
        assert_eq!(pixels[3], 255, "black should be opaque");
        // White should be transparent.
        assert_eq!(pixels[7], 0, "white should be transparent");
    }

    #[test]
    fn lut_parses_cube_file() {
        let cube = r#"TITLE "Test LUT"
LUT_3D_SIZE 2
0.0 0.0 0.0
1.0 0.0 0.0
0.0 1.0 0.0
1.0 1.0 0.0
0.0 0.0 1.0
1.0 0.0 1.0
0.0 1.0 1.0
1.0 1.0 1.0
"#;
        let lut = Lut::parse_cube(cube).unwrap();
        assert_eq!(lut.title.as_deref(), Some("Test LUT"));
        assert_eq!(lut.size, 2);
        assert_eq!(lut.data.len(), 8);
        // First entry should be (0, 0, 0).
        assert_eq!(lut.data[0], [0.0, 0.0, 0.0]);
        // Second entry should be (1, 0, 0).
        assert_eq!(lut.data[1], [1.0, 0.0, 0.0]);
    }

    #[test]
    fn lut_sample_returns_corners() {
        let cube = r#"LUT_3D_SIZE 2
0.0 0.0 0.0
1.0 0.0 0.0
0.0 1.0 0.0
1.0 1.0 0.0
0.0 0.0 1.0
1.0 0.0 1.0
0.0 1.0 1.0
1.0 1.0 1.0
"#;
        let lut = Lut::parse_cube(cube).unwrap();
        // Sample at (0, 0, 0) should return (0, 0, 0).
        let v = lut.sample(0.0, 0.0, 0.0);
        assert!((v[0] - 0.0).abs() < 1e-6);
        assert!((v[1] - 0.0).abs() < 1e-6);
        assert!((v[2] - 0.0).abs() < 1e-6);
        // Sample at (1, 1, 1) should return (1, 1, 1).
        let v = lut.sample(1.0, 1.0, 1.0);
        assert!((v[0] - 1.0).abs() < 1e-6);
        assert!((v[1] - 1.0).abs() < 1e-6);
        assert!((v[2] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn lut_effect_applies_to_pixels() {
        let cube = r#"LUT_3D_SIZE 2
0.0 0.0 0.0
1.0 0.0 0.0
0.0 1.0 0.0
1.0 1.0 0.0
0.0 0.0 1.0
1.0 0.0 1.0
0.0 1.0 1.0
1.0 1.0 1.0
"#;
        let lut = Lut::parse_cube(cube).unwrap();
        let effect = LutEffect::new(lut);
        let mut pixels = vec![128u8, 128, 128, 255];
        let mut view = make_view(&mut pixels, 1, 1);
        let mut params = BTreeMap::new();
        params.insert("intensity".into(), ParamValue::Const(1.0));
        let ctx = RenderCtx { working_color_space: "linear-rec709" };
        effect.process(&mut view, &params, &ctx);
        // Mid-gray (0.5) should sample to ~(0.5, 0.5, 0.5) after interpolation.
        assert!(pixels[0] > 100 && pixels[0] < 200, "R should be ~128, got {}", pixels[0]);
    }

    #[test]
    fn histogram_counts_pixels_per_bin() {
        let mut pixels = vec![0u8, 0, 0, 255,  255, 255, 255, 255,  128, 128, 128, 255];
        let mut view = make_view(&mut pixels, 3, 1);
        let h = Histogram::from_frame(&view);
        assert_eq!(h.r[0], 1);   // 1 pixel with R=0
        assert_eq!(h.r[128], 1); // 1 pixel with R=128
        assert_eq!(h.r[255], 1); // 1 pixel with R=255
        assert_eq!(h.g[0], 1);
        assert_eq!(h.b[0], 1);
    }

    #[test]
    fn waveform_returns_target_width_columns() {
        let mut pixels = vec![128u8; 32*32*4];
        let mut view = make_view(&mut pixels, 32, 32);
        let w = waveform_scope(&view, 16);
        assert_eq!(w.len(), 16 * 256);
    }

    #[test]
    fn vectorscope_returns_256x256() {
        let mut pixels = vec![128u8; 16*16*4];
        let mut view = make_view(&mut pixels, 16, 16);
        let v = vectorscope(&view);
        assert_eq!(v.len(), 256 * 256);
    }
}
