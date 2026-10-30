//! Dialogs and transient feedback cards.

use super::*;

impl App {
    // -- dialogs -----------------------------------------------------------

    pub(super) fn options_window(&mut self, ctx: &Context) {
        let mut clear_cache = false;
        let mut rescan = false;
        let mut open = self.settings_open;
        egui::Window::new("Settings")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(410.0)
            .default_pos([320.0, 200.0])
            .show(ctx, |ui| {
                ui.set_width(390.0);
                ui.add_space(2.0);

                Self::settings_group(ui, "Browser", |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Thumbnail size");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(format!("{} px", self.thumb_size));
                            let mut size = self.thumb_size as f32;
                            if theme::settings_slider(ui, &mut size, 32.0..=320.0, 1.0).changed() {
                                self.thumb_size = size.round() as u32;
                                self.caches.clear_thumbs();
                            }
                        });
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label("Show folders in browser");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if theme::settings_switch(ui, &mut self.show_folders).changed() {
                                rescan = true;
                            }
                        });
                    });
                });

                Self::settings_group(ui, "Viewer", |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Use black background");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if theme::settings_switch(ui, &mut self.canvas_dark).changed() {
                                ctx.request_repaint();
                            }
                        });
                    });
                });

                Self::settings_group(ui, "Slideshow", |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Change image every");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(format!("{:.1} s", self.slideshow_delay));
                            theme::settings_slider(ui, &mut self.slideshow_delay, 0.5..=30.0, 0.5);
                        });
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label("Enter full screen automatically");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if theme::settings_switch(ui, &mut self.slideshow_fullscreen).changed()
                            {
                                self.state_dirty = true;
                            }
                        });
                    });
                });

                Self::settings_group(ui, "Storage", |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Thumbnail cache");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Clear Cache…").clicked() {
                                clear_cache = true;
                            }
                        });
                    });
                    ui.add_space(2.0);
                    ui.label(
                        egui::RichText::new(imaging::cache_root().display().to_string())
                            .small()
                            .color(ui.visuals().weak_text_color()),
                    );
                });
            });
        self.settings_open = open;

        if clear_cache {
            let removed = imaging::clear_thumbnail_cache().unwrap_or(0);
            self.caches.clear_thumbs();
            let (_, dropped) = self.loader.bump_generation();
            self.caches.abandon(&dropped);
            self.notify(format!("Cleared {removed} cached thumbnail(s)"));
        }
        if rescan {
            self.rescan();
        }
    }

    /// An inset preference group: a compact approximation of the grouped
    /// rows used by macOS Settings, with controls aligned in a quiet card.
    pub(super) fn settings_group(
        ui: &mut egui::Ui,
        title: &str,
        add_contents: impl FnOnce(&mut egui::Ui),
    ) {
        ui.label(
            egui::RichText::new(title)
                .small()
                .color(ui.visuals().weak_text_color()),
        );
        ui.add_space(2.0);
        egui::Frame::new()
            .fill(ui.visuals().widgets.inactive.weak_bg_fill)
            .corner_radius(8.0)
            .inner_margin(egui::Margin::symmetric(12, 9))
            .show(ui, add_contents);
        ui.add_space(8.0);
    }

    pub(super) fn about_window(&mut self, ctx: &Context) {
        let mut open = self.about_open;
        let icon = self.about_icon(ctx);
        egui::Window::new("About Lumiere")
            .open(&mut open)
            .resizable(false)
            .default_pos([360.0, 220.0])
            .show(ctx, |ui| {
                // The window's default 8px margin is a touch cramped at this
                // size; pad each side by another 8 so the text has room.
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(80.0, 80.0), egui::Sense::hover());
                        match icon.as_ref() {
                            Some(icon) => {
                                ui.painter().image(
                                    icon.id(),
                                    rect,
                                    egui::Rect::from_min_max(
                                        egui::pos2(0.0, 0.0),
                                        egui::pos2(1.0, 1.0),
                                    ),
                                    egui::Color32::WHITE,
                                );
                            }
                            // The png is only there with the bundle or in the source
                            // tree; a drawn glyph keeps the panel intact elsewhere.
                            None => {
                                theme::draw_icon(
                                    ui,
                                    rect.center(),
                                    72.0,
                                    Icon::Image,
                                    ui.visuals().text_color(),
                                );
                            }
                        }
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new("Lumiere").strong());
                            ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                        });
                    });
                    ui.add_space(8.0);
                });
            });
        self.about_open = open;
    }

    /// The about panel's icon: the app's own artwork, loaded once from the
    /// shipped png so the fallback panel matches what Finder shows.
    pub(super) fn about_icon(&mut self, ctx: &Context) -> Option<TextureHandle> {
        if self.app_icon.is_none() {
            if let Some(path) = Self::about_icon_path() {
                if let Ok(decoded) = image::open(&path).map(|img| {
                    // egui samples without mipmaps, so shrinking a 512px
                    // source straight to 80px in one linear pass smears it;
                    // downsample properly to 2x display size first.
                    img.resize(160, 160, image::imageops::FilterType::Lanczos3)
                        .to_rgba8()
                }) {
                    let (width, height) = decoded.dimensions();
                    let img = egui::ColorImage::from_rgba_unmultiplied(
                        [width as usize, height as usize],
                        decoded.as_ref(),
                    );
                    self.app_icon = Some(ctx.load_texture("app-icon", img, Default::default()));
                }
            }
        }
        self.app_icon.clone()
    }

    /// Where the icon png lives: `Contents/Resources` when running from the
    /// bundled macOS app, next to the executable on a Linux install,
    /// `assets/` when run straight from the source tree.
    pub(super) fn about_icon_path() -> Option<PathBuf> {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(contents) = exe.parent().and_then(Path::parent) {
                let bundled = contents.join("Resources/AppIcon.png");
                if bundled.is_file() {
                    return Some(bundled);
                }
            }
            if let Some(next_to_exe) = exe.parent().map(|p| p.join("AppIcon.png")) {
                if next_to_exe.is_file() {
                    return Some(next_to_exe);
                }
            }
        }
        let dev = PathBuf::from("assets/AppIcon.png");
        dev.is_file().then_some(dev)
    }

    pub(super) fn explore_window(&mut self, ctx: &Context) {
        let mut open = self.explore_open;
        let mut choose: Option<PathBuf> = None;
        let mut browse: Option<PathBuf> = None;
        let current = self.explore_path.clone();
        egui::Window::new("Explore Folder")
            .open(&mut open)
            .default_pos([340.0, 180.0])
            .default_size([420.0, 380.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Up").clicked() {
                        if let Some(parent) = current.parent() {
                            browse = Some(parent.to_path_buf());
                        }
                    }
                    ui.label(egui::RichText::new(current.display().to_string()).monospace());
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let dirs = fsmodel::read_directory(&current, true, "*.*");
                        for entry in dirs.iter().filter(|e| e.is_dir) {
                            let response = ui.selectable_label(false, &entry.name);
                            if response.double_clicked() || response.clicked() {
                                browse = Some(entry.path.clone());
                            }
                        }
                        if dirs.iter().all(|e| !e.is_dir) {
                            ui.label("(no folders)");
                        }
                    });
                ui.separator();
                if ui.button("Select this folder").clicked() {
                    choose = Some(current.clone());
                }
            });
        self.explore_open = open;
        if let Some(path) = browse {
            self.explore_path = path;
        }
        if let Some(path) = choose {
            self.set_dir(path);
        }
    }

    pub(super) fn rename_window(&mut self, ctx: &Context) {
        let Some(path) = self.rename_target.clone() else {
            return;
        };
        let mut commit = false;
        let mut cancel = false;
        egui::Window::new("Rename")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                if let Some(parent) = path.parent() {
                    ui.label(parent.display().to_string());
                }
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.rename_text)
                        .desired_width(320.0)
                        .cursor_at_end(true),
                );
                self.text_field_ids.push(response.id);
                response.request_focus();
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Rename").clicked() {
                        commit = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    commit = true;
                }
            });

        if cancel {
            self.rename_target = None;
        }
        if commit {
            self.rename_target = None;
            let name = self.rename_text.clone();
            self.apply_rename(&path, &name);
        }
    }

    /// The floating card the app uses for feedback: a job in flight, or a
    /// short report of what just happened. It is never modal — the content
    /// stays usable underneath — and it goes away on its own.
    pub(super) fn notice_card(&mut self, ctx: &Context) {
        let (title, detail, actions) = if let Some(job) = self.job.as_ref() {
            let text = if job.done == 0 {
                format!("{} {} item(s){}", job.prefix, job.total, job.tail)
            } else {
                format!(
                    "{} {} of {}{} — {}",
                    job.prefix, job.done, job.total, job.tail, job.last_name
                )
            };
            (text, None, Vec::new())
        } else {
            let Some(notice) = self.notice.as_ref() else {
                return;
            };
            (
                notice.title.clone(),
                notice.detail.clone(),
                notice.actions.clone(),
            )
        };

        egui::Area::new(egui::Id::new("notice_card"))
            .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -14.0])
            .order(egui::Order::Middle)
            .show(ctx, |ui| {
                let visuals = &ctx.style().visuals;
                let frame = egui::Frame::new()
                    .fill(visuals.window_fill)
                    .stroke(visuals.widgets.noninteractive.bg_stroke)
                    .corner_radius(10.0)
                    .inner_margin(egui::Margin::symmetric(14, 10));
                frame.show(ui, |ui| {
                    ui.set_max_width(460.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(&title).strong());
                        // All results can be dismissed; the progress card
                        // stays visible until its worker finishes.
                        if self.job.is_none() {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let resp = ui.small_button("Dismiss");
                                    theme::note_rect(ui, "Dismiss", resp.rect);
                                    if resp.clicked() {
                                        self.notice = None;
                                    }
                                },
                            );
                        }
                    });
                    if let Some(detail) = &detail {
                        ui.label(egui::RichText::new(detail).weak());
                    }
                    if !actions.is_empty() {
                        ui.horizontal(|ui| {
                            for action in actions {
                                let label = match &action {
                                    NoticeAction::Reveal(_) => "Show in Finder",
                                    NoticeAction::Retry(_) => "Retry",
                                };
                                let resp = ui.button(label);
                                theme::note_rect(ui, label, resp.rect);
                                if resp.clicked() {
                                    match action {
                                        NoticeAction::Reveal(path) => {
                                            crate::panels::reveal_in_finder(&path);
                                        }
                                        NoticeAction::Retry(op) => self.start_job(op),
                                    }
                                    self.notice = None;
                                }
                            }
                        });
                    }
                });
            });
    }

    pub(super) fn transfer_window(&mut self, ctx: &Context) {
        let Some(pending) = self.transfer.clone() else {
            return;
        };
        let mut apply = false;
        let mut cancel = false;
        let title = if pending.copy {
            "Copy to Folder"
        } else {
            "Move to Folder"
        };
        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(format!("{} item(s)", pending.paths.len()));
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.transfer.as_mut().unwrap().dest)
                        .desired_width(320.0)
                        .cursor_at_end(true),
                );
                self.text_field_ids.push(response.id);
                response.request_focus();
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let label = if pending.copy { "Copy" } else { "Move" };
                    if ui.button(label).clicked() {
                        apply = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    apply = true;
                }
            });

        if cancel {
            self.transfer = None;
        }
        if apply {
            let dest = self
                .transfer
                .as_ref()
                .map(|pending| pending.dest.clone())
                .unwrap_or_default();
            self.transfer = None;
            let dest = PathBuf::from(dest.trim());
            self.start_job(FileOp::Transfer {
                paths: pending.paths,
                dest,
                copy: pending.copy,
            });
        }
    }

    /// The built-in confirmation dialog, shown where no native alert is
    /// available. It reuses the alert's own text, so the fallback and the
    /// native alert cannot drift apart.
    pub(super) fn confirm_trash_window(&mut self, ctx: &Context) {
        let Some(paths) = self.confirm_trash.clone() else {
            return;
        };
        let detail = crate::alerts::trash_alert_text(&paths).1;
        egui::Window::new("Move to Trash")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(&detail);
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Move to Trash").clicked() {
                        self.confirm_trash = None;
                        self.trash_now(paths);
                    }
                    if ui.button("Cancel").clicked() {
                        self.confirm_trash = None;
                    }
                });
            });
    }
}
