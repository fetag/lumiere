//! Native file/folder panels.
//!
//! On macOS the app asks AppKit for a real `NSOpenPanel`, so choosing a folder,
//! and picking a Move/Copy destination, uses the same Finder-styled dialog the
//! user knows. Everywhere else — and in the headless test build, where there is
//! no `NSApplication` to host a panel — the caller falls back to the built-in
//! egui dialogs.
//!
//! The panel is shown with a modal `runModal`, which spins AppKit's own event
//! loop. The egui canvas simply waits behind the panel for as long as it takes;
//! that is exactly how a native modal behaves, so there is nothing to special-
//! case on the egui side.

use std::path::{Path, PathBuf};

/// The outcome of asking the user to choose a directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PanelResult {
    /// The user picked a directory.
    Chose(PathBuf),
    /// The panel was shown and the user dismissed it.
    Cancelled,
    /// No native panel is available (wrong platform, wrong thread, or a test
    /// build); the caller should use the built-in dialog instead.
    Unavailable,
}

/// Show a native "choose a folder" panel and return the chosen directory.
///
/// `start` is the folder the panel opens in (usually the one currently shown),
/// so a re-open starts where the user left off. `prompt` is the label of the
/// default button (`Open`, `Move`, `Copy`, …).
pub fn choose_directory(title: &str, prompt: &str, start: Option<&Path>) -> PanelResult {
    #[cfg(target_os = "macos")]
    {
        native_choose_directory(title, prompt, start)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (title, prompt, start);
        PanelResult::Unavailable
    }
}

/// The macOS implementation: a real `NSOpenPanel` restricted to directories.
#[cfg(target_os = "macos")]
fn native_choose_directory(title: &str, prompt: &str, start: Option<&Path>) -> PanelResult {
    use objc2::rc::Retained;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSModalResponseOK, NSOpenPanel, NSSavePanel};
    use objc2_foundation::{NSString, NSURL};

    // Never from a test build: there is no `NSApplication` run loop to host a
    // modal panel, so the built-in dialog is the correct path there.
    if cfg!(test) {
        return PanelResult::Unavailable;
    }
    let Some(mtm) = MainThreadMarker::new() else {
        return PanelResult::Unavailable;
    };

    let panel = NSOpenPanel::openPanel(mtm);
    panel.setCanChooseDirectories(true);
    panel.setCanChooseFiles(false);
    panel.setAllowsMultipleSelection(false);

    // The title, prompt button, starting folder and `runModal` live on the
    // `NSSavePanel` superclass, so upcast the same pointer to reach them.
    let save: &NSSavePanel = unsafe { &*Retained::as_ptr(&panel).cast::<NSSavePanel>() };
    save.setCanCreateDirectories(true);
    let title = NSString::from_str(title);
    save.setTitle(Some(&title));
    let prompt = NSString::from_str(prompt);
    save.setPrompt(Some(&prompt));
    if let Some(dir) = start {
        let url = NSURL::fileURLWithPath(&NSString::from_str(dir.to_string_lossy().as_ref()));
        save.setDirectoryURL(Some(&url));
    }

    let response = save.runModal();
    if response != NSModalResponseOK {
        return PanelResult::Cancelled;
    }

    match panel
        .URLs()
        .firstObject()
        .and_then(|url| url.path())
        .map(|path| PathBuf::from(path.to_string()))
    {
        Some(path) if path.is_dir() => PanelResult::Chose(path),
        _ => PanelResult::Cancelled,
    }
}
