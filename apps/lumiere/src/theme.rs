//! The platform-adaptive visual layer: light/dark chrome that follows the OS,
//! installed system fonts, compact native-inspired controls, and monochrome line icons.
//!
//! Everything that needs a colour asks the current [`egui::Visuals`] instead
//! of a fixed palette, so switching the system appearance repaints the whole
//! UI without a restart.

use egui::{
    pos2, vec2, Align, Align2, Color32, FontData, FontFamily, FontId, Margin, Pos2, Rect, Response,
    Sense, Shape, Stroke, StrokeKind, Ui,
};
use std::sync::Arc;

/// The warning colour for a picture that cannot be decoded. It is a state
/// colour, not chrome, so (unlike everything else in the theme) it does not
/// ask the visuals: the same red reads as an error in light and dark.
pub const DECODE_FAILED: Color32 = Color32::from_rgb(0xC4, 0x3B, 0x30);

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

/// Prefer available system fonts for UI text, with bundled fonts and a CJK
/// fallback so Chinese file names render
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

/// [`tool_button`] whose hover tooltip also shows the keyboard shortcut, e.g.
/// "Move to Trash (⌫)". The button keeps `title` as its name.
pub fn tool_button_with_shortcut(
    ui: &mut Ui,
    glyph: Icon,
    title: &str,
    shortcut: &str,
    enabled: bool,
) -> Response {
    let response = tool_button_toggle(ui, glyph, title, enabled, false);
    response.on_hover_text(format!("{title} ({shortcut})"))
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
        draw_icon(ui, rect.center(), 16.0, glyph, fg);
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
        Stroke::new(2.0_f32, ui.visuals().hyperlink_color),
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

/// A compact macOS-style switch for settings rows. egui's stock checkbox is
/// intentionally minimal, which leaves only a tick at this scale; the track
/// and round thumb make the state legible in both appearances.
pub fn settings_switch(ui: &mut Ui, value: &mut bool) -> Response {
    let (rect, mut response) = ui.allocate_exact_size(vec2(32.0, 20.0), Sense::click());
    if response.clicked() {
        *value = !*value;
        response.mark_changed();
    }
    if ui.is_rect_visible(rect) {
        let visuals = ui.visuals();
        let track = if *value {
            visuals.selection.bg_fill
        } else {
            visuals.widgets.inactive.bg_fill
        };
        let thumb = visuals.widgets.inactive.fg_stroke.color;
        let p = ui.painter();
        p.rect_filled(rect, 10.0, track);
        p.rect_stroke(
            rect,
            10.0,
            visuals.widgets.inactive.bg_stroke,
            StrokeKind::Inside,
        );
        let x = if *value {
            rect.right() - 10.0
        } else {
            rect.left() + 10.0
        };
        p.circle_filled(pos2(x, rect.center().y), 7.0, thumb);
    }
    response
}

/// A high-contrast slider with a visible filled track and circular thumb.
/// It uses the system selection tint for the active segment, rather than
/// relying on egui's low-contrast default slider rail.
pub fn settings_slider(
    ui: &mut Ui,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    step: f32,
) -> Response {
    let (rect, mut response) = ui.allocate_exact_size(vec2(148.0, 20.0), Sense::click_and_drag());
    if let Some(pointer) = response
        .interact_pointer_pos()
        .filter(|_| response.dragged() || response.clicked())
    {
        let start = *range.start();
        let end = *range.end();
        let t = ((pointer.x - (rect.left() + 7.0)) / (rect.width() - 14.0)).clamp(0.0, 1.0);
        let next = (start + (end - start) * t) / step;
        let next = (next.round() * step).clamp(start, end);
        if (*value - next).abs() > f32::EPSILON {
            *value = next;
            response.mark_changed();
        }
    }
    if ui.is_rect_visible(rect) {
        let start = *range.start();
        let end = *range.end();
        let t = ((*value - start) / (end - start)).clamp(0.0, 1.0);
        let rail = Rect::from_center_size(rect.center(), vec2(rect.width() - 14.0, 4.0));
        let filled = Rect::from_min_max(
            rail.min,
            pos2(rail.left() + rail.width() * t, rail.bottom()),
        );
        let visuals = ui.visuals();
        let p = ui.painter();
        p.rect_filled(rail, 2.0, visuals.widgets.inactive.bg_fill);
        p.rect_stroke(
            rail,
            2.0,
            visuals.widgets.inactive.bg_stroke,
            StrokeKind::Inside,
        );
        p.rect_filled(filled, 2.0, visuals.selection.bg_fill);
        let x = rail.left() + rail.width() * t;
        p.circle_filled(
            pos2(x, rect.center().y),
            7.0,
            visuals.widgets.inactive.fg_stroke.color,
        );
        p.circle_stroke(
            pos2(x, rect.center().y),
            7.0,
            visuals.widgets.inactive.bg_stroke,
        );
    }
    response
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

// ---------------------------------------------------------------------------
// Monochrome line icons
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
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
    First,
    Prev,
    Next,
    Last,
    Slideshow,
    Stop,
    Delete,
    Info,
    Home,
    Image,
    Browser,
    /// The sidebar toggle: a window with its left band, drawn like the
    /// browser glyph.
    Sidebar,
}

/// Draws one icon, entirely in a single colour chosen by the caller (usually
/// a text colour from the current visuals), so it adapts to light/dark mode.
#[allow(dead_code)]
fn draw_icon_legacy(p: &egui::Painter, center: Pos2, size: f32, glyph: Icon, color: Color32) {
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
            Stroke::new(0.0_f32, color),
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
        Icon::Browser | Icon::Sidebar => {
            p.rect_stroke(bounds, 1.5, stroke, StrokeKind::Inside);
            p.line_segment(
                [
                    pos2(bounds.left() + s * 0.34, bounds.top() + w),
                    pos2(bounds.left() + s * 0.34, bounds.bottom() - w),
                ],
                stroke,
            );
        }
    }
}

/// Paint a [Bootstrap Icon](https://icons.getbootstrap.com/) as a white SVG
/// texture, then tint it with the colour chosen by the current visuals. The
/// SVG loader caches by URI, so this does not parse or upload it every frame.
///
/// The paths below are the small subset used by Lumiere, copied from
/// Bootstrap Icons (MIT licence). Keeping SVG rather than an icon font means
/// no private-use Unicode mapping and no font fallback surprises.
pub fn draw_icon(ui: &Ui, center: Pos2, size: f32, glyph: Icon, color: Color32) {
    let (name, paths) = bootstrap_paths(glyph);
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="white">{paths}</svg>"#
    );
    let rect = Rect::from_center_size(center, vec2(size, size));
    egui::Image::from_bytes(
        format!("bytes://lumiere/bootstrap-icons/{name}.svg"),
        svg.into_bytes(),
    )
    .tint(color)
    .paint_at(ui, rect);
}

fn bootstrap_paths(glyph: Icon) -> (&'static str, &'static str) {
    match glyph {
        Icon::Folder => (
            "folder",
            r#"<path d="M.54 3.87.5 3a2 2 0 0 1 2-2h3.672a2 2 0 0 1 1.414.586l.828.828A2 2 0 0 0 9.828 3h3.982a2 2 0 0 1 1.992 2.181l-.637 7A2 2 0 0 1 13.174 14H2.826a2 2 0 0 1-1.991-1.819l-.637-7a2 2 0 0 1 .342-1.31zM2.19 4a1 1 0 0 0-.996 1.09l.637 7a1 1 0 0 0 .995.91h10.348a1 1 0 0 0 .995-.91l.637-7A1 1 0 0 0 13.81 4zm4.69-1.707A1 1 0 0 0 6.172 2H2.5a1 1 0 0 0-1 .981l.006.139q.323-.119.684-.12h5.396z"/>"#,
        ),
        Icon::FolderUp => (
            "folder-symlink",
            r#"<path d="m11.798 8.271-3.182 1.97c-.27.166-.616-.036-.616-.372V9.1s-2.571-.3-4 2.4c.571-4.8 3.143-4.8 4-4.8v-.769c0-.336.346-.538.616-.371l3.182 1.969c.27.166.27.576 0 .742"/><path d="m.5 3 .04.87a2 2 0 0 0-.342 1.311l.637 7A2 2 0 0 0 2.826 14h10.348a2 2 0 0 0 1.991-1.819l.637-7A2 2 0 0 0 13.81 3H9.828a2 2 0 0 1-1.414-.586l-.828-.828A2 2 0 0 0 6.172 1H2.5a2 2 0 0 0-2 2m.694 2.09A1 1 0 0 1 2.19 4h11.62a1 1 0 0 1 .996 1.09l-.636 7a1 1 0 0 1-.996.91H2.826a1 1 0 0 1-.995-.91zM6.172 2a1 1 0 0 1 .707.293L7.586 3H2.19q-.362.002-.683.12L1.5 2.98a1 1 0 0 1 1-.98z"/>"#,
        ),
        Icon::Home => (
            "house",
            r#"<path d="M8.707 1.5a1 1 0 0 0-1.414 0L.646 8.146a.5.5 0 0 0 .708.708L2 8.207V13.5A1.5 1.5 0 0 0 3.5 15h9a1.5 1.5 0 0 0 1.5-1.5V8.207l.646.647a.5.5 0 0 0 .708-.708L13 5.793V2.5a.5.5 0 0 0-.5-.5h-1a.5.5 0 0 0-.5.5v1.293zM13 7.207V13.5a.5.5 0 0 1-.5.5h-9a.5.5 0 0 1-.5-.5V7.207l5-5z"/>"#,
        ),
        Icon::Refresh | Icon::RotateRight => (
            "arrow-clockwise",
            r#"<path fill-rule="evenodd" d="M8 3a5 5 0 1 0 4.546 2.914.5.5 0 0 1 .908-.417A6 6 0 1 1 8 2z"/><path d="M8 4.466V.534a.25.25 0 0 1 .41-.192l2.36 1.966c.12.1.12.284 0 .384L8.41 4.658A.25.25 0 0 1 8 4.466"/>"#,
        ),
        Icon::RotateLeft => (
            "arrow-counterclockwise",
            r#"<path fill-rule="evenodd" d="M8 3a5 5 0 1 1-4.546 2.914.5.5 0 0 0-.908-.417A6 6 0 1 0 8 2z"/><path d="M8 4.466V.534a.25.25 0 0 0-.41-.192L5.23 2.308a.25.25 0 0 0 0 .384l2.36 1.966A.25.25 0 0 0 8 4.466"/>"#,
        ),
        Icon::ThumbSmall => (
            "grid-3x3-gap",
            r#"<path d="M4 2v2H2V2zm1 12v-2a1 1 0 0 0-1-1H2a1 1 0 0 0-1 1v2a1 1 0 0 0 1 1h2a1 1 0 0 0 1-1m0-5V7a1 1 0 0 0-1-1H2a1 1 0 0 0-1 1v2a1 1 0 0 0 1 1h2a1 1 0 0 0 1-1m0-5V2a1 1 0 0 0-1-1H2a1 1 0 0 0-1 1v2a1 1 0 0 0 1 1h2a1 1 0 0 0 1-1m5 10v-2a1 1 0 0 0-1-1H7a1 1 0 0 0-1 1v2a1 1 0 0 0 1 1h2a1 1 0 0 0 1-1m0-5V7a1 1 0 0 0-1-1H7a1 1 0 0 0-1 1v2a1 1 0 0 0 1 1h2a1 1 0 0 0 1-1m0-5V2a1 1 0 0 0-1-1H7a1 1 0 0 0-1 1v2a1 1 0 0 0 1 1h2a1 1 0 0 0 1-1M9 2v2H7V2zm5 0v2h-2V2zM4 7v2H2V7zm5 0v2H7V7zm5 0h-2v2h2zM4 12v2H2v-2zm5 0v2H7v-2zm5 0v2h-2v-2zM12 1a1 1 0 0 0-1 1v2a1 1 0 0 0 1 1h2a1 1 0 0 0 1-1V2a1 1 0 0 0-1-1zm-1 6a1 1 0 0 1 1-1h2a1 1 0 0 1 1 1v2a1 1 0 0 1-1 1h-2a1 1 0 0 1-1-1zm1 4a1 1 0 0 0-1 1v2a1 1 0 0 0 1 1h2a1 1 0 0 0 1-1v-2a1 1 0 0 0-1-1z"/>"#,
        ),
        Icon::ThumbMedium => (
            "grid",
            r#"<path d="M1 2.5A1.5 1.5 0 0 1 2.5 1h3A1.5 1.5 0 0 1 7 2.5v3A1.5 1.5 0 0 1 5.5 7h-3A1.5 1.5 0 0 1 1 5.5zM2.5 2a.5.5 0 0 0-.5.5v3a.5.5 0 0 0 .5.5h3a.5.5 0 0 0 .5-.5v-3a.5.5 0 0 0-.5-.5zm6.5.5A1.5 1.5 0 0 1 10.5 1h3A1.5 1.5 0 0 1 15 2.5v3A1.5 1.5 0 0 1 13.5 7h-3A1.5 1.5 0 0 1 9 5.5zm1.5-.5a.5.5 0 0 0-.5.5v3a.5.5 0 0 0 .5.5h3a.5.5 0 0 0 .5-.5v-3a.5.5 0 0 0-.5-.5zM1 10.5A1.5 1.5 0 0 1 2.5 9h3A1.5 1.5 0 0 1 7 10.5v3A1.5 1.5 0 0 1 5.5 15h-3A1.5 1.5 0 0 1 1 13.5zm1.5-.5a.5.5 0 0 0-.5.5v3a.5.5 0 0 0 .5.5h3a.5.5 0 0 0 .5-.5v-3a.5.5 0 0 0-.5-.5zm6.5.5A1.5 1.5 0 0 1 10.5 9h3A1.5 1.5 0 0 1 15 10.5v3A1.5 1.5 0 0 1 13.5 15h-3A1.5 1.5 0 0 1 9 13.5zm1.5-.5a.5.5 0 0 0-.5.5v3a.5.5 0 0 0 .5.5h3a.5.5 0 0 0 .5-.5v-3a.5.5 0 0 0-.5-.5z"/>"#,
        ),
        Icon::ThumbLarge => (
            "square",
            r#"<path d="M14 1a1 1 0 0 1 1 1v12a1 1 0 0 1-1 1H2a1 1 0 0 1-1-1V2a1 1 0 0 1 1-1zM2 0a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V2a2 2 0 0 0-2-2z"/>"#,
        ),
        Icon::ZoomIn => (
            "zoom-in",
            r#"<path fill-rule="evenodd" d="M6.5 12a5.5 5.5 0 1 0 0-11 5.5 5.5 0 0 0 0 11M13 6.5a6.5 6.5 0 1 1-13 0 6.5 6.5 0 0 1 13 0"/><path d="M10.344 11.742q.044.06.098.115l3.85 3.85a1 1 0 0 0 1.415-1.414l-3.85-3.85a1 1 0 0 0-.115-.1 6.5 6.5 0 0 1-1.398 1.4z"/><path fill-rule="evenodd" d="M6.5 3a.5.5 0 0 1 .5.5V6h2.5a.5.5 0 0 1 0 1H7v2.5a.5.5 0 0 1-1 0V7H3.5a.5.5 0 0 1 0-1H6V3.5a.5.5 0 0 1 .5-.5"/>"#,
        ),
        Icon::ZoomOut => (
            "zoom-out",
            r#"<path fill-rule="evenodd" d="M6.5 12a5.5 5.5 0 1 0 0-11 5.5 5.5 0 0 0 0 11M13 6.5a6.5 6.5 0 1 1-13 0 6.5 6.5 0 0 1 13 0"/><path d="M10.344 11.742q.044.06.098.115l3.85 3.85a1 1 0 0 0 1.415-1.414l-3.85-3.85a1 1 0 0 0-.115-.1 6.5 6.5 0 0 1-1.398 1.4z"/><path fill-rule="evenodd" d="M3 6.5a.5.5 0 0 1 .5-.5h6a.5.5 0 0 1 0 1h-6a.5.5 0 0 1-.5-.5"/>"#,
        ),
        Icon::Fit => (
            "arrows-angle-expand",
            r#"<path fill-rule="evenodd" d="M5.828 10.172a.5.5 0 0 0-.707 0l-4.096 4.096V11.5a.5.5 0 0 0-1 0v3.975a.5.5 0 0 0 .5.5H4.5a.5.5 0 0 0 0-1H1.732l4.096-4.096a.5.5 0 0 0 0-.707m4.344-4.344a.5.5 0 0 0 .707 0l4.096-4.096V4.5a.5.5 0 1 0 1 0V.525a.5.5 0 0 0-.5-.5H11.5a.5.5 0 0 0 0 1h2.768l-4.096 4.096a.5.5 0 0 0 0 .707"/>"#,
        ),
        Icon::Actual => (
            "aspect-ratio",
            r#"<path d="M0 3.5A1.5 1.5 0 0 1 1.5 2h13A1.5 1.5 0 0 1 16 3.5v9a1.5 1.5 0 0 1-1.5 1.5h-13A1.5 1.5 0 0 1 0 12.5zM1.5 3a.5.5 0 0 0-.5.5v9a.5.5 0 0 0 .5.5h13a.5.5 0 0 0 .5-.5v-9a.5.5 0 0 0-.5-.5z"/><path d="M2 4.5a.5.5 0 0 1 .5-.5h3a.5.5 0 0 1 0 1H3v2.5a.5.5 0 0 1-1 0zm12 7a.5.5 0 0 1-.5.5h-3a.5.5 0 0 1 0-1H13V8.5a.5.5 0 0 1 1 0z"/>"#,
        ),
        Icon::First => (
            "skip-backward-fill",
            r#"<path d="M.5 3.5A.5.5 0 0 0 0 4v8a.5.5 0 0 0 1 0V8.753l6.267 3.636c.54.313 1.233-.066 1.233-.697v-2.94l6.267 3.636c.54.314 1.233-.065 1.233-.696V4.308c0-.63-.693-1.01-1.233-.696L8.5 7.248v-2.94c0-.63-.692-1.01-1.233-.696L1 7.248V4a.5.5 0 0 0-.5-.5"/>"#,
        ),
        Icon::Prev => (
            "caret-left-fill",
            r#"<path d="m3.86 8.753 5.482 4.796c.646.566 1.658.106 1.658-.753V3.204a1 1 0 0 0-1.659-.753l-5.48 4.796a1 1 0 0 0 0 1.506z"/>"#,
        ),
        Icon::Next => (
            "caret-right-fill",
            r#"<path d="m12.14 8.753-5.482 4.796c-.646.566-1.658.106-1.658-.753V3.204a1 1 0 0 1 1.659-.753l5.48 4.796a1 1 0 0 1 0 1.506z"/>"#,
        ),
        Icon::Last => (
            "skip-forward-fill",
            r#"<path d="M15.5 3.5a.5.5 0 0 1 .5.5v8a.5.5 0 0 1-1 0V8.753l-6.267 3.636c-.54.313-1.233-.066-1.233-.697v-2.94l-6.267 3.636C.693 12.703 0 12.324 0 11.693V4.308c0-.63.693-1.01 1.233-.696L7.5 7.248v-2.94c0-.63.693-1.01 1.233-.696L15 7.248V4a.5.5 0 0 1 .5-.5"/>"#,
        ),
        Icon::Slideshow => (
            "play-btn",
            r#"<path d="M6.79 5.093A.5.5 0 0 0 6 5.5v5a.5.5 0 0 0 .79.407l3.5-2.5a.5.5 0 0 0 0-.814z"/><path d="M0 4a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H2a2 2 0 0 1-2-2zm15 0a1 1 0 0 0-1-1H2a1 1 0 0 0-1 1v8a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1z"/>"#,
        ),
        Icon::Stop => (
            "stop-fill",
            r#"<path d="M5 3.5h6A1.5 1.5 0 0 1 12.5 5v6a1.5 1.5 0 0 1-1.5 1.5H5A1.5 1.5 0 0 1 3.5 11V5A1.5 1.5 0 0 1 5 3.5"/>"#,
        ),
        Icon::Delete => (
            "trash",
            r#"<path d="M5.5 5.5A.5.5 0 0 1 6 6v6a.5.5 0 0 1-1 0V6a.5.5 0 0 1 .5-.5m2.5 0a.5.5 0 0 1 .5.5v6a.5.5 0 0 1-1 0V6a.5.5 0 0 1 .5-.5m3 .5a.5.5 0 0 0-1 0v6a.5.5 0 0 0-1 0z"/><path d="M14.5 3a1 1 0 0 1-1 1H13v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V4h-.5a1 1 0 0 1-1-1V2a1 1 0 0 1 1-1H6a1 1 0 0 1 1-1h2a1 1 0 0 1 1 1h3.5a1 1 0 0 1 1 1zM4.118 4 4 4.059V13a1 1 0 0 0 1 1h6a1 1 0 0 0 1-1V4.059L11.882 4zM2.5 3h11V2h-11z"/>"#,
        ),
        Icon::Info => (
            "info-circle",
            r#"<path d="M8 15A7 7 0 1 1 8 1a7 7 0 0 1 0 14m0 1A8 8 0 1 0 8 0a8 8 0 0 0 0 16"/><path d="m8.93 6.588-2.29.287-.082.38.45.083c.294.07.352.176.288.469l-.738 3.468c-.194.897.105 1.319.808 1.319.545 0 1.178-.252 1.465-.598l.088-.416c-.2.176-.492.246-.686.246-.275 0-.375-.193-.304-.533zM9 4.5a1 1 0 1 1-2 0 1 1 0 0 1 2 0"/>"#,
        ),
        Icon::Image => (
            "image",
            r#"<path d="M6.002 5.5a1.5 1.5 0 1 1-3 0 1.5 1.5 0 0 1 3 0"/><path d="M2.002 1a2 2 0 0 0-2 2v10a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V3a2 2 0 0 0-2-2zm12 1a1 1 0 0 1 1 1v6.5l-3.777-1.947a.5.5 0 0 0-.577.093l-3.71 3.71-2.66-1.772a.5.5 0 0 0-.63.062L1.002 12V3a1 1 0 0 1 1-1z"/>"#,
        ),
        Icon::Browser | Icon::Sidebar => (
            "layout-sidebar",
            r#"<path d="M0 3a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H2a2 2 0 0 1-2-2zm5-1v12h9a1 1 0 0 0 1-1V3a1 1 0 0 0-1-1zM4 2H2a1 1 0 0 0-1 1v10a1 1 0 0 0 1 1h2z"/>"#,
        ),
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
