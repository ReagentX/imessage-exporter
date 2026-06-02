// Hide the console window on Windows release builds (keep it for debug logs).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod backend;
mod model;
mod pdf;
mod settings;
mod theme;

use eframe::egui;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([theme::window::INITIAL_WIDTH, theme::window::INITIAL_HEIGHT])
            .with_min_inner_size([theme::window::MIN_WIDTH, theme::window::MIN_HEIGHT])
            .with_title("iMessage Exporter"),
        ..Default::default()
    };

    eframe::run_native(
        "iMessage Exporter",
        native_options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}
