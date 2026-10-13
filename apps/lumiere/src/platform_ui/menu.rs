use crate::app::Action;
pub use lumiere_desktop::menu_model::*;
#[cfg(target_os = "macos")]
mod macos;

/// Pushes the current menu description to the platform menu bar. A no-op where
/// there is no native menu bar to push it to.
pub fn sync(bar: &MenuBar) {
    #[cfg(target_os = "macos")]
    macos::sync(bar);
    #[cfg(not(target_os = "macos"))]
    let _ = bar;
}

/// Menu items the user picked since the last call.
pub fn take_events() -> Vec<Action> {
    #[cfg(target_os = "macos")]
    {
        macos::take_events()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Vec::new()
    }
}

/// Gives the native menu bar a way to wake the UI up.
///
/// A menu pick is an AppKit event; it never reaches the window, so nothing else
/// would ask egui for the frame that drains the picks.
pub fn attach(ctx: &egui::Context) {
    #[cfg(target_os = "macos")]
    macos::attach(ctx);
    #[cfg(not(target_os = "macos"))]
    let _ = ctx;
}

/// Whether the platform provides the menu bar, in which case the app must not
/// draw its own.
pub const NATIVE: bool = cfg!(target_os = "macos");
