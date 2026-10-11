//! The classic three pane browser: folder tree, thumbnail grid, preview pane.

use egui::text::{LayoutJob, TextWrapping};
use egui::{pos2, vec2, Align, Color32, FontId, Rect, Sense, Stroke, TextFormat, Ui, Vec2};

use crate::app::{Action, App, Mode};
use crate::imaging;
use crate::loader::Kind;
use crate::theme::{self, Icon};

/// The stable identity of a grid cell, so focus requests and focus tracking
/// keep working even when the list re-sorts or re-scans.
pub(crate) fn grid_cell_id(index: usize) -> egui::Id {
    egui::Id::new("lumiere grid cell").with(index)
}

/// The stable identity of a folder-tree row.
pub(crate) fn tree_row_id(index: usize) -> egui::Id {
    egui::Id::new("lumiere tree row").with(index)
}

/// The arrow keys act on the focused pane's rows/cells and must not be
/// hijacked by egui's own directional focus search, so each pane locks them
/// for the focused row. Tab and Escape stay free, so the user can still
/// leave the pane or drop the focus.
fn focus_lock() -> egui::EventFilter {
    egui::EventFilter {
        horizontal_arrows: true,
        vertical_arrows: true,
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Left pane: folder tree
// ---------------------------------------------------------------------------

pub fn tree_pane(app: &mut App, ui: &mut Ui) {
    // Which row (if any) holds the keyboard focus right now; `None` also
    // covers "focus is on something else entirely". The tracker is kept in
    // step with the live focus.
    let visible = app.tree.visible();
    let focused_row = match ui.ctx().memory(|m| m.focused()) {
        Some(focused_id) => visible
            .iter()
            .find(|(index, _)| tree_row_id(*index) == focused_id)
            .map(|(index, _)| *index),
        None => None,
    };
    app.tree_focus = focused_row;

    // Row navigation, only while a tree row is the one that has focus.
    if focused_row.is_some() {
        let mut delta: Option<isize> = None;
        let mut open = false;
        let mut collapse = false;
        let mut expand = false;
        ui.ctx().input_mut(|i| {
            if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                delta = Some(1);
            } else if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                delta = Some(-1);
            } else if i.consume_key(egui::Modifiers::NONE, egui::Key::Enter) {
                open = true;
            } else if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight) {
                expand = true;
            } else if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft) {
                collapse = true;
            }
        });

        if open {
            if let Some(index) = focused_row {
                app.tree_selected = Some(index);
                app.tree.ensure_loaded(index);
                app.tree.set_expanded(index, true);
                app.set_dir(app.tree.node(index).path.clone());
            }
        }
        if expand || collapse {
            if let Some(index) = focused_row {
                if app.tree.node(index).has_children || !app.tree.node(index).loaded {
                    // Load the children first, exactly like the expander's
                    // click does, so an opened row actually reveals them.
                    app.tree.ensure_loaded(index);
                    app.tree.set_expanded(index, expand);
                }
                // Keep the focus on the row while it opens or closes; the
                // search the arrow would otherwise start is cancelled.
                ui.ctx()
                    .memory_mut(|m| m.move_focus(egui::FocusDirection::None));
            }
        }
        if let Some(delta) = delta {
            let count = visible.len() as isize;
            if count > 0 {
                let row = focused_row
                    .map(|index| visible.iter().position(|(i, _)| *i == index).unwrap_or(0))
                    .unwrap_or(0);
                let next = (row as isize + delta).clamp(0, count - 1) as usize;
                let index = visible[next].0;
                app.tree_selected = Some(index);
                // The row follows the selection, and egui's own directional
                // focus search is cancelled so the two never drift apart.
                ui.ctx().memory_mut(|m| {
                    m.move_focus(egui::FocusDirection::None);
                    m.request_focus(tree_row_id(index));
                });
            }
        }
    }

    let items = visible;

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (index, depth) in items {
                let path = app.tree.node(index).path.clone();
                let label = app.tree.node(index).label.clone();
                let expanded = app.tree.node(index).expanded;
                let loaded = app.tree.node(index).loaded;
                let may_have_children = !loaded || app.tree.node(index).has_children;
                let selected = app.tree_selected == Some(index);

                ui.horizontal(|ui| {
                    ui.add_space(depth as f32 * 12.0);

                    if may_have_children {
                        let (rect, response) =
                            ui.allocate_exact_size(vec2(11.0, 11.0), Sense::click());
                        if ui.is_rect_visible(rect) {
                            let p = ui.painter();
                            // A quiet plus/minus in the ambient text colour.
                            let center = rect.center();
                            let stroke = egui::Stroke::new(1.2, ui.visuals().weak_text_color());
                            p.line_segment(
                                [
                                    pos2(center.x - 3.0, center.y),
                                    pos2(center.x + 3.0, center.y),
                                ],
                                stroke,
                            );
                            if !expanded {
                                p.line_segment(
                                    [
                                        pos2(center.x, center.y - 3.0),
                                        pos2(center.x, center.y + 3.0),
                                    ],
                                    stroke,
                                );
                            }
                        }
                        if response.clicked() {
                            app.tree.toggle(index);
                        }
                    } else {
                        ui.add_space(15.0);
                    }

                    let icon = if selected {
                        Icon::FolderUp
                    } else {
                        Icon::Folder
                    };
                    let (icon_rect, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                    theme::draw_icon(
                        ui.painter(),
                        icon_rect.center(),
                        14.0,
                        icon,
                        ui.visuals().text_color(),
                    );

                    let text_response = ui.add(
                        egui::Label::new(egui::RichText::new(label.clone()).size(13.0))
                            .selectable(false)
                            .truncate(),
                    );
                    let row_id = tree_row_id(index);
                    let text_response = ui.interact(text_response.rect, row_id, Sense::click());

                    // A screen reader should hear this row the way Finder's
                    // sidebar rows are heard: a named, selectable tree item
                    // whose children are opened or closed.
                    ui.ctx().accesskit_node_builder(text_response.id, |node| {
                        node.set_role(egui::accesskit::Role::TreeItem);
                        node.set_label(label.as_str());
                        node.set_selected(selected);
                        if may_have_children {
                            node.set_expanded(expanded);
                        }
                    });

                    // Keyboard focus lands here through Tab or a screen
                    // reader; remember it, ring it, and lock the arrows so
                    // navigation stays inside the sidebar.
                    if text_response.has_focus() {
                        app.tree_focus = Some(index);
                        theme::focus_ring(ui, text_response.rect, 2.0);
                        ui.ctx()
                            .memory_mut(|m| m.set_focus_lock_filter(row_id, focus_lock()));
                    }
                    if text_response.gained_focus() {
                        ui.scroll_to_rect(text_response.rect, None);
                    }
                    theme::note_rect(ui, &format!("tree:{label}"), text_response.rect);
                    if text_response.clicked() {
                        app.tree_selected = Some(index);
                        app.tree.ensure_loaded(index);
                        app.tree.set_expanded(index, true);
                        app.set_dir(path.clone());
                    }
                });
            }
        });
}

// ---------------------------------------------------------------------------
// Centre pane: thumbnail grid
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Cell {
    thumb: u32,
    w: f32,
    h: f32,
}

#[derive(Clone, Copy)]
enum Click {
    Select(usize),
    AddToSelection(usize),
    Range(usize),
    Open(usize),
}

/// Modifiers held during the click that produced this frame's pointer release.
///
/// `InputState::modifiers` is a frame-start snapshot supplied by the backend and
/// can be stale (it has been observed as `NONE` for injected events), so the
/// pointer event's own modifiers are preferred.
fn click_modifiers(events: &[egui::Event], fallback: egui::Modifiers) -> egui::Modifiers {
    events
        .iter()
        .rev()
        .find_map(|event| match event {
            egui::Event::PointerButton {
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers,
                ..
            } => Some(*modifiers),
            _ => None,
        })
        .unwrap_or(fallback)
}

pub fn grid(app: &mut App, ui: &mut Ui) {
    let cell = Cell {
        thumb: app.thumb_size,
        w: app.thumb_size as f32 + 14.0,
        h: app.thumb_size as f32 + 36.0,
    };
    let cell_w = cell.w;
    let cell_h = cell.h;
    let available_width = ui.available_width();
    let cols = ((available_width - 8.0) / cell_w).floor().max(1.0) as usize;

    let modifiers = ui.input(|i| click_modifiers(&i.events, i.modifiers));

    // Which cell (if any) holds the keyboard focus right now, and keep the
    // tracker in step with it. `None` covers both "no focus at all" and
    // "focus is on something else entirely".
    let focused_cell = match ui.ctx().memory(|m| m.focused()) {
        Some(focused_id) => (0..app.entries.len()).find(|&index| grid_cell_id(index) == focused_id),
        None => None,
    };
    app.grid_focus = focused_cell;

    let mut click: Option<Click> = None;

    // Arrow-key navigation, while nothing is editing text and either the grid
    // has no focus at all or the focus is on one of its cells. Selection and
    // keyboard focus move together, so a screen reader and a keyboard user
    // always agree about what is picked.
    if focused_cell.is_some() || ui.ctx().memory(|m| m.focused().is_none()) {
        let mut delta: Option<isize> = None;
        let mut open = false;
        ui.ctx().input_mut(|i| {
            if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight) {
                delta = Some(1);
            } else if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft) {
                delta = Some(-1);
            } else if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                delta = Some(cols as isize);
            } else if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                delta = Some(-(cols as isize));
            } else if focused_cell.is_some()
                && i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
            {
                open = true;
            }
        });
        if open {
            click = Some(Click::Open(app.selected));
        }
        if let Some(delta) = delta {
            let count = app.entries.len() as isize;
            if count > 0 {
                let mut next = app.selected as isize + delta;
                next = next.clamp(0, count - 1);
                app.selected = next as usize;
                app.anchor = app.selected;
                app.multi.clear();
                app.multi.insert(app.selected);
                app.scroll_to_selected = true;
                // egui's own directional focus search would otherwise fire in
                // the same frame and drag the focus to some neighbouring
                // widget, out of step with the selection; cancel it and pin
                // the focus to the cell the selection just moved to.
                ui.ctx().memory_mut(|m| {
                    m.move_focus(egui::FocusDirection::None);
                    m.request_focus(grid_cell_id(app.selected));
                });
            }
        }
    }

    // Take the entry list out of `app` so the cells can mutate the caches.
    let entries = std::mem::take(&mut app.entries);
    let rows = entries.len().div_ceil(cols);
    let mut visible_rows = 0..0;
    if app.scroll_to_selected {
        app.scroll_to_selected = false;
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::new("scroll_to"), app.selected));
    }
    let scroll_to = ui
        .ctx()
        .data_mut(|d| d.remove_temp::<usize>(egui::Id::new("scroll_to")));

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show_rows(ui, cell_h, rows, |ui, range| {
            visible_rows = range.clone();
            if let Some(index) = scroll_to {
                let row = index / cols;
                let row_h = cell_h + ui.spacing().item_spacing.y;
                let top = ui.max_rect().top() + (row as f32 - range.start as f32) * row_h;
                ui.scroll_to_rect(Rect::from_min_size(pos2(0.0, top), vec2(1.0, cell_h)), None);
            }
            for row in range {
                ui.horizontal(|ui| {
                    for column in 0..cols {
                        let index = row * cols + column;
                        let Some(entry) = entries.get(index) else {
                            break;
                        };
                        if let Some(hit) = thumbnail(app, ui, entry, index, cell, modifiers) {
                            click = Some(hit);
                        }
                    }
                });
            }
        });

    // Ask for the thumbnails of the rows just outside the viewport, so
    // scrolling does not show a wall of empty cells.
    let warm = prefetch_range(visible_rows, cols, entries.len());
    if !warm.is_empty() {
        for entry in entries[warm].iter().filter(|e| e.is_image) {
            if app.caches.thumb(&entry.path, cell.thumb).is_none() {
                app.caches
                    .request(&app.loader, Kind::Thumb, &entry.path, cell.thumb);
            }
        }
    }

    app.entries = entries;

    let Some(hit) = click else { return };
    match hit {
        Click::Select(index) => {
            app.selected = index;
            app.anchor = index;
            app.multi.clear();
            app.multi.insert(index);
        }
        Click::AddToSelection(index) => {
            if !app.multi.remove(&index) {
                app.multi.insert(index);
            }
            app.selected = index;
            app.anchor = index;
        }
        Click::Range(index) => {
            let (from, to) = if index >= app.anchor {
                (app.anchor, index)
            } else {
                (index, app.anchor)
            };
            app.multi.clear();
            for i in from..=to {
                app.multi.insert(i);
            }
            app.selected = index;
        }
        Click::Open(index) => {
            app.selected = index;
            app.multi.clear();
            app.multi.insert(index);
            if app.entries[index].is_image {
                app.mode = Mode::Viewer;
                app.fit = true;
                app.pan = Vec2::ZERO;
            } else {
                let path = app.entries[index].path.clone();
                app.set_dir(path);
            }
        }
    }
}

/// One row before and one row after the visible rows, as entry indices.
fn prefetch_range(
    visible: std::ops::Range<usize>,
    cols: usize,
    total: usize,
) -> std::ops::Range<usize> {
    if visible.end <= visible.start {
        return 0..0;
    }
    let from = visible.start.saturating_sub(1) * cols;
    let to = (visible.end + 1) * cols;
    from..to.min(total)
}

fn thumbnail(
    app: &mut App,
    ui: &mut Ui,
    entry: &crate::fsmodel::Entry,
    index: usize,
    cell: Cell,
    modifiers: egui::Modifiers,
) -> Option<Click> {
    let size = cell.thumb;
    let (rect, _) = ui.allocate_exact_size(vec2(cell.w, cell.h), Sense::hover());
    let response = ui.interact(rect, grid_cell_id(index), Sense::click());
    if !ui.is_rect_visible(rect) && !response.has_focus() {
        return None;
    }

    let selected = app.multi.contains(&index) || app.selected == index;
    // The grid is painted rather than built from widgets, so without this the
    // whole browser area is silent to VoiceOver. Each cell is a named list
    // item that a screen reader can select, and it announces the entry's
    // type and size in the value, the way a Finder cell does.
    let value = if entry.is_dir {
        "Folder".to_owned()
    } else {
        format!(
            "{}, {}",
            imaging::extension(&entry.path).to_uppercase(),
            imaging::human_bytes(entry.size)
        )
    };
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_role(egui::accesskit::Role::ListItem);
        node.set_role_description(if entry.is_dir { "folder" } else { "image" });
        node.set_label(entry.name.as_str());
        node.set_value(value.as_str());
        node.set_selected(selected);
    });

    // Keyboard focus lands here through Tab or a screen reader; remember it,
    // ring it, and lock the arrows so the selection — not some neighbouring
    // widget — follows the keys.
    if response.has_focus() {
        app.grid_focus = Some(index);
        theme::focus_ring(ui, rect, 2.0);
        ui.ctx()
            .memory_mut(|m| m.set_focus_lock_filter(response.id, focus_lock()));
    }
    if response.gained_focus() {
        ui.scroll_to_rect(rect, None);
    }

    let painter = ui.painter();

    // The selection is the adaptive tint, not a fixed colour: pale blue in
    // light mode, deep blue in dark mode.
    if selected {
        painter.rect_filled(rect, 0, ui.visuals().selection.bg_fill);
    }

    let box_size = size as f32;
    let box_rect = Rect::from_min_size(
        pos2(rect.center().x - box_size / 2.0, rect.top() + 4.0),
        vec2(box_size, box_size),
    );

    if entry.is_dir {
        theme::draw_icon(
            painter,
            box_rect.center(),
            box_size * 0.5,
            Icon::Folder,
            ui.visuals().text_color(),
        );
    } else if let Some(thumb) = app.caches.thumb(&entry.path, size) {
        let scale = (box_size / thumb.width as f32)
            .min(box_size / thumb.height as f32)
            .min(1.0);
        let drawn = Vec2::new(thumb.width as f32, thumb.height as f32) * scale;
        let drawn_rect = Rect::from_center_size(box_rect.center(), drawn);
        painter.image(
            thumb.texture.id(),
            drawn_rect,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    } else {
        app.caches
            .request(&app.loader, Kind::Thumb, &entry.path, size);
        theme::draw_icon(
            painter,
            box_rect.center(),
            box_size * 0.4,
            Icon::Image,
            ui.visuals().weak_text_color(),
        );
    }

    if selected {
        painter.rect_stroke(
            box_rect,
            0.0,
            ui.visuals().selection.stroke,
            egui::StrokeKind::Inside,
        );
    } else {
        painter.rect_stroke(
            box_rect,
            0.0,
            ui.visuals().widgets.noninteractive.bg_stroke,
            egui::StrokeKind::Inside,
        );
    }

    // File name, centred and truncated to the cell width.
    let text_color = ui.visuals().text_color();
    let format = TextFormat {
        font_id: FontId::proportional(11.0),
        ..Default::default()
    };
    let mut job = LayoutJob::single_section(entry.name.clone(), format);
    job.wrap = TextWrapping::truncate_at_width(cell.w - 4.0);
    job.halign = Align::Center;
    job.justify = false;
    let galley = painter.layout_job(job);
    let text_pos = pos2(
        rect.center().x - galley.size().x / 2.0,
        box_rect.bottom() + 4.0,
    );
    painter.galley(text_pos, galley, text_color);

    if response.secondary_clicked() {
        app.selected = index;
    }
    response.context_menu(|ui| {
        if entry.is_image {
            if ui.button("View").clicked() {
                app.mode = Mode::Viewer;
                ui.close();
            }
            if ui.button("Open with default app").clicked() {
                app.selected = index;
                app.act(Action::OpenWithDefault, ui.ctx());
                ui.close();
            }
        } else if ui.button("Explore").clicked() {
            app.set_dir(entry.path.clone());
            ui.close();
        }
        if ui.button("Get Info").clicked() {
            app.selected = index;
            app.show_inspector = true;
            ui.close();
        }
        if ui.button("Move to Trash").clicked() {
            app.selected = index;
            app.begin_trash(vec![entry.path.clone()]);
            ui.close();
        }
    });

    if response.double_clicked() {
        return Some(Click::Open(index));
    }
    if response.clicked() {
        if modifiers.shift {
            return Some(Click::Range(index));
        }
        if modifiers.command {
            return Some(Click::AddToSelection(index));
        }
        return Some(Click::Select(index));
    }
    None
}

// ---------------------------------------------------------------------------
// Right pane: preview with the file's metadata below it
// ---------------------------------------------------------------------------

pub fn preview_pane(app: &mut App, ui: &mut Ui) {
    // egui sizes this panel from the content's used rect every frame, so the
    // content must span exactly the panel's inner width and never exceed it;
    // otherwise the pane drifts toward the width of whatever entry is
    // selected (usually its long file name) instead of holding the width the
    // user dragged it to.
    ui.take_available_width();

    let Some(entry) = app.current().cloned() else {
        ui.label("No selection");
        return;
    };

    let width = (ui.available_width() - 8.0).max(32.0);

    // The preview sits in a rounded well on the standard field background
    // (white in light mode, near black in dark), not a sunken Win95 frame.
    let well = |ui: &Ui, rect: Rect| {
        ui.painter().rect(
            rect,
            6.0,
            ui.visuals().extreme_bg_color,
            Stroke::NONE,
            egui::StrokeKind::Middle,
        );
    };

    if entry.is_image {
        let request_size = width as u32;
        if let Some(preview) = app.caches.thumb(&entry.path, request_size) {
            let scale = (width / preview.width as f32)
                .min(width / preview.height as f32)
                .min(1.0);
            let drawn = vec2(preview.width as f32, preview.height as f32) * scale;
            let height = (ui.available_height() - 24.0).min(drawn.y).max(16.0);
            let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
            let drawn_rect = Rect::from_center_size(
                rect.center(),
                vec2(drawn.x.min(rect.width()), drawn.y.min(rect.height())),
            );
            well(ui, rect);
            ui.painter().image(
                preview.texture.id(),
                drawn_rect,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        } else {
            app.caches
                .request(&app.loader, Kind::Preview, &entry.path, request_size);
            let (rect, _) = ui.allocate_exact_size(vec2(width, width * 0.7), Sense::hover());
            well(ui, rect);
            theme::draw_icon(
                ui.painter(),
                rect.center(),
                32.0,
                Icon::Image,
                ui.visuals().weak_text_color(),
            );
        }
    } else {
        let (rect, _) = ui.allocate_exact_size(vec2(width, width * 0.5), Sense::hover());
        well(ui, rect);
        theme::draw_icon(
            ui.painter(),
            rect.center(),
            width * 0.4,
            Icon::Folder,
            ui.visuals().text_color(),
        );
    }

    ui.add_space(6.0);
    // A long file name wraps to as many lines as it needs inside the pane's
    // fixed width (egui also breaks single long words), so it can never push
    // the pane wider.
    ui.label(egui::RichText::new(entry.name.clone()).strong().size(13.0));
    ui.label(format!(
        "{}  {}",
        imaging::human_bytes(entry.size),
        imaging::format_time(entry.modified)
    ));

    // The metadata below the picture. It omits the rows the header above
    // already prints (name, size, modified) and scrolls when it overflows.
    let mut rows: Vec<(String, String)> = vec![
        (
            "Type".into(),
            if entry.is_dir {
                "Folder".to_owned()
            } else {
                imaging::extension(&entry.path).to_uppercase()
            },
        ),
        ("Path".into(), entry.path.display().to_string()),
    ];
    if let Some(full) = app.caches.full(&entry.path) {
        rows.push((
            "Dimensions".into(),
            format!("{} x {}", full.width, full.height),
        ));
        rows.push(("Format".into(), full.info.format.clone()));
        for (key, value) in &full.info.exif {
            rows.push((key.clone(), value.clone()));
        }
    } else if entry.is_image {
        app.caches
            .request(&app.loader, Kind::Preview, &entry.path, width as u32);
        rows.push(("Dimensions".into(), "loading...".into()));
        let info = app.caches.info(&entry.path);
        rows.push(("Format".into(), info.format));
        rows.extend(info.exif);
    }

    ui.add_space(6.0);
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // A Grid sizes its columns to the full, untruncated text, so a
            // long path would push the pane wider. Each row is a fixed-width
            // key plus a value that truncates at the pane's edge instead
            // (the full value stays in the hover tooltip), so the values
            // still line up while the pane keeps its width.
            let key_font = egui::FontId::proportional(12.0);
            let key_col = rows
                .iter()
                .map(|(key, _)| {
                    ui.painter()
                        .layout_no_wrap(key.clone(), key_font.clone(), egui::Color32::WHITE)
                        .size()
                        .x
                })
                .fold(0.0, f32::max)
                .ceil();
            for (key, value) in rows {
                ui.horizontal(|ui| {
                    ui.scope(|ui| {
                        ui.set_min_width(key_col);
                        ui.label(egui::RichText::new(key).weak().size(12.0));
                    });
                    ui.add(
                        egui::Label::new(egui::RichText::new(value).size(12.0).monospace())
                            .truncate(),
                    );
                });
            }
        });
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    fn raw_input(time: f64) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(Rect::from_min_max(pos2(0.0, 0.0), pos2(1280.0, 820.0))),
            time: Some(time),
            focused: true,
            ..Default::default()
        }
    }

    fn pointer_events(pos: egui::Pos2, modifiers: egui::Modifiers) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers,
            },
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers,
            },
        ]
    }

    #[test]
    fn prefetch_range_warms_one_row_on_each_side() {
        assert_eq!(prefetch_range(2..4, 8, 200), 8..40);
        assert_eq!(prefetch_range(0..1, 8, 200), 0..16);
        assert_eq!(prefetch_range(24..25, 8, 200), 184..200);
        assert_eq!(prefetch_range(0..0, 8, 200), 0..0);
    }

    #[test]
    fn click_modifiers_prefers_the_pointer_event() {
        let events = pointer_events(pos2(10.0, 10.0), egui::Modifiers::COMMAND);
        assert_eq!(
            click_modifiers(&events, egui::Modifiers::NONE),
            egui::Modifiers::COMMAND
        );
        assert_eq!(
            click_modifiers(&[], egui::Modifiers::SHIFT),
            egui::Modifiers::SHIFT
        );
    }

    fn click_column(
        ctx: &egui::Context,
        app: &mut App,
        column: usize,
        modifiers: egui::Modifiers,
        time: f64,
    ) {
        let cell_w = app.thumb_size as f32 + 14.0;
        let cell_h = app.thumb_size as f32 + 36.0;
        let spacing = ctx.style().spacing.item_spacing.x;
        let pad = 8.0;
        let pos = pos2(
            pad + column as f32 * (cell_w + spacing) + cell_w * 0.5,
            pad + cell_h * 0.5,
        );
        // egui hit-tests against the previous pass, so the cells must be laid
        // out once before they can receive a click.
        for (n, events) in [
            vec![egui::Event::PointerMoved(pos)],
            pointer_events(pos, modifiers),
        ]
        .into_iter()
        .enumerate()
        {
            let mut input = raw_input(time + n as f64);
            input.events = events;
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| grid(app, ui));
            });
        }
    }

    #[test]
    fn cmd_and_shift_clicks_extend_the_selection() {
        let dir = std::env::temp_dir().join("lumiere_grid_test_clicks");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["one.png", "two.png", "three.png"] {
            let img = image::RgbaImage::from_pixel(64, 48, image::Rgba([9, 99, 199, 255]));
            img.save(dir.join(name)).unwrap();
        }

        let ctx = egui::Context::default();
        let mut app = App::build(&ctx, Some(dir.clone()));

        click_column(&ctx, &mut app, 0, egui::Modifiers::NONE, 1.0);
        assert_eq!(app.selected, 0);
        assert_eq!(app.multi.len(), 1);

        click_column(&ctx, &mut app, 2, egui::Modifiers::COMMAND, 2.0);
        assert_eq!(app.selected, 2);
        assert_eq!(app.multi.len(), 2);

        click_column(&ctx, &mut app, 1, egui::Modifiers::SHIFT, 3.0);
        assert_eq!(app.selected, 1);
        assert!(!app.multi.contains(&0));
        assert!(app.multi.contains(&1) && app.multi.contains(&2));

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
