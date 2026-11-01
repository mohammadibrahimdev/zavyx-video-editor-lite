//! Screens and panels.

pub mod editor;
pub mod home;
pub mod inspector;
pub mod library;
pub mod player;
pub mod timeline;

use eframe::egui;

use crate::app::{Screen, ZavyxApp};

impl eframe::App for ZavyxApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.cache.poll(ctx);
        player::update_texture(ctx, self);

        // Follow the playback clock.
        if self.engine.is_playing() {
            self.playhead = self.engine.position();
            ctx.request_repaint();
        } else if self.engine.take_reached_end() {
            self.playhead = self.duration();
        }

        // Ask before closing with unsaved changes.
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close
            && self.screen == Screen::Editor && self.is_dirty()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm_close = true;
        }

        match self.screen {
            Screen::Home => home::show(ctx, self),
            Screen::Editor => editor::show(ctx, self),
        }
        editor::dialogs(ctx, self);

        // A slider or text edit finished: record it as one undo step.
        let busy = ctx.input(|i| i.pointer.any_down()) || ctx.wants_keyboard_input();
        if self.live_before.is_some() && !busy {
            self.commit_live();
        }
        self.sync_engine();
        let now = ctx.input(|i| i.time);
        self.maybe_autosave(now);
    }
}
