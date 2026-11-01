//! Left panel: media pool, titles, transitions, effects and filters.

use eframe::egui::{self, Align2, Color32, Rect, RichText, Sense, Vec2};
use zavyx_ve_core::MediaId;

use crate::app::{Drag, LibTab, ZavyxApp};
use crate::style::{self, icon, semibold};
use crate::ui::home::elide;

const TABS: [(LibTab, &str, &str); 6] = [
    (LibTab::Media, icon::FILM_STRIP, "Media"),
    (LibTab::Audio, icon::MUSIC_NOTES, "Audio"),
    (LibTab::Titles, icon::TEXT_T, "Titles"),
    (LibTab::Transitions, icon::INTERSECT, "Transitions"),
    (LibTab::Effects, icon::SPARKLE, "Effects"),
    (LibTab::Filters, icon::SWATCHES, "Filters"),
];

pub fn show(ui: &mut egui::Ui, app: &mut ZavyxApp) {
    // Category tabs.
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (tab, glyph, label) in TABS {
            let on = app.lib_tab == tab;
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(70.0, 50.0), Sense::click());
            style::a11y(&resp, egui::WidgetType::Button, &format!("{label} tab"));
            let p = ui.painter();
            if resp.hovered() && !on {
                p.rect_filled(rect, 6.0, style::RAISED);
            }
            let color = if on { style::ACCENT_TEXT } else { style::TEXT_DIM };
            p.text(rect.center_top() + Vec2::new(0.0, 16.0), Align2::CENTER_CENTER, glyph, egui::FontId::proportional(20.0), color);
            p.text(rect.center_top() + Vec2::new(0.0, 38.0), Align2::CENTER_CENTER, label,
                if on { semibold(12.0) } else { egui::FontId::proportional(12.0) }, if on { style::TEXT } else { style::TEXT_DIM });
            if on {
                p.rect_filled(Rect::from_center_size(rect.center_bottom(), Vec2::new(26.0, 3.0)), 2.0, style::VIOLET_TOP);
            }
            if resp.clicked() && !on {
                app.lib_tab = tab;
                app.lib_category = 0;
                app.search.clear();
            }
        }
    });
    ui.add(egui::Separator::default().spacing(0.0));

    let cats = categories(app.lib_tab);
    let avail = ui.available_size();
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        // Sub-categories.
        ui.allocate_ui_with_layout(Vec2::new(140.0, avail.y), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_min_size(Vec2::new(140.0, avail.y));
            ui.painter().rect_filled(ui.max_rect(), 0.0, style::BG.linear_multiply(1.15));
            ui.add_space(8.0);
            for (i, name) in cats.iter().enumerate() {
                let on = app.lib_category == i;
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(132.0, 32.0), Sense::click());
                style::a11y(&resp, egui::WidgetType::Button, name);
                let rect = rect.translate(Vec2::new(4.0, 0.0));
                if on {
                    ui.painter().rect_filled(rect, 6.0, style::RAISED);
                } else if resp.hovered() {
                    ui.painter().rect_filled(rect, 6.0, style::RAISED.linear_multiply(0.6));
                }
                ui.painter().text(rect.left_center() + Vec2::new(12.0, 0.0), Align2::LEFT_CENTER, *name,
                    if on { semibold(13.0) } else { egui::FontId::proportional(13.0) },
                    if on { style::TEXT } else { style::TEXT_DIM });
                if resp.clicked() { app.lib_category = i; }
            }
        });
        // Content.
        ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), avail.y), egui::Layout::top_down(egui::Align::Min), |ui| {
            egui::Frame::none().inner_margin(egui::Margin::symmetric(12.0, 8.0)).show(ui, |ui| {
                content_toolbar(ui, app);
                ui.add_space(6.0);
                egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| match app.lib_tab {
                    LibTab::Media | LibTab::Audio => media_grid(ui, app),
                    LibTab::Titles => titles_grid(ui, app),
                    LibTab::Transitions => transitions_grid(ui, app),
                    LibTab::Effects | LibTab::Filters => effects_grid(ui, app),
                });
            });
        });
    });
}

fn categories(tab: LibTab) -> Vec<&'static str> {
    match tab {
        LibTab::Media => vec!["Project Media", "Videos", "Images", "Audio"],
        LibTab::Audio => vec!["Project Audio"],
        LibTab::Titles => vec!["All Titles", "Basic", "Lower Thirds", "Social"],
        LibTab::Transitions => vec!["All", "Dissolve", "Wipe", "Slide & Push", "Shapes", "Dynamic"],
        LibTab::Effects => vec!["All", "Blur", "Distort", "Keying & Masks", "Layout"],
        LibTab::Filters => vec!["All Filters"],
    }
}

fn content_toolbar(ui: &mut egui::Ui, app: &mut ZavyxApp) {
    ui.horizontal(|ui| {
        if matches!(app.lib_tab, LibTab::Media | LibTab::Audio)
            && ui.button(RichText::new(format!("{}  Import", icon::UPLOAD_SIMPLE)).font(semibold(13.0))).clicked()
        {
            app.import_dialog();
        }
        let hint = format!("{}  Search", icon::MAGNIFYING_GLASS);
        ui.add(egui::TextEdit::singleline(&mut app.search).hint_text(hint).desired_width(ui.available_width().min(220.0)));
    });
}

fn matches(app: &ZavyxApp, name: &str) -> bool {
    app.search.trim().is_empty() || name.to_lowercase().contains(&app.search.trim().to_lowercase())
}

/// Grid layout helper: calls `card` for each item in rows that fill the width.
fn grid<T>(ui: &mut egui::Ui, items: &[T], card_w: f32, mut card: impl FnMut(&mut egui::Ui, &T, f32)) {
    let gap = 10.0;
    let cols = ((ui.available_width() + gap) / (card_w + gap)).floor().max(1.0) as usize;
    let w = ((ui.available_width() - gap * (cols as f32 - 1.0)) / cols as f32).floor();
    for row in items.chunks(cols) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for item in row {
                card(ui, item, w);
            }
        });
        ui.add_space(gap);
    }
}

/// Picture card with a label underneath; returns the card response.
fn card(ui: &mut egui::Ui, w: f32, tex: Option<&egui::TextureHandle>, label: &str, badge: Option<String>,
        fallback: &str, selected: bool) -> (egui::Response, Rect) {
    let pic_h = (w * 9.0 / 16.0).round();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, pic_h + 24.0), Sense::click_and_drag());
    style::a11y(&resp, egui::WidgetType::Button, label);
    let pic = Rect::from_min_size(rect.min, Vec2::new(w, pic_h));
    let p = ui.painter();
    p.rect_filled(pic, 6.0, style::RAISED);
    if let Some(tex) = tex {
        let size = tex.size_vec2();
        let scale = (pic.width() / size.x).min(pic.height() / size.y);
        let img = Rect::from_center_size(pic.center(), size * scale);
        p.image(tex.id(), img, Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
    } else {
        p.text(pic.center(), Align2::CENTER_CENTER, fallback, egui::FontId::proportional(26.0), style::TEXT_FAINT);
    }
    if let Some(b) = badge {
        let g = p.layout_no_wrap(b, egui::FontId::proportional(11.0), Color32::WHITE);
        let br = Rect::from_min_size(pic.right_bottom() - g.size() - Vec2::new(10.0, 8.0), g.size() + Vec2::new(6.0, 2.0));
        p.rect_filled(br, 3.0, Color32::from_black_alpha(170));
        p.galley(br.min + Vec2::new(3.0, 1.0), g, Color32::WHITE);
    }
    if selected || resp.hovered() {
        p.rect_stroke(pic, 6.0, egui::Stroke::new(if selected { 2.0_f32 } else { 1.0 }, style::VIOLET_TOP));
    }
    p.text(egui::pos2(rect.min.x + 2.0, pic.max.y + 12.0), Align2::LEFT_CENTER,
        elide(label, (w / 6.6) as usize), egui::FontId::proportional(12.0), style::TEXT);
    (resp, pic)
}

/// A round "+" button in the card corner; true when clicked.
fn add_button(ui: &mut egui::Ui, pic: Rect, visible: bool, name: &str) -> bool {
    let r = Rect::from_center_size(pic.right_top() + Vec2::new(-16.0, 16.0), Vec2::splat(24.0));
    let resp = ui.interact(r, ui.id().with(("add", pic.min.x as i32, pic.min.y as i32)), Sense::click());
    style::a11y(&resp, egui::WidgetType::Button, name);
    if !visible && !resp.hovered() && !resp.has_focus() {
        return resp.clicked();
    }
    ui.painter().circle_filled(r.center(), 12.0, if resp.hovered() { style::VIOLET_TOP } else { style::ACCENT });
    ui.painter().text(r.center(), Align2::CENTER_CENTER, icon::PLUS, egui::FontId::proportional(14.0), Color32::WHITE);
    resp.on_hover_text("Add to timeline").clicked()
}

fn media_grid(ui: &mut egui::Ui, app: &mut ZavyxApp) {
    let audio_tab = app.lib_tab == LibTab::Audio;
    let filter = app.lib_category;
    let mut items: Vec<(MediaId, String, f64, bool, bool, bool)> = app.project.media_pool.items.iter()
        .map(|(id, (r, i))| {
            let still = zavyx_ve_media::cli::is_still_image(&i.path);
            (*id, r.display_name.clone(), i.duration.to_f64(), i.has_video, i.has_audio, still)
        })
        .filter(|(_, name, _, video, _, still)| {
            let kind_ok = if audio_tab { !video } else {
                match filter { 1 => *video && !still, 2 => *still, 3 => !video, _ => true }
            };
            kind_ok && matches(app, name)
        })
        .collect();

    items.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));
    if items.is_empty() {
        empty_media(ui, app, audio_tab);
        return;
    }
    let mut add: Option<MediaId> = None;
    grid(ui, &items, 150.0, |ui, (id, name, dur, video, _, still), w| {
        let tex = app.cache.posters.get(id);
        let badge = if *still { None } else { Some(crate::style::short_time(*dur)) };
        let fallback = if *video { icon::FILM_STRIP } else { icon::MUSIC_NOTES };
        let (resp, pic) = card(ui, w, tex, name, badge, fallback, false);
        if !*video {
            if let Some(wave) = app.cache.waves.get(id) {
                draw_mini_wave(ui.painter(), pic.shrink(10.0), wave);
            }
        }
        let resp = resp.on_hover_text(format!("{name}\nDouble-click or drag to the timeline"));
        if resp.drag_started() {
            egui::DragAndDrop::set_payload(ui.ctx(), Drag::Media(*id));
        }
        if resp.dragged() {
            drag_ghost(ui, name);
        }
        if add_button(ui, pic, resp.hovered(), &format!("Add {name} to timeline")) || resp.double_clicked() {
            add = Some(*id);
        }
        resp.context_menu(|ui| {
            if ui.button(format!("{}  Add to timeline", icon::PLUS)).clicked() { add = Some(*id); ui.close_menu(); }
        });
    });
    if let Some(id) = add {
        app.append_media(id);
    }
}

fn empty_media(ui: &mut egui::Ui, app: &mut ZavyxApp, audio: bool) {
    ui.add_space(6.0);
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 200.0), Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, 10.0, style::RAISED.linear_multiply(0.7));
    p.rect_stroke(rect.shrink(1.0), 10.0, egui::Stroke::new(1.0_f32, if resp.hovered() { style::VIOLET_TOP } else { style::BORDER }));
    p.text(rect.center() - Vec2::new(0.0, 46.0), Align2::CENTER_CENTER, icon::UPLOAD_SIMPLE,
        egui::FontId::proportional(38.0), style::ACCENT_TEXT);
    let btn = Rect::from_center_size(rect.center() + Vec2::new(0.0, 8.0), Vec2::new(96.0, 30.0));
    style::gradient_rect(p, btn, 6.0, if resp.hovered() { 1.1 } else { 1.0 });
    p.text(btn.center(), Align2::CENTER_CENTER, "Import", semibold(14.0), Color32::WHITE);
    let sub = if audio { "Music, voice-overs and sound effects" } else { "Videos, audios, and images" };
    p.text(rect.center() + Vec2::new(0.0, 46.0), Align2::CENTER_CENTER, sub, egui::FontId::proportional(13.0), style::TEXT_DIM);
    p.text(rect.center() + Vec2::new(0.0, 68.0), Align2::CENTER_CENTER, "or drop files onto this window",
        egui::FontId::proportional(11.5), style::TEXT_FAINT);
    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        app.import_dialog();
    }
}

fn draw_mini_wave(p: &egui::Painter, r: Rect, peaks: &[f32]) {
    if peaks.is_empty() { return; }
    let n = (r.width() / 3.0) as usize;
    for i in 0..n {
        let idx = i * peaks.len() / n.max(1);
        let v = peaks[idx.min(peaks.len() - 1)];
        let x = r.min.x + i as f32 * 3.0;
        let h = (v * r.height()).max(1.0);
        p.line_segment([egui::pos2(x, r.center().y - h / 2.0), egui::pos2(x, r.center().y + h / 2.0)],
            egui::Stroke::new(2.0_f32, style::WAVE));
    }
}

fn drag_ghost(ui: &egui::Ui, label: &str) {
    if let Some(pos) = ui.ctx().pointer_interact_pos() {
        let layer = egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("drag_ghost"));
        let p = ui.ctx().layer_painter(layer);
        let g = p.layout_no_wrap(label.to_string(), semibold(12.5), Color32::WHITE);
        let r = Rect::from_min_size(pos + Vec2::new(12.0, 8.0), g.size() + Vec2::new(16.0, 10.0));
        p.rect_filled(r, 6.0, style::ACCENT);
        p.galley(r.min + Vec2::new(8.0, 5.0), g, Color32::WHITE);
    }
}

// --- Titles -------------------------------------------------------------------------

fn title_category(id: &str) -> usize {
    if id.contains("lower") || id.contains("subtitle") || id.contains("callout") || id.contains("credits") { 2 }
    else if id.contains("tiktok") || id.contains("instagram") || id.contains("youtube") || id.contains("end_screen") { 3 }
    else { 1 }
}

fn titles_grid(ui: &mut egui::Ui, app: &mut ZavyxApp) {
    let cat = app.lib_category;
    let templates: Vec<(String, String, [f32; 3])> = zavyx_ve_render::templates::all_templates().into_iter()
        .filter(|t| cat == 0 || title_category(t.id) == cat)
        .filter(|t| matches(app, t.name))
        .map(|t| {
            let c = |k: &str| t.params.get(k).map(|v| v.at(zavyx_ve_core::Rational::ZERO) as f32).unwrap_or(1.0);
            (t.id.to_string(), t.name.to_string(), [c("color_r"), c("color_g"), c("color_b")])
        })
        .collect();
    let mut add: Option<String> = None;
    grid(ui, &templates, 150.0, |ui, (id, name, rgb), w| {
        let (resp, pic) = card(ui, w, None, name, None, "", false);
        let p = ui.painter();
        style::gradient_fill(p, pic, 6.0, Color32::from_rgb(0x24, 0x1F, 0x36), Color32::from_rgb(0x12, 0x10, 0x1C));
        let color = Color32::from_rgb((rgb[0] * 255.0) as u8, (rgb[1] * 255.0) as u8, (rgb[2] * 255.0) as u8);
        let label = if title_category(id) == 2 { "Name Here" } else { "Title" };
        let pos = if title_category(id) == 2 { pic.left_bottom() + Vec2::new(12.0, -18.0) } else { pic.center() };
        let align = if title_category(id) == 2 { Align2::LEFT_CENTER } else { Align2::CENTER_CENTER };
        p.text(pos + Vec2::new(1.5, 1.5), align, label, semibold(20.0), Color32::from_black_alpha(160));
        p.text(pos, align, label, semibold(20.0), color);
        if resp.hovered() {
            p.rect_stroke(pic, 6.0, egui::Stroke::new(1.0_f32, style::VIOLET_TOP));
        }
        let resp = resp.on_hover_text("Double-click or drag to the timeline");
        if resp.drag_started() {
            egui::DragAndDrop::set_payload(ui.ctx(), Drag::Title(id.clone()));
        }
        if resp.dragged() { drag_ghost(ui, name); }
        if add_button(ui, pic, resp.hovered(), &format!("Add {name} to timeline")) || resp.double_clicked() {
            add = Some(id.clone());
        }
    });
    if let Some(id) = add {
        app.add_title(&id, None);
    }
}

// --- Transitions ----------------------------------------------------------------------

fn transition_category(id: &str) -> usize {
    let id = id.trim_start_matches(zavyx_ve_render::TRANSITION_PREFIX);
    if ["dissolve", "dip_to_black", "dip_to_white", "flash"].contains(&id) { 1 }
    else if id.starts_with("wipe_") && !["wipe_circle", "wipe_square", "wipe_diamond", "wipe_star", "wipe_heart"].contains(&id) { 2 }
    else if id.starts_with("slide") || id.starts_with("push") || id == "squeeze" { 3 }
    else if id.starts_with("wipe_") || id.starts_with("iris") { 4 }
    else { 5 }
}

fn transitions_grid(ui: &mut egui::Ui, app: &mut ZavyxApp) {
    let cat = app.lib_category;
    let items: Vec<(String, String)> = zavyx_ve_render::all_transitions().iter()
        .filter(|t| cat == 0 || transition_category(t.id()) == cat)
        .filter(|t| matches(app, t.name()))
        .map(|t| (t.id().to_string(), t.name().to_string()))
        .collect();
    hint(ui, app, "Select a clip, then double-click a transition (or press +) to lead into it — or drag one onto a clip.");
    let mut apply: Option<String> = None;
    grid(ui, &items, 130.0, |ui, (id, name), w| {
        let tex = app.cache.previews.get(&format!("tr:{id}"));
        let (resp, pic) = card(ui, w, tex, name, None, icon::INTERSECT, false);
        if resp.drag_started() {
            egui::DragAndDrop::set_payload(ui.ctx(), Drag::Transition(id.clone()));
        }
        if resp.dragged() { drag_ghost(ui, name); }
        if add_button(ui, pic, resp.hovered(), &format!("Apply {name}")) || resp.double_clicked() {
            apply = Some(id.clone());
        }
    });
    if let Some(id) = apply {
        app.apply_transition(&id, None);
    }
}

// --- Effects and filters -------------------------------------------------------------

const HIDDEN: [&str; 3] = ["zavyx.transform", "zavyx.opacity", "zavyx.title"];
const COLOR: [&str; 10] = ["zavyx.exposure", "zavyx.contrast", "zavyx.highlights", "zavyx.shadows", "zavyx.whites",
    "zavyx.blacks", "zavyx.saturation", "zavyx.vibrance", "zavyx.temperature", "zavyx.tint"];
const FILTERS: [&str; 17] = ["zavyx.black_and_white", "zavyx.sepia", "zavyx.duotone", "zavyx.old_film", "zavyx.vhs",
    "zavyx.glitch", "zavyx.scanlines", "zavyx.crt", "zavyx.posterize", "zavyx.invert", "zavyx.halftone",
    "zavyx.edge_detect", "zavyx.glow", "zavyx.bloom", "zavyx.film_grain", "zavyx.rgb_split", "zavyx.chromatic_aberration"];

/// Ids shown in the Color tab of the inspector instead of the effect list.
pub fn is_color_adjustment(id: &str) -> bool {
    COLOR.contains(&id)
}

fn effect_category(id: &str) -> usize {
    match id {
        "zavyx.gaussian_blur" | "zavyx.box_blur" | "zavyx.motion_blur" | "zavyx.radial_blur" | "zavyx.mosaic" => 1,
        "zavyx.chroma_key" | "zavyx.luma_key" | "zavyx.mask.rectangle" | "zavyx.mask.ellipse" => 3,
        "zavyx.pip" | "zavyx.split_screen" => 4,
        _ => 2,
    }
}

fn effects_grid(ui: &mut egui::Ui, app: &mut ZavyxApp) {
    let filters = app.lib_tab == LibTab::Filters;
    let cat = app.lib_category;
    let items: Vec<(String, String)> = zavyx_ve_render::all_effects().iter()
        .filter(|e| !HIDDEN.contains(&e.id()) && !COLOR.contains(&e.id()))
        .filter(|e| FILTERS.contains(&e.id()) == filters)
        .filter(|e| filters || cat == 0 || effect_category(e.id()) == cat)
        .filter(|e| matches(app, e.name()))
        .map(|e| (e.id().to_string(), e.name().to_string()))
        .collect();
    hint(ui, app, if filters { "Select a clip, then double-click a filter (or press +) to apply it." }
        else { "Select a clip, then double-click an effect (or press +) — or drag it onto a clip." });
    let mut apply: Option<String> = None;
    grid(ui, &items, 130.0, |ui, (id, name), w| {
        let tex = app.cache.previews.get(&format!("fx:{id}"));
        let (resp, pic) = card(ui, w, tex, name, None, icon::SPARKLE, false);
        if resp.drag_started() {
            egui::DragAndDrop::set_payload(ui.ctx(), Drag::Effect(id.clone()));
        }
        if resp.dragged() { drag_ghost(ui, name); }
        if add_button(ui, pic, resp.hovered(), &format!("Apply {name}")) || resp.double_clicked() {
            apply = Some(id.clone());
        }
    });
    if let Some(id) = apply {
        app.apply_effect(&id, None);
    }
}

fn hint(ui: &mut egui::Ui, app: &ZavyxApp, text: &str) {
    if app.selected.is_none() {
        ui.label(RichText::new(format!("{}  {text}", icon::INFO)).small().color(style::TEXT_DIM));
        ui.add_space(4.0);
    }
}
