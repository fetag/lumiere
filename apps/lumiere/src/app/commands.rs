//! Commands, keyboard input, and slideshow transitions.

use super::*;

/// Open `path` with the OS default application (the "Open with default app"
/// menu item). Each OS launches the default handler differently, so the
/// mechanism is per-platform.
fn open_with_default_app(path: &std::path::Path) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(path).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd")
        .arg("/c")
        .arg("start")
        .arg("")
        .arg(path)
        .spawn();
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
}

impl App {
    // -- command dispatch --------------------------------------------------

    /// Puts back the view captured when the slideshow started: the zoom, the
    /// pan and the full-screen state the user had before the show began.
    pub(super) fn restore_slideshow_view(&mut self, ctx: &Context) {
        let Some(view) = self.slideshow_saved.take() else {
            return;
        };
        self.fit = view.fit;
        self.zoom = view.zoom;
        self.pan = view.pan;
        if view.fullscreen != self.fullscreen {
            self.fullscreen = view.fullscreen;
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(view.fullscreen));
        }
    }

    pub fn act(&mut self, action: Action, ctx: &Context) {
        match action {
            Action::OpenFolder => {
                match crate::panels::choose_directory("Choose a Folder", "Choose", Some(&self.dir))
                {
                    crate::panels::PanelResult::Chose(path) => self.set_dir(path),
                    crate::panels::PanelResult::Cancelled => {}
                    crate::panels::PanelResult::Unavailable => {
                        // No native panel (other OS, or a test build): the built-in
                        // folder browser.
                        self.explore_open = true;
                        self.explore_path = self.dir.clone();
                    }
                }
            }
            Action::OpenWithDefault => {
                if let Some(path) = self.current_image_path() {
                    open_with_default_app(&path);
                }
            }
            Action::UpFolder => {
                if let Some(parent) = self.dir.parent().map(|p| p.to_path_buf()) {
                    self.set_dir(parent);
                }
            }
            Action::GoHome => {
                if let Some(home) = dirs::home_dir() {
                    self.set_dir(home);
                }
            }
            Action::GoPictures => {
                if let Some(pics) = dirs::picture_dir() {
                    self.set_dir(pics);
                }
            }
            Action::Reload => {
                self.rescan();
                self.caches.clear_failures();
            }
            Action::MoveToTrash => self.begin_trash(self.selection_paths()),
            Action::Rename => {
                if let Some(entry) = self.current().cloned() {
                    self.rename_text = entry.name.clone();
                    self.rename_target = Some(entry.path.clone());
                }
            }
            Action::MoveToFolder => self.begin_transfer(false),
            Action::CopyToFolder => self.begin_transfer(true),
            Action::GetInfo => self.show_inspector = true,
            Action::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Action::SelectAll => {
                self.multi.clear();
                for (i, entry) in self.entries.iter().enumerate() {
                    if entry.is_image {
                        self.multi.insert(i);
                    }
                }
            }
            Action::SelectNone => self.multi.clear(),
            Action::InvertSelection => {
                let mut next = HashSet::new();
                for (i, entry) in self.entries.iter().enumerate() {
                    if entry.is_image && !self.multi.contains(&i) {
                        next.insert(i);
                    }
                }
                self.multi = next;
            }
            Action::NextImage => {
                if self.mode == Mode::Viewer {
                    let dropped = self.loader.purge_thumbs();
                    self.caches.abandon(&dropped);
                }
                self.step_image(1);
            }
            Action::PrevImage => {
                if self.mode == Mode::Viewer {
                    let dropped = self.loader.purge_thumbs();
                    self.caches.abandon(&dropped);
                }
                self.step_image(-1);
            }
            Action::FirstImage => self.jump_image(true),
            Action::LastImage => self.jump_image(false),
            Action::OpenViewer => {
                self.ensure_image_selected();
                if self.current_image_path().is_some() {
                    self.mode = Mode::Viewer;
                    let dropped = self.loader.purge_thumbs();
                    self.caches.abandon(&dropped);
                    self.fit = true;
                    self.pan = Vec2::ZERO;
                }
            }
            Action::CloseViewer => {
                self.mode = Mode::Browser;
                if self.slideshow {
                    self.slideshow = false;
                    self.restore_slideshow_view(ctx);
                }
                // Without a running slideshow the full-screen state is the
                // user's own choice (⌃⌘F), so closing the viewer leaves the
                // window exactly as it was.
            }
            Action::ZoomIn => {
                self.zoom = (self.zoom * 1.25).min(crate::viewer::ZOOM_MAX);
                self.fit = false;
            }
            Action::ZoomOut => {
                self.zoom = (self.zoom / 1.25).max(crate::viewer::ZOOM_MIN);
                self.fit = false;
            }
            Action::FitWindow => {
                self.fit = true;
                self.pan = Vec2::ZERO;
            }
            Action::ActualSize => {
                self.zoom = 1.0;
                self.fit = false;
                self.pan = Vec2::ZERO;
            }
            Action::RotateCW => self.rotation = (self.rotation + 90) % 360,
            Action::RotateCCW => self.rotation = (self.rotation + 270) % 360,
            Action::FlipH => self.flip_h = !self.flip_h,
            Action::FlipV => self.flip_v = !self.flip_v,
            Action::Slideshow => {
                if !self.entries.iter().any(|e| e.is_image) {
                    return;
                }
                self.ensure_image_selected();
                self.mode = Mode::Viewer;
                let dropped = self.loader.purge_thumbs();
                self.caches.abandon(&dropped);
                // Snapshot the view so StopSlideshow can put the window and
                // the zoom back the way the user had them.
                self.slideshow_saved = Some(SlideshowView {
                    fullscreen: self.fullscreen,
                    fit: self.fit,
                    zoom: self.zoom,
                    pan: self.pan,
                });
                self.slideshow = true;
                self.slideshow_at = Instant::now();
                if self.slideshow_fullscreen && !self.fullscreen {
                    self.fullscreen = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
                }
            }
            Action::StopSlideshow => {
                self.slideshow = false;
                self.restore_slideshow_view(ctx);
            }
            Action::ToggleFullscreen => {
                self.fullscreen = !self.fullscreen;
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
            }
            Action::ThumbSmall => self.thumb_size = 64,
            Action::ThumbMedium => self.thumb_size = 120,
            Action::ThumbLarge => self.thumb_size = 200,
            Action::SortByName => self.sort = SortKey::Name,
            Action::SortByDate => self.sort = SortKey::Date,
            Action::SortBySize => self.sort = SortKey::Size,
            Action::SortByType => self.sort = SortKey::Type,
            Action::ToggleReverse => self.sort_reverse = !self.sort_reverse,
            Action::ToggleFolders => self.show_folders = !self.show_folders,
            Action::ToggleSidebar => {
                self.show_sidebar = !self.show_sidebar;
                self.state_dirty = true;
            }
            Action::ToggleInspector => {
                self.show_inspector = !self.show_inspector;
                self.state_dirty = true;
            }
            Action::ToggleToolbar => self.show_toolbar = !self.show_toolbar,
            Action::ToggleStatus => self.show_status = !self.show_status,
            Action::Settings => self.settings_open = true,
            Action::About => self.about_open = true,
        }
        if matches!(
            action,
            Action::SortByName
                | Action::SortByDate
                | Action::SortBySize
                | Action::SortByType
                | Action::ToggleReverse
                | Action::ToggleFolders
        ) {
            self.rescan();
        }
        if matches!(
            action,
            Action::ThumbSmall
                | Action::ThumbMedium
                | Action::ThumbLarge
                | Action::SortByName
                | Action::SortByDate
                | Action::SortBySize
                | Action::SortByType
                | Action::ToggleReverse
                | Action::ToggleFolders
        ) {
            self.state_dirty = true;
        }
    }

    // -- keyboard ----------------------------------------------------------

    /// Keyboard map.
    ///
    /// On macOS the native menu bar owns the `⌘` shortcuts and AppKit consumes
    /// the key event before egui ever sees it; the `⌘` handling here is what the
    /// in-window fallback menu and the headless tests drive. Both paths read the
    /// same [`Shortcut`] values out of [`Self::menu_spec`], so the menu label and
    /// the working shortcut cannot disagree.
    pub(super) fn handle_keys(&mut self, ctx: &Context) {
        if ctx.memory(|m| m.focused().is_some()) {
            return;
        }
        let shortcuts = self.shortcuts();
        let mut actions: Vec<Action> = Vec::new();
        let viewer_mode = self.mode == Mode::Viewer;
        ctx.input_mut(|i| {
            if viewer_mode {
                for key in [
                    egui::Key::Space,
                    egui::Key::ArrowRight,
                    egui::Key::ArrowDown,
                ] {
                    if i.consume_key(egui::Modifiers::NONE, key) {
                        actions.push(Action::NextImage);
                    }
                }
                // Backspace is Move to Trash (the menu shortcut), not "previous".
                for key in [egui::Key::ArrowLeft, egui::Key::ArrowUp] {
                    if i.consume_key(egui::Modifiers::NONE, key) {
                        actions.push(Action::PrevImage);
                    }
                }
            } else {
                if i.consume_key(egui::Modifiers::NONE, egui::Key::PageDown) {
                    actions.push(Action::NextImage);
                }
                if i.consume_key(egui::Modifiers::NONE, egui::Key::PageUp) {
                    actions.push(Action::PrevImage);
                }
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Home) {
                actions.push(Action::FirstImage);
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::End) {
                actions.push(Action::LastImage);
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
                if self.confirm_trash.is_some()
                    || self.rename_target.is_some()
                    || self.transfer.is_some()
                    || self.settings_open
                    || self.about_open
                    || self.explore_open
                {
                    self.confirm_trash = None;
                    self.rename_target = None;
                    self.transfer = None;
                    self.settings_open = false;
                    self.about_open = false;
                    self.explore_open = false;
                } else if self.slideshow {
                    actions.push(Action::StopSlideshow);
                } else if self.fullscreen {
                    actions.push(Action::ToggleFullscreen);
                } else if self.mode == Mode::Viewer {
                    actions.push(Action::CloseViewer);
                }
            }

            // `⌘⌫` still moves to the Trash, as before the bare key did.
            for key in [egui::Key::Backspace, egui::Key::Delete] {
                if i.consume_key(egui::Modifiers::COMMAND, key) {
                    actions.push(Action::MoveToTrash);
                }
            }

            // Menu-declared shortcuts, most specific modifier set first.
            for (shortcut, action) in shortcuts {
                let modifiers = egui_modifiers(shortcut);
                let mut hit = false;
                for key in egui_keys(shortcut.key) {
                    hit |= i.consume_key(modifiers, key);
                }
                // `⌘+` is `⇧⌘=` on the keyboards that have no dedicated `+`.
                if shortcut.key == Key::Char('+') {
                    hit |= i.consume_key(modifiers | egui::Modifiers::SHIFT, egui::Key::Equals);
                }
                if hit {
                    actions.push(action);
                }
            }

            // F2 renames as well; the menu advertises the `⌥⌘R` spelling because
            // function keys are not how a macOS user is meant to find this.
            if i.consume_key(egui::Modifiers::NONE, egui::Key::F2) {
                actions.push(Action::Rename);
            }

            // Viewer-only single keys, kept because they are handy and clash
            // with nothing.
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Plus)
                || i.consume_key(egui::Modifiers::SHIFT, egui::Key::Equals)
            {
                actions.push(Action::ZoomIn);
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Minus) {
                actions.push(Action::ZoomOut);
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::F) {
                actions.push(Action::FitWindow);
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Num1) {
                actions.push(Action::ActualSize);
            }
        });
        for action in actions {
            self.act(action, ctx);
        }
    }

    pub(super) fn tick_slideshow(&mut self, ctx: &Context) {
        if !self.slideshow {
            return;
        }
        if self.slideshow_at.elapsed().as_secs_f32() >= self.slideshow_delay {
            self.slideshow_at = Instant::now();
            let indices = self.image_indices();
            let position = indices
                .iter()
                .position(|&i| i == self.selected)
                .unwrap_or(0);
            if position + 1 >= indices.len() {
                self.act(Action::StopSlideshow, ctx);
                self.notify("Slideshow finished");
            } else {
                let dropped = self.loader.purge_thumbs();
                self.caches.abandon(&dropped);
                self.step_image(1);
            }
        }
        ctx.request_repaint_after(Duration::from_millis(100));
    }

    // -- layout ------------------------------------------------------------

    pub(super) fn title(&self) -> String {
        match self.mode {
            Mode::Browser => format!("{APP_TITLE} - {}", self.dir.display()),
            Mode::Viewer => self
                .current()
                .map(|e| format!("{} - {}", e.name, APP_TITLE))
                .unwrap_or_else(|| APP_TITLE.to_owned()),
        }
    }

    pub(super) fn pump(&mut self, ctx: &Context) {
        let protect = if self.mode == Mode::Viewer {
            self.current_image_path()
        } else {
            None
        };
        let arrived = self.caches.pump(ctx, &self.loader, protect.as_deref());
        if arrived > 0 || !self.caches.pending.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(40));
        }
    }
}
