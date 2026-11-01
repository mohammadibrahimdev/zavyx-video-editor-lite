//! Middle panel: project info, or the selected clip's properties.

use eframe::egui::{self, RichText, Vec2};
use zavyx_ve_core::effect::{ParamType, ParamValue};
use zavyx_ve_core::keyframe::{Easing, Keyframe, KeyframeTrack};
use zavyx_ve_core::{ClipBlock, ClipId, EffectInstance, Rational, TrackKind};

use crate::app::{InspTab, ZavyxApp};
use crate::mixer::{AudioSettings, AUDIO_SETTINGS};
use crate::style::{self, icon, semibold};

pub fn show(ui: &mut egui::Ui, app: &mut ZavyxApp) {
    match app.selected_clip() {
        Some((track, clip)) => {
            let kind = app.seq().track(track).map(|t| t.kind).unwrap_or(TrackKind::Video);
            clip_panel(ui, app, clip, kind);
        }
        None => project_info(ui, app),
    }
}

/// A left-aligned label in a fixed-width column.
fn field_label(ui: &mut egui::Ui, text: impl Into<egui::WidgetText>, width: f32) {
    ui.allocate_ui_with_layout(Vec2::new(width, 22.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_width(width);
        ui.add(egui::Label::new(text).truncate());
    });
}

fn header(ui: &mut egui::Ui, text: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(text).font(semibold(14.5)).color(style::ACCENT_TEXT));
    ui.add_space(6.0);
}

fn info_row(ui: &mut egui::Ui, k: &str, v: impl Into<String>) {
    ui.horizontal(|ui| {
        field_label(ui, style::dim(k), 130.0);
        ui.label(v.into());
    });
}

fn project_info(ui: &mut egui::Ui, app: &mut ZavyxApp) {
    header(ui, "Project Info");
    egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
        ui.horizontal(|ui| {
            field_label(ui, style::dim("Project Name"), 130.0);
            let mut name = app.project.name.clone();
            if ui.add(egui::TextEdit::singleline(&mut name).desired_width(180.0)).changed() {
                app.project.name = name;
            }
        });
        let loc = app.project.last_save_path.as_ref().and_then(|p| p.parent()).map(|p| p.display().to_string())
            .unwrap_or_else(|| "Not saved yet".into());
        info_row(ui, "Project Files Location", loc);
        let s = &app.project.settings;
        info_row(ui, "Resolution", format!("{} x {}", s.width, s.height));
        info_row(ui, "Frame Rate", format!("{:.3}", app.fps()).trim_end_matches('0').trim_end_matches('.').to_string() + " fps");
        info_row(ui, "Color Space", "SDR - Rec.709");
        info_row(ui, "Sample Rate", format!("{} Hz", s.sample_rate));
        info_row(ui, "Duration", style::timecode(app.duration(), app.fps()));
        info_row(ui, "Media Files", app.project.media_pool.items.len().to_string());
        ui.add_space(14.0);
        ui.separator();
        ui.add_space(6.0);
        frame_rate_picker(ui, app);
        ui.add_space(10.0);
        ui.label(style::dim("Tip: click a clip on the timeline to edit its size, color, audio and speed."));
    });
}

fn frame_rate_picker(ui: &mut egui::Ui, app: &mut ZavyxApp) {
    use zavyx_ve_core::FrameRate;
    let rates = [("24", FrameRate::FPS_24), ("25", FrameRate::FPS_25), ("29.97", FrameRate::FPS_29_97),
                 ("30", FrameRate::FPS_30), ("50", FrameRate::FPS_50), ("60", FrameRate::FPS_60)];
    ui.horizontal(|ui| {
        field_label(ui, style::dim("Change frame rate"), 130.0);
        let current = app.project.settings.time_base;
        let label = rates.iter().find(|(_, r)| *r == current).map(|(l, _)| *l).unwrap_or("Custom");
        egui::ComboBox::from_id_salt("fps").selected_text(format!("{label} fps")).show_ui(ui, |ui| {
            for (l, r) in rates {
                if ui.selectable_label(current == r, format!("{l} fps")).clicked() {
                    app.project.settings.time_base = r;
                    if let Some(seq) = app.project.active_sequence_mut() { seq.time_base = r; }
                    app.mark_changed();
                }
            }
        });
    });
}

fn clip_panel(ui: &mut egui::Ui, app: &mut ZavyxApp, clip: ClipBlock, kind: TrackKind) {
    let info = app.media_info(clip.media_id).cloned();
    let is_title = clip.effects.iter().any(|e| e.effect_id == zavyx_ve_render::TITLE_EFFECT);
    let has_video = !is_title && info.as_ref().map(|i| i.has_video).unwrap_or(false);
    let has_audio = info.as_ref().map(|i| i.has_audio).unwrap_or(false);
    let still = info.as_ref().map(|i| zavyx_ve_media::cli::is_still_image(&i.path)).unwrap_or(false);

    let mut tabs = Vec::new();
    if is_title { tabs.push(InspTab::Title); }
    if has_video || is_title { tabs.push(InspTab::Video); }
    if has_video { tabs.push(InspTab::Color); }
    if has_audio { tabs.push(InspTab::Audio); }
    if !is_title && !still && (has_video || has_audio) { tabs.push(InspTab::Speed); }
    if kind == TrackKind::Video { tabs.push(InspTab::Effects); }
    if !tabs.contains(&app.insp_tab) {
        app.insp_tab = tabs.first().copied().unwrap_or(InspTab::Video);
    }

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let glyph = if is_title { icon::TEXT_T } else if has_video { icon::FILM_STRIP } else { icon::MUSIC_NOTES };
        ui.label(RichText::new(glyph).size(18.0).color(style::ACCENT_TEXT));
        ui.label(RichText::new(crate::ui::home::elide(&clip.label, 34)).font(semibold(14.5)));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(style::dim(style::timecode(clip.duration().to_f64(), app.fps())));
        });
    });
    ui.horizontal(|ui| {
        for tab in &tabs {
            let label = match tab {
                InspTab::Title => "Title", InspTab::Video => "Video", InspTab::Color => "Color",
                InspTab::Audio => "Audio", InspTab::Speed => "Speed", InspTab::Effects => "Effects",
            };
            let on = app.insp_tab == *tab;
            let text = RichText::new(label).font(if on { semibold(13.5) } else { egui::FontId::proportional(13.5) })
                .color(if on { style::TEXT } else { style::TEXT_DIM });
            let resp = ui.add(egui::Button::new(text).frame(false).min_size(Vec2::new(0.0, 28.0)));
            if on {
                let r = resp.rect;
                ui.painter().rect_filled(egui::Rect::from_min_max(egui::pos2(r.min.x, r.max.y - 2.0), r.max), 1.0, style::VIOLET_TOP);
            }
            if resp.clicked() { app.insp_tab = *tab; }
        }
    });
    ui.add(egui::Separator::default().spacing(2.0));

    egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
        ui.add_space(6.0);
        match app.insp_tab {
            InspTab::Title => title_tab(ui, app, &clip),
            InspTab::Video => video_tab(ui, app, &clip),
            InspTab::Color => color_tab(ui, app, &clip),
            InspTab::Audio => audio_tab(ui, app, &clip),
            InspTab::Speed => speed_tab(ui, app, &clip),
            InspTab::Effects => effects_tab(ui, app, &clip),
        }
        ui.add_space(20.0);
    });
}

// --- Parameter helpers --------------------------------------------------------------

/// Index of the clip's effect with this id, adding it (with `init`) if missing.
fn ensure_effect(c: &mut ClipBlock, id: &str, init: impl FnOnce(&mut EffectInstance)) -> usize {
    if let Some(i) = c.effects.iter().position(|e| e.effect_id == id) {
        return i;
    }
    let mut fx = EffectInstance::new(id);
    init(&mut fx);
    // Transform/opacity run before other effects.
    let at = if id == "zavyx.transform" || id == "zavyx.opacity" { 0 } else { c.effects.len() };
    c.effects.insert(at, fx);
    at
}

fn local_time(app: &ZavyxApp, clip: &ClipBlock) -> Rational {
    let t = Rational::from_seconds(app.playhead) - clip.in_point;
    if t < Rational::ZERO { Rational::ZERO } else if t > clip.duration() { clip.duration() } else { t }
}

/// A labelled slider bound to one effect parameter, with an optional
/// keyframe diamond. Edits are live; the undo step is made on release.
#[allow(clippy::too_many_arguments)]
fn param_slider(
    ui: &mut egui::Ui, app: &mut ZavyxApp, clip: &ClipBlock, effect_id: &str, param: &str, label: &str,
    range: std::ops::RangeInclusive<f64>, default: f64, keyframes: bool, display: impl Fn(f64) -> String,
) {
    let local = local_time(app, clip);
    let fx = clip.effects.iter().find(|e| e.effect_id == effect_id);
    let value_now = fx.and_then(|e| e.params.get(param)).map(|v| v.at(local)).unwrap_or(default);
    let kf_track = fx.and_then(|e| e.params.get(param)).and_then(|v| match v {
        ParamValue::Keyframed(t) => Some(t.clone()),
        _ => None,
    });
    let half_frame = Rational::from_seconds(0.5 / app.fps());
    let kf_here = kf_track.as_ref().and_then(|t| t.keyframes.iter().find(|k| {
        let d = k.time - local;
        (if d < Rational::ZERO { -d } else { d }) <= half_frame
    }).map(|k| k.time));

    let mut v = value_now;
    let id = clip.id;
    ui.horizontal(|ui| {
        field_label(ui, label, 96.0);
        let resp = ui.add(egui::Slider::new(&mut v, range.clone()).show_value(false));
        ui.add_sized(Vec2::new(54.0, 20.0), egui::Label::new(style::dim(display(v))));
        if resp.changed() {
            let (eid, p, kf) = (effect_id.to_string(), param.to_string(), kf_track.is_some());
            app.live_edit_clip(id, "Change property", move |c| {
                let i = ensure_effect(c, &eid, |_| {});
                let slot = c.effects[i].params.entry(p).or_insert(ParamValue::Const(default));
                match slot {
                    ParamValue::Keyframed(t) if kf => t.add(Keyframe { time: local, value: v, easing: Easing::EaseInOut }),
                    _ => *slot = ParamValue::Const(v),
                }
            });
        }
        if resp.double_clicked() {
            let (eid, p) = (effect_id.to_string(), param.to_string());
            app.edit_clip(id, "Reset property", move |c| {
                if let Some(e) = c.effects.iter_mut().find(|e| e.effect_id == eid) {
                    e.params.insert(p, ParamValue::Const(default));
                }
            });
        }
        if keyframes {
            let tip = if kf_here.is_some() { "Remove keyframe here" } else { "Add keyframe at playhead" };
            if keyframe_button(ui, kf_here.is_some(), kf_track.is_some(), tip).clicked() {
                let (eid, p) = (effect_id.to_string(), param.to_string());
                app.edit_clip(id, "Keyframe", move |c| {
                    let i = ensure_effect(c, &eid, |_| {});
                    let slot = c.effects[i].params.entry(p).or_insert(ParamValue::Const(default));
                    match (slot.clone(), kf_here) {
                        (ParamValue::Keyframed(mut t), Some(at)) => {
                            t.remove_at_time(at);
                            *slot = if t.is_empty() { ParamValue::Const(value_now) } else { ParamValue::Keyframed(t) };
                        }
                        (ParamValue::Keyframed(mut t), None) => {
                            t.add(Keyframe { time: local, value: value_now, easing: Easing::EaseInOut });
                            *slot = ParamValue::Keyframed(t);
                        }
                        _ => {
                            let mut t = KeyframeTrack::new(value_now);
                            t.add(Keyframe { time: local, value: value_now, easing: Easing::EaseInOut });
                            *slot = ParamValue::Keyframed(t);
                        }
                    }
                });
            }
        }
    });
}

/// Diamond toggle: filled when a keyframe sits at the playhead.
fn keyframe_button(ui: &mut egui::Ui, here: bool, animated: bool, tip: &str) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(Vec2::splat(20.0), egui::Sense::click());
    style::a11y(&resp, egui::WidgetType::Button, tip);
    let c = r.center();
    let s = if resp.hovered() { 6.5 } else { 5.5 };
    let pts = vec![c + Vec2::new(0.0, -s), c + Vec2::new(s, 0.0), c + Vec2::new(0.0, s), c + Vec2::new(-s, 0.0)];
    let color = if animated { style::VIOLET_TOP } else { style::TEXT_DIM };
    let fill = if here { color } else { egui::Color32::TRANSPARENT };
    ui.painter().add(egui::Shape::convex_polygon(pts, fill, egui::Stroke::new(1.5_f32, color)));
    resp.on_hover_text(tip)
}

fn pct(v: f64) -> String { format!("{:.0}%", v * 100.0) }
fn signed(v: f64) -> String { format!("{:+.0}", v * 100.0) }

// --- Tabs ------------------------------------------------------------------------------

fn video_tab(ui: &mut egui::Ui, app: &mut ZavyxApp, clip: &ClipBlock) {
    header(ui, "Transform");
    param_slider(ui, app, clip, "zavyx.transform", "scale", "Scale", 0.1..=4.0, 1.0, true, pct);
    param_slider(ui, app, clip, "zavyx.transform", "x", "Position X", -1.0..=1.0, 0.0, true, signed);
    param_slider(ui, app, clip, "zavyx.transform", "y", "Position Y", -1.0..=1.0, 0.0, true, signed);
    header(ui, "Compositing");
    param_slider(ui, app, clip, "zavyx.opacity", "opacity", "Opacity", 0.0..=1.0, 1.0, true, pct);
    ui.add_space(8.0);
    ui.label(style::dim("The diamond adds a keyframe at the playhead. Move the playhead, change the value, and the clip animates between them."));
    ui.add_space(8.0);
    if ui.button(format!("{}  Reset", icon::ARROW_COUNTER_CLOCKWISE)).clicked() {
        app.edit_clip(clip.id, "Reset transform", |c| {
            c.effects.retain(|e| e.effect_id != "zavyx.transform" && e.effect_id != "zavyx.opacity");
        });
    }
}

const COLOR_ROWS: [(&str, &str); 10] = [
    ("zavyx.exposure", "Exposure"), ("zavyx.contrast", "Contrast"), ("zavyx.saturation", "Saturation"),
    ("zavyx.vibrance", "Vibrance"), ("zavyx.temperature", "Temperature"), ("zavyx.tint", "Tint"),
    ("zavyx.highlights", "Highlights"), ("zavyx.shadows", "Shadows"), ("zavyx.whites", "Whites"),
    ("zavyx.blacks", "Blacks"),
];

fn color_tab(ui: &mut egui::Ui, app: &mut ZavyxApp, clip: &ClipBlock) {
    header(ui, "Color");
    for (id, label) in COLOR_ROWS.iter().take(6) {
        param_slider(ui, app, clip, id, "amount", label, -1.0..=1.0, 0.0, false, signed);
    }
    header(ui, "Light");
    for (id, label) in COLOR_ROWS.iter().skip(6) {
        param_slider(ui, app, clip, id, "amount", label, -1.0..=1.0, 0.0, false, signed);
    }
    ui.add_space(8.0);
    ui.label(style::dim("Double-click a slider to reset it."));
    ui.add_space(6.0);
    if ui.button(format!("{}  Reset color", icon::ARROW_COUNTER_CLOCKWISE)).clicked() {
        app.edit_clip(clip.id, "Reset color", |c| c.effects.retain(|e| !crate::ui::library::is_color_adjustment(&e.effect_id)));
    }
}

fn audio_tab(ui: &mut egui::Ui, app: &mut ZavyxApp, clip: &ClipBlock) {
    let s = AudioSettings::of(clip);
    let len = clip.duration().to_f64();
    header(ui, "Volume");
    let id = clip.id;
    let mut volume = s.volume as f64;
    let mut fade_in = s.fade_in;
    let mut fade_out = s.fade_out;
    let mut changed = false;
    ui.horizontal(|ui| {
        field_label(ui, "Volume", 96.0);
        changed |= ui.add(egui::Slider::new(&mut volume, 0.0..=2.0).show_value(false)).changed();
        ui.label(style::dim(format!("{:.0}%", volume * 100.0)));
    });
    header(ui, "Fade");
    ui.horizontal(|ui| {
        field_label(ui, "Fade in", 96.0);
        changed |= ui.add(egui::Slider::new(&mut fade_in, 0.0..=(len / 2.0).max(0.1)).show_value(false)).changed();
        ui.label(style::dim(format!("{fade_in:.1}s")));
    });
    ui.horizontal(|ui| {
        field_label(ui, "Fade out", 96.0);
        changed |= ui.add(egui::Slider::new(&mut fade_out, 0.0..=(len / 2.0).max(0.1)).show_value(false)).changed();
        ui.label(style::dim(format!("{fade_out:.1}s")));
    });
    if changed {
        app.live_edit_clip(id, "Change audio", move |c| {
            let i = ensure_effect(c, AUDIO_SETTINGS, |_| {});
            let fx = &mut c.effects[i];
            fx.set_const("volume", volume);
            fx.set_const("fade_in", fade_in);
            fx.set_const("fade_out", fade_out);
        });
    }
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        if ui.button(format!("{}  Mute clip", icon::SPEAKER_SLASH)).clicked() {
            app.edit_clip(id, "Mute clip", |c| {
                let i = ensure_effect(c, AUDIO_SETTINGS, |_| {});
                c.effects[i].set_const("volume", 0.0);
            });
        }
        if ui.button(format!("{}  Reset", icon::ARROW_COUNTER_CLOCKWISE)).clicked() {
            app.edit_clip(id, "Reset audio", |c| c.effects.retain(|e| e.effect_id != AUDIO_SETTINGS));
        }
    });
}

fn speed_tab(ui: &mut egui::Ui, app: &mut ZavyxApp, clip: &ClipBlock) {
    header(ui, "Uniform Speed");
    let id = clip.id;
    let mut speed = clip.speed;
    ui.horizontal(|ui| {
        field_label(ui, "Speed", 96.0);
        let resp = ui.add(egui::Slider::new(&mut speed, 0.1..=8.0).logarithmic(true).show_value(false));
        ui.label(style::dim(format!("{speed:.2}x")));
        if resp.drag_stopped() || (resp.changed() && !resp.dragged()) {
            set_speed(app, id, speed);
        }
    });
    ui.horizontal(|ui| {
        ui.add_space(100.0);
        for s in [0.25, 0.5, 1.0, 2.0, 4.0] {
            if ui.selectable_label((clip.speed - s).abs() < 1e-3, format!("{s}x")).clicked() {
                set_speed(app, id, s);
            }
        }
    });
    ui.add_space(4.0);
    info_row(ui, "Duration", style::timecode(clip.duration().to_f64(), app.fps()));
    let mut rev = clip.reverse;
    if ui.checkbox(&mut rev, "Reverse (plays backwards, audio is muted)").changed() {
        app.edit_clip(id, "Reverse", move |c| {
            c.reverse = rev;
            // A reversed clip starts at the end of its media range.
            let span = c.media_duration();
            c.media_in = if rev { c.media_in + span } else { c.media_in - span };
        });
    }
}

/// Change speed and stretch the clip so it still shows the same media.
fn set_speed(app: &mut ZavyxApp, id: ClipId, speed: f64) {
    let Some((track, c)) = app.find_clip(id) else { return };
    let media_len = c.duration().to_f64() * c.speed;
    let new_out = c.in_point + Rational::from_seconds(media_len / speed);
    app.edit("Change speed", |seq| {
        if let Some(t) = seq.track_mut(track) {
            if let Some(cm) = t.clip_mut(id) {
                cm.speed = speed;
                cm.out_point = new_out;
            }
            t.clear_range(c.in_point, new_out, &[id]);
        }
    });
}

fn effects_tab(ui: &mut egui::Ui, app: &mut ZavyxApp, clip: &ClipBlock) {
    let id = clip.id;
    // Incoming transition.
    if let Some(tr) = clip.effects.iter().find(|e| e.effect_id.starts_with(zavyx_ve_render::TRANSITION_PREFIX)).cloned() {
        header(ui, "Transition");
        let name = zavyx_ve_render::lookup_transition(&tr.effect_id).map(|t| t.name().to_string()).unwrap_or(tr.effect_id.clone());
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{}  {name}", icon::INTERSECT)).font(semibold(13.5)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if style::icon_button(ui, icon::TRASH, "Remove transition", true, false).clicked() {
                    app.edit_clip(id, "Remove transition", |c| c.effects.retain(|e| !e.effect_id.starts_with(zavyx_ve_render::TRANSITION_PREFIX)));
                }
            });
        });
        let mut dur = tr.param("duration", Rational::ZERO).unwrap_or(1.0);
        ui.horizontal(|ui| {
            field_label(ui, "Duration", 96.0);
            let max = clip.duration().to_f64().max(0.1);
            if ui.add(egui::Slider::new(&mut dur, 0.1..=max.min(5.0)).show_value(false)).changed() {
                let tid = tr.effect_id.clone();
                app.live_edit_clip(id, "Transition duration", move |c| {
                    if let Some(e) = c.effects.iter_mut().find(|e| e.effect_id == tid) { e.set_const("duration", dur); }
                });
            }
            ui.label(style::dim(format!("{dur:.1}s")));
        });
        ui.add_space(6.0);
    }

    header(ui, "Effects");
    let list: Vec<(usize, EffectInstance)> = clip.effects.iter().cloned().enumerate()
        .filter(|(_, e)| {
            let id = e.effect_id.as_str();
            !(id == "zavyx.transform" || id == "zavyx.opacity" || id == zavyx_ve_render::TITLE_EFFECT
                || id == AUDIO_SETTINGS || id.starts_with(zavyx_ve_render::TRANSITION_PREFIX)
                || crate::ui::library::is_color_adjustment(id))
        })
        .collect();
    if list.is_empty() {
        ui.label(style::dim("No effects yet. Pick one in the Effects or Filters tab on the left."));
        return;
    }
    for (index, fx) in list {
        let Some(effect) = zavyx_ve_render::lookup_effect(&fx.effect_id) else { continue };
        egui::Frame::none().fill(style::RAISED).rounding(8.0).inner_margin(10.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let mut on = fx.enabled;
                if ui.checkbox(&mut on, "").on_hover_text("Turn on/off").changed() {
                    let eid = fx.id;
                    app.edit_clip(id, "Toggle effect", move |c| {
                        if let Some(e) = c.effects.iter_mut().find(|e| e.id == eid) { e.enabled = on; }
                    });
                }
                ui.label(RichText::new(effect.name()).font(semibold(13.5)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if style::icon_button(ui, icon::TRASH, "Remove effect", true, false).clicked() {
                        let eid = fx.id;
                        app.edit_clip(id, "Remove effect", move |c| c.effects.retain(|e| e.id != eid));
                    }
                });
            });
            for def in effect.param_schema().params {
                let current = fx.params.get(&def.id).cloned();
                let eid = fx.id;
                let pid = def.id.clone();
                match def.ty {
                    ParamType::Float { min, max, default, .. } => {
                        let mut v = current.map(|v| v.at(Rational::ZERO)).unwrap_or(default);
                        ui.horizontal(|ui| {
                            field_label(ui, &def.label, 96.0);
                            if ui.add(egui::Slider::new(&mut v, min..=max).show_value(false)).changed() {
                                app.live_edit_clip(id, "Change effect", move |c| {
                                    if let Some(e) = c.effects.iter_mut().find(|e| e.id == eid) { e.set_const(pid, v); }
                                });
                            }
                            ui.label(style::dim(format!("{v:.2}")));
                        });
                    }
                    ParamType::Int { min, max, default } => {
                        let mut v = current.map(|v| v.at(Rational::ZERO) as i64).unwrap_or(default);
                        ui.horizontal(|ui| {
                            field_label(ui, &def.label, 96.0);
                            if ui.add(egui::Slider::new(&mut v, min..=max)).changed() {
                                app.live_edit_clip(id, "Change effect", move |c| {
                                    if let Some(e) = c.effects.iter_mut().find(|e| e.id == eid) { e.set_const(pid, v as f64); }
                                });
                            }
                        });
                    }
                    ParamType::Bool { default } => {
                        let mut v = current.map(|v| v.at(Rational::ZERO) > 0.5).unwrap_or(default);
                        if ui.checkbox(&mut v, &def.label).changed() {
                            app.edit_clip(id, "Change effect", move |c| {
                                if let Some(e) = c.effects.iter_mut().find(|e| e.id == eid) { e.set_const(pid, v as i32 as f64); }
                            });
                        }
                    }
                    ParamType::Choice { options, default } => {
                        let idx = current.map(|v| v.at(Rational::ZERO).round() as usize)
                            .unwrap_or_else(|| options.iter().position(|o| *o == default).unwrap_or(0));
                        ui.horizontal(|ui| {
                            field_label(ui, &def.label, 96.0);
                            egui::ComboBox::from_id_salt(("choice", index, &def.id))
                                .selected_text(options.get(idx).cloned().unwrap_or_default())
                                .show_ui(ui, |ui| {
                                    for (i, o) in options.iter().enumerate() {
                                        if ui.selectable_label(i == idx, o).clicked() {
                                            let pid = pid.clone();
                                            app.edit_clip(id, "Change effect", move |c| {
                                                if let Some(e) = c.effects.iter_mut().find(|e| e.id == eid) { e.set_const(pid, i as f64); }
                                            });
                                        }
                                    }
                                });
                        });
                    }
                    _ => {}
                }
            }
        });
        ui.add_space(8.0);
    }
}

const FONTS: [(&str, &str); 10] = [
    ("Segoe UI Bold", "segoeuib.ttf"), ("Segoe UI", "segoeui.ttf"), ("Arial Bold", "arialbd.ttf"),
    ("Arial", "arial.ttf"), ("Impact", "impact.ttf"), ("Georgia", "georgia.ttf"), ("Verdana", "verdana.ttf"),
    ("Trebuchet", "trebucbd.ttf"), ("Consolas", "consola.ttf"), ("Comic Sans", "comicbd.ttf"),
];

fn title_tab(ui: &mut egui::Ui, app: &mut ZavyxApp, clip: &ClipBlock) {
    let id = clip.id;
    let fx = clip.effects.iter().find(|e| e.effect_id == zavyx_ve_render::TITLE_EFFECT).cloned().unwrap_or_else(|| EffectInstance::new(zavyx_ve_render::TITLE_EFFECT));
    let get = |k: &str, d: f64| fx.params.get(k).map(|v| v.at(Rational::ZERO)).unwrap_or(d);
    let set = move |app: &mut ZavyxApp, desc: &str, k: &'static str, v: f64| {
        app.live_edit_clip(id, desc, move |c| {
            let i = ensure_effect(c, zavyx_ve_render::TITLE_EFFECT, |_| {});
            c.effects[i].set_const(k, v);
        });
    };

    header(ui, "Text");
    let mut text = fx.text("text").unwrap_or("Your Title").to_string();
    let resp = ui.add(egui::TextEdit::multiline(&mut text).desired_rows(3).desired_width(f32::INFINITY));
    if resp.changed() {
        let t = text.clone();
        app.live_edit_clip(id, "Edit title", move |c| {
            let i = ensure_effect(c, zavyx_ve_render::TITLE_EFFECT, |_| {});
            c.effects[i].set_text("text", t.clone());
            c.label = t.lines().next().unwrap_or("Title").to_string();
        });
    }
    if resp.lost_focus() { app.commit_live(); }

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        field_label(ui, "Font", 96.0);
        let current = fx.text("font").unwrap_or("segoeuib.ttf").to_string();
        let shown = FONTS.iter().find(|(_, f)| *f == current).map(|(n, _)| *n).unwrap_or("Segoe UI Bold");
        egui::ComboBox::from_id_salt("title_font").selected_text(shown).show_ui(ui, |ui| {
            for (name, file) in FONTS {
                if font_exists(file) && ui.selectable_label(file == current, name).clicked() {
                    app.edit_clip(id, "Change font", move |c| {
                        let i = ensure_effect(c, zavyx_ve_render::TITLE_EFFECT, |_| {});
                        c.effects[i].set_text("font", file);
                    });
                }
            }
        });
    });
    let mut size = get("size", 64.0);
    ui.horizontal(|ui| {
        field_label(ui, "Size", 96.0);
        if ui.add(egui::Slider::new(&mut size, 8.0..=300.0).show_value(false)).changed() { set(app, "Title size", "size", size); }
        ui.label(style::dim(format!("{size:.0}")));
    });
    let mut rgb = [get("color_r", 1.0) as f32, get("color_g", 1.0) as f32, get("color_b", 1.0) as f32];
    ui.horizontal(|ui| {
        field_label(ui, "Color", 96.0);
        if ui.color_edit_button_rgb(&mut rgb).changed() {
            app.live_edit_clip(id, "Title color", move |c| {
                let i = ensure_effect(c, zavyx_ve_render::TITLE_EFFECT, |_| {});
                c.effects[i].set_const("color_r", rgb[0] as f64);
                c.effects[i].set_const("color_g", rgb[1] as f64);
                c.effects[i].set_const("color_b", rgb[2] as f64);
            });
        }
        for (name, c) in [("White", [1.0, 1.0, 1.0]), ("Violet", [0.55, 0.42, 1.0]), ("Yellow", [1.0, 0.85, 0.2]), ("Black", [0.0, 0.0, 0.0])] {
            let col = egui::Color32::from_rgb((c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8);
            let (r, resp) = ui.allocate_exact_size(Vec2::splat(18.0), egui::Sense::click());
            ui.painter().circle_filled(r.center(), 8.0, col);
            ui.painter().circle_stroke(r.center(), 8.0, egui::Stroke::new(1.0_f32, style::BORDER));
            if resp.on_hover_text(name).clicked() {
                app.edit_clip(id, "Title color", move |cl| {
                    let i = ensure_effect(cl, zavyx_ve_render::TITLE_EFFECT, |_| {});
                    cl.effects[i].set_const("color_r", c[0]);
                    cl.effects[i].set_const("color_g", c[1]);
                    cl.effects[i].set_const("color_b", c[2]);
                });
            }
        }
    });
    let align = get("align", 0.0).round() as i32;
    ui.horizontal(|ui| {
        field_label(ui, "Align", 96.0);
        for (i, glyph, tip) in [(0, icon::TEXT_ALIGN_LEFT, "Left"), (1, icon::TEXT_ALIGN_CENTER, "Center"), (2, icon::TEXT_ALIGN_RIGHT, "Right")] {
            if style::icon_button(ui, glyph, tip, true, align == i).clicked() {
                app.edit_clip(id, "Align title", move |c| {
                    let k = ensure_effect(c, zavyx_ve_render::TITLE_EFFECT, |_| {});
                    let x = match i { 1 => 0.5, 2 => 0.9, _ => 0.1 };
                    c.effects[k].set_const("align", i as f64);
                    c.effects[k].set_const("x", x);
                });
            }
        }
    });

    header(ui, "Position");
    let mut x = get("x", 0.1);
    let mut y = get("y", 0.4);
    ui.horizontal(|ui| {
        field_label(ui, "Horizontal", 96.0);
        if ui.add(egui::Slider::new(&mut x, 0.0..=1.0).show_value(false)).changed() { set(app, "Move title", "x", x); }
        ui.label(style::dim(pct(x)));
    });
    ui.horizontal(|ui| {
        field_label(ui, "Vertical", 96.0);
        if ui.add(egui::Slider::new(&mut y, 0.0..=1.0).show_value(false)).changed() { set(app, "Move title", "y", y); }
        ui.label(style::dim(pct(y)));
    });

    header(ui, "Style");
    for (k, label, max, d) in [("shadow", "Shadow", 1.0, 0.0), ("outline", "Outline", 12.0, 0.0), ("bg_opacity", "Background", 1.0, 0.0), ("opacity", "Opacity", 1.0, 1.0)] {
        let mut v = get(k, d);
        ui.horizontal(|ui| {
            field_label(ui, label, 96.0);
            if ui.add(egui::Slider::new(&mut v, 0.0..=max).show_value(false)).changed() { set(app, "Title style", k, v); }
            ui.label(style::dim(if max > 1.0 { format!("{v:.0}") } else { pct(v) }));
        });
    }
}

fn font_exists(file: &str) -> bool {
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
    std::path::Path::new(&format!("{windir}\\Fonts\\{file}")).is_file()
}
