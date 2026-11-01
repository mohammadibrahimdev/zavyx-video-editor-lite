//! Start screen: create or open a project.

use eframe::egui::{self, Align2, Color32, Rect, RichText, Sense, Vec2};
use zavyx_ve_core::AspectRatio;

use crate::app::ZavyxApp;
use crate::style::{self, icon, semibold};

pub fn show(ctx: &egui::Context, app: &mut ZavyxApp) {
    egui::SidePanel::left("home_nav")
        .exact_width(250.0)
        .resizable(false)
        .frame(egui::Frame::none().fill(style::BG).inner_margin(egui::Margin::symmetric(18.0, 22.0)))
        .show(ctx, |ui| sidebar(ui, app));

    egui::CentralPanel::default()
        .frame(egui::Frame::none().fill(style::PANEL).inner_margin(egui::Margin::symmetric(28.0, 22.0)))
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| main(ui, app));
        });
}

fn sidebar(ui: &mut egui::Ui, app: &mut ZavyxApp) {
    ui.horizontal(|ui| {
        style::logo(ui, 46.0);
        ui.add_space(6.0);
        ui.vertical(|ui| {
            ui.add_space(3.0);
            ui.label(RichText::new("Zavyx").font(semibold(19.0)));
            ui.label(RichText::new("Video Editor").font(semibold(15.0)).color(style::TEXT_DIM));
        });
    });
    ui.add_space(26.0);
    nav_item(ui, icon::PLUS_SQUARE, "Create Project", true);
    if nav_item(ui, icon::FOLDER_OPEN, "Open Project", false).clicked() {
        app.open_dialog();
    }
    if nav_item(ui, icon::INFO, "About", false).clicked() {
        app.about_open = true;
    }

    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
        ui.hyperlink_to(RichText::new("github.com/mohammadibrahimdev").small(), crate::REPO_URL);
        ui.label(RichText::new(format!("Zavyx VE v{} · by Mohammad Ibrahim", crate::VERSION)).small().color(style::TEXT_FAINT));
        ui.label(RichText::new("Free · No ads · No watermark").small().color(style::TEXT_FAINT));
    });
}

fn nav_item(ui: &mut egui::Ui, glyph: &str, text: &str, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 44.0), Sense::click());
    style::a11y(&resp, egui::WidgetType::Button, text);
    let p = ui.painter();
    if selected {
        p.rect_filled(rect, 8.0, style::RAISED);
    } else if resp.hovered() {
        p.rect_filled(rect, 8.0, style::RAISED.linear_multiply(0.6));
    }
    let color = if selected { style::ACCENT_TEXT } else { style::TEXT };
    p.text(rect.left_center() + Vec2::new(16.0, 0.0), Align2::LEFT_CENTER, glyph, egui::FontId::proportional(20.0), color);
    p.text(rect.left_center() + Vec2::new(48.0, 0.0), Align2::LEFT_CENTER, text, semibold(15.0), style::TEXT);
    ui.add_space(4.0);
    resp
}

const ASPECTS: [(AspectRatio, &str, &str); 5] = [
    (AspectRatio::Landscape, "16:9", "Widescreen · YouTube"),
    (AspectRatio::Vertical, "9:16", "Vertical · Shorts, Reels, TikTok"),
    (AspectRatio::Square, "1:1", "Square · Instagram"),
    (AspectRatio::Portrait45, "4:5", "Portrait · Instagram feed"),
    (AspectRatio::Cinematic, "21:9", "Cinematic"),
];

fn main(ui: &mut egui::Ui, app: &mut ZavyxApp) {
    // Aspect ratio row + Open Project.
    ui.horizontal(|ui| {
        for (aspect, label, tip) in ASPECTS.iter().take(2) {
            aspect_chip(ui, app, *aspect, label, tip);
        }
        let more_label = ASPECTS.iter().skip(2).find(|(a, ..)| *a == app.new_aspect)
            .map(|(_, l, _)| format!("{l}  {}", icon::CARET_DOWN))
            .unwrap_or_else(|| format!("More  {}", icon::CARET_DOWN));
        ui.menu_button(RichText::new(more_label).font(semibold(14.0)), |ui| {
            for (aspect, label, tip) in ASPECTS.iter().skip(2) {
                if ui.button(format!("{label}   {tip}")).clicked() {
                    app.new_aspect = *aspect;
                    ui.close_menu();
                }
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(RichText::new(format!("{}  Open Project", icon::FOLDER_OPEN)).font(semibold(14.0))).clicked() {
                app.open_dialog();
            }
        });
    });
    ui.add_space(14.0);

    // Big "New Project" card.
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 170.0), Sense::click());
    style::a11y(&resp, egui::WidgetType::Button, "New Project");
    let boost = if resp.hovered() { 1.1 } else { 1.0 };
    style::gradient_fill(ui.painter(), rect, 12.0,
        style::brighten(style::VIOLET_TOP, boost), style::brighten(Color32::from_rgb(0x3A, 0x1F, 0xB0), boost));
    decorate_card(ui.painter(), rect);
    let p = ui.painter();
    p.text(rect.left_top() + Vec2::new(30.0, 52.0), Align2::LEFT_CENTER, icon::PLUS_SQUARE,
        egui::FontId::proportional(36.0), Color32::WHITE);
    p.text(rect.left_top() + Vec2::new(30.0, 108.0), Align2::LEFT_CENTER, "New Project", semibold(30.0), Color32::WHITE);
    let aspect = ASPECTS.iter().find(|(a, ..)| *a == app.new_aspect).map(|(_, l, t)| format!("{l} · {t}")).unwrap_or_default();
    p.text(rect.left_top() + Vec2::new(32.0, 142.0), Align2::LEFT_CENTER, aspect,
        egui::FontId::proportional(14.0), Color32::from_white_alpha(210));
    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        app.new_project(app.new_aspect);
    }
    ui.add_space(14.0);

    // Quick actions.
    let gap = 12.0;
    let card_w = ((ui.available_width() - gap * 2.0) / 3.0).max(120.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        if quick_card(ui, card_w, icon::UPLOAD_SIMPLE, "Import Media", "Start a project from your videos").clicked() {
            app.new_project(app.new_aspect);
            app.import_dialog();
        }
        if quick_card(ui, card_w, icon::DEVICE_MOBILE, "Vertical Video", "9:16 for Shorts, Reels and TikTok").clicked() {
            app.new_project(AspectRatio::Vertical);
        }
        if quick_card(ui, card_w, icon::FOLDER_OPEN, "Open Project", "Continue a saved .zvep project").clicked() {
            app.open_dialog();
        }
    });
    ui.add_space(20.0);

    if app.recovery.is_some() {
        egui::Frame::none().fill(style::RAISED).rounding(10.0).inner_margin(14.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(icon::CLOCK_COUNTER_CLOCKWISE).size(22.0).color(style::ACCENT_TEXT));
                ui.vertical(|ui| {
                    ui.label(RichText::new("Unsaved work found").font(semibold(15.0)));
                    ui.label(style::dim("Zavyx VE closed before your last project was saved."));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Discard").clicked() { app.discard_autosave(); }
                    if style::primary_button(ui, "Recover").clicked() { app.recover_autosave(); }
                });
            });
        });
        ui.add_space(16.0);
    }

    // Recent projects.
    ui.label(RichText::new("Local Projects").font(semibold(17.0)).color(style::ACCENT_TEXT));
    ui.add_space(8.0);
    let recent = app.settings.recent.clone();
    if recent.is_empty() {
        ui.add_space(30.0);
        ui.vertical_centered(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(120.0, 90.0), Sense::hover());
            empty_box(ui.painter(), rect);
            ui.add_space(8.0);
            ui.label(RichText::new("No recent projects").font(semibold(14.0)));
            ui.label(style::dim("Projects you save will show up here."));
        });
        return;
    }
    let now = crate::settings::now_secs();
    let cols = ((ui.available_width() + gap) / (220.0 + gap)).floor().max(1.0) as usize;
    let w = (ui.available_width() - gap * (cols as f32 - 1.0)) / cols as f32;
    let mut open: Option<std::path::PathBuf> = None;
    let mut forget: Option<std::path::PathBuf> = None;
    for row in recent.chunks(cols) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for r in row {
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 92.0), Sense::click());
                style::a11y(&resp, egui::WidgetType::Button, &format!("Open {}", r.name));
                let p = ui.painter();
                p.rect_filled(rect, 10.0, if resp.hovered() { style::HOVER } else { style::RAISED });
                p.text(rect.left_top() + Vec2::new(16.0, 26.0), Align2::LEFT_CENTER, icon::FILM_STRIP,
                    egui::FontId::proportional(22.0), style::ACCENT_TEXT);
                p.text(rect.left_top() + Vec2::new(46.0, 26.0), Align2::LEFT_CENTER, &r.name, semibold(15.0), style::TEXT);
                p.text(rect.left_top() + Vec2::new(16.0, 56.0), Align2::LEFT_CENTER,
                    crate::settings::age_label(r.opened_at, now), egui::FontId::proportional(12.5), style::TEXT_DIM);
                let folder = r.path.parent().map(|d| d.display().to_string()).unwrap_or_default();
                p.text(rect.left_top() + Vec2::new(16.0, 75.0), Align2::LEFT_CENTER,
                    elide(&folder, (w / 7.0) as usize), egui::FontId::proportional(11.5), style::TEXT_FAINT);
                let resp = resp.on_hover_text(r.path.display().to_string());
                if resp.clicked() { open = Some(r.path.clone()); }
                resp.context_menu(|ui| {
                    if ui.button("Remove from list").clicked() { forget = Some(r.path.clone()); ui.close_menu(); }
                });
            }
        });
        ui.add_space(gap);
    }
    if let Some(p) = open { app.open_path(&p); }
    if let Some(p) = forget {
        app.settings.recent.retain(|r| r.path != p);
        app.settings.save();
    }
}

fn aspect_chip(ui: &mut egui::Ui, app: &mut ZavyxApp, aspect: AspectRatio, label: &str, tip: &str) {
    let on = app.new_aspect == aspect;
    let glyph = if aspect == AspectRatio::Vertical { icon::DEVICE_MOBILE } else { icon::MONITOR_PLAY };
    let text = RichText::new(format!("{glyph}  {label}")).font(semibold(14.0))
        .color(if on { style::ACCENT_TEXT } else { style::TEXT });
    let btn = egui::Button::new(text)
        .fill(if on { style::ACCENT.linear_multiply(0.25) } else { style::RAISED })
        .stroke(if on { egui::Stroke::new(1.0_f32, style::VIOLET_TOP) } else { egui::Stroke::NONE })
        .min_size(Vec2::new(86.0, 34.0));
    if ui.add(btn).on_hover_text(tip).clicked() {
        app.new_aspect = aspect;
    }
}

fn quick_card(ui: &mut egui::Ui, w: f32, glyph: &str, title: &str, sub: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 96.0), Sense::click());
    style::a11y(&resp, egui::WidgetType::Button, title);
    let p = ui.painter();
    p.rect_filled(rect, 10.0, if resp.hovered() { style::HOVER } else { style::RAISED });
    p.text(rect.left_top() + Vec2::new(18.0, 28.0), Align2::LEFT_CENTER, glyph,
        egui::FontId::proportional(24.0), style::ACCENT_TEXT);
    p.text(rect.left_top() + Vec2::new(18.0, 58.0), Align2::LEFT_CENTER, title, semibold(15.0), style::TEXT);
    p.text(rect.left_top() + Vec2::new(18.0, 79.0), Align2::LEFT_CENTER,
        elide(sub, (w / 6.8) as usize), egui::FontId::proportional(12.0), style::TEXT_DIM);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Soft circles on the New Project card.
fn decorate_card(p: &egui::Painter, rect: Rect) {
    let p = p.with_clip_rect(rect);
    p.circle_filled(rect.right_top() + Vec2::new(-90.0, 30.0), 120.0, Color32::from_white_alpha(14));
    p.circle_filled(rect.right_bottom() + Vec2::new(-260.0, 20.0), 80.0, Color32::from_white_alpha(10));
    p.circle_filled(rect.right_bottom() + Vec2::new(-30.0, -10.0), 60.0, Color32::from_white_alpha(12));
}

/// An open, empty box (empty-state picture).
fn empty_box(p: &egui::Painter, r: Rect) {
    let c = r.center();
    let body = Rect::from_center_size(c + Vec2::new(0.0, 14.0), Vec2::new(70.0, 44.0));
    p.rect_filled(body, 4.0, style::HOVER);
    let lid = |dx: f32, tilt: f32| {
        vec![
            egui::pos2(c.x + dx * 35.0, body.min.y),
            egui::pos2(c.x, body.min.y),
            egui::pos2(c.x + dx * 20.0, body.min.y - 22.0 - tilt),
            egui::pos2(c.x + dx * 55.0, body.min.y - 16.0 - tilt),
        ]
    };
    p.add(egui::Shape::convex_polygon(lid(-1.0, 0.0), style::RAISED.linear_multiply(1.6), egui::Stroke::NONE));
    p.add(egui::Shape::convex_polygon(lid(1.0, 4.0), style::BORDER.linear_multiply(1.8), egui::Stroke::NONE));
    p.circle_filled(c + Vec2::new(0.0, -30.0), 3.0, style::VIOLET_TOP.linear_multiply(0.7));
    p.circle_filled(c + Vec2::new(14.0, -40.0), 2.0, style::VIOLET_TOP.linear_multiply(0.5));
}

pub fn elide(s: &str, max: usize) -> String {
    if s.chars().count() <= max.max(4) {
        s.to_string()
    } else {
        let keep: String = s.chars().take(max.max(4) - 1).collect();
        format!("{keep}…")
    }
}
