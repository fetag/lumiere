//! Native modal alerts.
//!
//! On macOS the destructive confirmation is a real `NSAlert`, which runs
//! AppKit's own event loop: the egui canvas simply waits behind it for as
//! long as it takes, exactly like the native file panels in [`crate::platform::panels`].
//! Everywhere else — and in the headless test build, where there is no
//! `NSApplication` run loop to host a modal — the caller falls back to the
//! built-in egui dialog.

use std::path::PathBuf;

/// The outcome of asking the user to confirm a destructive operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Confirmation {
    /// The user accepted the operation.
    Confirmed,
    /// The alert was shown and the user dismissed it.
    Cancelled,
    /// No native alert is available (wrong platform, wrong thread, or a test
    /// build); the caller should use the built-in dialog instead.
    Unavailable,
}

/// How many names the alert lists individually before collapsing the rest
/// into a count, so a large selection does not fill the whole screen.
const MAX_LISTED_NAMES: usize = 5;

/// The `(messageText, informativeText)` of the "move to Trash" confirmation.
///
/// The names are listed as-is and the count is always in the message, so the
/// user sees exactly what is about to be moved. Shared with the built-in
/// fallback dialog, so both backends say the same thing.
pub fn trash_alert_text(paths: &[PathBuf]) -> (String, String) {
    let names = trash_names(paths);
    match names.len() {
        0 => ("Move to Trash".into(), "Nothing is selected.".into()),
        1 => {
            let name = &names[0];
            (
                "Move to Trash".into(),
                format!("Move \u{201C}{name}\u{201D} to the Trash?"),
            )
        }
        _ => {
            let mut detail = format!("Move {} items to the Trash?", names.len());
            for (i, name) in names.iter().take(MAX_LISTED_NAMES).enumerate() {
                detail.push_str(&format!("\n{}. {name}", i + 1));
            }
            if names.len() > MAX_LISTED_NAMES {
                detail.push_str(&format!(
                    "\n\u{2026}and {} more",
                    names.len() - MAX_LISTED_NAMES
                ));
            }
            ("Move to Trash".into(), detail)
        }
    }
}

/// The display names of `paths`, for the confirmation and the result toast.
pub use crate::trash_names;

/// Ask, with a native alert, whether `paths` may be moved to the Trash.
pub fn confirm_trash(paths: &[PathBuf]) -> Confirmation {
    #[cfg(target_os = "macos")]
    {
        native_confirm_trash(paths)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = paths;
        Confirmation::Unavailable
    }
}

/// The macOS implementation: a real `NSAlert`.
#[cfg(target_os = "macos")]
fn native_confirm_trash(paths: &[PathBuf]) -> Confirmation {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSAlert, NSAlertSecondButtonReturn, NSAlertStyle};
    use objc2_foundation::NSString;

    // Never from a test build: there is no `NSApplication` run loop to host a
    // modal, so the built-in dialog is the correct path there.
    if cfg!(any(test, feature = "test-support")) {
        return Confirmation::Unavailable;
    }
    let Some(mtm) = MainThreadMarker::new() else {
        return Confirmation::Unavailable;
    };

    let (message, detail) = trash_alert_text(paths);
    let alert = NSAlert::new(mtm);
    // Trash is recoverable, so this is a warning, not the critical style:
    // AppKit's own documentation reserves the caution badge for actions that
    // might result in unexpected loss of data.
    alert.setAlertStyle(NSAlertStyle::Warning);
    let message = NSString::from_str(&message);
    alert.setMessageText(&message);
    let detail = NSString::from_str(&detail);
    alert.setInformativeText(&detail);

    // The first button added is the default button — the one Return and Esc
    // activate — so Cancel goes first: a destructive operation must never be
    // the one the keyboard confirms on its own.
    let cancel = NSString::from_str("Cancel");
    alert.addButtonWithTitle(&cancel);
    let confirm = NSString::from_str("Move to Trash");
    alert.addButtonWithTitle(&confirm);

    if alert.runModal() == NSAlertSecondButtonReturn {
        Confirmation::Confirmed
    } else {
        // First button (Return) or Escape: either way, the user declined.
        Confirmation::Cancelled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn trash_alert_text_names_a_single_item() {
        let (head, detail) = trash_alert_text(&[PathBuf::from("/tmp/pictures/a.jpg")]);
        assert_eq!(head, "Move to Trash");
        assert!(detail.contains("a.jpg"), "{detail}");
    }

    #[test]
    fn trash_alert_text_counts_and_truncates_a_large_selection() {
        let many: Vec<PathBuf> = (0..8)
            .map(|i| PathBuf::from(format!("/tmp/p{i}.jpg")))
            .collect();
        let (_, detail) = trash_alert_text(&many);
        assert!(detail.contains("8 items"), "{detail}");
        assert!(detail.contains("1. p0.jpg"), "{detail}");
        assert!(!detail.contains("p6.jpg"), "{detail}");
        assert!(detail.contains("3 more"), "{detail}");
    }

    #[test]
    fn trash_names_falls_back_to_the_full_path() {
        let names = trash_names(&[PathBuf::from("/")]);
        assert_eq!(names, vec!["/".to_owned()]);
    }
}
