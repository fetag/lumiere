//! The application menu bar.
//!
//! The menu is described once, in [`MenuBar`], and rendered by whichever back
//! end the platform has: a real macOS menu bar on macOS ([`macos`]) and an
//! in-window egui menu bar everywhere else. Keeping one description means the
//! titles, shortcuts and enabled states cannot drift apart between the two.

use crate::app::Action;

/// A key in a menu shortcut, spelled independently of any platform key code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Return,
    /// The backspace key, drawn as `\u{232B}`.
    Delete,
}

/// A menu shortcut. Modifier state is explicit so the same value can become an
/// `NSMenuItem` key equivalent and the text shown next to an egui menu row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shortcut {
    pub key: Key,
    pub command: bool,
    pub control: bool,
    pub option: bool,
    pub shift: bool,
}

impl Shortcut {
    /// No modifiers. Only a couple of commands are bound this way; a macOS
    /// shortcut is nearly always `⌘`-based, so prefer [`Self::cmd`].
    pub const fn plain(key: Key) -> Self {
        Self {
            key,
            command: false,
            control: false,
            option: false,
            shift: false,
        }
    }

    /// `⌘` plus the key, the starting point for almost every item here.
    pub const fn cmd(key: Key) -> Self {
        Self {
            command: true,
            ..Self::plain(key)
        }
    }

    pub const fn control(mut self) -> Self {
        self.control = true;
        self
    }

    pub const fn option(mut self) -> Self {
        self.option = true;
        self
    }

    pub const fn shift(mut self) -> Self {
        self.shift = true;
        self
    }

    /// The macOS glyph spelling, e.g. `⌃⌘F` or `⌘⇧L`. Used by the in-window
    /// fallback menu on platforms that have no native one.
    #[cfg(any(test, not(target_os = "macos")))]
    pub fn display(&self) -> String {
        let mut out = String::new();
        if self.control {
            out.push('\u{2303}');
        }
        if self.option {
            out.push('\u{2325}');
        }
        if self.shift {
            out.push('\u{21E7}');
        }
        if self.command {
            out.push('\u{2318}');
        }
        out.push_str(match self.key {
            Key::Char(c) => return format!("{out}{}", c.to_uppercase()),
            Key::Return => "\u{21A9}",
            Key::Delete => "\u{232B}",
        });
        out
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Command {
        label: String,
        shortcut: Option<Shortcut>,
        action: Action,
        enabled: bool,
        /// Tick box state for toggles; `false` still reserves the tick column.
        marked: bool,
    },
    Separator,
    Submenu {
        label: String,
        items: Vec<Item>,
    },
}

impl Item {
    pub fn command(label: impl Into<String>, action: Action) -> Self {
        Self::Command {
            label: label.into(),
            shortcut: None,
            action,
            enabled: true,
            marked: false,
        }
    }

    pub fn key(
        label: impl Into<String>,
        action: Action,
        shortcut: impl Into<Option<Shortcut>>,
    ) -> Self {
        Self::Command {
            label: label.into(),
            shortcut: shortcut.into(),
            action,
            enabled: true,
            marked: false,
        }
    }

    pub fn toggle(label: impl Into<String>, action: Action, marked: bool) -> Self {
        Self::Command {
            label: label.into(),
            shortcut: None,
            action,
            enabled: true,
            marked,
        }
    }

    pub fn toggle_key(
        label: impl Into<String>,
        action: Action,
        shortcut: impl Into<Option<Shortcut>>,
        marked: bool,
    ) -> Self {
        Self::Command {
            label: label.into(),
            shortcut: shortcut.into(),
            action,
            enabled: true,
            marked,
        }
    }

    pub fn submenu(label: impl Into<String>, items: impl Into<Vec<Item>>) -> Self {
        Self::Submenu {
            label: label.into(),
            items: items.into(),
        }
    }
}

/// The whole menu bar: the application menu plus the titled menus after it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MenuBar {
    /// The application menu. macOS draws its title as the app name; the egui
    /// back end labels it with the app title.
    pub app: Vec<Item>,
    /// File, Edit, View, Go, ... in display order.
    pub menus: Vec<(String, Vec<Item>)>,
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_use_macos_glyph_order() {
        // Modifier glyphs go Control, Option, Shift, Command, in that order.
        assert_eq!(
            Shortcut::cmd(Key::Char('f')).control().display(),
            "\u{2303}\u{2318}F"
        );
        assert_eq!(
            Shortcut::cmd(Key::Char('l')).shift().display(),
            "\u{21E7}\u{2318}L"
        );
        assert_eq!(Shortcut::cmd(Key::Delete).display(), "\u{2318}\u{232B}");
        assert_eq!(Shortcut::cmd(Key::Char(',')).display(), "\u{2318},");
        assert_eq!(Shortcut::plain(Key::Return).display(), "\u{21A9}");
    }
}
