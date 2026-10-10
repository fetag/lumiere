//! Lumiere - a classic 3.x style image browser and viewer for macOS.

mod app;
mod browser;
mod classic;
mod fsmodel;
mod imaging;
mod loader;
mod state;
mod viewer;

use eframe::egui;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let saved = state::State::load();
    let width = if saved.width > 0.0 {
        saved.width
    } else {
        1120.0
    };
    let height = if saved.height > 0.0 {
        saved.height
    } else {
        740.0
    };

    let viewport = egui::ViewportBuilder::default()
        .with_title(app::APP_TITLE)
        .with_inner_size([width, height])
        .with_min_inner_size([640.0, 420.0]);

    let options = eframe::NativeOptions {
        viewport,
        multisampling: 0,
        ..Default::default()
    };

    let start = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .or_else(|| saved.last_dir.clone());

    eframe::run_native(
        app::APP_TITLE,
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, start.clone())))),
    )?;

    Ok(())
}
