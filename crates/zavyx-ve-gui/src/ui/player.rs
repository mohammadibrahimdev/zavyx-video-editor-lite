//! Right panel: the preview player with transport controls.

use eframe::egui::{self, Align2, Color32, Rect, RichText, Sense, Vec2};

use crate::app::ZavyxApp;
use crate::style::{self, icon, semibold};

/// Pull the newest frame from the engine into a texture.
pub fn update_texture(ctx: &egui::Context, app: &mut ZavyxApp) {
    if let Some(f) = app.engine.take_new_frame() {
        let img = egui::ColorImage::from_rgba_unmultiplied([f.width as usize, f.height as usize], &f.pixels);
        match &mut app.preview_texture {
            Some(t) => t.set(img, egui::TextureOptions::LINEAR),
            None => app.preview_texture = Some(ctx.load_texture("preview", img, egui::TextureOptions::LINEAR)),
        }
    }
}

pub fn show(ui: &mut egui::Ui, app: &mut ZavyxApp) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Player").font(semibold(14.0)).color(style::ACCENT_TEXT));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if style::icon_button(ui, icon::CAMERA, "Save a snapshot of this frame (PNG)", app.preview_texture.is_some(), false).clicked() {
                snapshot(app);
            }
            let label = match app.preview_scale {
                s if s >= 0.99 => "Full Quality",
                s if s >= 0.49 => "1/2 Quality",
                _ => "1/4 Quality",
            };
            egui::ComboBox::from_id_salt("pq").selected_text(label).width(110.0).show_ui(ui, |ui| {
                for (s, l) in [(1.0, "Full Quality"), (0.5, "1/2 Quality"), (0.25, "1/4 Quality")] {
                    if ui.selectable_label((app.preview_scale - s).abs() < 0.01, l).clicked() {
                        app.preview_scale = s;
                    }
                }
            });
        });
    });
    ui.add_space(4.0);
    let controls_h = 62.0;
    let area = Rect::from_min_size(ui.cursor().min, Vec2::new(ui.available_width(), (ui.available_height() - controls_h).max(60.0)));
    ui.allocate_rect(area, Sense::hover());
    picture(ui, app, area);
    controls(ui, app);
}

/// Draw the current frame letterboxed into `area` and tell the engine its size.
pub fn picture(ui: &mut egui::Ui, app: &mut ZavyxApp, area: Rect) {
    let p = ui.painter_at(area);
    p.rect_filled(area, 4.0, Color32::BLACK);
    let s = &app.project.settings;
    let aspect = s.width as f32 / s.height.max(1) as f32;
    let mut size = Vec2::new(area.width(), area.width() / aspect);
    if size.y > area.height() {
        size = Vec2::new(area.height() * aspect, area.height());
    }
    let frame_rect = Rect::from_center_size(area.center(), size);
    // Render at the on-screen pixel size (times the quality setting).
    let ppp = ui.ctx().pixels_per_point();
    let w = ((size.x * ppp * app.preview_scale) as u32).clamp(64, 1920);
    let h = ((size.y * ppp * app.preview_scale) as u32).clamp(64, 1920);
    app.engine.set_preview_size(w, h);

    if app.duration() <= 0.0 {
        p.text(frame_rect.center(), Align2::CENTER_CENTER, icon::FILM_STRIP, egui::FontId::proportional(40.0), style::TEXT_FAINT);
        p.text(frame_rect.center() + Vec2::new(0.0, 36.0), Align2::CENTER_CENTER,
            "Your video will play here", egui::FontId::proportional(13.0), style::TEXT_FAINT);
    } else if let Some(tex) = &app.preview_texture {
        p.image(tex.id(), frame_rect, Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
    }
    let resp = ui.interact(area, ui.id().with("preview_click"), Sense::click());
    if resp.double_clicked() {
        app.preview_full = !app.preview_full;
    } else if resp.clicked() {
        app.toggle_play();
    }
}

pub fn controls(ui: &mut egui::Ui, app: &mut ZavyxApp) {
    let dur = app.duration().max(0.0);
    let fps = app.fps();
    // Scrub bar.
    let (bar, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 16.0), Sense::click_and_drag());
    let track = Rect::from_center_size(bar.center(), Vec2::new(bar.width() - 8.0, 4.0));
    let p = ui.painter();
    p.rect_filled(track, 2.0, style::HOVER);
    let frac = if dur > 0.0 { (app.playhead / dur).clamp(0.0, 1.0) as f32 } else { 0.0 };
    let filled = Rect::from_min_max(track.min, egui::pos2(track.min.x + track.width() * frac, track.max.y));
    p.rect_filled(filled, 2.0, style::VIOLET_TOP);
    p.circle_filled(egui::pos2(filled.max.x, track.center().y), if resp.hovered() || resp.dragged() { 7.0 } else { 5.0 }, Color32::WHITE);
    if (resp.dragged() || resp.clicked()) && dur > 0.0 {
        if let Some(pos) = resp.interact_pointer_pos() {
            let f = ((pos.x - track.min.x) / track.width()).clamp(0.0, 1.0) as f64;
            app.seek(f * dur);
        }
    }

    ui.horizontal(|ui| {
        let has = dur > 0.0;
        if style::icon_button(ui, icon::SKIP_BACK, "Go to start (Home)", has, false).clicked() { app.seek(0.0); }
        if style::icon_button(ui, icon::CARET_LEFT, "Previous frame (←)", has, false).clicked() { app.step_frames(-1); }
        let playing = app.engine.is_playing();
        let (r, resp) = ui.allocate_exact_size(Vec2::splat(34.0), if has { Sense::click() } else { Sense::hover() });
        ui.painter().circle_filled(r.center(), 16.0, if resp.hovered() && has { style::VIOLET_TOP } else if has { style::ACCENT } else { style::HOVER });
        ui.painter().text(r.center(), Align2::CENTER_CENTER, if playing { icon::PAUSE } else { icon::PLAY },
            egui::FontId::proportional(17.0), Color32::WHITE);
        style::a11y(&resp, egui::WidgetType::Button, if playing { "Pause" } else { "Play" });
        if resp.on_hover_text("Play / Pause (Space)").clicked() && has { app.toggle_play(); }
        if style::icon_button(ui, icon::CARET_RIGHT, "Next frame (→)", has, false).clicked() { app.step_frames(1); }
        if style::icon_button(ui, icon::STOP, "Stop", has, false).clicked() {
            app.engine.pause();
            app.seek(0.0);
        }
        ui.add_space(8.0);
        ui.label(RichText::new(style::timecode(app.playhead, fps)).monospace().color(style::TEXT));
        ui.label(RichText::new(format!("/ {}", style::timecode(dur, fps))).monospace().color(style::TEXT_DIM));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (glyph, tip) = if app.preview_full { (icon::CORNERS_IN, "Exit full screen (Esc)") } else { (icon::CORNERS_OUT, "Full screen preview") };
            if style::icon_button(ui, glyph, tip, true, false).clicked() {
                app.preview_full = !app.preview_full;
            }
        });
    });
}

/// Save the current frame as a PNG next to the project (or in Pictures).
fn snapshot(app: &mut ZavyxApp) {
    if app.duration() <= 0.0 { return; }
    let f = crate::export::render_still(&crate::mixer::snapshot(&app.project), app.playhead);
    let name = format!("{} {}.png", app.project.name, style::timecode(app.playhead, app.fps()).replace(':', "-"));
    let Some(path) = rfd::FileDialog::new().add_filter("PNG image", &["png"]).set_file_name(name).save_file() else { return };
    match image::RgbaImage::from_raw(f.width, f.height, f.pixels).map(|img| img.save(&path)) {
        Some(Ok(())) => app.notify(format!("Snapshot saved: {}", path.display())),
        _ => app.error("Could not save the snapshot"),
    }
}
