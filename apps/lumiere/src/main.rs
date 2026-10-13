//! Lumiere - a classic 3.x style image browser and viewer for macOS.

mod app;
mod browser;
mod panels;
mod platform_ui;
mod theme;
mod viewer;

use lumiere_desktop::platform::{alerts, window};
use lumiere_desktop::{file_ops, fsmodel, imaging, loader, state};
use platform_ui::{dnd, macos_open, menu};

use eframe::egui;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Register the document class before AppKit handles the launch document.
    macos_open::prepare();
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
    // eframe otherwise installs its bundled black "e" icon at runtime on
    // macOS, replacing the icon that AppKit loaded from the .app bundle.
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../../../assets/AppIcon.png"))?;

    let viewport = egui::ViewportBuilder::default()
        .with_title(app::APP_TITLE)
        .with_icon(icon)
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
