//! The macOS visual layer: adaptive light/dark chrome that follows the OS,
//! the system font, native-style toolbar controls, and monochrome line icons.
//!
//! Everything that needs a colour asks the current [`egui::Visuals`] instead
//! of a fixed palette, so switching the system appearance repaints the whole
//! UI without a restart.

use egui::{
    pos2, vec2, Align, Align2, Color32, FontData, FontFamily, FontId, Margin, Pos2, Rect, Response,
    Sense, Shape, Stroke, StrokeKind, Ui,
};
use std::sync::Arc;

/// Tighten the default spacing and type to match the compact panes, and leave
/// every colour, corner radius, shadow and selection tint at egui's defaults.
/// Those defaults are adaptive: the active style is picked per frame from the
/// OS appearance (light or dark), so no theme state of our own can drift out
/// of sync with the system.
pub fn apply_theme(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = vec2(4.0, 4.0);
        style.spacing.button_padding = vec2(4.0, 2.0);
        style.spacing.window_margin = Margin::same(4);
        style.spacing.menu_margin = Margin::same(2);
        style.spacing.indent = 16.0;
        style.spacing.slider_width = 120.0;

        let body = FontId::proportional(13.0);
        style.text_styles = [
            (egui::TextStyle::Heading, FontId::proportional(16.0)),
            (egui::TextStyle::Body, body.clone()),
            (egui::TextStyle::Button, body.clone()),
            (egui::TextStyle::Small, FontId::proportional(11.0)),
            (egui::TextStyle::Monospace, FontId::monospace(12.0)),
        ]
        .into();
    });
}

/// Use the macOS system font for UI text when it is available, with the
/// bundled fonts and a CJK font as fallbacks, so Chinese file names render
/// instead of turning into boxes.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let mut changed = false;

    // The system font first in line, so Latin text uses it and the bundled
    // font only covers what it is missing.
    for (path, name, family) in [
        (
            "/System/Library/Fonts/SFNS.ttf",
            "sf",
            FontFamily::Proportional,
        ),
        (
            "/System/Library/Fonts/SFNSMono.ttf",
            "sf-mono",
            FontFamily::Monospace,
        ),
    ] {
        if let Some(bytes) = load_font(path) {
            fonts
                .font_data
                .insert(name.to_owned(), Arc::new(FontData::from_owned(bytes)));
            fonts
                .families
                .entry(family)
                .or_default()
                .insert(0, name.to_owned());
            changed = true;
        }
    }

    // CJK fallback, last in line.
    const CJK_CANDIDATES: &[&str] = &[
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        "/System/Library/Fonts/Supplemental/NotoSansCJK-Regular.ttc",
        "/Library/Fonts/Arial Unicode.ttf",
        "/usr/share/fonts/truetype/arphic/uming.ttc",
    ];
    for path in CJK_CANDIDATES {
        if let Some(bytes) = load_font(path) {
            fonts
                .font_data
                .insert("cjk".to_owned(), Arc::new(FontData::from_owned(bytes)));
            for family in [FontFamily::Proportional, FontFamily::Monospace] {
                fonts
                    .families
                    .entry(family)
                    .or_default()
                    .push("cjk".to_owned());
            }
            changed = true;
            break;
        }
    }

    if changed {
        ctx.set_fonts(fonts);
    }
}

fn load_font(path: &str) -> Option<Vec<u8>> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() < 1_000 {
        return None;
    }
    Some(bytes)
}

// ---------------------------------------------------------------------------
// Toolbar controls
// ---------------------------------------------------------------------------

/// A toolbar button that looks like the rest of the native chrome: flat and
/// borderless at rest, with a subtle rounded background that appears on hover
/// and press, all in colours taken from the current light/dark visuals.
pub fn tool_button(ui: &mut Ui, glyph: Icon, title: &str, enabled: bool) -> Response {
    tool_button_toggle(ui, glyph, title, enabled, false)
}

/// [`tool_button`] for toggle controls: while `active` the button keeps a
/// filled, selection-tinted background, so the user can see that what it
/// toggles (e.g. the inspector pane) is currently shown.
pub fn tool_button_toggle(
    ui: &mut Ui,
    glyph: Icon,
    title: &str,
    enabled: bool,
    active: bool,
) -> Response {
    let size = vec2(30.0, 26.0);
    let (rect, response) = ui.allocate_exact_size(
        size,
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    note_rect(ui, title, rect);

    // The icon is drawn, not a text widget, so name it for the accessibility
    // tree the way a native toolbar button would be.
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_role(egui::accesskit::Role::Button);
        node.set_label(title);
    });

    // A keyboard user reaches these buttons with Tab or the arrow keys; mark
    // the one that has focus. Arrows are deliberately left un-locked so they
    // step between the neighbouring buttons, as a native toolbar does.
    if response.has_focus() {
        focus_ring(ui, rect, 4.0);
    }

    if ui.is_rect_visible(rect) {
        let visuals = ui.visuals();
        let p = ui.painter();
        let pressed = enabled && response.is_pointer_button_down_on();
        let hovered = enabled && response.hovered();
        let (fill, radius, fg) = if pressed {
            (
                visuals.widgets.active.weak_bg_fill,
                visuals.widgets.active.corner_radius.nw as f32,
                visuals.widgets.active.fg_stroke.color,
            )
        } else if hovered {
            (
                visuals.widgets.hovered.weak_bg_fill,
                visuals.widgets.hovered.corner_radius.nw as f32,
                visuals.widgets.hovered.fg_stroke.color,
            )
        } else if active {
            // The persistent "on" look of a toggle: the same selection tint a
            // selected thumbnail gets, with the ordinary text colour on top.
            (
                visuals.selection.bg_fill,
                visuals.widgets.inactive.corner_radius.nw as f32,
                visuals.text_color(),
            )
        } else {
            (
                Color32::TRANSPARENT,
                0.0,
                if enabled {
                    visuals.text_color()
                } else {
                    visuals.text_color().linear_multiply(visuals.disabled_alpha)
                },
            )
        };
        if fill != Color32::TRANSPARENT {
            p.rect(rect, radius, fill, Stroke::NONE, StrokeKind::Middle);
        }
        draw_icon(p, rect.center(), 16.0, glyph, fg);
    }
    response.on_hover_text(title)
}

/// The visible keyboard focus ring: a 2px accent-blue outline just outside
/// `rect`, in the system accent colour, so it stays readable in light and
/// dark mode alike. Drawn for whatever control the keyboard is on.
pub fn focus_ring(ui: &Ui, rect: Rect, radius: f32) {
    ui.painter().rect_stroke(
        rect.expand(2.0),
        radius + 2.0,
        Stroke::new(2.0, ui.visuals().hyperlink_color),
        egui::StrokeKind::Outside,
    );
}

/// The single subtle line between toolbar button groups.
pub fn vtool_separator(ui: &mut Ui) {
    let height = ui.available_height().min(20.0);
    let (rect, _) = ui.allocate_exact_size(vec2(9.0, height), Sense::hover());
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        let x = rect.center().x;
        p.line_segment(
            [pos2(x, rect.top() + 3.0), pos2(x, rect.bottom() - 3.0)],
            ui.visuals().widgets.noninteractive.bg_stroke,
        );
    }
}

/// One segment of the status bar: quiet, adaptive text with no sunken frame.
pub fn status_field(ui: &mut Ui, text: &str, min_width: f32) {
    let (rect, _) = ui.allocate_at_least(vec2(min_width, 18.0), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let color = ui.visuals().weak_text_color();
    let format = egui::text::TextFormat {
        font_id: FontId::proportional(11.0),
        color,
        ..Default::default()
    };
    let mut job = egui::text::LayoutJob::single_section(text.to_owned(), format);
    job.wrap = egui::text::TextWrapping::truncate_at_width(rect.width() - 4.0);
    job.halign = Align::LEFT;
    let galley = ui.painter().layout_job(job);
    ui.painter().galley(
        pos2(rect.left() + 2.0, rect.center().y - galley.size().y / 2.0),
        galley,
        color,
    );
}

/// A dialog section header: plain bold text, no coloured bar.
pub fn section_label(ui: &mut Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(egui::RichText::new(text).strong().size(13.0));
}

// ---------------------------------------------------------------------------
// Monochrome line icons
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
pub enum Icon {
    Folder,
    FolderUp,
    Refresh,
    ThumbSmall,
    ThumbMedium,
    ThumbLarge,
    ZoomIn,
    ZoomOut,
    Fit,
    Actual,
    RotateLeft,
    RotateRight,
    FlipH,
    FlipV,
    First,
    Prev,
    Next,
    Last,
    Slideshow,
    Stop,
    Fullscreen,
    Delete,
    Info,
    Home,
    Image,
    Browser,
}

/// Draws one icon, entirely in a single colour chosen by the caller (usually
/// a text colour from the current visuals), so it adapts to light/dark mode.
pub fn draw_icon(p: &egui::Painter, center: Pos2, size: f32, glyph: Icon, color: Color32) {
    let s = size;
    let bounds = Rect::from_center_size(center, vec2(s, s));
    let w = (s * 0.10).clamp(1.0, 1.8);
    let stroke = Stroke::new(w, color);

    // A closed outline path (no fill): the modern replacement for the old
    // hard-edged Win95 outlines.
    let polygon = |points: Vec<Pos2>| {
        p.add(Shape::Path(egui::epaint::PathShape::closed_line(
            points, stroke,
        )));
    };

    let triangle = |points: [Pos2; 3]| {
        p.add(Shape::convex_polygon(
            points.to_vec(),
            color,
            Stroke::new(0.0, color),
        ));
    };

    // Four L-shaped corner brackets inside an inset of the bounds.
    let corners = |inset: f32| {
        let r = bounds.shrink2(vec2(s * inset, s * inset));
        let arm = r.width() * 0.30;
        for (corner, dx, dy) in [
            (r.min, 1.0, 1.0),
            (pos2(r.right(), r.top()), -1.0, 1.0),
            (pos2(r.left(), r.bottom()), 1.0, -1.0),
            (r.max, -1.0, -1.0),
        ] {
            p.line_segment([corner, corner + vec2(arm * dx, 0.0)], stroke);
            p.line_segment([corner, corner + vec2(0.0, arm * dy)], stroke);
        }
    };

    let folder_body = || {
        let top = bounds.top() + s * 0.28;
        let tab_top = top - s * 0.16;
        polygon(vec![
            pos2(bounds.left(), bounds.bottom()),
            pos2(bounds.left(), top),
            pos2(bounds.left(), tab_top),
            pos2(bounds.left() + s * 0.42, tab_top),
            pos2(bounds.left() + s * 0.52, top),
            pos2(bounds.right(), top),
            pos2(bounds.right(), bounds.bottom()),
        ]);
    };

    let magnifier = |inner: Option<bool>| {
        let radius = s * 0.26;
        let glass = pos2(center.x - s * 0.09, center.y - s * 0.09);
        p.circle(glass, radius, Color32::TRANSPARENT, stroke);
        p.line_segment(
            [
                glass + vec2(radius * 0.7, radius * 0.7),
                pos2(bounds.right(), bounds.bottom()),
            ],
            Stroke::new(w * 1.3, color),
        );
        if let Some(plus) = inner {
            p.line_segment(
                [
                    glass + vec2(-radius * 0.55, 0.0),
                    glass + vec2(radius * 0.55, 0.0),
                ],
                stroke,
            );
            if plus {
                p.line_segment(
                    [
                        glass + vec2(0.0, -radius * 0.55),
                        glass + vec2(0.0, radius * 0.55),
                    ],
                    stroke,
                );
            }
        }
    };

    let rotate = |clockwise: bool| {
        let radius = s * 0.32;
        let (start, end) = if clockwise {
            (-60.0f32, 240.0f32)
        } else {
            (240.0, -60.0)
        };
        let step = if clockwise { 12.0 } else { -12.0 };
        let mut points = Vec::new();
        let mut angle = start;
        while (angle - end) * step <= 0.0 {
            let r = angle.to_radians();
            points.push(pos2(
                center.x + radius * r.cos(),
                center.y + radius * r.sin(),
            ));
            angle += step;
        }
        p.add(Shape::line(points, stroke));

        let tip_angle = end.to_radians();
        let tip = pos2(
            center.x + radius * tip_angle.cos(),
            center.y + radius * tip_angle.sin(),
        );
        let turn = if clockwise {
            std::f32::consts::FRAC_PI_2
        } else {
            -std::f32::consts::FRAC_PI_2
        };
        let tangent = tip_angle + turn;
        let dir = vec2(tangent.cos(), tangent.sin());
        let perp = vec2(-dir.y, dir.x);
        triangle([
            tip + dir * s * 0.2,
            tip - perp * s * 0.15,
            tip + perp * s * 0.15,
        ]);
    };

    let arrow = |dir: i8| {
        let h = s * 0.24;
        let w = s * 0.28;
        match dir {
            -1 => triangle([
                pos2(center.x - h, center.y),
                pos2(center.x + w, center.y - s * 0.28),
                pos2(center.x + w, center.y + s * 0.28),
            ]),
            1 => triangle([
                pos2(center.x + h, center.y),
                pos2(center.x - w, center.y - s * 0.28),
                pos2(center.x - w, center.y + s * 0.28),
            ]),
            _ => {}
        }
    };

    match glyph {
        Icon::Folder => folder_body(),
        Icon::FolderUp => {
            folder_body();
            p.line_segment(
                [
                    pos2(center.x, bounds.bottom() - s * 0.06),
                    pos2(center.x, bounds.top() + s * 0.34),
                ],
                stroke,
            );
            triangle([
                pos2(center.x, bounds.top() + s * 0.24),
                pos2(center.x - s * 0.14, bounds.top() + s * 0.44),
                pos2(center.x + s * 0.14, bounds.top() + s * 0.44),
            ]);
        }
        Icon::Home => {
            polygon(vec![
                pos2(center.x, bounds.top()),
                pos2(bounds.right(), bounds.top() + s * 0.36),
                pos2(bounds.right(), bounds.bottom()),
                pos2(bounds.left(), bounds.bottom()),
                pos2(bounds.left(), bounds.top() + s * 0.36),
            ]);
            // The door.
            p.rect_stroke(
                Rect::from_min_max(
                    pos2(center.x - s * 0.12, center.y + s * 0.14),
                    pos2(center.x + s * 0.12, bounds.bottom()),
                ),
                1.0,
                stroke,
                StrokeKind::Inside,
            );
        }
        Icon::Refresh | Icon::RotateRight => rotate(true),
        Icon::RotateLeft => rotate(false),
        Icon::ThumbSmall | Icon::ThumbMedium | Icon::ThumbLarge => {
            let cells = match glyph {
                Icon::ThumbSmall => 3,
                Icon::ThumbMedium => 2,
                _ => 1,
            };
            let pad = s * 0.09;
            let cell = (s - pad * (cells as f32 + 1.0)) / cells as f32;
            for y in 0..cells {
                for x in 0..cells {
                    let min = pos2(
                        bounds.left() + pad + x as f32 * (cell + pad),
                        bounds.top() + pad + y as f32 * (cell + pad),
                    );
                    let rect = Rect::from_min_size(min, vec2(cell, cell));
                    p.rect_stroke(rect, 1.0, stroke, StrokeKind::Inside);
                }
            }
        }
        Icon::ZoomIn => magnifier(Some(true)),
        Icon::ZoomOut => magnifier(Some(false)),
        Icon::Fit | Icon::Actual => {
            corners(0.08);
            let inset = if glyph == Icon::Fit {
                s * 0.30
            } else {
                s * 0.20
            };
            p.rect_stroke(
                bounds.shrink2(vec2(inset, inset)),
                1.0,
                stroke,
                StrokeKind::Inside,
            );
        }
        Icon::Fullscreen => corners(0.08),
        Icon::First | Icon::Prev | Icon::Next | Icon::Last => {
            let dir = if glyph == Icon::First || glyph == Icon::Prev {
                -1
            } else {
                1
            };
            arrow(dir);
            if glyph == Icon::First || glyph == Icon::Last {
                let x = if dir < 0 {
                    bounds.left() + w
                } else {
                    bounds.right() - w
                };
                p.line_segment(
                    [
                        pos2(x, bounds.top() + s * 0.1),
                        pos2(x, bounds.bottom() - s * 0.1),
                    ],
                    stroke,
                );
            }
        }
        Icon::Slideshow => {
            p.rect_stroke(bounds.shrink2(vec2(w, w)), 2.0, stroke, StrokeKind::Inside);
            triangle([
                pos2(center.x - s * 0.10, center.y - s * 0.18),
                pos2(center.x - s * 0.10, center.y + s * 0.18),
                pos2(center.x + s * 0.22, center.y),
            ]);
        }
        Icon::Stop => {
            p.rect_filled(bounds.shrink2(vec2(s * 0.22, s * 0.22)), 2.0, color);
        }
        Icon::Delete => {
            let top = bounds.top() + s * 0.24;
            let body = Rect::from_min_max(
                pos2(bounds.left() + s * 0.2, top),
                pos2(bounds.right() - s * 0.2, bounds.bottom()),
            );
            p.rect_stroke(body, 2.0, stroke, StrokeKind::Inside);
            // Lid.
            p.line_segment(
                [
                    pos2(bounds.left() + s * 0.08, top),
                    pos2(bounds.right() - s * 0.08, top),
                ],
                stroke,
            );
            // Handle.
            p.line_segment(
                [
                    pos2(center.x - s * 0.1, top - s * 0.12),
                    pos2(center.x + s * 0.1, top - s * 0.12),
                ],
                stroke,
            );
            // Ribs.
            for x in [center.x - s * 0.1, center.x, center.x + s * 0.1] {
                p.line_segment(
                    [pos2(x, top + s * 0.16), pos2(x, bounds.bottom() - s * 0.12)],
                    Stroke::new(w * 0.8, color),
                );
            }
        }
        Icon::Info => {
            p.circle(center, s * 0.42, Color32::TRANSPARENT, stroke);
            p.text(
                center,
                Align2::CENTER_CENTER,
                "i",
                FontId::proportional(s * 0.5),
                color,
            );
        }
        Icon::Image => {
            p.rect_stroke(bounds, 1.5, stroke, StrokeKind::Inside);
            triangle([
                pos2(bounds.left() + s * 0.12, bounds.bottom() - s * 0.14),
                pos2(center.x, bounds.top() + s * 0.26),
                pos2(bounds.right() - s * 0.12, bounds.bottom() - s * 0.14),
            ]);
            p.circle(
                pos2(bounds.left() + s * 0.26, bounds.top() + s * 0.26),
                s * 0.10,
                Color32::TRANSPARENT,
                stroke,
            );
        }
        Icon::Browser => {
            p.rect_stroke(bounds, 1.5, stroke, StrokeKind::Inside);
            p.line_segment(
                [
                    pos2(bounds.left() + s * 0.34, bounds.top() + w),
                    pos2(bounds.left() + s * 0.34, bounds.bottom() - w),
                ],
                stroke,
            );
        }
        Icon::FlipH | Icon::FlipV => {
            let horizontal = glyph == Icon::FlipH;
            let (a, b) = if horizontal {
                (
                    [
                        pos2(bounds.left() + s * 0.24, bounds.top() + s * 0.16),
                        pos2(bounds.left() + s * 0.24, bounds.bottom() - s * 0.16),
                        pos2(center.x - s * 0.06, center.y),
                    ],
                    [
                        pos2(bounds.right() - s * 0.24, bounds.top() + s * 0.16),
                        pos2(bounds.right() - s * 0.24, bounds.bottom() - s * 0.16),
                        pos2(center.x + s * 0.06, center.y),
                    ],
                )
            } else {
                (
                    [
                        pos2(bounds.left() + s * 0.16, bounds.top() + s * 0.24),
                        pos2(bounds.right() - s * 0.16, bounds.top() + s * 0.24),
                        pos2(center.x, center.y - s * 0.06),
                    ],
                    [
                        pos2(bounds.left() + s * 0.16, bounds.bottom() - s * 0.24),
                        pos2(bounds.right() - s * 0.16, bounds.bottom() - s * 0.24),
                        pos2(center.x, center.y + s * 0.06),
                    ],
                )
            };
            triangle(a);
            triangle(b);
            // The axis the image flips around.
            if horizontal {
                p.line_segment(
                    [
                        pos2(center.x, bounds.top() - 1.0),
                        pos2(center.x, bounds.bottom() + 1.0),
                    ],
                    Stroke::new(w * 0.8, color),
                );
            } else {
                p.line_segment(
                    [
                        pos2(bounds.left() - 1.0, center.y),
                        pos2(bounds.right() + 1.0, center.y),
                    ],
                    Stroke::new(w * 0.8, color),
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Test-only registry of where labelled controls were drawn, so headless tests
// can click a control by name instead of guessing coordinates.
// ---------------------------------------------------------------------------

#[cfg(test)]
type RectMap = std::collections::HashMap<String, egui::Rect>;

#[cfg(test)]
fn rect_map_id() -> egui::Id {
    egui::Id::new("lumiere_test_rects")
}

#[cfg(test)]
pub fn note_rect(ui: &Ui, label: &str, rect: egui::Rect) {
    let label = label.to_owned();
    ui.ctx().data_mut(|d| {
        d.get_temp_mut_or_default::<RectMap>(rect_map_id())
            .insert(label, rect);
    });
}

#[cfg(not(test))]
pub fn note_rect(_ui: &Ui, _label: &str, _rect: egui::Rect) {}

#[cfg(test)]
pub fn rect_of(ctx: &egui::Context, label: &str) -> Option<egui::Rect> {
    ctx.data_mut(|d| d.get_temp::<RectMap>(rect_map_id()))
        .and_then(|map| map.get(label).copied())
}

#[cfg(test)]
pub fn clear_rects(ctx: &egui::Context) {
    ctx.data_mut(|d| d.remove_temp::<RectMap>(rect_map_id()));
}
