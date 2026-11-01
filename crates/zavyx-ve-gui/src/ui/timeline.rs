//! Bottom panel: toolbar, ruler, tracks with clips, playhead and audio meter.

use eframe::egui::{self, Align2, Color32, CursorIcon, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use zavyx_ve_core::{ClipBlock, ClipId, Rational, TrackId, TrackKind};

use crate::app::{ClipDrag, Drag, DragMode, ZavyxApp};
use crate::style::{self, icon, semibold};

const HEADER_W: f32 = 168.0;
const METER_W: f32 = 46.0;
const RULER_H: f32 = 28.0;
const EDGE: f32 = 7.0;

struct Row {
    id: TrackId,
    kind: TrackKind,
    name: String,
    rect: Rect,
    locked: bool,
    hidden: bool,
    muted: bool,
}

pub fn show(ui: &mut egui::Ui, app: &mut ZavyxApp) {
    toolbar(ui, app);
    ui.add(egui::Separator::default().spacing(0.0));
    let area = ui.available_rect_before_wrap();
    ui.allocate_rect(area, Sense::hover());

    let lanes_x = (area.min.x + HEADER_W, area.max.x - METER_W - 6.0);
    let ruler = Rect::from_min_max(Pos2::new(lanes_x.0, area.min.y), Pos2::new(lanes_x.1, area.min.y + RULER_H));
    let rows_area = Rect::from_min_max(Pos2::new(area.min.x, ruler.max.y), Pos2::new(lanes_x.1, area.max.y - 12.0));
    let hbar = Rect::from_min_max(Pos2::new(lanes_x.0, rows_area.max.y + 2.0), Pos2::new(lanes_x.1, area.max.y - 2.0));
    let meter = Rect::from_min_max(Pos2::new(area.max.x - METER_W, area.min.y + 4.0), Pos2::new(area.max.x - 4.0, area.max.y - 4.0));
    app.lanes_x = lanes_x;

    handle_wheel(ui, app, Rect::from_min_max(ruler.min, rows_area.max), lanes_x.0);
    let rows = layout_rows(app, rows_area);

    // Lane backgrounds and headers.
    let painter = ui.painter_at(rows_area);
    for row in &rows {
        let lane = Rect::from_min_max(Pos2::new(lanes_x.0, row.rect.min.y), Pos2::new(lanes_x.1, row.rect.max.y));
        painter.rect_filled(lane.shrink2(Vec2::new(0.0, 1.0)), 0.0, style::BG.linear_multiply(1.3));
        track_header(ui, app, row, Rect::from_min_max(row.rect.min, Pos2::new(lanes_x.0 - 2.0, row.rect.max.y)), rows_area);
    }
    add_track_buttons(ui, app, &rows, rows_area);

    ruler_ui(ui, app, ruler);
    clips_ui(ui, app, &rows, lanes_x, rows_area);
    drop_targets(ui, app, &rows, lanes_x, rows_area);

    if rows.is_empty() || app.duration() <= 0.0 {
        let empty = Rect::from_min_max(Pos2::new(lanes_x.0, rows_area.min.y), Pos2::new(lanes_x.1, rows_area.max.y));
        if app.seq().tracks.iter().all(|t| t.blocks.is_empty()) {
            let r = Rect::from_center_size(empty.center(), Vec2::new(empty.width().min(520.0), 74.0));
            let p = ui.painter_at(empty);
            p.rect_stroke(r, 8.0, Stroke::new(1.0_f32, style::BORDER));
            p.text(r.center() - Vec2::new(0.0, 12.0), Align2::CENTER_CENTER, icon::IMAGE, FontId::proportional(22.0), style::TEXT_FAINT);
            p.text(r.center() + Vec2::new(0.0, 16.0), Align2::CENTER_CENTER,
                "Drag and drop media and effects here to create your video.", FontId::proportional(13.0), style::TEXT_DIM);
        }
    }

    playhead_line(ui, app, ruler, rows_area, lanes_x);
    scrollbar(ui, app, hbar, lanes_x);
    audio_meter(ui, app, meter);
}

fn x_of(app: &ZavyxApp, lanes_x: (f32, f32), t: f64) -> f32 {
    lanes_x.0 + ((t - app.timeline_scroll) * app.zoom as f64) as f32
}

fn t_of(app: &ZavyxApp, lanes_x: (f32, f32), x: f32) -> f64 {
    app.timeline_scroll + ((x - lanes_x.0) / app.zoom) as f64
}

fn display_name(name: &str, kind: TrackKind) -> String {
    let n = name.trim_start_matches(|c: char| c.is_alphabetic());
    match kind {
        TrackKind::Video => format!("Video {n}"),
        TrackKind::Audio => format!("Audio {n}"),
        TrackKind::Subtitle => format!("Subtitle {n}"),
    }
}

fn layout_rows(app: &ZavyxApp, area: Rect) -> Vec<Row> {
    let seq = app.seq();
    let mut order: Vec<&zavyx_ve_core::Track> = seq.tracks.iter().filter(|t| t.kind == TrackKind::Video).rev().collect();
    order.extend(seq.tracks.iter().filter(|t| t.kind != TrackKind::Video));
    let mut y = area.min.y + 4.0 - app.track_scroll;
    order.into_iter().map(|t| {
        let h = if t.kind == TrackKind::Video { 66.0 } else { 52.0 };
        let rect = Rect::from_min_size(Pos2::new(area.min.x, y), Vec2::new(area.width(), h));
        y += h + 2.0;
        Row { id: t.id, kind: t.kind, name: display_name(&t.name, t.kind), rect, locked: t.locked, hidden: t.hidden, muted: t.muted }
    }).collect()
}

fn handle_wheel(ui: &mut egui::Ui, app: &mut ZavyxApp, area: Rect, lanes_x0: f32) {
    let hovered = ui.ctx().pointer_hover_pos().map(|p| area.contains(p)).unwrap_or(false);
    if !hovered {
        return;
    }
    let (delta, ctrl, shift, pos) = ui.input(|i| (i.raw_scroll_delta, i.modifiers.ctrl, i.modifiers.shift, i.pointer.hover_pos()));
    if delta == Vec2::ZERO {
        return;
    }
    if ctrl {
        let factor = (1.0 + delta.y * 0.002).clamp(0.5, 2.0);
        let anchor_x = pos.map(|p| p.x).unwrap_or(lanes_x0);
        let anchor_t = app.timeline_scroll + ((anchor_x - lanes_x0) / app.zoom) as f64;
        app.zoom = (app.zoom * factor).clamp(2.0, 600.0);
        app.timeline_scroll = (anchor_t - ((anchor_x - lanes_x0) / app.zoom) as f64).max(0.0);
    } else if shift {
        app.track_scroll = (app.track_scroll - delta.y).max(0.0);
    } else {
        let d = if delta.x.abs() > delta.y.abs() { delta.x } else { delta.y };
        app.timeline_scroll = (app.timeline_scroll - (d / app.zoom) as f64).max(0.0);
    }
}

fn toolbar(ui: &mut egui::Ui, app: &mut ZavyxApp) {
    ui.horizontal(|ui| {
        ui.add_space(6.0);
        let sel = app.selected.is_some();
        if style::icon_button(ui, icon::ARROW_COUNTER_CLOCKWISE, "Undo (Ctrl+Z)", app.history.can_undo() || app.live_before.is_some(), false).clicked() { app.undo(); }
        if style::icon_button(ui, icon::ARROW_CLOCKWISE, "Redo (Ctrl+Y)", app.history.can_redo(), false).clicked() { app.redo(); }
        ui.separator();
        if style::icon_button(ui, icon::TRASH, "Delete (Del)", sel, false).clicked() { app.delete_selected(false); }
        if style::icon_button(ui, icon::SCISSORS, "Split at playhead (Ctrl+B)", app.duration() > 0.0, false).clicked() { app.split_at_playhead(); }
        if style::icon_button(ui, icon::COPY, "Duplicate (Ctrl+D)", sel, false).clicked() { app.duplicate_selected(); }
        ui.separator();
        if style::icon_button(ui, icon::TEXT_T, "Add title at playhead", true, false).clicked() {
            app.add_title("title.center_white", None);
        }
        if style::icon_button(ui, icon::BOOKMARK_SIMPLE, "Add marker (M)", true, false).clicked() { app.add_marker(); }
        if style::icon_button(ui, icon::MAGNET, "Snapping", true, app.snap).clicked() { app.snap = !app.snap; }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(METER_W + 4.0);
            if style::icon_button(ui, icon::ARROWS_OUT_LINE_HORIZONTAL, "Fit timeline to window", true, false).clicked() {
                let w = ui.ctx().screen_rect().width() - HEADER_W - METER_W - 40.0;
                app.zoom = (w / app.duration().max(1.0) as f32).clamp(2.0, 600.0);
                app.timeline_scroll = 0.0;
            }
            if style::icon_button(ui, icon::PLUS, "Zoom in (+)", true, false).clicked() { app.zoom = (app.zoom * 1.25).min(600.0); }
            let mut z = app.zoom.ln();
            ui.add(egui::Slider::new(&mut z, 2f32.ln()..=600f32.ln()).show_value(false));
            app.zoom = z.exp();
            if style::icon_button(ui, icon::MINUS, "Zoom out (-)", true, false).clicked() { app.zoom = (app.zoom / 1.25).max(2.0); }
        });
    });
}

fn track_header(ui: &mut egui::Ui, app: &mut ZavyxApp, row: &Row, rect: Rect, clip: Rect) {
    if !clip.intersects(rect) {
        return;
    }
    let p = ui.painter_at(clip);
    p.rect_filled(rect.shrink2(Vec2::new(4.0, 1.0)), 4.0, style::RAISED);
    let glyph = if row.kind == TrackKind::Video { icon::FILM_STRIP } else { icon::MUSIC_NOTES };
    p.text(rect.left_top() + Vec2::new(14.0, 16.0), Align2::LEFT_CENTER, glyph, FontId::proportional(14.0), style::TEXT_DIM);
    p.text(rect.left_top() + Vec2::new(34.0, 16.0), Align2::LEFT_CENTER, &row.name, semibold(12.5), style::TEXT);

    let y = rect.max.y - 18.0;
    let mut x = rect.min.x + 12.0;
    let id = row.id;
    let mut button = |ui: &mut egui::Ui, glyph: &str, on: bool, tip: &str| -> bool {
        let r = Rect::from_center_size(Pos2::new(x + 9.0, y), Vec2::splat(20.0));
        x += 24.0;
        if !clip.contains_rect(r) { return false; }
        let resp = ui.interact(r, ui.id().with(("trk", id, glyph)), Sense::click());
        if resp.hovered() { ui.painter().rect_filled(r, 4.0, style::HOVER); }
        ui.painter().text(r.center(), Align2::CENTER_CENTER, glyph, FontId::proportional(14.0),
            if on { style::VIOLET_TOP } else { style::TEXT_DIM });
        resp.on_hover_text(tip).clicked()
    };
    let lock_glyph = if row.locked { icon::LOCK } else { icon::LOCK_OPEN };
    if button(ui, lock_glyph, row.locked, if row.locked { "Unlock track" } else { "Lock track" }) {
        app.edit("Lock track", |s| if let Some(t) = s.track_mut(id) { t.locked = !t.locked; });
    }
    let (g, on, tip) = if row.kind == TrackKind::Video {
        (if row.hidden { icon::EYE_SLASH } else { icon::EYE }, row.hidden, "Show/hide track")
    } else {
        (if row.muted { icon::SPEAKER_SLASH } else { icon::SPEAKER_HIGH }, row.muted, "Mute track")
    };
    if button(ui, g, on, tip) {
        let video = row.kind == TrackKind::Video;
        app.edit(tip, |s| if let Some(t) = s.track_mut(id) {
            if video { t.hidden = !t.hidden } else { t.muted = !t.muted }
        });
    }
    if row.kind == TrackKind::Video && button(ui, if row.muted { icon::SPEAKER_SLASH } else { icon::SPEAKER_HIGH }, row.muted, "Mute this track's audio") {
        app.edit("Mute track", |s| if let Some(t) = s.track_mut(id) { t.muted = !t.muted; });
    }
    let resp = ui.interact(rect.intersect(clip), ui.id().with(("hdr", id)), Sense::click());
    resp.context_menu(|ui| {
        if ui.button(format!("{}  Remove track", icon::TRASH)).clicked() {
            app.remove_track(id);
            ui.close_menu();
        }
    });
}

fn add_track_buttons(ui: &mut egui::Ui, app: &mut ZavyxApp, rows: &[Row], area: Rect) {
    let y = rows.last().map(|r| r.rect.max.y + 8.0).unwrap_or(area.min.y + 8.0);
    let r = Rect::from_min_size(Pos2::new(area.min.x + 8.0, y), Vec2::new(HEADER_W - 20.0, 22.0));
    if !area.contains_rect(r) {
        return;
    }
    let half = r.width() / 2.0 - 2.0;
    for (i, (kind, label)) in [(TrackKind::Video, "+ Video"), (TrackKind::Audio, "+ Audio")].into_iter().enumerate() {
        let br = Rect::from_min_size(r.min + Vec2::new(i as f32 * (half + 4.0), 0.0), Vec2::new(half, 22.0));
        let resp = ui.interact(br, ui.id().with(("addtrk", i)), Sense::click());
        ui.painter().rect_filled(br, 4.0, if resp.hovered() { style::HOVER } else { style::RAISED.linear_multiply(0.7) });
        ui.painter().text(br.center(), Align2::CENTER_CENTER, label, FontId::proportional(11.5), style::TEXT_DIM);
        if resp.on_hover_text("Add a track").clicked() {
            app.add_track(kind);
        }
    }
}

/// Seconds between labelled ruler ticks for the current zoom.
fn ruler_step(zoom: f32, fps: f64) -> f64 {
    let frame = 1.0 / fps.max(1.0);
    let candidates = [frame, frame * 5.0, 0.5, 1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0, 600.0, 1800.0];
    *candidates.iter().find(|s| (**s as f32) * zoom >= 90.0).unwrap_or(&3600.0)
}

fn ruler_label(t: f64, step: f64, fps: f64) -> String {
    if step < 1.0 {
        let tc = style::timecode(t, fps);
        tc[3..].to_string()
    } else {
        style::short_time(t)
    }
}

fn ruler_ui(ui: &mut egui::Ui, app: &mut ZavyxApp, ruler: Rect) {
    let p = ui.painter_at(ruler);
    p.rect_filled(ruler, 0.0, style::PANEL);
    let lanes_x = (ruler.min.x, ruler.max.x);
    let fps = app.fps();
    let step = ruler_step(app.zoom, fps);
    let minor = step / 5.0;
    let start = (app.timeline_scroll / minor).floor() as i64;
    let end = (t_of(app, lanes_x, ruler.max.x) / minor).ceil() as i64;
    for i in start..=end {
        let t = i as f64 * minor;
        let x = x_of(app, lanes_x, t);
        let major = i % 5 == 0;
        let h = if major { 10.0 } else { 5.0 };
        p.line_segment([Pos2::new(x, ruler.max.y - h), Pos2::new(x, ruler.max.y)],
            Stroke::new(1.0_f32, if major { style::TEXT_DIM } else { style::TEXT_FAINT }));
        if major {
            p.text(Pos2::new(x + 4.0, ruler.min.y + 9.0), Align2::LEFT_CENTER, ruler_label(t, step, fps),
                FontId::monospace(10.5), style::TEXT_DIM);
        }
    }
    // Markers.
    let markers: Vec<(zavyx_ve_core::MarkerId, f64, String)> = app.seq().markers.values()
        .map(|m| (m.id, m.time.to_f64(), m.note.clone())).collect();
    let mut seek_to = None;
    let mut remove = None;
    for (id, t, note) in markers {
        let x = x_of(app, lanes_x, t);
        let r = Rect::from_center_size(Pos2::new(x, ruler.max.y - 8.0), Vec2::new(10.0, 14.0));
        p.add(egui::Shape::convex_polygon(vec![r.left_top(), r.right_top(), r.right_bottom() - Vec2::new(0.0, 4.0),
            r.center_bottom(), r.left_bottom() - Vec2::new(0.0, 4.0)], Color32::from_rgb(0xFF, 0xB0, 0x2E), Stroke::NONE));
        let resp = ui.interact(r, ui.id().with(("marker", id)), Sense::click()).on_hover_text(&note);
        if resp.clicked() { seek_to = Some(t); }
        resp.context_menu(|ui| {
            if ui.button(format!("{}  Delete marker", icon::TRASH)).clicked() { remove = Some(id); ui.close_menu(); }
        });
    }
    if let Some(t) = seek_to { app.seek(t); }
    if let Some(id) = remove {
        app.edit("Delete marker", |s| { s.markers.remove(&id); });
    }

    let resp = ui.interact(ruler, ui.id().with("ruler"), Sense::click_and_drag());
    if resp.clicked() || resp.dragged() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let t = t_of(app, lanes_x, pos.x).max(0.0);
            app.scrubbing = resp.dragged();
            app.seek(t.min(app.duration()));
        }
    }
    if resp.drag_stopped() {
        app.scrubbing = false;
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
}

fn clip_color(app: &ZavyxApp, clip: &ClipBlock, kind: TrackKind) -> Color32 {
    if clip.effects.iter().any(|e| e.effect_id == zavyx_ve_render::TITLE_EFFECT) {
        style::CLIP_TITLE
    } else if kind == TrackKind::Audio || app.media_info(clip.media_id).map(|i| !i.has_video).unwrap_or(false) {
        style::CLIP_AUDIO
    } else {
        style::CLIP_VIDEO
    }
}

fn clips_ui(ui: &mut egui::Ui, app: &mut ZavyxApp, rows: &[Row], lanes_x: (f32, f32), area: Rect) {
    let lanes = Rect::from_min_max(Pos2::new(lanes_x.0, area.min.y), Pos2::new(lanes_x.1, area.max.y));
    let p = ui.painter_at(lanes);
    let mut clicked_empty = false;
    for row in rows {
        let Some(track) = app.seq().track(row.id).cloned() else { continue };
        let lane = Rect::from_min_max(Pos2::new(lanes_x.0, row.rect.min.y), Pos2::new(lanes_x.1, row.rect.max.y));
        if !lanes.intersects(lane) { continue; }
        let lane_resp = ui.interact(lane.intersect(lanes), ui.id().with(("lane", row.id)), Sense::click());
        if lane_resp.clicked() { clicked_empty = true; }

        for clip in track.blocks.iter().filter_map(|b| b.as_clip()) {
            // While moving, the dragged clip is drawn as a ghost elsewhere.
            let dragging = app.clip_drag.as_ref().filter(|d| d.clip == clip.id);
            let (cin, cout) = preview_bounds(app, clip, dragging);
            let x0 = x_of(app, lanes_x, cin);
            let x1 = x_of(app, lanes_x, cout);
            if x1 < lanes.min.x || x0 > lanes.max.x { continue; }
            let mut r = Rect::from_min_max(Pos2::new(x0, lane.min.y + 2.0), Pos2::new(x1.max(x0 + 2.0), lane.max.y - 2.0));
            if let Some(d) = dragging.filter(|d| d.mode == DragMode::Move) {
                if let Some(target) = target_row(app, rows, d) {
                    r = Rect::from_min_max(Pos2::new(x0, target.rect.min.y + 2.0), Pos2::new(x1.max(x0 + 2.0), target.rect.max.y - 2.0));
                }
                p.rect_stroke(Rect::from_min_max(Pos2::new(x_of(app, lanes_x, clip.in_point.to_f64()), lane.min.y + 2.0),
                    Pos2::new(x_of(app, lanes_x, clip.out_point.to_f64()), lane.max.y - 2.0)), 5.0, Stroke::new(1.0_f32, style::TEXT_FAINT));
            }
            draw_clip(ui, app, &p, clip, row.kind, r, row.locked);
            if row.locked { continue; }

            let resp = ui.interact(r.intersect(lanes), ui.id().with(("clip", clip.id)), Sense::click_and_drag());
            style::a11y(&resp, egui::WidgetType::Button, &format!("Clip {}", clip.label));
            let hover_x = resp.hover_pos().map(|p| p.x);
            let near_start = hover_x.map(|x| x - r.min.x < EDGE && r.width() > EDGE * 3.0).unwrap_or(false);
            let near_end = hover_x.map(|x| r.max.x - x < EDGE && r.width() > EDGE * 3.0).unwrap_or(false);
            if (near_start || near_end) && app.clip_drag.is_none() {
                ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
            } else if resp.hovered() && app.clip_drag.is_none() {
                ui.ctx().set_cursor_icon(CursorIcon::Grab);
            }
            if resp.clicked() {
                app.selected = Some(clip.id);
            }
            if resp.drag_started() {
                app.selected = Some(clip.id);
                let press = resp.interact_pointer_pos().map(|p| t_of(app, lanes_x, p.x)).unwrap_or(cin);
                let mode = if near_start { DragMode::TrimStart } else if near_end { DragMode::TrimEnd } else { DragMode::Move };
                app.engine.pause();
                app.clip_drag = Some(ClipDrag { clip: clip.id, track: row.id, mode, grab: press - clip.in_point.to_f64(), before: app.seq().clone() });
            }
            if resp.dragged() {
                ui.ctx().set_cursor_icon(if matches!(app.clip_drag.as_ref().map(|d| d.mode), Some(DragMode::Move)) { CursorIcon::Grabbing } else { CursorIcon::ResizeHorizontal });
            }
            if resp.drag_stopped() {
                finish_drag(app, rows, lanes_x);
            }
            resp.context_menu(|ui| clip_menu(ui, app, clip.id));
        }
    }
    if clicked_empty && app.clip_drag.is_none() {
        app.selected = None;
    }
}

/// Where a clip is drawn: its real place, or the live drag position.
fn preview_bounds(app: &ZavyxApp, clip: &ClipBlock, drag: Option<&ClipDrag>) -> (f64, f64) {
    let (cin, cout) = (clip.in_point.to_f64(), clip.out_point.to_f64());
    let Some(d) = drag else { return (cin, cout) };
    let Some(t) = app.egui_ctx.pointer_interact_pos().map(|p| app.drag_time(p.x)) else { return (cin, cout) };
    match d.mode {
        DragMode::Move => {
            let start = snap_time(app, (t - d.grab).max(0.0), Some(clip.id));
            let start = snap_end(app, start, cout - cin, clip.id);
            (start, start + (cout - cin))
        }
        DragMode::TrimStart => {
            let (min, _) = trim_limits(app, clip);
            (snap_time(app, t, Some(clip.id)).clamp(min, cout - 0.1), cout)
        }
        DragMode::TrimEnd => {
            let (_, max) = trim_limits(app, clip);
            (cin, snap_time(app, t, Some(clip.id)).clamp(cin + 0.1, max))
        }
    }
}

/// Earliest start and latest end a clip can be trimmed to (bounded by its media).
fn trim_limits(app: &ZavyxApp, clip: &ClipBlock) -> (f64, f64) {
    let unlimited = clip.effects.iter().any(|e| e.effect_id == zavyx_ve_render::TITLE_EFFECT)
        || app.media_info(clip.media_id).map(|i| zavyx_ve_media::cli::is_still_image(&i.path)).unwrap_or(false);
    if unlimited || clip.reverse {
        return (0.0, f64::MAX);
    }
    let media_len = app.media_info(clip.media_id).map(|i| i.duration.to_f64()).unwrap_or(f64::MAX);
    let speed = clip.speed.max(0.01);
    let min = (clip.in_point.to_f64() - clip.media_in.to_f64() / speed).max(0.0);
    let max = clip.in_point.to_f64() + (media_len - clip.media_in.to_f64()) / speed;
    (min, max)
}

fn snap_time(app: &ZavyxApp, t: f64, exclude: Option<ClipId>) -> f64 {
    if !app.snap {
        return t;
    }
    let tol = 8.0 / app.zoom as f64;
    let mut best = (tol, t);
    let mut consider = |c: f64| {
        let d = (c - t).abs();
        if d < best.0 { best = (d, c); }
    };
    consider(app.playhead);
    consider(0.0);
    for track in &app.seq().tracks {
        for c in track.blocks.iter().filter_map(|b| b.as_clip()).filter(|c| Some(c.id) != exclude) {
            consider(c.in_point.to_f64());
            consider(c.out_point.to_f64());
        }
    }
    for m in app.seq().markers.values() {
        consider(m.time.to_f64());
    }
    best.1
}

/// Also let a moved clip's end snap.
fn snap_end(app: &ZavyxApp, start: f64, len: f64, id: ClipId) -> f64 {
    if !app.snap { return start; }
    let end = snap_time(app, start + len, Some(id));
    if (end - (start + len)).abs() > 1e-9 && snap_time(app, start, Some(id)) == start { end - len } else { start }
}

fn target_row<'a>(app: &ZavyxApp, rows: &'a [Row], d: &ClipDrag) -> Option<&'a Row> {
    let pos = app.egui_ctx.pointer_interact_pos()?;
    let kind = rows.iter().find(|r| r.id == d.track)?.kind;
    rows.iter().find(|r| r.kind == kind && !r.locked && pos.y >= r.rect.min.y && pos.y < r.rect.max.y)
        .or_else(|| rows.iter().find(|r| r.id == d.track))
}

fn finish_drag(app: &mut ZavyxApp, rows: &[Row], _lanes_x: (f32, f32)) {
    let Some(d) = app.clip_drag.clone() else { return };
    let Some((_, clip)) = app.find_clip(d.clip) else { app.clip_drag = None; return };
    let (start, end) = preview_bounds(app, &clip, Some(&d));
    let to_track = target_row(app, rows, &d).map(|r| r.id).unwrap_or(d.track);
    app.clip_drag = None;
    let (s, e) = (Rational::from_seconds(start), Rational::from_seconds(end));
    if s == clip.in_point && e == clip.out_point && to_track == d.track {
        return;
    }
    let desc = match d.mode { DragMode::Move => "Move clip", _ => "Trim clip" };
    let mode = d.mode;
    app.edit(desc, |seq| {
        let Some(mut c) = seq.track(d.track).and_then(|t| t.clip(d.clip)).cloned() else { return };
        match mode {
            DragMode::Move => {}
            DragMode::TrimStart => {
                let shift = s - c.in_point;
                c.media_in = if c.reverse { c.media_in - shift * Rational::from_seconds(c.speed) }
                    else { c.media_in + shift * Rational::from_seconds(c.speed) };
                let floor = Rational::ZERO;
                if c.media_in < floor { c.media_in = floor; }
            }
            DragMode::TrimEnd => {}
        }
        c.in_point = s;
        c.out_point = e;
        if let Some(t) = seq.track_mut(d.track) { t.remove_clip(d.clip); }
        if let Some(t) = seq.track_mut(to_track) {
            t.clear_range(s, e, &[]);
            t.insert_clip(c);
        }
    });
}

fn clip_menu(ui: &mut egui::Ui, app: &mut ZavyxApp, id: ClipId) {
    app.selected = Some(id);
    if ui.button(format!("{}  Split at playhead", icon::SCISSORS)).clicked() { app.split_at_playhead(); ui.close_menu(); }
    if ui.button(format!("{}  Copy", icon::COPY)).clicked() { app.copy_selected(); ui.close_menu(); }
    if ui.add_enabled(app.clipboard.is_some(), egui::Button::new(format!("{}  Paste at playhead", icon::CLIPBOARD_TEXT))).clicked() { app.paste(); ui.close_menu(); }
    if ui.button(format!("{}  Duplicate", icon::COPY_SIMPLE)).clicked() { app.duplicate_selected(); ui.close_menu(); }
    ui.separator();
    if ui.button(format!("{}  Delete", icon::TRASH)).clicked() { app.delete_selected(false); ui.close_menu(); }
    if ui.button(format!("{}  Ripple delete (close the gap)", icon::ARROW_LINE_LEFT)).clicked() { app.delete_selected(true); ui.close_menu(); }
}

fn draw_clip(ui: &egui::Ui, app: &ZavyxApp, p: &egui::Painter, clip: &ClipBlock, kind: TrackKind, r: Rect, locked: bool) {
    let selected = app.selected == Some(clip.id);
    let base = clip_color(app, clip, kind);
    let p = p.with_clip_rect(r.intersect(p.clip_rect()));
    p.rect_filled(r, 5.0, base);
    let info = app.media_info(clip.media_id);
    let is_title = clip.effects.iter().any(|e| e.effect_id == zavyx_ve_render::TITLE_EFFECT);
    let label_h = 17.0;
    let body = Rect::from_min_max(Pos2::new(r.min.x, r.min.y + label_h), r.max);

    if !is_title && info.map(|i| i.has_video).unwrap_or(false) {
        // Thumbnail tiles.
        if let Some(strip) = app.cache.strips.get(&clip.media_id).filter(|s| !s.is_empty()) {
            let audio_h = if info.map(|i| i.has_audio).unwrap_or(false) { 14.0 } else { 0.0 };
            let th = body.height() - audio_h;
            let tw = (th * 16.0 / 9.0).max(8.0);
            let lanes_x0 = r.min.x - ((clip.in_point.to_f64() - app.timeline_scroll) * app.zoom as f64) as f32;
            let first = (((p.clip_rect().min.x - r.min.x) / tw).floor().max(0.0)) as i32;
            let mut x = r.min.x + first as f32 * tw;
            while x < r.max.x && x < p.clip_rect().max.x {
                let t = app.timeline_scroll + ((x + tw / 2.0 - lanes_x0) / app.zoom) as f64;
                let mt = clip.timeline_to_media_time(Rational::from_seconds(t)).to_f64();
                let tex = strip.iter().min_by(|a, b| (a.0 - mt).abs().partial_cmp(&(b.0 - mt).abs()).unwrap()).map(|(_, t)| t);
                if let Some(tex) = tex {
                    let tile = Rect::from_min_size(Pos2::new(x, body.min.y), Vec2::new(tw, th));
                    let size = tex.size_vec2();
                    let scale = (tile.width() / size.x).max(tile.height() / size.y);
                    let uv_w = (tile.width() / (size.x * scale)).min(1.0);
                    let uv_h = (tile.height() / (size.y * scale)).min(1.0);
                    let uv = Rect::from_center_size(Pos2::new(0.5, 0.5), Vec2::new(uv_w, uv_h));
                    p.image(tex.id(), tile, uv, Color32::from_white_alpha(235));
                }
                x += tw;
            }
            if audio_h > 0.0 {
                let wr = Rect::from_min_max(Pos2::new(r.min.x, r.max.y - audio_h), r.max);
                p.rect_filled(wr, 0.0, style::CLIP_AUDIO.linear_multiply(0.9));
                draw_wave(&p, app, clip, wr);
            }
        }
    } else if !is_title {
        draw_wave(&p, app, clip, body.shrink2(Vec2::new(0.0, 2.0)));
    } else {
        let text = clip.effects.iter().find(|e| e.effect_id == zavyx_ve_render::TITLE_EFFECT)
            .and_then(|e| e.text("text")).unwrap_or("Title").lines().next().unwrap_or("").to_string();
        p.text(body.left_center() + Vec2::new(8.0, 0.0), Align2::LEFT_CENTER, format!("{}  {text}", icon::TEXT_T),
            FontId::proportional(12.5), Color32::from_white_alpha(230));
    }

    // Label bar.
    let bar = Rect::from_min_size(r.min, Vec2::new(r.width(), label_h));
    p.rect_filled(bar, egui::Rounding { nw: 5.0, ne: 5.0, sw: 0.0, se: 0.0 }, Color32::from_black_alpha(90));
    let mut label = clip.label.clone();
    if (clip.speed - 1.0).abs() > 1e-3 { label = format!("{label}  {:.2}x", clip.speed); }
    if clip.reverse { label = format!("{label}  ⟲"); }
    let fx_count = clip.effects.iter().filter(|e| {
        let id = e.effect_id.as_str();
        id != zavyx_ve_render::TITLE_EFFECT && id != crate::mixer::AUDIO_SETTINGS && !id.starts_with(zavyx_ve_render::TRANSITION_PREFIX)
    }).count();
    let text_x = bar.min.x.max(p.clip_rect().min.x) + 6.0;
    if fx_count > 0 {
        p.text(Pos2::new(text_x, bar.center().y), Align2::LEFT_CENTER, icon::SPARKLE, FontId::proportional(11.0), style::ACCENT_TEXT);
    }
    p.text(Pos2::new(text_x + if fx_count > 0 { 16.0 } else { 0.0 }, bar.center().y), Align2::LEFT_CENTER,
        label, FontId::proportional(11.5), Color32::WHITE);

    // Incoming transition wedge.
    if let Some(tr) = clip.effects.iter().find(|e| e.effect_id.starts_with(zavyx_ve_render::TRANSITION_PREFIX)) {
        let d = tr.param("duration", Rational::ZERO).unwrap_or(1.0);
        let w = (d as f32 * app.zoom).min(r.width());
        let wedge = vec![r.left_top(), Pos2::new(r.min.x + w, r.min.y), r.left_bottom()];
        p.add(egui::Shape::convex_polygon(wedge, Color32::from_white_alpha(60), Stroke::new(1.0_f32, Color32::from_white_alpha(140))));
        p.text(Pos2::new(r.min.x + w.min(26.0) / 2.0 + 2.0, r.center().y), Align2::CENTER_CENTER, icon::INTERSECT,
            FontId::proportional(12.0), Color32::WHITE);
    }

    if locked {
        p.rect_filled(r, 5.0, Color32::from_black_alpha(110));
    }
    let stroke = if selected { Stroke::new(2.0_f32, Color32::WHITE) } else { Stroke::new(1.0_f32, Color32::from_black_alpha(140)) };
    p.rect_stroke(r, 5.0, stroke);
    let _ = ui;
}

fn draw_wave(p: &egui::Painter, app: &ZavyxApp, clip: &ClipBlock, r: Rect) {
    let Some(peaks) = app.cache.waves.get(&clip.media_id) else { return };
    if peaks.is_empty() { return; }
    let per_sec = crate::media_cache::WAVE_PER_SECOND as f64;
    let gain = crate::mixer::AudioSettings::of(clip).volume.min(2.0);
    let x_start = r.min.x.max(p.clip_rect().min.x);
    let x_end = r.max.x.min(p.clip_rect().max.x);
    let lanes_x0 = r.min.x - ((clip.in_point.to_f64() - app.timeline_scroll) * app.zoom as f64) as f32;
    let mut x = x_start;
    while x < x_end {
        let t = app.timeline_scroll + ((x - lanes_x0) / app.zoom) as f64;
        let mt = clip.timeline_to_media_time(Rational::from_seconds(t)).to_f64();
        let i = (mt * per_sec) as usize;
        let v = peaks.get(i).copied().unwrap_or(0.0) * gain;
        let h = (v * r.height()).clamp(1.0, r.height());
        p.line_segment([Pos2::new(x, r.center().y - h / 2.0), Pos2::new(x, r.center().y + h / 2.0)], Stroke::new(1.5_f32, style::WAVE));
        x += 2.0;
    }
}

/// Highlight where a library item would land and handle the drop.
fn drop_targets(ui: &mut egui::Ui, app: &mut ZavyxApp, rows: &[Row], lanes_x: (f32, f32), area: Rect) {
    let Some(payload) = egui::DragAndDrop::payload::<Drag>(ui.ctx()) else { return };
    let Some(pos) = ui.ctx().pointer_hover_pos() else { return };
    let lanes = Rect::from_min_max(Pos2::new(lanes_x.0, area.min.y), Pos2::new(lanes_x.1, area.max.y));
    if !lanes.contains(pos) {
        return;
    }
    let t = t_of(app, lanes_x, pos.x).max(0.0);
    let row = rows.iter().find(|r| pos.y >= r.rect.min.y && pos.y < r.rect.max.y);
    let p = ui.painter_at(lanes);
    let released = ui.input(|i| i.pointer.any_released());
    match (&*payload, row) {
        (Drag::Media(id), Some(row)) => {
            let audio_only = app.media_info(*id).map(|i| !i.has_video).unwrap_or(false);
            let ok_kind = (row.kind == TrackKind::Audio) == audio_only;
            let len = app.media_info(*id).map(|i| i.duration.to_f64()).unwrap_or(5.0);
            let start = snap_time(app, t, None);
            if ok_kind {
                let r = Rect::from_min_max(Pos2::new(x_of(app, lanes_x, start), row.rect.min.y + 2.0),
                    Pos2::new(x_of(app, lanes_x, start + len), row.rect.max.y - 2.0));
                p.rect_filled(r, 5.0, style::ACCENT.linear_multiply(0.35));
                p.rect_stroke(r, 5.0, Stroke::new(1.5_f32, style::VIOLET_TOP));
            }
            if released {
                egui::DragAndDrop::clear_payload(ui.ctx());
                app.place_media(*id, row.id, start);
            }
        }
        (Drag::Title(tpl), Some(row)) if row.kind == TrackKind::Video => {
            let start = snap_time(app, t, None);
            let r = Rect::from_min_max(Pos2::new(x_of(app, lanes_x, start), row.rect.min.y + 2.0),
                Pos2::new(x_of(app, lanes_x, start + 5.0), row.rect.max.y - 2.0));
            p.rect_filled(r, 5.0, style::CLIP_TITLE.linear_multiply(0.6));
            if released {
                let tpl = tpl.clone();
                egui::DragAndDrop::clear_payload(ui.ctx());
                app.add_title(&tpl, Some((row.id, start)));
            }
        }
        (Drag::Effect(_) | Drag::Transition(_), Some(row)) => {
            let rt = Rational::from_seconds(t);
            let hit = app.seq().track(row.id).and_then(|tr| tr.block_at(rt)).and_then(|b| b.as_clip()).map(|c| (c.id, c.in_point.to_f64(), c.out_point.to_f64()));
            if let Some((cid, a, b)) = hit {
                let r = Rect::from_min_max(Pos2::new(x_of(app, lanes_x, a), row.rect.min.y + 2.0), Pos2::new(x_of(app, lanes_x, b), row.rect.max.y - 2.0));
                p.rect_stroke(r, 5.0, Stroke::new(2.0_f32, style::VIOLET_TOP));
                if released {
                    let payload = (*payload).clone();
                    egui::DragAndDrop::clear_payload(ui.ctx());
                    match payload {
                        Drag::Effect(e) => app.apply_effect(&e, Some(cid)),
                        Drag::Transition(tr) => app.apply_transition(&tr, Some(cid)),
                        _ => {}
                    }
                }
            }
        }
        _ => {}
    }
}

fn playhead_line(ui: &mut egui::Ui, app: &mut ZavyxApp, ruler: Rect, rows: Rect, lanes_x: (f32, f32)) {
    let x = x_of(app, lanes_x, app.playhead);
    if x < lanes_x.0 - 1.0 || x > lanes_x.1 + 1.0 {
        return;
    }
    let full = Rect::from_min_max(Pos2::new(lanes_x.0, ruler.min.y), Pos2::new(lanes_x.1, rows.max.y));
    let p = ui.painter_at(full);
    p.line_segment([Pos2::new(x, ruler.min.y + 6.0), Pos2::new(x, rows.max.y)], Stroke::new(1.5_f32, style::VIOLET_TOP));
    let head = Rect::from_center_size(Pos2::new(x, ruler.min.y + 8.0), Vec2::new(12.0, 14.0));
    p.add(egui::Shape::convex_polygon(vec![head.left_top(), head.right_top(), head.right_bottom() - Vec2::new(0.0, 5.0),
        head.center_bottom(), head.left_bottom() - Vec2::new(0.0, 5.0)], style::VIOLET_TOP, Stroke::NONE));
}

fn scrollbar(ui: &mut egui::Ui, app: &mut ZavyxApp, bar: Rect, lanes_x: (f32, f32)) {
    let visible = ((lanes_x.1 - lanes_x.0) / app.zoom) as f64;
    let total = (app.duration() + visible * 0.5).max(visible);
    let p = ui.painter();
    p.rect_filled(bar, 4.0, style::BG);
    let w = (bar.width() as f64 * visible / total).max(30.0) as f32;
    let x = bar.min.x + ((app.timeline_scroll / total) as f32 * bar.width()).min(bar.width() - w);
    let thumb = Rect::from_min_size(Pos2::new(x, bar.min.y), Vec2::new(w, bar.height()));
    let resp = ui.interact(bar, ui.id().with("hscroll"), Sense::drag());
    p.rect_filled(thumb, 4.0, if resp.dragged() || resp.hovered() { style::HOVER.linear_multiply(1.6) } else { style::HOVER });
    if resp.dragged() {
        let dx = resp.drag_delta().x as f64;
        app.timeline_scroll = (app.timeline_scroll + dx / bar.width() as f64 * total).clamp(0.0, (total - visible).max(0.0));
    }
}

fn audio_meter(ui: &mut egui::Ui, app: &mut ZavyxApp, r: Rect) {
    let (l, rr) = app.engine.levels();
    let decay = 0.88;
    app.meter = (l.max(app.meter.0 * decay), rr.max(app.meter.1 * decay));
    if app.meter.0 > 0.001 || app.meter.1 > 0.001 {
        ui.ctx().request_repaint();
    }
    let p = ui.painter_at(r);
    p.text(r.center_top() + Vec2::new(0.0, 8.0), Align2::CENTER_CENTER, "dB", FontId::proportional(10.0), style::TEXT_FAINT);
    let bars = Rect::from_min_max(r.min + Vec2::new(4.0, 18.0), Pos2::new(r.min.x + 20.0, r.max.y - 4.0));
    let to_frac = |v: f32| {
        let db = 20.0 * v.max(1e-5).log10();
        ((db + 54.0) / 54.0).clamp(0.0, 1.0)
    };
    for (i, v) in [app.meter.0, app.meter.1].into_iter().enumerate() {
        let col = Rect::from_min_size(bars.min + Vec2::new(i as f32 * 8.0, 0.0), Vec2::new(6.0, bars.height()));
        p.rect_filled(col, 1.0, style::BG);
        let f = to_frac(v);
        let fill = Rect::from_min_max(Pos2::new(col.min.x, col.max.y - col.height() * f), col.max);
        let color = if v > 0.89 { style::ERROR } else if v > 0.5 { Color32::from_rgb(0xF2, 0xC1, 0x4E) } else { style::SUCCESS };
        p.rect_filled(fill, 1.0, color);
    }
    for db in [0, -6, -12, -18, -24, -30, -36, -42, -48, -54] {
        let y = bars.max.y - bars.height() * ((db as f32 + 54.0) / 54.0);
        p.text(Pos2::new(bars.max.x + 4.0, y), Align2::LEFT_CENTER, db.to_string(), FontId::proportional(8.5), style::TEXT_FAINT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ruler_step_grows_as_zoom_shrinks() {
        assert!(ruler_step(600.0, 30.0) < 1.0);
        assert_eq!(ruler_step(60.0, 30.0), 2.0);
        assert!(ruler_step(2.0, 30.0) >= 60.0);
    }

    #[test]
    fn track_names_read_naturally() {
        assert_eq!(display_name("V2", TrackKind::Video), "Video 2");
        assert_eq!(display_name("A1", TrackKind::Audio), "Audio 1");
    }
}
