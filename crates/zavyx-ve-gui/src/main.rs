//! zavyx-ve-gui — ZavyxVE.exe — the Zavyx Video Editor desktop app.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod engine;
mod media_cache;
mod settings;
mod style;
mod ui;

use zavyx_ve_render::{export, mixer};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const AUTHOR_URL: &str = "https://github.com/mohammadibrahimdev";
pub const REPO_URL: &str = "https://github.com/mohammadibrahimdev/zavyx-video-editor";

fn main() -> eframe::Result<()> {
    env_logger::init();
    let viewport = egui::ViewportBuilder::default()
        .with_inner_size([1440.0, 900.0])
        .with_min_inner_size([1024.0, 640.0])
        .with_maximized(true)
        .with_title("Zavyx Video Editor")
        .with_icon(window_icon())
        .with_drag_and_drop(true);
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native("Zavyx Video Editor", options, Box::new(|cc| Ok(Box::new(app::ZavyxApp::new(cc)))))
}

/// The window icon (the .exe icon is embedded by build.rs).
fn window_icon() -> egui::IconData {
    let img = image::load_from_memory(include_bytes!("../../../assets/logo-256.png"))
        .map(|i| i.to_rgba8())
        .unwrap_or_else(|_| image::RgbaImage::new(1, 1));
    let (width, height) = img.dimensions();
    egui::IconData { rgba: img.into_raw(), width, height }
}
