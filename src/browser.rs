//! The classic three pane browser: folder tree, thumbnail grid, preview pane.

use egui::text::{LayoutJob, TextWrapping};
use egui::{pos2, vec2, Align, Color32, FontId, Rect, Sense, TextFormat, Ui, Vec2};

use crate::app::{Action, App, Mode};
use crate::classic::{self, Icon};
use crate::imaging;
use crate::loader::Kind;

// ---------------------------------------------------------------------------
// Left pane: folder tree
// ---------------------------------------------------------------------------

pub fn tree_pane(app: &mut App, ui: &mut Ui) {
    let items = app.tree.visible();

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
                            classic::bevel_sunken(p, rect);
                            let center = rect.center();
                            let stroke = egui::Stroke::new(1.0, classic::TEXT);
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
                    classic::draw_icon(ui.painter(), icon_rect.center(), 14.0, icon, classic::TEXT);

                    let text_response = ui.add(
                        egui::Label::new(egui::RichText::new(label.clone()).size(13.0))
                            .selectable(false)
                            .truncate(),
                    );
                    let text_response =
                        ui.interact(text_response.rect, ui.id().with(("tree", index)), Sense::click());
                    classic::note_rect(ui, &format!("tree:{label}"), text_response.rect);
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

    // Arrow key navigation, only when nothing is editing text.
    if ui.ctx().memory(|m| m.focused().is_none()) {
        let mut delta: Option<isize> = None;
        ui.ctx().input_mut(|i| {
            if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight) {
                delta = Some(1);
            } else if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft) {
                delta = Some(-1);
            } else if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                delta = Some(cols as isize);
            } else if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                delta = Some(-(cols as isize));
            }
        });
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
            }
        }
    }

    let modifiers = ui.input(|i| click_modifiers(&i.events, i.modifiers));

    // Move the entry list out of `app` so the cells can mutate the caches.
    let entries = std::mem::take(&mut app.entries);
    let rows = entries.len().div_ceil(cols);
    let mut click: Option<Click> = None;
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
    let (rect, response) = ui.allocate_exact_size(vec2(cell.w, cell.h), Sense::click());
    if !ui.is_rect_visible(rect) {
        return None;
    }

    let selected = app.multi.contains(&index) || app.selected == index;
    let painter = ui.painter();

    if selected {
        painter.rect_filled(rect, 0, classic::NAVY);
    }

    let box_size = size as f32;
    let box_rect = Rect::from_min_size(
        pos2(rect.center().x - box_size / 2.0, rect.top() + 4.0),
        vec2(box_size, box_size),
    );
    painter.rect_filled(box_rect, 0, Color32::WHITE);

    if entry.is_dir {
        classic::draw_icon(
            painter,
            box_rect.center(),
            box_size * 0.62,
            Icon::Folder,
            classic::TEXT,
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
        classic::draw_icon(
            painter,
            box_rect.center(),
            box_size * 0.4,
            Icon::Image,
            classic::SHADOW,
        );
    }

    if selected {
        painter.rect_filled(box_rect.shrink(1.0), 0, Color32::TRANSPARENT);
        classic::bevel_raised(painter, box_rect);
    } else {
        painter.rect_stroke(
            box_rect,
            0,
            egui::Stroke::new(1.0, classic::SHADOW),
            egui::StrokeKind::Inside,
        );
    }

    // File name, centred and truncated to the cell width.
    let text_color = if selected {
        Color32::WHITE
    } else {
        classic::TEXT
    };
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
                app.act(Action::Open, ui.ctx());
                ui.close();
            }
        } else if ui.button("Explore").clicked() {
            app.set_dir(entry.path.clone());
            ui.close();
        }
        if ui.button("Properties").clicked() {
            app.selected = index;
            app.preview_tab = 1;
            app.show_preview = true;
            ui.close();
        }
        if ui.button("Delete").clicked() {
            app.selected = index;
            app.confirm_delete = Some(entry.path.clone());
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
// Right pane: preview + information
// ---------------------------------------------------------------------------

pub fn preview_pane(app: &mut App, ui: &mut Ui) {
    let mut tab = app.preview_tab;
    ui.horizontal(|ui| {
        if ui.selectable_label(tab == 0, "Preview").clicked() {
            tab = 0;
        }
        if ui.selectable_label(tab == 1, "Information").clicked() {
            tab = 1;
        }
    });
    ui.separator();
    app.preview_tab = tab;

    let Some(entry) = app.current().cloned() else {
        ui.label("No selection");
        return;
    };

    let width = (ui.available_width() - 8.0).max(32.0);

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
            ui.painter().rect_filled(rect, 0, Color32::WHITE);
            classic::bevel_sunken(ui.painter(), rect);
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
            ui.painter().rect_filled(rect, 0, Color32::WHITE);
            classic::bevel_sunken(ui.painter(), rect);
            classic::draw_icon(
                ui.painter(),
                rect.center(),
                32.0,
                Icon::Image,
                classic::SHADOW,
            );
        }
    } else {
        let (rect, _) = ui.allocate_exact_size(vec2(width, width * 0.5), Sense::hover());
        ui.painter().rect_filled(rect, 0, Color32::WHITE);
        classic::bevel_sunken(ui.painter(), rect);
        classic::draw_icon(
            ui.painter(),
            rect.center(),
            width * 0.4,
            Icon::Folder,
            classic::TEXT,
        );
    }

    ui.add_space(6.0);
    ui.label(egui::RichText::new(entry.name.clone()).strong().size(13.0));
    ui.label(format!(
        "{}  {}",
        imaging::human_bytes(entry.size),
        imaging::format_time(entry.modified)
    ));

    if tab == 1 {
        ui.add_space(6.0);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut rows: Vec<(String, String)> = vec![
                    ("Name".into(), entry.name.clone()),
                    (
                        "Type".into(),
                        if entry.is_dir {
                            "Folder".to_owned()
                        } else {
                            imaging::extension(&entry.path).to_uppercase()
                        },
                    ),
                    ("Size".into(), imaging::human_bytes(entry.size)),
                    ("Modified".into(), imaging::format_time(entry.modified)),
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

                egui::Grid::new("info_grid")
                    .num_columns(2)
                    .spacing([8.0, 3.0])
                    .striped(false)
                    .show(ui, |ui| {
                        for (key, value) in rows {
                            ui.label(egui::RichText::new(key).weak().size(12.0));
                            ui.label(egui::RichText::new(value).size(12.0).monospace());
                            ui.end_row();
                        }
                    });
            });
    }
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
