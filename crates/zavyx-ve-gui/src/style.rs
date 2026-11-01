//! Zavyx look: colours, fonts and small reusable widgets.

use egui::{Color32, FontFamily, FontId, Rect, Response, RichText, Rounding, Sense, Stroke, Ui, Vec2};

pub use egui_phosphor::regular as icon;

// Brand colours (Zavyx Standards §3).
pub const VIOLET_TOP: Color32 = Color32::from_rgb(0x8B, 0x6C, 0xFF);
pub const VIOLET_BOTTOM: Color32 = Color32::from_rgb(0x5B, 0x34, 0xE6);
pub const ACCENT: Color32 = Color32::from_rgb(0x6D, 0x4A, 0xFF);
pub const SUCCESS: Color32 = Color32::from_rgb(0x22, 0xA0, 0x5A);
pub const ERROR: Color32 = Color32::from_rgb(0xD2, 0x3C, 0x3C);

// Dark editor palette.
pub const BG: Color32 = Color32::from_rgb(0x0E, 0x0C, 0x14);
pub const PANEL: Color32 = Color32::from_rgb(0x17, 0x14, 0x1F);
pub const RAISED: Color32 = Color32::from_rgb(0x20, 0x1C, 0x2B);
pub const HOVER: Color32 = Color32::from_rgb(0x2B, 0x26, 0x3A);
pub const BORDER: Color32 = Color32::from_rgb(0x2B, 0x27, 0x39);
pub const TEXT: Color32 = Color32::from_rgb(0xEC, 0xE9, 0xF5);
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x9D, 0x97, 0xB0);
pub const TEXT_FAINT: Color32 = Color32::from_rgb(0x6A, 0x64, 0x7E);
pub const ACCENT_TEXT: Color32 = Color32::from_rgb(0xA9, 0x93, 0xFF);

// Timeline clip colours.
pub const CLIP_VIDEO: Color32 = Color32::from_rgb(0x3E, 0x33, 0x86);
pub const CLIP_AUDIO: Color32 = Color32::from_rgb(0x1D, 0x5C, 0x52);
pub const CLIP_TITLE: Color32 = Color32::from_rgb(0x7A, 0x3F, 0x9E);
pub const WAVE: Color32 = Color32::from_rgb(0x5F, 0xD4, 0xB4);

pub const SEMIBOLD: &str = "semibold";

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}

/// Install fonts (Segoe UI when present, plus Phosphor icons) and visuals.
pub fn install(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
    let load = |file: &str| std::fs::read(format!("{windir}\\Fonts\\{file}")).ok();
    if let Some(bytes) = load("segoeui.ttf") {
        fonts.font_data.insert("segoe".into(), egui::FontData::from_owned(bytes));
        fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "segoe".into());
    }
    let mut semi = Vec::new();
    if let Some(bytes) = load("seguisb.ttf") {
        fonts.font_data.insert("segoe-semibold".into(), egui::FontData::from_owned(bytes));
        semi.push("segoe-semibold".to_string());
    }
    semi.extend(fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default());
    fonts.families.insert(FontFamily::Name(SEMIBOLD.into()), semi);
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    if let Some(list) = fonts.families.get_mut(&FontFamily::Name(SEMIBOLD.into())) {
        list.insert(1, "phosphor".into());
    }
    ctx.set_fonts(fonts);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = Vec2::new(6.0, 6.0);
    style.spacing.button_padding = Vec2::new(10.0, 5.0);
    style.spacing.interact_size.y = 26.0;
    style.spacing.slider_width = 150.0;
    style.text_styles.insert(egui::TextStyle::Body, FontId::proportional(13.5));
    style.text_styles.insert(egui::TextStyle::Button, FontId::proportional(13.5));
    style.text_styles.insert(egui::TextStyle::Small, FontId::proportional(11.5));
    style.text_styles.insert(egui::TextStyle::Heading, semibold(17.0));

    let mut v = egui::Visuals::dark();
    v.panel_fill = PANEL;
    v.window_fill = RAISED;
    v.extreme_bg_color = Color32::from_rgb(0x0B, 0x0A, 0x10);
    v.faint_bg_color = RAISED;
    v.override_text_color = Some(TEXT);
    v.window_stroke = Stroke::new(1.0_f32, BORDER);
    v.window_rounding = Rounding::same(10.0);
    v.menu_rounding = Rounding::same(8.0);
    v.selection.bg_fill = ACCENT.linear_multiply(0.55);
    v.selection.stroke = Stroke::new(1.0_f32, VIOLET_TOP);
    v.hyperlink_color = ACCENT_TEXT;
    v.slider_trailing_fill = true;
    for (w, fill) in [
        (&mut v.widgets.noninteractive, PANEL),
        (&mut v.widgets.inactive, RAISED),
        (&mut v.widgets.hovered, HOVER),
        (&mut v.widgets.active, ACCENT),
        (&mut v.widgets.open, HOVER),
    ] {
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
        w.rounding = Rounding::same(6.0);
        w.bg_stroke = Stroke::NONE;
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT_DIM);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
    v.widgets.active.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
    style.visuals = v;
    ctx.set_style(style);
}

/// Give a custom-painted widget an accessible name (screen readers, UI automation).
pub fn a11y(resp: &Response, kind: egui::WidgetType, label: &str) {
    let label = label.to_string();
    resp.widget_info(move || egui::WidgetInfo::labeled(kind, true, label.clone()));
}

/// A flat icon button; `on` draws it highlighted (toggles).
pub fn icon_button(ui: &mut Ui, glyph: &str, tip: &str, enabled: bool, on: bool) -> Response {
    let size = Vec2::splat(28.0);
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    let p = ui.painter();
    if on {
        p.rect_filled(rect, 6.0, ACCENT.linear_multiply(0.35));
    } else if enabled && resp.hovered() {
        p.rect_filled(rect, 6.0, HOVER);
    }
    a11y(&resp, egui::WidgetType::Button, tip);
    let color = if !enabled { TEXT_FAINT } else if on { ACCENT_TEXT } else { TEXT };
    p.text(rect.center(), egui::Align2::CENTER_CENTER, glyph, FontId::proportional(17.0), color);
    if enabled { resp.on_hover_text(tip) } else { resp.on_disabled_hover_text(tip) }
}

/// Filled violet call-to-action button.
pub fn primary_button(ui: &mut Ui, text: &str) -> Response {
    let galley = ui.painter().layout_no_wrap(text.to_string(), semibold(14.0), Color32::WHITE);
    let size = Vec2::new(galley.size().x + 32.0, 32.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    a11y(&resp, egui::WidgetType::Button, text);
    gradient_rect(ui.painter(), rect, 7.0, if resp.hovered() { 1.12 } else { 1.0 });
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, Color32::WHITE);
    resp
}

/// Rounded rect with the Zavyx violet gradient (top-left → bottom-right).
pub fn gradient_rect(p: &egui::Painter, rect: Rect, rounding: f32, boost: f32) {
    gradient_fill(p, rect, rounding, brighten(VIOLET_TOP, boost), brighten(VIOLET_BOTTOM, boost));
}

/// Rounded rect filled with a diagonal gradient from `a` (top-left) to `b`.
pub fn gradient_fill(p: &egui::Painter, rect: Rect, rounding: f32, a: Color32, b: Color32) {
    let mut outline = Vec::new();
    egui::epaint::tessellator::path::rounded_rectangle(&mut outline, rect, Rounding::same(rounding));
    let span = (rect.width() + rect.height()).max(1.0);
    let color_at = |pos: egui::Pos2| lerp(a, b, ((pos.x - rect.min.x) + (pos.y - rect.min.y)) / span);
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.center(), color_at(rect.center()));
    for pos in &outline {
        mesh.colored_vertex(*pos, color_at(*pos));
    }
    let n = outline.len() as u32;
    for i in 0..n {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n);
    }
    p.add(mesh);
}

pub fn brighten(c: Color32, k: f32) -> Color32 {
    let f = |v: u8| ((v as f32 * k).min(255.0)) as u8;
    Color32::from_rgb(f(c.r()), f(c.g()), f(c.b()))
}

pub fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(f(a.r(), b.r()), f(a.g(), b.g()), f(a.b(), b.b()))
}

/// The product mark: violet rounded square with a white Z.
pub fn logo(ui: &mut Ui, size: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    gradient_rect(ui.painter(), rect, size * 0.22, 1.0);
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "Z", semibold(size * 0.62), Color32::WHITE);
    resp
}

/// Dim caption text.
pub fn dim(text: impl Into<String>) -> RichText {
    RichText::new(text).color(TEXT_DIM)
}

/// Section heading inside a panel.
pub fn section(ui: &mut Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(RichText::new(text).font(semibold(13.5)).color(TEXT));
}

/// `HH:MM:SS:FF` timecode.
pub fn timecode(seconds: f64, fps: f64) -> String {
    let fps = fps.max(1.0);
    let total_frames = (seconds.max(0.0) * fps + 1e-6).floor() as i64;
    let f = total_frames % fps.round() as i64;
    let s = total_frames / fps.round() as i64;
    format!("{:02}:{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60, f)
}

/// Short `M:SS` label for durations.
pub fn short_time(seconds: f64) -> String {
    let s = seconds.max(0.0).round() as i64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timecode_counts_frames() {
        assert_eq!(timecode(0.0, 30.0), "00:00:00:00");
        assert_eq!(timecode(61.5, 30.0), "00:01:01:15");
        assert_eq!(timecode(3600.0, 25.0), "01:00:00:00");
    }

    #[test]
    fn short_time_formats() {
        assert_eq!(short_time(5.2), "00:05");
        assert_eq!(short_time(125.0), "02:05");
        assert_eq!(short_time(3725.0), "1:02:05");
    }
}
