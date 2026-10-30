use lumiere_core::command::Action;
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

    /// 顯示符合目前平台的快捷鍵，並與實際鍵盤處理使用的修飾鍵一致。
    pub fn display(&self) -> String {
        self.display_for_macos(cfg!(target_os = "macos"))
    }

    fn display_for_macos(&self, macos: bool) -> String {
        if !macos {
            let mut parts = Vec::new();
            // egui 在非 macOS 將 command 對應到 Ctrl；兩者不可重複顯示。
            if self.command || self.control {
                parts.push("Ctrl".to_owned());
            }
            if self.option {
                parts.push("Alt".to_owned());
            }
            if self.shift {
                parts.push("Shift".to_owned());
            }
            parts.push(match self.key {
                Key::Char(c) => c.to_uppercase().to_string(),
                Key::Return => "Enter".to_owned(),
                Key::Delete => "Delete / Backspace".to_owned(),
            });
            return parts.join("+");
        }
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

impl Item {
    /// Applies an enabled state to an already-built item.
    pub fn with_enabled(self, enabled: bool) -> Self {
        let mut item = self;
        if let Item::Command { enabled: slot, .. } = &mut item {
            *slot = enabled;
        }
        item
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_use_macos_glyph_order() {
        // Modifier glyphs go Control, Option, Shift, Command, in that order.
        assert_eq!(
            Shortcut::cmd(Key::Char('f'))
                .control()
                .display_for_macos(true),
            "\u{2303}\u{2318}F"
        );
        assert_eq!(
            Shortcut::cmd(Key::Char('l'))
                .shift()
                .display_for_macos(true),
            "\u{21E7}\u{2318}L"
        );
        assert_eq!(
            Shortcut::cmd(Key::Delete).display_for_macos(true),
            "\u{2318}\u{232B}"
        );
        assert_eq!(
            Shortcut::cmd(Key::Char(',')).display_for_macos(true),
            "\u{2318},"
        );
        assert_eq!(
            Shortcut::plain(Key::Return).display_for_macos(true),
            "\u{21A9}"
        );
    }

    #[test]
    fn windows_and_linux_shortcuts_match_ctrl_key_events() {
        assert_eq!(
            Shortcut::cmd(Key::Char('o')).display_for_macos(false),
            "Ctrl+O"
        );
        assert_eq!(
            Shortcut::cmd(Key::Char('f'))
                .control()
                .display_for_macos(false),
            "Ctrl+F"
        );
        assert_eq!(
            Shortcut::cmd(Key::Char('r'))
                .option()
                .shift()
                .display_for_macos(false),
            "Ctrl+Alt+Shift+R"
        );
        assert_eq!(
            Shortcut::plain(Key::Delete).display_for_macos(false),
            "Delete / Backspace"
        );
        assert_eq!(
            Shortcut::plain(Key::Return).display_for_macos(false),
            "Enter"
        );
    }
}
