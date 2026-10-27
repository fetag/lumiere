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

pub fn default_application(path: &Path) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        // Match the fixed list in `apps_to_open` so test builds never reach
        // LaunchServices from the prefetch thread.
        if cfg!(any(test, feature = "test-support")) {
            return Some(PathBuf::from("/System/Applications/Preview.app"));
        }
        use objc2_app_kit::NSWorkspace;
        use objc2_foundation::{NSString, NSURL};
        let url = NSURL::fileURLWithPath(&NSString::from_str(path.to_string_lossy().as_ref()));
        NSWorkspace::sharedWorkspace()
            .URLForApplicationToOpenURL(&url)?
            .path()
            .map(|value| PathBuf::from(value.to_string()))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        None
    }
}

pub fn application_icon(path: &Path) -> Option<lumiere_core::imaging::Decoded> {
    #[cfg(target_os = "macos")]
    {
        // Icon decoding is real AppKit work; tests only exercise the text of
        // the submenu, so skip it there.
        if cfg!(any(test, feature = "test-support")) {
            let _ = path;
            return None;
        }
        use objc2_app_kit::NSWorkspace;
        use objc2_foundation::NSString;
        let icon = NSWorkspace::sharedWorkspace()
            .iconForFile(&NSString::from_str(path.to_string_lossy().as_ref()));
        let data = icon.TIFFRepresentation()?;
        let image = image::load_from_memory(&data.to_vec())
            .ok()?
            .thumbnail(32, 32)
            .to_rgba8();
        Some(lumiere_core::imaging::Decoded::from_rgba(image))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        None
    }
}

/// Select a single application bundle using the native macOS open panel.
pub fn choose_application() -> PanelResult {
    #[cfg(target_os = "macos")]
    {
        use objc2::MainThreadMarker;
        use objc2_app_kit::{NSModalResponseOK, NSOpenPanel};
        use objc2_foundation::{NSString, NSURL};
        if cfg!(any(test, feature = "test-support")) {
            return PanelResult::Unavailable;
        }
        let Some(mtm) = MainThreadMarker::new() else {
            return PanelResult::Unavailable;
        };
        let panel = NSOpenPanel::openPanel(mtm);
        panel.setCanChooseFiles(true);
        panel.setCanChooseDirectories(false);
        panel.setAllowsMultipleSelection(false);
        panel.setTreatsFilePackagesAsDirectories(false);
        // Compatible with the project's macOS 11 deployment target; constrain
        // selection to application bundles rather than arbitrary documents.
        #[allow(deprecated)]
        panel.setAllowedFileTypes(Some(&objc2_foundation::NSArray::from_retained_slice(&[
            NSString::from_str("app"),
        ])));
        panel.setTitle(Some(&NSString::from_str("Choose an Application")));
        panel.setPrompt(Some(&NSString::from_str("Open")));
        panel.setDirectoryURL(Some(&NSURL::fileURLWithPath(&NSString::from_str(
            "/Applications",
        ))));
        if panel.runModal() != NSModalResponseOK {
            return PanelResult::Cancelled;
        }
        panel
            .URLs()
            .firstObject()
            .and_then(|url| url.path())
            .map(|path| PanelResult::Chose(PathBuf::from(path.to_string())))
            .unwrap_or(PanelResult::Cancelled)
    }
    #[cfg(not(target_os = "macos"))]
    {
        PanelResult::Unavailable
    }
}

// AppKit presents a sharing picker asynchronously. Keep the picker alive on
// the main thread after `show…` returns; otherwise its Rust `Retained` owner
// is dropped immediately and the sheet can vanish before AppKit displays it.
#[cfg(target_os = "macos")]
thread_local! {
    static ACTIVE_SHARE_PICKER: std::cell::RefCell<
        Option<objc2::rc::Retained<objc2_app_kit::NSSharingServicePicker>>,
    > = const { std::cell::RefCell::new(None) };
}

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
    if cfg!(any(test, feature = "test-support")) {
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

/// Reveal `path` in a Finder window with the item selected — the "Show in
/// Finder" step a failed operation offers. Returns true when a native
/// Finder window was asked to; everywhere else, and in a test build where
/// there is no `NSApplication` to host it, the call is a no-op.
#[cfg(target_os = "macos")]
pub fn reveal_in_finder(path: &Path) -> bool {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::NSString;

    if cfg!(any(test, feature = "test-support")) {
        return false;
    }
    if !path.exists() {
        return false;
    }
    let Some(_mtm) = MainThreadMarker::new() else {
        return false;
    };
    let full_path = NSString::from_str(path.to_string_lossy().as_ref());
    let root_path = NSString::from_str(
        path.parent()
            .unwrap_or(Path::new("/"))
            .to_string_lossy()
            .as_ref(),
    );
    NSWorkspace::sharedWorkspace().selectFile_inFileViewerRootedAtPath(Some(&full_path), &root_path)
}

/// There is no Finder on other platforms for the file to be shown in.
#[cfg(not(target_os = "macos"))]
pub fn reveal_in_finder(path: &Path) -> bool {
    let _ = path;
    false
}

/// Show the system share picker for `paths` (the Share item in the
/// right-click menu). Returns true when the picker was asked to; everywhere
/// else, and in a test build where there is no window to anchor it, the call
/// is a no-op.
#[cfg(target_os = "macos")]
pub fn share_paths(paths: &[PathBuf]) -> bool {
    use objc2::rc::Retained;
    use objc2::AnyThread;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSSharingServicePicker};
    use objc2_foundation::{NSArray, NSRectEdge, NSString, NSURL};

    if cfg!(any(test, feature = "test-support")) || paths.is_empty() {
        return false;
    }
    // AppKit only works on the main thread.
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    // The picker anchors to a live view in the app's window.
    let Some(view) = NSApplication::sharedApplication(mtm)
        .keyWindow()
        .and_then(|window| window.contentView())
    else {
        return false;
    };
    let urls: Vec<Retained<NSURL>> = paths
        .iter()
        .map(|path| {
            let name = path.to_string_lossy();
            NSURL::fileURLWithPath(&NSString::from_str(name.as_ref()))
        })
        .collect();
    let urls = NSArray::from_retained_slice(&urls);
    // The bindings declare the items as a bare `NSArray`, but the objects in
    // it are NSURLs; both are the same array at the Objective-C level, so a
    // pointer re-interpretation is all it takes.
    //
    // SAFETY: `NSArray<NSURL>` and `NSArray` share the layout of an
    // Objective-C object pointer, and the array is alive for the call.
    let raw_ptr: *const NSArray = Retained::as_ptr(&urls).cast();
    let raw: &NSArray = unsafe { &*raw_ptr };
    // SAFETY: the items are file URLs, which conform to the pasteboard
    // writing the picker requires.
    let picker =
        unsafe { NSSharingServicePicker::initWithItems(NSSharingServicePicker::alloc(), raw) };
    // `frame` is expressed in the superview's coordinates. The picker API
    // expects its rectangle in `view` coordinates, so use `bounds` or the
    // picker can be positioned off-window on a titled/resized egui window.
    picker.showRelativeToRect_ofView_preferredEdge(view.bounds(), &view, NSRectEdge::MinY);
    // The picker is asynchronous. Retaining the current picker until the next
    // invocation is intentionally conservative and avoids a use-after-free
    // style lifetime bug in the bridge. AppKit owns the displayed UI itself.
    ACTIVE_SHARE_PICKER.with(|active| *active.borrow_mut() = Some(picker));
    true
}

/// There is no share picker on other platforms.
#[cfg(not(target_os = "macos"))]
pub fn share_paths(paths: &[PathBuf]) -> bool {
    let _ = paths;
    false
}

/// The applications that can open `path`, as (name, app bundle path) pairs,
/// from LaunchServices — the list behind the "Open With…" item. In a test
/// build there is no LaunchServices to query, so a fixed list stands in for
/// it; the dialog and its wiring are what get exercised.
#[cfg(target_os = "macos")]
pub fn apps_to_open(path: &Path) -> Vec<(String, PathBuf)> {
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::{NSString, NSURL};

    if cfg!(any(test, feature = "test-support")) {
        return vec![
            (
                "Preview".to_owned(),
                PathBuf::from("/System/Applications/Preview.app"),
            ),
            (
                "Photos".to_owned(),
                PathBuf::from("/System/Applications/Photos.app"),
            ),
        ];
    }
    let name = path.to_string_lossy();
    let url = NSURL::fileURLWithPath(&NSString::from_str(name.as_ref()));
    NSWorkspace::sharedWorkspace()
        .URLsForApplicationsToOpenURL(&url)
        .iter()
        .filter_map(|app| {
            let app_path = app.path()?;
            let app_path = PathBuf::from(app_path.to_string());
            // "Preview.app" -> "Preview".
            let display = app_path
                .file_name()?
                .to_string_lossy()
                .strip_suffix(".app")
                .unwrap_or(&app_path.to_string_lossy())
                .to_owned();
            Some((display, app_path))
        })
        .collect()
}

/// There are no LaunchServices apps to list on other platforms.
#[cfg(not(target_os = "macos"))]
pub fn apps_to_open(path: &Path) -> Vec<(String, PathBuf)> {
    let _ = path;
    Vec::new()
}

/// Open `paths` in the application at `app` (a `.app` bundle path) — the
/// chosen row of the "Open With…" dialog. Returns true when the system was
/// asked to; in a test build the call pretends success so the dialog's
/// wiring can still be exercised.
#[cfg(target_os = "macos")]
pub fn open_with(paths: &[PathBuf], app: &Path) -> bool {
    use objc2::rc::Retained;
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::{NSArray, NSString, NSURL};

    if paths.is_empty() {
        return false;
    }
    if cfg!(any(test, feature = "test-support")) {
        let _ = app;
        return true;
    }
    let urls: Vec<Retained<NSURL>> = paths
        .iter()
        .map(|path| {
            let name = path.to_string_lossy();
            NSURL::fileURLWithPath(&NSString::from_str(name.as_ref()))
        })
        .collect();
    let urls = NSArray::from_retained_slice(&urls);
    let app_name = app.to_string_lossy();
    let app_url = NSURL::fileURLWithPath(&NSString::from_str(app_name.as_ref()));
    let configuration = objc2_app_kit::NSWorkspaceOpenConfiguration::new();
    NSWorkspace::sharedWorkspace().openURLs_withApplicationAtURL_configuration_completionHandler(
        &urls,
        &app_url,
        &configuration,
        None,
    );
    true
}

/// There is no application bundle to open the files with on other platforms.
#[cfg(not(target_os = "macos"))]
pub fn open_with(paths: &[PathBuf], app: &Path) -> bool {
    let _ = (paths, app);
    false
}
