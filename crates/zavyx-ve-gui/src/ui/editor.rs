//! Editor screen layout, top bar, keyboard shortcuts and dialogs.

use eframe::egui::{self, Align2, Color32, Key, Modifiers, RichText, Vec2};

use crate::app::{Screen, ZavyxApp};
use crate::export::{Format, Quality, State};
use crate::style::{self, icon, semibold};
use crate::ui::{inspector, library, player, timeline};

pub fn show(ctx: &egui::Context, app: &mut ZavyxApp) {
    shortcuts(ctx, app);
    dropped_files(ctx, app);

    egui::TopBottomPanel::top("topbar")
        .exact_height(44.0)
        .frame(egui::Frame::none().fill(style::BG).inner_margin(egui::Margin::symmetric(10.0, 6.0)))
        .show(ctx, |ui| topbar(ui, app));

    if app.preview_full {
        egui::CentralPanel::default().frame(egui::Frame::none().fill(Color32::BLACK).inner_margin(12.0)).show(ctx, |ui| {
            let area = egui::Rect::from_min_size(ui.cursor().min, Vec2::new(ui.available_width(), ui.available_height() - 64.0));
            ui.allocate_rect(area, egui::Sense::hover());
            player::picture(ui, app, area);
            player::controls(ui, app);
        });
        return;
    }

    let screen = ctx.screen_rect();
    egui::TopBottomPanel::bottom("timeline")
        .resizable(true)
        .default_height((screen.height() * 0.42).max(240.0))
        .min_height(180.0)
        .frame(egui::Frame::none().fill(style::PANEL).inner_margin(egui::Margin { left: 0.0, right: 0.0, top: 4.0, bottom: 0.0 }))
        .show(ctx, |ui| timeline::show(ui, app));

    egui::SidePanel::left("library")
        .resizable(true)
        .default_width((screen.width() * 0.36).max(420.0))
        .min_width(330.0)
        .frame(egui::Frame::none().fill(style::PANEL).inner_margin(egui::Margin::symmetric(6.0, 4.0)))
        .show(ctx, |ui| library::show(ui, app));

    egui::SidePanel::right("player")
        .resizable(true)
        .default_width((screen.width() * 0.36).max(380.0))
        .min_width(300.0)
        .frame(egui::Frame::none().fill(style::PANEL).inner_margin(egui::Margin::symmetric(12.0, 8.0)))
        .show(ctx, |ui| player::show(ui, app));

    egui::CentralPanel::default()
        .frame(egui::Frame::none().fill(style::PANEL).inner_margin(egui::Margin::symmetric(14.0, 8.0)))
        .show(ctx, |ui| inspector::show(ui, app));
}

fn topbar(ui: &mut egui::Ui, app: &mut ZavyxApp) {
    ui.horizontal_centered(|ui| {
        if style::logo(ui, 28.0).on_hover_text("Back to Home").interact(egui::Sense::click()).clicked() {
            go_home(app);
        }
        ui.add_space(4.0);
        ui.menu_button("File", |ui| {
            if ui.button("New Project").clicked() { ui.close_menu(); go_home(app); }
            if ui.button("Open Project…          Ctrl+O").clicked() { ui.close_menu(); app.open_dialog(); }
            ui.separator();
            if ui.button("Save Project            Ctrl+S").clicked() { ui.close_menu(); app.save(); }
            if ui.button("Save Project As…   Ctrl+Shift+S").clicked() { ui.close_menu(); app.save_as(); }
            ui.separator();
            if ui.button("Import Media…          Ctrl+I").clicked() { ui.close_menu(); app.import_dialog(); }
            if ui.button("Export…                    Ctrl+E").clicked() { ui.close_menu(); app.open_export(); }
            ui.separator();
            if ui.button("Back to Home").clicked() { ui.close_menu(); go_home(app); }
            if ui.button("Exit").clicked() { ui.close_menu(); ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close); }
        });
        ui.menu_button("Edit", |ui| {
            if ui.add_enabled(app.history.can_undo() || app.live_before.is_some(), egui::Button::new("Undo                 Ctrl+Z")).clicked() { ui.close_menu(); app.undo(); }
            if ui.add_enabled(app.history.can_redo(), egui::Button::new("Redo                 Ctrl+Y")).clicked() { ui.close_menu(); app.redo(); }
            ui.separator();
            let sel = app.selected.is_some();
            if ui.add_enabled(sel, egui::Button::new("Copy                 Ctrl+C")).clicked() { ui.close_menu(); app.copy_selected(); }
            if ui.add_enabled(app.clipboard.is_some(), egui::Button::new("Paste                Ctrl+V")).clicked() { ui.close_menu(); app.paste(); }
            if ui.add_enabled(sel, egui::Button::new("Duplicate          Ctrl+D")).clicked() { ui.close_menu(); app.duplicate_selected(); }
            if ui.add_enabled(sel, egui::Button::new("Delete               Del")).clicked() { ui.close_menu(); app.delete_selected(false); }
            if ui.add_enabled(sel, egui::Button::new("Ripple Delete    Shift+Del")).clicked() { ui.close_menu(); app.delete_selected(true); }
            ui.separator();
            if ui.button("Split at Playhead  Ctrl+B").clicked() { ui.close_menu(); app.split_at_playhead(); }
            if ui.button("Add Marker           M").clicked() { ui.close_menu(); app.add_marker(); }
        });
        ui.menu_button("View", |ui| {
            if ui.button("Zoom In              +").clicked() { ui.close_menu(); app.zoom = (app.zoom * 1.25).min(600.0); }
            if ui.button("Zoom Out             -").clicked() { ui.close_menu(); app.zoom = (app.zoom / 1.25).max(2.0); }
            ui.checkbox(&mut app.snap, "Snapping");
            if ui.button("Full Screen Preview").clicked() { ui.close_menu(); app.preview_full = true; }
        });
        ui.menu_button("Help", |ui| {
            if ui.button("Keyboard Shortcuts").clicked() { ui.close_menu(); app.shortcuts_open = true; }
            if ui.button("About Zavyx VE").clicked() { ui.close_menu(); app.about_open = true; }
        });

        // Project name in the middle.
        let title = format!("{}{}", app.project.name, if app.is_dirty() { " •" } else { "" });
        let center = ui.max_rect().center();
        ui.painter().text(center, Align2::CENTER_CENTER, title, semibold(14.0), style::TEXT_DIM);

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if style::primary_button(ui, &format!("{}  Export", icon::EXPORT)).on_hover_text("Export video (Ctrl+E)").clicked() {
                app.open_export();
            }
            ui.add_space(6.0);
            if style::icon_button(ui, icon::FLOPPY_DISK, "Save project (Ctrl+S)", true, false).clicked() {
                app.save();
            }
        });
    });
}

fn go_home(app: &mut ZavyxApp) {
    app.commit_live();
    if app.is_dirty() {
        app.confirm_close = true;
        app.allow_close = false;
        app.pending_home = true;
        return;
    }
    app.engine.pause();
    app.screen = Screen::Home;
}

fn shortcuts(ctx: &egui::Context, app: &mut ZavyxApp) {
    if ctx.wants_keyboard_input() {
        return;
    }
    let ctrl = Modifiers::COMMAND;
    let shift_ctrl = Modifiers::COMMAND | Modifiers::SHIFT;
    let mut pressed = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_key(m, k));
    if pressed(Modifiers::NONE, Key::Space) { app.toggle_play(); }
    if pressed(shift_ctrl, Key::Z) || pressed(ctrl, Key::Y) { app.redo(); }
    if pressed(ctrl, Key::Z) { app.undo(); }
    if pressed(shift_ctrl, Key::S) { app.save_as(); }
    if pressed(ctrl, Key::S) { app.save(); }
    if pressed(ctrl, Key::O) { app.open_dialog(); }
    if pressed(ctrl, Key::I) { app.import_dialog(); }
    if pressed(ctrl, Key::E) { app.open_export(); }
    if pressed(ctrl, Key::B) { app.split_at_playhead(); }
    if pressed(ctrl, Key::C) { app.copy_selected(); }
    if pressed(ctrl, Key::V) { app.paste(); }
    if pressed(ctrl, Key::D) { app.duplicate_selected(); }
    if pressed(Modifiers::SHIFT, Key::Delete) { app.delete_selected(true); }
    if pressed(Modifiers::NONE, Key::Delete) || pressed(Modifiers::NONE, Key::Backspace) { app.delete_selected(false); }
    if pressed(Modifiers::NONE, Key::ArrowLeft) { app.step_frames(-1); }
    if pressed(Modifiers::NONE, Key::ArrowRight) { app.step_frames(1); }
    if pressed(Modifiers::NONE, Key::Home) { app.seek(0.0); }
    if pressed(Modifiers::NONE, Key::End) { let d = app.duration(); app.seek(d); }
    if pressed(Modifiers::NONE, Key::M) { app.add_marker(); }
    if pressed(Modifiers::NONE, Key::Plus) || pressed(Modifiers::NONE, Key::Equals) { app.zoom = (app.zoom * 1.25).min(600.0); }
    if pressed(Modifiers::NONE, Key::Minus) { app.zoom = (app.zoom / 1.25).max(2.0); }
    if pressed(Modifiers::NONE, Key::Escape) {
        if app.preview_full { app.preview_full = false; } else { app.selected = None; }
    }
}

fn dropped_files(ctx: &egui::Context, app: &mut ZavyxApp) {
    let files: Vec<std::path::PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
    if files.is_empty() {
        return;
    }
    let (projects, media): (Vec<_>, Vec<_>) = files.into_iter()
        .partition(|p| p.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("zvep")).unwrap_or(false));
    if let Some(p) = projects.first() {
        app.open_path(p);
    } else if !media.is_empty() {
        app.import_files(&media);
    }
}

// --- Dialogs ---------------------------------------------------------------------------

pub fn dialogs(ctx: &egui::Context, app: &mut ZavyxApp) {
    if app.about_open {
        about(ctx, app);
    }
    if app.shortcuts_open {
        shortcuts_help(ctx, app);
    }
    if app.export_job.is_some() {
        export_progress(ctx, app);
    } else if app.export_dialog.is_some() {
        export_dialog(ctx, app);
    }
    if app.confirm_close {
        confirm_unsaved(ctx, app);
    }
    toast(ctx, app);
}

fn modal_dim(ctx: &egui::Context) {
    let layer = egui::LayerId::new(egui::Order::Middle, egui::Id::new("modal_dim"));
    ctx.layer_painter(layer).rect_filled(ctx.screen_rect(), 0.0, Color32::from_black_alpha(150));
}

fn about(ctx: &egui::Context, app: &mut ZavyxApp) {
    let mut open = true;
    egui::Window::new("About Zavyx VE").open(&mut open).collapsible(false).resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(6.0);
                style::logo(ui, 72.0);
                ui.add_space(8.0);
                ui.label(RichText::new("Zavyx Video Editor").font(semibold(20.0)));
                ui.label(style::dim(format!("Version {}", crate::VERSION)));
                ui.add_space(8.0);
                ui.label("Made by Mohammad Ibrahim");
                ui.hyperlink_to("github.com/mohammadibrahimdev", crate::AUTHOR_URL);
                ui.hyperlink_to("Project repository", crate::REPO_URL);
                ui.add_space(6.0);
                ui.label(style::dim("Free & open source · no ads, no watermark, no telemetry"));
                ui.label(style::dim("Video and audio processing by FFmpeg (ffmpeg.org)"));
                ui.add_space(6.0);
            });
        });
    app.about_open = open;
}

fn shortcuts_help(ctx: &egui::Context, app: &mut ZavyxApp) {
    let mut open = true;
    egui::Window::new("Keyboard Shortcuts").open(&mut open).collapsible(false).resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
            egui::Grid::new("keys").num_columns(2).spacing([28.0, 6.0]).show(ui, |ui| {
                for (k, v) in [
                    ("Space", "Play / pause"), ("← / →", "Previous / next frame"), ("Home / End", "Go to start / end"),
                    ("Ctrl+B", "Split at playhead"), ("Del", "Delete clip"), ("Shift+Del", "Ripple delete"),
                    ("Ctrl+C / Ctrl+V", "Copy / paste clip"), ("Ctrl+D", "Duplicate clip"), ("Ctrl+Z / Ctrl+Y", "Undo / redo"),
                    ("M", "Add marker"), ("+ / -", "Zoom timeline"), ("Ctrl + mouse wheel", "Zoom timeline"),
                    ("Ctrl+I", "Import media"), ("Ctrl+S", "Save project"), ("Ctrl+E", "Export"), ("Esc", "Exit full screen"),
                ] {
                    ui.label(RichText::new(k).monospace().color(style::ACCENT_TEXT));
                    ui.label(v);
                    ui.end_row();
                }
            });
        });
    app.shortcuts_open = open;
}

fn export_dialog(ctx: &egui::Context, app: &mut ZavyxApp) {
    modal_dim(ctx);
    let mut close = false;
    let mut start = false;
    let proj_h = app.project.settings.height;
    let proj_w = app.project.settings.width;
    let dur = app.duration();
    let Some(d) = app.export_dialog.as_mut() else { return };
    egui::Window::new("Export").collapsible(false).resizable(false).title_bar(false).order(egui::Order::Foreground)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO).fixed_size(Vec2::new(640.0, 400.0))
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Export").font(semibold(18.0)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if style::icon_button(ui, icon::X, "Close", true, false).clicked() { close = true; }
                });
            });
            ui.separator();
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(170.0);
                    ui.label(style::dim("Format"));
                    for f in Format::ALL {
                        let on = d.format == f;
                        let text = RichText::new(f.label()).font(if on { semibold(14.0) } else { egui::FontId::proportional(14.0) });
                        if ui.add_sized(Vec2::new(160.0, 30.0), egui::SelectableLabel::new(on, text)).on_hover_text(f.description()).clicked() {
                            d.format = f;
                        }
                    }
                });
                ui.separator();
                ui.vertical(|ui| {
                    ui.label(style::dim(d.format.description()));
                    ui.add_space(8.0);
                    egui::Grid::new("export_grid").num_columns(2).spacing([16.0, 10.0]).show(ui, |ui| {
                        ui.label("Name");
                        ui.add(egui::TextEdit::singleline(&mut d.name).desired_width(280.0));
                        ui.end_row();
                        ui.label("Save to");
                        ui.horizontal(|ui| {
                            ui.add(egui::Label::new(crate::ui::home::elide(&d.folder.display().to_string(), 34)).truncate());
                            if ui.button(icon::FOLDER_OPEN).on_hover_text("Choose folder").clicked() {
                                if let Some(f) = rfd::FileDialog::new().set_directory(&d.folder).pick_folder() { d.folder = f; }
                            }
                        });
                        ui.end_row();
                        if !d.format.audio_only() {
                            ui.label("Resolution");
                            let options: Vec<u32> = [2160u32, 1440, 1080, 720, 480, 360].into_iter()
                                .filter(|h| *h <= proj_h.max(proj_w).max(1080)).collect();
                            let res_label = |h: u32| {
                                let w = ((proj_w as f64 * h as f64 / proj_h.max(1) as f64).round() as u32) & !1;
                                format!("{w} x {h}")
                            };
                            egui::ComboBox::from_id_salt("res").selected_text(res_label(d.height)).show_ui(ui, |ui| {
                                for h in std::iter::once(proj_h).chain(options).collect::<std::collections::BTreeSet<_>>().into_iter().rev() {
                                    let tag = if h == proj_h { " (project)" } else { "" };
                                    ui.selectable_value(&mut d.height, h, format!("{}{tag}", res_label(h)));
                                }
                            });
                            ui.end_row();
                            ui.label("Frame rate");
                            egui::ComboBox::from_id_salt("efps").selected_text(format!("{:.2} fps", d.fps).replace(".00", "")).show_ui(ui, |ui| {
                                for f in [24.0, 25.0, 30.0, 50.0, 60.0] {
                                    ui.selectable_value(&mut d.fps, f, format!("{f} fps"));
                                }
                            });
                            ui.end_row();
                        }
                        ui.label("Quality");
                        ui.horizontal(|ui| {
                            for q in [Quality::Low, Quality::Medium, Quality::High] {
                                ui.selectable_value(&mut d.quality, q, q.label());
                            }
                        });
                        ui.end_row();
                        ui.label("Duration");
                        ui.label(style::short_time(dur));
                        ui.end_row();
                    });
                    ui.add_space(14.0);
                    ui.label(style::dim(format!("File: {}.{}", d.name, d.format.extension())));
                });
            });
            ui.add_space(10.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if style::primary_button(ui, "Export").clicked() { start = true; }
                if ui.button("Cancel").clicked() { close = true; }
            });
        });
    if close {
        app.export_dialog = None;
    } else if start {
        app.start_export();
    }
}

fn export_progress(ctx: &egui::Context, app: &mut ZavyxApp) {
    modal_dim(ctx);
    let Some(job) = app.export_job.as_ref() else { return };
    let state = job.state();
    let fraction = job.fraction();
    let mut close = false;
    let mut cancel = false;
    egui::Window::new("Exporting").collapsible(false).resizable(false).title_bar(false).order(egui::Order::Foreground)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO).fixed_size(Vec2::new(460.0, 170.0))
        .show(ctx, |ui| {
            ui.add_space(6.0);
            match &state {
                State::Running(stage) => {
                    ui.label(RichText::new(format!("{stage}…")).font(semibold(16.0)));
                    ui.add_space(10.0);
                    ui.add(egui::ProgressBar::new(fraction).show_percentage().desired_height(14.0));
                    ui.add_space(14.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Cancel").clicked() { cancel = true; }
                    });
                }
                State::Done(path) => {
                    ui.label(RichText::new(format!("{}  Export complete", icon::CHECK_CIRCLE)).font(semibold(16.0)).color(style::SUCCESS));
                    ui.add_space(6.0);
                    ui.label(style::dim(path.display().to_string()));
                    ui.add_space(14.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if style::primary_button(ui, "Open folder").clicked() {
                            open_folder(path);
                        }
                        if ui.button("Close").clicked() { close = true; }
                    });
                }
                State::Failed(e) => {
                    ui.label(RichText::new(format!("{}  Export failed", icon::WARNING_CIRCLE)).font(semibold(16.0)).color(style::ERROR));
                    ui.add_space(6.0);
                    ui.add(egui::Label::new(style::dim(e.clone())).wrap());
                    ui.add_space(10.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Close").clicked() { close = true; }
                    });
                }
                State::Cancelled => { close = true; }
            }
        });
    if cancel {
        job.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    if close {
        let done = matches!(state, State::Done(_));
        app.export_job = None;
        app.export_dialog = None;
        if matches!(state, State::Cancelled) { app.notify("Export cancelled"); }
        let _ = done;
    }
}

fn open_folder(path: &std::path::Path) {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("explorer").arg("/select,").arg(path).spawn();
    }
    #[cfg(not(windows))]
    {
        if let Some(dir) = path.parent() {
            let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
        }
    }
}

fn confirm_unsaved(ctx: &egui::Context, app: &mut ZavyxApp) {
    modal_dim(ctx);
    let mut choice: Option<u8> = None;
    egui::Window::new("Save changes?").collapsible(false).resizable(false).order(egui::Order::Foreground)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
            ui.label(format!("Do you want to save the changes to \"{}\"?", app.project.name));
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if style::primary_button(ui, "Save").clicked() { choice = Some(0); }
                if ui.button("Don't save").clicked() { choice = Some(1); }
                if ui.button("Cancel").clicked() { choice = Some(2); }
            });
        });
    let Some(choice) = choice else { return };
    app.confirm_close = false;
    let proceed = match choice {
        0 => app.save(),
        1 => true,
        _ => { app.pending_home = false; false }
    };
    if !proceed {
        return;
    }
    if let Some(a) = crate::app::autosave_path() {
        let _ = std::fs::remove_file(a);
    }
    if app.pending_home {
        app.pending_home = false;
        app.history.clear();
        app.engine.pause();
        app.screen = Screen::Home;
    } else {
        app.allow_close = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}

fn toast(ctx: &egui::Context, app: &mut ZavyxApp) {
    let now = ctx.input(|i| i.time);
    let Some(t) = app.toast.as_ref() else { return };
    if now > t.until {
        app.toast = None;
        return;
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(250));
    egui::Area::new(egui::Id::new("toast")).order(egui::Order::Tooltip)
        .anchor(Align2::CENTER_TOP, Vec2::new(0.0, 58.0))
        .show(ctx, |ui| {
            egui::Frame::none().fill(if t.error { style::ERROR.linear_multiply(0.9) } else { style::RAISED })
                .rounding(8.0).inner_margin(egui::Margin::symmetric(16.0, 10.0))
                .stroke(egui::Stroke::new(1.0_f32, style::BORDER))
                .show(ui, |ui| {
                    let glyph = if t.error { icon::WARNING_CIRCLE } else { icon::CHECK_CIRCLE };
                    ui.label(RichText::new(format!("{glyph}  {}", t.text)).color(Color32::WHITE));
                });
        });
}
