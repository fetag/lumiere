//! Menu descriptions and browser window chrome.

use super::*;

impl App {
    /// The shared description used by native menus and keyboard dispatch.
    pub fn menu_spec(&self) -> crate::menu::MenuBar {
        let has_entry = self.current().is_some();
        let has_image = self.current_image_path().is_some();
        let sort = self.sort;

        let app = vec![
            Item::command("About Lumiere", Action::About),
            Item::Separator,
            Item::key(
                "Settings\u{2026}",
                Action::Settings,
                Shortcut::cmd(Key::Char(',')),
            ),
        ];

        let file = vec![
            Item::key(
                "Open\u{2026}",
                Action::OpenFolder,
                Shortcut::cmd(Key::Char('o')),
            ),
            Item::Separator,
            Item::key("Get Info", Action::GetInfo, Shortcut::cmd(Key::Char('i'))),
            Item::Command {
                label: "Rename\u{2026}".into(),
                shortcut: Some(Shortcut::cmd(Key::Char('r')).option()),
                action: Action::Rename,
                enabled: has_entry,
                marked: false,
            },
            self.key_item(
                "Move to Trash",
                Action::MoveToTrash,
                Shortcut::plain(Key::Delete),
            )
            .with_enabled(has_entry),
            Item::Separator,
            Item::command("Move to Folder\u{2026}", Action::MoveToFolder),
            Item::command("Copy to Folder\u{2026}", Action::CopyToFolder),
        ];

        let edit = vec![
            self.key_item(
                "Select All",
                Action::SelectAll,
                Shortcut::cmd(Key::Char('a')),
            ),
            self.key_item(
                "Select None",
                Action::SelectNone,
                Shortcut::cmd(Key::Char('a')).shift(),
            ),
            self.key_item(
                "Invert Selection",
                Action::InvertSelection,
                Shortcut::cmd(Key::Char('a')).option(),
            ),
            Item::Separator,
            Item::key(
                "Rotate Left",
                Action::RotateCCW,
                Shortcut::cmd(Key::Char('l')).shift(),
            ),
            Item::key(
                "Rotate Right",
                Action::RotateCW,
                Shortcut::cmd(Key::Char('r')).shift(),
            ),
            Item::toggle("Flip Horizontal", Action::FlipH, self.flip_h),
            Item::toggle("Flip Vertical", Action::FlipV, self.flip_v),
        ];

        let view = vec![
            Item::key("Reload", Action::Reload, Shortcut::cmd(Key::Char('r'))),
            Item::Separator,
            Item::key(
                "Actual Size",
                Action::ActualSize,
                Shortcut::cmd(Key::Char('0')),
            ),
            Item::key("Zoom In", Action::ZoomIn, Shortcut::cmd(Key::Char('+'))),
            Item::key("Zoom Out", Action::ZoomOut, Shortcut::cmd(Key::Char('-'))),
            Item::key(
                "Fit to Window",
                Action::FitWindow,
                Shortcut::cmd(Key::Char('f')).shift(),
            ),
            Item::Separator,
            Item::toggle_key(
                "Show Sidebar",
                Action::ToggleSidebar,
                Shortcut::cmd(Key::Char('s')).control(),
                self.show_sidebar,
            ),
            Item::toggle_key(
                "Show Toolbar",
                Action::ToggleToolbar,
                Shortcut::cmd(Key::Char('t')).option(),
                self.show_toolbar,
            ),
            Item::toggle_key(
                "Show Inspector",
                Action::ToggleInspector,
                Shortcut::cmd(Key::Char('i')).option(),
                self.show_inspector,
            ),
            Item::toggle("Show Status Bar", Action::ToggleStatus, self.show_status),
            Item::Separator,
            Item::toggle(
                "Small Thumbnails",
                Action::ThumbSmall,
                self.thumb_size <= 80,
            ),
            Item::toggle(
                "Medium Thumbnails",
                Action::ThumbMedium,
                self.thumb_size > 80 && self.thumb_size < 160,
            ),
            Item::toggle(
                "Large Thumbnails",
                Action::ThumbLarge,
                self.thumb_size >= 160,
            ),
            Item::submenu(
                "Sort",
                vec![
                    Item::toggle("by Name", Action::SortByName, sort == SortKey::Name),
                    Item::toggle("by Date", Action::SortByDate, sort == SortKey::Date),
                    Item::toggle("by Size", Action::SortBySize, sort == SortKey::Size),
                    Item::toggle("by Type", Action::SortByType, sort == SortKey::Type),
                    Item::Separator,
                    Item::toggle("Reverse order", Action::ToggleReverse, self.sort_reverse),
                ],
            ),
            Item::toggle("Show Folders", Action::ToggleFolders, self.show_folders),
            Item::Separator,
            Item::key(
                if self.fullscreen {
                    "Exit Full Screen"
                } else {
                    "Enter Full Screen"
                },
                Action::ToggleFullscreen,
                Shortcut::cmd(Key::Char('f')).control(),
            ),
        ];

        let go = vec![
            Item::key(
                "Up One Level",
                Action::UpFolder,
                Shortcut::cmd(Key::Char('u')).shift(),
            ),
            Item::key(
                "Home",
                Action::GoHome,
                Shortcut::cmd(Key::Char('h')).shift(),
            ),
            Item::key(
                "Pictures",
                Action::GoPictures,
                Shortcut::cmd(Key::Char('p')).shift(),
            ),
            Item::Separator,
            Item::command("First Image", Action::FirstImage),
            Item::command("Previous Image", Action::PrevImage),
            Item::command("Next Image", Action::NextImage),
            Item::command("Last Image", Action::LastImage),
            Item::Separator,
            self.key_item(
                "View Image",
                Action::OpenViewer,
                Shortcut::plain(Key::Return),
            )
            .with_enabled(has_image),
            Item::Command {
                label: "Back to Browser".into(),
                shortcut: None,
                action: Action::CloseViewer,
                enabled: self.mode == Mode::Viewer,
                marked: false,
            },
            Item::Separator,
            if self.slideshow {
                Item::key(
                    "Stop Slideshow",
                    Action::StopSlideshow,
                    Shortcut::cmd(Key::Char('y')).shift(),
                )
            } else {
                Item::Command {
                    label: "Play Slideshow".into(),
                    shortcut: Some(Shortcut::cmd(Key::Char('y')).shift()),
                    action: Action::Slideshow,
                    enabled: self.entries.iter().any(|e| e.is_image),
                    marked: false,
                }
            },
        ];

        crate::menu::MenuBar {
            app,
            menus: vec![
                ("File".into(), file),
                ("Edit".into(), edit),
                ("View".into(), view),
                ("Go".into(), go),
            ],
        }
    }

    /// A menu item whose key equivalent is withdrawn while a text field owns the
    /// keyboard: `⌘A` has to select the text, `⌘\u{232B}` delete to the start of the
    /// line, and Return has to confirm the edit.
    pub(super) fn key_item(&self, label: &str, action: Action, shortcut: Shortcut) -> Item {
        if self.text_focused {
            Item::command(label, action)
        } else {
            Item::key(label, action, shortcut)
        }
    }

    /// Every shortcut the menus declare, most specific modifier set first:
    /// `consume_key` ignores extra Shift/Alt unless the pattern asks for them,
    /// so `⇧⌘R` has to be matched before `⌘R`.
    pub(super) fn shortcuts(&self) -> Vec<(Shortcut, Action)> {
        fn walk(items: &[Item], out: &mut Vec<(Shortcut, Action)>) {
            for item in items {
                match item {
                    Item::Command {
                        shortcut: Some(shortcut),
                        action,
                        ..
                    } => out.push((*shortcut, *action)),
                    Item::Submenu { items, .. } => walk(items, out),
                    Item::Command { .. } | Item::Separator => {}
                }
            }
        }
        let spec = self.menu_spec();
        let mut out = Vec::new();
        walk(&spec.app, &mut out);
        for (_, items) in &spec.menus {
            walk(items, &mut out);
        }
        out.sort_by_key(|(shortcut, _)| std::cmp::Reverse(shortcut_modifier_count(shortcut)));
        out
    }

    /// The in-window menu bar. Only platforms without a native menu bar draw it.
    #[cfg(not(target_os = "macos"))]
    pub(super) fn menu_bar(&mut self, ui: &mut egui::Ui) {
        let spec = self.menu_spec();
        let mut actions: Vec<Action> = Vec::new();
        egui::MenuBar::new().ui(ui, |ui| {
            top_menu(ui, APP_TITLE, |ui| {
                menu_items(ui, &spec.app, &mut actions);
                ui.separator();
                let quit = Shortcut::cmd(Key::Char('q'));
                let response = ui.add_enabled(
                    true,
                    egui::Button::new(menu_row(ui, "Quit Lumiere", Some(&quit), None))
                        .min_size(egui::vec2(240.0, 0.0)),
                );
                if response.clicked() {
                    actions.push(Action::Quit);
                    ui.close();
                }
            });
            for (title, items) in &spec.menus {
                top_menu(ui, title, |ui| menu_items(ui, items, &mut actions));
            }
        });
        let ctx = ui.ctx().clone();
        for action in actions {
            self.act(action, &ctx);
        }
    }

    /// Draws the in-window menu bar. macOS has a real one at the top of the
    /// screen, so there the whole thing is a no-op.
    #[cfg(target_os = "macos")]
    pub(super) fn menu_bar_panel(&mut self, _ctx: &Context) {}

    #[cfg(not(target_os = "macos"))]
    pub(super) fn menu_bar_panel(&mut self, ctx: &Context) {
        egui::TopBottomPanel::top("menubar")
            .exact_height(22.0)
            .show(ctx, |ui| {
                self.menu_bar(ui);
            });
    }

    /// The browser-mode toolbar. It carries browsing controls only: the
    /// zoom / rotate / flip / full-screen controls belong to the viewer
    /// (its floating toolbar owns them there), and every control removed
    /// from this row still lives in the Edit/View/Go menus with its
    /// shortcut, so nothing is lost — the row just stops crowding 800 px
    /// windows with buttons that do nothing until an image is opened.
    pub(super) fn tool_row(&mut self, ui: &mut egui::Ui) {
        let has_image = self.entries.iter().any(|e| e.is_image);
        let mut pending: Vec<Action> = Vec::new();
        ui.horizontal(|ui| {
            // The sidebar toggle leads the row, the way Finder's does.
            if theme::tool_button_toggle(ui, Icon::Sidebar, "Show Sidebar", true, self.show_sidebar)
                .clicked()
            {
                pending.push(Action::ToggleSidebar);
            }
            theme::vtool_separator(ui);
            if theme::tool_button(ui, Icon::FolderUp, "Up one level", true).clicked() {
                pending.push(Action::UpFolder);
            }
            if theme::tool_button(ui, Icon::Home, "Home folder", true).clicked() {
                pending.push(Action::GoHome);
            }
            if theme::tool_button(ui, Icon::Refresh, "Refresh", true).clicked() {
                pending.push(Action::Reload);
            }
            theme::vtool_separator(ui);
            if theme::tool_button(ui, Icon::ThumbSmall, "Small thumbnails", true).clicked() {
                pending.push(Action::ThumbSmall);
            }
            if theme::tool_button(ui, Icon::ThumbMedium, "Medium thumbnails", true).clicked() {
                pending.push(Action::ThumbMedium);
            }
            if theme::tool_button(ui, Icon::ThumbLarge, "Large thumbnails", true).clicked() {
                pending.push(Action::ThumbLarge);
            }
            theme::vtool_separator(ui);
            if theme::tool_button(ui, Icon::First, "First image", true).clicked() {
                pending.push(Action::FirstImage);
            }
            if theme::tool_button(ui, Icon::Prev, "Previous image", true).clicked() {
                pending.push(Action::PrevImage);
            }
            if theme::tool_button(ui, Icon::Next, "Next image", true).clicked() {
                pending.push(Action::NextImage);
            }
            if theme::tool_button(ui, Icon::Last, "Last image", true).clicked() {
                pending.push(Action::LastImage);
            }
            theme::vtool_separator(ui);
            // Enabled exactly like the Go ▸ Play Slideshow menu item.
            if theme::tool_button(ui, Icon::Slideshow, "Slideshow", has_image).clicked() {
                pending.push(Action::Slideshow);
            }
            theme::vtool_separator(ui);
            // Enabled exactly like the File ▸ Move to Trash menu item.
            let trash_key = Shortcut::plain(Key::Delete).display();
            if theme::tool_button_with_shortcut(
                ui,
                Icon::Delete,
                "Move to Trash",
                &trash_key,
                self.current().is_some(),
            )
            .clicked()
            {
                pending.push(Action::MoveToTrash);
            }
            // The Information view was merged into the preview pane, so this
            // button carries that pane: it shows the pane and, when it is
            // already open, hides it again.
            if theme::tool_button_toggle(ui, Icon::Info, "Get Info", true, self.show_inspector)
                .clicked()
            {
                pending.push(Action::ToggleInspector);
            }
        });

        for action in pending {
            self.act(action, ui.ctx());
        }
    }

    /// The address row is the current-location control: an editable path
    /// with a Go button, plus a button that opens the wildcard filter as a
    /// popover instead of a standing DOS-style filter field.
    pub(super) fn address_row(&mut self, ui: &mut egui::Ui) {
        let mut navigate: Option<PathBuf> = None;
        let mut rescan = false;
        let mut filter_anchor: Option<egui::Rect> = None;

        ui.horizontal(|ui| {
            ui.label("Address:");
            let response = ui.add_sized(
                [(ui.available_width() - 230.0).max(120.0), 20.0],
                egui::TextEdit::singleline(&mut self.path_edit).font(egui::FontId::monospace(12.0)),
            );
            self.text_field_ids.push(response.id);
            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                navigate = Some(PathBuf::from(self.path_edit.trim()));
            }
            let go = ui.button("Go");
            theme::note_rect(ui, "Go", go.rect);
            if go.clicked() {
                navigate = Some(PathBuf::from(self.path_edit.trim()));
            }
            // The button wears the active pattern, so an unusual filter is
            // visible at a glance.
            let filter_label = if self.filter == "*.*" {
                "Filter\u{2026}".to_owned()
            } else {
                self.filter.clone()
            };
            let filter_btn = ui.button(filter_label);
            theme::note_rect(ui, "Filter", filter_btn.rect);
            filter_anchor = Some(filter_btn.rect);
            if filter_btn.clicked() {
                self.filter_open = !self.filter_open;
            }
        });

        if self.filter_open {
            // The popover hangs below the button that opened it. An Area
            // keeps it out of the row's layout and above the content.
            let mut commit = false;
            let mut close = false;
            if let Some(anchor) = filter_anchor {
                let x = (anchor.min.x + 210.0).min(ui.ctx().viewport_rect().max.x - 220.0);
                egui::Area::new(egui::Id::new("lumiere filter popover"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(egui::pos2(x, anchor.max.y + 4.0))
                    .show(ui.ctx(), |ui| {
                        egui::Frame::window(ui.style()).show(ui, |ui| {
                            ui.set_min_width(200.0);
                            ui.label("Files:");
                            ui.horizontal(|ui| {
                                let response = ui.add_sized(
                                    [130.0, 20.0],
                                    egui::TextEdit::singleline(&mut self.filter_edit),
                                );
                                self.text_field_ids.push(response.id);
                                theme::note_rect(ui, "Files filter", response.rect);
                                // A commit on focus loss keeps a typed
                                // pattern from being lost by clicking away.
                                if response.lost_focus() {
                                    commit = true;
                                }
                                let apply = ui.button("Apply");
                                theme::note_rect(ui, "Apply", apply.rect);
                                if apply.clicked() {
                                    commit = true;
                                    close = true;
                                }
                                let clear = ui.button("Clear");
                                theme::note_rect(ui, "Clear", clear.rect);
                                if clear.clicked() {
                                    self.filter_edit = "*.*".to_owned();
                                    commit = true;
                                    close = true;
                                }
                            });
                        });
                    });
            }
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                close = true;
            }
            if commit {
                self.filter = self.filter_edit.clone();
                self.state_dirty = true;
                rescan = true;
            }
            if close {
                self.filter_open = false;
            }
        }

        if let Some(path) = navigate {
            if path.is_dir() {
                self.set_dir(path);
            } else {
                self.notify(format!("Cannot open {}", self.path_edit));
            }
        }
        if rescan {
            self.rescan();
        }
    }

    pub(super) fn status_bar(&mut self, ui: &mut egui::Ui) {
        let images = self.entries.iter().filter(|e| e.is_image).count();
        let folders = self.entries.iter().filter(|e| e.is_dir).count();
        let left = format!(
            "{} image(s), {} folder(s), {} selected",
            images,
            folders,
            self.multi.len().max(1)
        );

        let middle = self
            .current()
            .map(|e| {
                format!(
                    "{}  {}  {}",
                    e.name,
                    imaging::human_bytes(e.size),
                    imaging::format_time(e.modified)
                )
            })
            .unwrap_or_default();

        let right = if self.mode == Mode::Viewer {
            format!("{:.0}%", self.zoom * 100.0)
        } else {
            format!("{} x {}", self.thumb_size, self.thumb_size)
        };

        // The bar is opt-in now that feedback rides the floating notice
        // card, so when it is on it shows plain folder facts, nothing else.
        ui.horizontal(|ui| {
            theme::status_field(ui, &left, 320.0);
            theme::status_field(ui, &middle, (ui.available_width() - 120.0).max(80.0));
            theme::status_field(ui, &right, 90.0);
        });
    }
}
