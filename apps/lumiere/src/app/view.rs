//! Frame composition and window layout.

use super::*;

impl App {
    /// One frame of UI, usable from tests as well as the eframe backend.
    pub fn draw(&mut self, ctx: &Context) {
        self.pump_listing();
        if !cfg!(test) && self.dir_watch.poll(ctx, &self.dir) {
            self.refresh_listing();
        }
        // Finder can send this while Lumiere is already running. Drain all
        // documents so a multi-file Open With request has deterministic
        // last-document-selected behaviour.
        for file in crate::macos_open::take_opened_files() {
            self.open_document(file);
        }
        // The text fields are re-collected as the panels are laid out; a
        // focused button or grid cell must not count as "typing".
        self.text_field_ids.clear();
        if let Some(path) = self.current_image_path() {
            self.open_with_cache.prefetch(&path);
        }
        // Warm one representative per format as soon as a folder is shown,
        // so hovering the submenu never performs native lookup or icon decode.
        self.open_with_cache
            .warm_folder(&self.entries, self.loader.generation());
        self.open_with_cache.pump(ctx);

        // AppKit transient UI must run after the egui context menu that
        // requested it has closed; otherwise macOS can drop the presentation.
        if let Some(action) = self.pending_system_action.take() {
            match action {
                PendingSystemAction::Share(paths) => self.share_selection(paths),
                PendingSystemAction::Reveal(paths) => self.reveal_selection(paths),
                PendingSystemAction::OpenWith { paths, application } => {
                    let application = match application {
                        Some(application) => Some(application),
                        None => match crate::panels::choose_application() {
                            crate::panels::PanelResult::Chose(application) => Some(application),
                            crate::panels::PanelResult::Cancelled => None,
                            crate::panels::PanelResult::Unavailable => {
                                self.notify("Application chooser is unavailable");
                                None
                            }
                        },
                    };
                    if let Some(application) = application {
                        if !crate::panels::open_with(&paths, &application) {
                            self.notify("Couldn't open the file in the selected application");
                        }
                    }
                }
            }
        }

        // Whatever the file-operation worker finished this frame becomes
        // the result card before anything is painted.
        self.pump_job(ctx);

        // A file dragged from Finder delivers no events at all — the press
        // and release belong to the source app — so the drop is watched
        // through the pasteboard and the physical button state instead.
        self.pump_dnd(ctx);

        let title = self.title();
        if title != self.last_title {
            self.last_title = title.clone();
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        }
        self.pump(ctx);
        if let Some(target) = self.restore_pos {
            self.restore_frames += 1;
            if self.restore_frames == 1 {
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(target));
            } else if self.restore_frames >= 3 {
                let reported = ctx.input(|i| {
                    i.viewport()
                        .outer_rect
                        .map(|rect| rect.min)
                        .unwrap_or_default()
                });
                let delta = target - reported;
                if delta.length() > 2.0 {
                    ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(target + delta));
                }
                self.restore_pos = None;
            }
        }
        let viewport_size = ctx.input(|i| {
            i.viewport()
                .inner_rect
                .map(|rect| rect.size())
                .unwrap_or_default()
        });
        let viewport_pos = ctx.input(|i| {
            i.viewport()
                .outer_rect
                .map(|rect| rect.min)
                .unwrap_or_default()
        });
        if viewport_size.x > 0.0 && (viewport_size - self.last_size).length() > 2.0 {
            self.last_size = viewport_size;
            self.state_dirty = true;
        }
        if viewport_pos.x != 0.0 && (viewport_pos - self.last_pos).length() > 2.0 {
            self.last_pos = viewport_pos;
            self.state_dirty = true;
        }
        if self.state_dirty && !cfg!(test) {
            self.state_dirty = false;
            let state = crate::state::State {
                last_dir: Some(self.dir.clone()),
                width: viewport_size.x,
                height: viewport_size.y,
                x: viewport_pos.x,
                y: viewport_pos.y,
                thumb_size: self.thumb_size,
                sort: sort_key_name(self.sort).to_owned(),
                sort_reverse: self.sort_reverse,
                filter: self.filter.clone(),
                show_folders: i32::from(self.show_folders),
                show_sidebar: i32::from(self.show_sidebar),
                show_inspector: i32::from(self.show_inspector),
                slideshow_fullscreen: i32::from(self.slideshow_fullscreen),
                sidebar_dirs: self.sidebar_dirs.clone(),
                recent_dirs: self.recent_dirs.clone(),
            };
            if let Err(error) = crate::state::save(&state) {
                log::warn!("could not save session: {error}");
                self.notify("Couldn't save window and browsing preferences");
            }
        }
        // AppKit offers key equivalents to the menu bar before the window ever
        // sees the key, so a menu that claims `⌘A` or Return would take them away
        // from an egui text field that is open. Track who owns the keyboard.
        self.handle_keys(ctx);
        self.tick_slideshow(ctx);

        // The native menu bar is the app's only menu on macOS: push the current
        // description to it and pick up whatever the user clicked.
        if crate::menu::NATIVE {
            crate::menu::sync(&self.menu_spec());
            for action in crate::menu::take_events() {
                self.act(action, ctx);
            }
        }

        // The card goes away on its own: quick reports fade fast, failures
        // that offer next steps stay a little longer. Nothing else repaints
        // an idle window, so while a card is up the app keeps a slow
        // repaint going — that is what makes the expiry (and a job's
        // progress ticks) actually take effect on screen.
        if self.notice.is_some() || self.job.is_some() {
            ctx.request_repaint_after(Duration::from_millis(150));
        }
        if let Some(notice) = &self.notice {
            // A failure with Retry/Reveal is actionable information, not a
            // toast. Keep it until dismissed so it cannot disappear while
            // the user is reading the error details.
            if notice.actions.is_empty() && notice.born.elapsed().as_secs_f32() > 2.0 {
                self.notice = None;
            }
        }

        let chrome = !self.fullscreen && self.mode == Mode::Browser;

        if chrome {
            self.menu_bar_panel(ctx);
            if self.show_toolbar {
                egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
                    self.tool_row(ui);
                });
            }
            egui::TopBottomPanel::top("address").show(ctx, |ui| {
                self.address_row(ui);
            });
            if self.show_status {
                egui::TopBottomPanel::bottom("statusbar")
                    .exact_height(24.0)
                    .show(ctx, |ui| {
                        self.status_bar(ui);
                    });
            }
            // The content pane takes priority in a narrow window: both side
            // panels step aside (their on/off state is kept, so they come
            // back when the window widens again). An unknown width counts as
            // wide, so the panes stay visible by default.
            let panels_fit = ctx.input(|i| {
                let width = i.viewport_rect().width();
                width <= 0.0 || width >= 700.0
            });
            if !panels_fit && (self.show_sidebar || self.show_inspector) {
                egui::TopBottomPanel::top("compact_panel_notice")
                    .exact_height(22.0)
                    .show(ctx, |ui| {
                        ui.label(
                            egui::RichText::new(
                                "Sidebar and Inspector are hidden until the window is wider.",
                            )
                            .small()
                            .color(ui.visuals().weak_text_color()),
                        );
                    });
            }
            if self.show_sidebar && panels_fit {
                egui::SidePanel::left("sidebar")
                    .resizable(true)
                    .default_width(200.0)
                    .min_width(140.0)
                    .show(ctx, |ui| {
                        crate::browser::sidebar_pane(self, ui);
                    });
            }
            if self.show_inspector && panels_fit {
                egui::SidePanel::right("previewpane")
                    .resizable(true)
                    .default_width(280.0)
                    .min_width(160.0)
                    .show(ctx, |ui| {
                        crate::browser::preview_pane(self, ui);
                    });
            }
        } else if !self.fullscreen {
            self.menu_bar_panel(ctx);
            if self.show_status {
                egui::TopBottomPanel::bottom("statusbar")
                    .exact_height(24.0)
                    .show(ctx, |ui| {
                        self.status_bar(ui);
                    });
            }
        }

        let fill = if self.mode == Mode::Viewer {
            canvas_color(self, ctx)
        } else {
            ctx.style().visuals.panel_fill
        };
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(fill)
                    .inner_margin(egui::Margin::same(0)),
            )
            .show(ctx, |ui| match self.mode {
                Mode::Browser => crate::browser::grid(self, ui),
                Mode::Viewer => crate::viewer::view(self, ui),
            });

        // The feedback card floats above the content, under the dialogs.
        self.notice_card(ctx);

        self.options_window(ctx);
        self.about_window(ctx);
        self.explore_window(ctx);
        self.rename_window(ctx);
        self.transfer_window(ctx);
        self.confirm_trash_window(ctx);

        // Only a real text field holding the focus withdraws the menu's key
        // equivalents; a focused button, grid cell or tree row does not.
        self.text_focused = self
            .text_field_ids
            .iter()
            .any(|id| ctx.memory(|m| m.focused() == Some(*id)));
    }
}
