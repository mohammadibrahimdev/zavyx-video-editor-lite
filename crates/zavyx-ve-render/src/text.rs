//! Titles / text rendering using `ab_glyph`.
//!
//! Pure-Rust text rasterization. Fonts come from a file: the title's `font`
//! param (a path, or a file name in the Windows fonts folder), else the
//! effect's default font. Sizes are in pixels of a 1080-pixel-tall frame, so
//! a title looks the same in the small preview and in a 4K export.
//!
//! Supports multi-line text, left/centre/right alignment, opacity, drop
//! shadow, outline and a background box.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use zavyx_ve_core::effect::{Effect, FrameView, RenderCtx, ParamSchema, ParamDef, ParamType, ParamValue};

/// Title effect: renders text onto the frame at a given position.
pub struct TitleEffect {
    fonts: parking_lot::Mutex<HashMap<PathBuf, Option<Arc<ab_glyph::FontArc>>>>,
    font_path: PathBuf,
}

/// A bold sans font available on this computer, used when a title names none.
pub fn default_font_path() -> PathBuf {
    let candidates = [
        "C:\\Windows\\Fonts\\segoeuib.ttf",
        "C:\\Windows\\Fonts\\arialbd.ttf",
        "C:\\Windows\\Fonts\\arial.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
    ];
    candidates.iter().map(PathBuf::from).find(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from(candidates[0]))
}

impl TitleEffect {
    pub fn new(font_path: impl Into<PathBuf>) -> Self {
        TitleEffect {
            fonts: parking_lot::Mutex::new(HashMap::new()),
            font_path: font_path.into(),
        }
    }

    fn resolve(&self, name: Option<&str>) -> PathBuf {
        let Some(name) = name.filter(|n| !n.trim().is_empty()) else {
            return self.font_path.clone();
        };
        let p = Path::new(name);
        if p.is_file() {
            return p.to_path_buf();
        }
        if let Some(windir) = std::env::var_os("WINDIR") {
            let f = PathBuf::from(windir).join("Fonts").join(name);
            if f.is_file() {
                return f;
            }
        }
        self.font_path.clone()
    }

    fn load_font(&self, name: Option<&str>) -> Option<Arc<ab_glyph::FontArc>> {
        let path = self.resolve(name);
        let mut fonts = self.fonts.lock();
        fonts.entry(path.clone())
            .or_insert_with(|| {
                let bytes = std::fs::read(&path).ok()?;
                ab_glyph::FontArc::try_from_vec(bytes).ok().map(Arc::new)
            })
            .clone()
    }
}

fn float(id: &str, label: &str, min: f64, max: f64, default: f64) -> ParamDef {
    ParamDef {
        id: id.into(), label: label.into(),
        ty: ParamType::Float { min, max, step: 0.001, default },
        keyframeable: true,
    }
}

impl Effect for TitleEffect {
    fn id(&self) -> &str { "zavyx.title" }
    fn name(&self) -> &str { "Title" }
    fn param_schema(&self) -> ParamSchema {
        ParamSchema {
            params: vec![
                ParamDef { id: "text".into(), label: "Text".into(),
                    ty: ParamType::Text { default: "Your Title".into() }, keyframeable: false },
                ParamDef { id: "align".into(), label: "Align".into(),
                    ty: ParamType::Choice {
                        options: vec!["Left".into(), "Center".into(), "Right".into()],
                        default: "Left".into(),
                    },
                    keyframeable: false },
                float("size", "Size", 8.0, 300.0, 64.0),
                float("x", "Position X", 0.0, 1.0, 0.1),
                float("y", "Position Y", 0.0, 1.0, 0.4),
                float("color_r", "Red", 0.0, 1.0, 1.0),
                float("color_g", "Green", 0.0, 1.0, 1.0),
                float("color_b", "Blue", 0.0, 1.0, 1.0),
                float("opacity", "Opacity", 0.0, 1.0, 1.0),
                float("shadow", "Shadow", 0.0, 1.0, 0.0),
                float("outline", "Outline", 0.0, 12.0, 0.0),
                float("bg_opacity", "Background", 0.0, 1.0, 0.0),
            ]
        }
    }

    fn process(&self, frame: &mut FrameView<'_>, params: &BTreeMap<String, ParamValue>, _ctx: &RenderCtx<'_>) {
        use ab_glyph::{Font, ScaleFont};
        let t = frame.time;
        let num = |k: &str, d: f64| params.get(k).map(|v| v.at(t)).unwrap_or(d) as f32;
        let text = params.get("text").and_then(ParamValue::text).unwrap_or("Your Title");
        let font_name = params.get("font").and_then(ParamValue::text);
        let Some(font) = self.load_font(font_name) else { return };

        let (w, h) = (frame.width as usize, frame.height as usize);
        if w == 0 || h == 0 || text.trim().is_empty() {
            return;
        }
        let k = frame.height as f32 / 1080.0;
        let size = (num("size", 64.0) * k).max(1.0);
        let align = num("align", 0.0).round() as i32;
        let anchor_x = num("x", 0.1) * w as f32;
        let top = num("y", 0.4) * h as f32;
        let color = [num("color_r", 1.0), num("color_g", 1.0), num("color_b", 1.0)];
        let opacity = num("opacity", 1.0).clamp(0.0, 1.0);
        let shadow = num("shadow", 0.0).clamp(0.0, 1.0);
        let outline = (num("outline", 0.0) * k).max(0.0);
        let bg = num("bg_opacity", 0.0).clamp(0.0, 1.0);
        let bg_color = [num("bg_r", 0.0), num("bg_g", 0.0), num("bg_b", 0.0)];
        if opacity <= 0.0 {
            return;
        }

        // Rasterize all lines into one coverage mask.
        let scaled = font.as_scaled(size);
        let line_h = (scaled.ascent() - scaled.descent() + scaled.line_gap()).max(size);
        let mut mask = vec![0f32; w * h];
        let (mut min_x, mut max_x) = (f32::MAX, f32::MIN);
        let lines: Vec<&str> = text.lines().collect();
        for (li, line) in lines.iter().enumerate() {
            let width: f32 = line.chars().map(|c| scaled.h_advance(scaled.glyph_id(c))).sum();
            let start_x = match align {
                1 => anchor_x - width / 2.0,
                2 => anchor_x - width,
                _ => anchor_x,
            };
            min_x = min_x.min(start_x);
            max_x = max_x.max(start_x + width);
            let baseline = top + scaled.ascent() + li as f32 * line_h;
            let mut caret = start_x;
            for ch in line.chars() {
                let mut g = scaled.scaled_glyph(ch);
                g.position = ab_glyph::point(caret, baseline);
                caret += scaled.h_advance(g.id);
                if let Some(o) = font.outline_glyph(g) {
                    let b = o.px_bounds();
                    o.draw(|gx, gy, cov| {
                        let px = b.min.x as i32 + gx as i32;
                        let py = b.min.y as i32 + gy as i32;
                        if px >= 0 && py >= 0 && (px as usize) < w && (py as usize) < h {
                            let m = &mut mask[py as usize * w + px as usize];
                            *m = (*m + cov).min(1.0);
                        }
                    });
                }
            }
        }
        let block_h = line_h * lines.len().max(1) as f32;

        if bg > 0.0 {
            let pad = size * 0.3;
            let x0 = (min_x - pad).max(0.0) as usize;
            let x1 = ((max_x + pad).max(0.0) as usize).min(w);
            let y0 = (top - pad * 0.5).max(0.0) as usize;
            let y1 = ((top + block_h + pad * 0.5).max(0.0) as usize).min(h);
            for y in y0..y1 {
                for x in x0..x1 {
                    blend(frame.pixels, y * w + x, bg_color, bg * opacity);
                }
            }
        }
        if shadow > 0.0 {
            let off = (size * 0.06).max(1.0) as usize;
            for y in off..h {
                for x in off..w {
                    let m = mask[(y - off) * w + (x - off)];
                    if m > 0.0 {
                        blend(frame.pixels, y * w + x, [0.0; 3], m * shadow * 0.8 * opacity);
                    }
                }
            }
        }
        if outline > 0.0 {
            let r = outline.ceil() as i32;
            let mut grown = vec![0f32; w * h];
            for y in 0..h as i32 {
                for x in 0..w as i32 {
                    let m = mask[y as usize * w + x as usize];
                    if m <= 0.0 { continue; }
                    for dy in -r..=r {
                        for dx in -r..=r {
                            if (dx * dx + dy * dy) as f32 > outline * outline + 0.5 { continue; }
                            let (nx, ny) = (x + dx, y + dy);
                            if nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h {
                                let g = &mut grown[ny as usize * w + nx as usize];
                                *g = g.max(m);
                            }
                        }
                    }
                }
            }
            for (i, g) in grown.iter().enumerate() {
                if *g > 0.0 {
                    blend(frame.pixels, i, [0.0; 3], *g * opacity);
                }
            }
        }
        for (i, m) in mask.iter().enumerate() {
            if *m > 0.0 {
                blend(frame.pixels, i, color, *m * opacity);
            }
        }
    }
}

/// Alpha-over one pixel with colour `c` at coverage `a`.
fn blend(pixels: &mut [u8], i: usize, c: [f32; 3], a: f32) {
    if a <= 0.0 { return; }
    let px = &mut pixels[i * 4..i * 4 + 4];
    let da = px[3] as f32 / 255.0;
    let oa = a + da * (1.0 - a);
    for ch in 0..3 {
        let d = px[ch] as f32;
        px[ch] = ((c[ch] * 255.0 * a + d * da * (1.0 - a)) / oa).round().clamp(0.0, 255.0) as u8;
    }
    px[3] = (oa * 255.0).round() as u8;
}

#[cfg(test)]
mod tests {
    use super::*;
    use zavyx_ve_core::Rational;

    fn render(params: BTreeMap<String, ParamValue>) -> Option<Vec<u8>> {
        let font = default_font_path();
        if !font.is_file() {
            eprintln!("Skipping title test — no system font found");
            return None;
        }
        let title = TitleEffect::new(font);
        let mut pixels = vec![0u8; 400 * 216 * 4];
        let mut view = FrameView { width: 400, height: 216, pixels: &mut pixels, time: Rational::ZERO };
        let ctx = RenderCtx { working_color_space: "linear-rec709" };
        title.process(&mut view, &params, &ctx);
        Some(pixels)
    }

    fn visible_rows(pixels: &[u8]) -> Vec<usize> {
        (0..216).filter(|y| (0..400).any(|x| pixels[(y * 400 + x) * 4 + 3] > 0)).collect()
    }

    #[test]
    fn title_renders_text_when_font_available() {
        let mut params = BTreeMap::new();
        params.insert("size".into(), ParamValue::Const(100.0));
        params.insert("text".into(), ParamValue::Text("Hello".into()));
        let Some(px) = render(params) else { return };
        assert!(px.chunks_exact(4).any(|p| p[3] > 0), "title should draw something");
    }

    #[test]
    fn multi_line_text_is_taller() {
        let mut one = BTreeMap::new();
        one.insert("text".into(), ParamValue::Text("Line".into()));
        one.insert("y".into(), ParamValue::Const(0.05));
        let mut two = one.clone();
        two.insert("text".into(), ParamValue::Text("Line\nLine".into()));
        let (Some(a), Some(b)) = (render(one), render(two)) else { return };
        assert!(visible_rows(&b).len() > visible_rows(&a).len() * 3 / 2);
    }

    #[test]
    fn background_box_covers_more_than_text() {
        let mut plain = BTreeMap::new();
        plain.insert("text".into(), ParamValue::Text("Box".into()));
        let mut boxed = plain.clone();
        boxed.insert("bg_opacity".into(), ParamValue::Const(1.0));
        let (Some(a), Some(b)) = (render(plain), render(boxed)) else { return };
        let count = |p: &[u8]| p.chunks_exact(4).filter(|px| px[3] > 0).count();
        assert!(count(&b) > count(&a));
    }
}
