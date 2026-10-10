//! The classic Windows-95 era look: flat grey panels, hard 3D bevels,
//! square selection boxes and hand drawn toolbar glyphs.

use egui::{
    pos2, vec2, Align2, Color32, FontId, Margin, Pos2, Rect, Response, Sense, Shape, Stroke,
    StrokeKind, Ui,
};

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

pub const FACE: Color32 = rgb(0xC0, 0xC0, 0xC0);
pub const HIGHLIGHT: Color32 = rgb(0xFF, 0xFF, 0xFF);
pub const LIGHT: Color32 = rgb(0xDF, 0xDF, 0xDF);
pub const SHADOW: Color32 = rgb(0x80, 0x80, 0x80);
pub const DARK: Color32 = rgb(0x00, 0x00, 0x00);
pub const NAVY: Color32 = rgb(0x00, 0x00, 0x80);
pub const TEXT: Color32 = rgb(0x00, 0x00, 0x00);
pub const CANVAS: Color32 = rgb(0x80, 0x80, 0x80);
pub const GOLD: Color32 = rgb(0xF7, 0xE7, 0xA0);
pub const BLUE: Color32 = rgb(0x80, 0x80, 0xFF);
pub const RED: Color32 = rgb(0xC0, 0x00, 0x00);
pub const GREEN: Color32 = rgb(0x00, 0x80, 0x00);
pub const BROWN: Color32 = rgb(0xC0, 0x80, 0x40);
pub const TAN: Color32 = rgb(0xE8, 0xD0, 0xA0);
pub const SUN: Color32 = rgb(0xE0, 0xC0, 0x00);
pub const LEAF: Color32 = rgb(0x40, 0x80, 0x40);

pub const BEVEL: f32 = 1.0;

fn hline(p: &egui::Painter, y: f32, from: f32, to: f32, color: Color32) {
    p.line_segment([pos2(from, y), pos2(to, y)], Stroke::new(BEVEL, color));
}

fn vline(p: &egui::Painter, x: f32, from: f32, to: f32, color: Color32) {
    p.line_segment([pos2(x, from), pos2(x, to)], Stroke::new(BEVEL, color));
}

/// Classic raised edge (two pixels wide).
pub fn bevel_raised(p: &egui::Painter, rect: Rect) {
    let r = rect;
    hline(p, r.top(), r.left(), r.right() - 1.0, HIGHLIGHT);
    vline(p, r.left(), r.top(), r.bottom() - 1.0, HIGHLIGHT);
    hline(p, r.top() + 1.0, r.left() + 1.0, r.right() - 1.0, LIGHT);
    vline(p, r.left() + 1.0, r.top() + 1.0, r.bottom() - 1.0, LIGHT);
    hline(p, r.bottom() - 1.0, r.left(), r.right() - 1.0, DARK);
    vline(p, r.right() - 1.0, r.top(), r.bottom() - 1.0, DARK);
    hline(p, r.bottom() - 2.0, r.left() + 1.0, r.right() - 1.0, SHADOW);
    vline(p, r.right() - 2.0, r.top() + 1.0, r.bottom() - 1.0, SHADOW);
}

/// Classic sunken edge (two pixels wide).
pub fn bevel_sunken(p: &egui::Painter, rect: Rect) {
    let r = rect;
    hline(p, r.top(), r.left(), r.right() - 1.0, SHADOW);
    vline(p, r.left(), r.top(), r.bottom() - 1.0, SHADOW);
    hline(p, r.top() + 1.0, r.left() + 1.0, r.right() - 1.0, DARK);
    vline(p, r.left() + 1.0, r.top() + 1.0, r.bottom() - 1.0, DARK);
    hline(p, r.bottom() - 1.0, r.left(), r.right() - 1.0, HIGHLIGHT);
    vline(p, r.right() - 1.0, r.top(), r.bottom() - 1.0, HIGHLIGHT);
    hline(p, r.bottom() - 2.0, r.left() + 1.0, r.right() - 1.0, LIGHT);
    vline(p, r.right() - 2.0, r.top() + 1.0, r.bottom() - 1.0, LIGHT);
}

/// A flat toolbar button that only shows its bevel while hovered / pressed.
pub fn tool_button(ui: &mut Ui, glyph: Icon, title: &str, enabled: bool) -> Response {
    let size = vec2(30.0, 28.0);
    let (rect, response) = ui.allocate_exact_size(
        size,
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    note_rect(ui, title, rect);
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        let pressed = enabled && response.is_pointer_button_down_on();
        let hovered = enabled && response.hovered();
        if pressed {
            p.rect_filled(rect, 0, FACE);
            bevel_sunken(p, rect);
        } else if hovered {
            p.rect_filled(rect, 0, LIGHT);
            bevel_raised(p, rect);
        }
        let center = rect.center();
        let color = if enabled { TEXT } else { SHADOW };
        draw_icon(p, center, 18.0, glyph, color);
    }
    response.on_hover_text(title)
}

pub fn vtool_separator(ui: &mut Ui) {
    let height = ui.available_height().min(22.0);
    let (rect, _) = ui.allocate_exact_size(vec2(8.0, height), Sense::hover());
    let p = ui.painter();
    let x = rect.center().x;
    vline(p, x - 1.0, rect.top() + 2.0, rect.bottom() - 2.0, SHADOW);
    vline(p, x, rect.top() + 2.0, rect.bottom() - 2.0, HIGHLIGHT);
}

pub fn status_field(ui: &mut Ui, text: &str, min_width: f32) {
    let height = 18.0;
    let (rect, _) = ui.allocate_at_least(vec2(min_width, height), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 0, FACE);
    bevel_sunken(p, rect);
    p.text(
        pos2(rect.left() + 4.0, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        FontId::proportional(12.0),
        TEXT,
    );
}

pub fn section_label(ui: &mut Ui, text: &str) {
    let (rect, _) = ui.allocate_at_least(vec2(ui.available_width(), 18.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 0, NAVY);
    p.text(
        pos2(rect.left() + 4.0, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        FontId::proportional(12.0),
        Color32::WHITE,
    );
}

// ---------------------------------------------------------------------------
// Hand drawn glyphs
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

fn triangle(p: &egui::Painter, points: [Pos2; 3], fill: Color32) {
    p.add(Shape::convex_polygon(
        points.to_vec(),
        fill,
        Stroke::new(0.0, fill),
    ));
}

pub fn draw_icon(p: &egui::Painter, center: Pos2, size: f32, glyph: Icon, color: Color32) {
    let s = size;
    let l = center - vec2(s * 0.5, s * 0.5);
    let r = center + vec2(s * 0.5, s * 0.5);
    let box_rect = Rect::from_min_max(l, r);

    let folder = |p: &egui::Painter| {
        let body = Rect::from_min_max(
            pos2(box_rect.left(), box_rect.top() + s * 0.22),
            pos2(box_rect.right(), box_rect.bottom()),
        );
        p.rect_filled(body, 0, GOLD);
        p.rect_stroke(body, 0, Stroke::new(1.0, DARK), StrokeKind::Outside);
        let tab = Rect::from_min_max(
            pos2(body.left(), body.top() - s * 0.12),
            pos2(body.left() + s * 0.42, body.top()),
        );
        p.rect_filled(tab, 0, GOLD);
        p.rect_stroke(tab, 0, Stroke::new(1.0, DARK), StrokeKind::Outside);
    };

    let arrow = |p: &egui::Painter, dir: i8| {
        let h = s * 0.34;
        let w = s * 0.26;
        match dir {
            -1 => {
                triangle(
                    p,
                    [
                        pos2(center.x - h, center.y),
                        pos2(center.x + w, center.y - s * 0.3),
                        pos2(center.x + w, center.y + s * 0.3),
                    ],
                    color,
                );
            }
            1 => {
                triangle(
                    p,
                    [
                        pos2(center.x + h, center.y),
                        pos2(center.x - w, center.y - s * 0.3),
                        pos2(center.x - w, center.y + s * 0.3),
                    ],
                    color,
                );
            }
            _ => {}
        }
    };

    let magnifier = |p: &egui::Painter, inner: Option<i8>| {
        let radius = s * 0.28;
        let glass = pos2(center.x - s * 0.1, center.y - s * 0.1);
        p.circle(glass, radius, Color32::TRANSPARENT, Stroke::new(1.5, color));
        p.line_segment(
            [
                glass + vec2(radius * 0.7, radius * 0.7),
                pos2(box_rect.right(), box_rect.bottom()),
            ],
            Stroke::new(2.0, color),
        );
        if let Some(sign) = inner {
            p.line_segment(
                [
                    glass + vec2(-radius * 0.6, 0.0),
                    glass + vec2(radius * 0.6, 0.0),
                ],
                Stroke::new(1.2, color),
            );
            if sign > 0 {
                p.line_segment(
                    [
                        glass + vec2(0.0, -radius * 0.6),
                        glass + vec2(0.0, radius * 0.6),
                    ],
                    Stroke::new(1.2, color),
                );
            }
        }
    };

    let rotate = |p: &egui::Painter, clockwise: bool| {
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
        p.add(Shape::line(points, Stroke::new(1.6, color)));

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
        triangle(
            p,
            [
                tip + dir * s * 0.2,
                tip - perp * s * 0.15,
                tip + perp * s * 0.15,
            ],
            color,
        );
    };

    match glyph {
        Icon::Folder => folder(p),
        Icon::FolderUp => {
            folder(p);
            p.line_segment(
                [
                    pos2(center.x, box_rect.bottom() - s * 0.05),
                    pos2(center.x, box_rect.top() + s * 0.3),
                ],
                Stroke::new(1.5, NAVY),
            );
            triangle(
                p,
                [
                    pos2(center.x, box_rect.top() + s * 0.2),
                    pos2(center.x - s * 0.16, box_rect.top() + s * 0.42),
                    pos2(center.x + s * 0.16, box_rect.top() + s * 0.42),
                ],
                NAVY,
            );
        }
        Icon::Home => {
            triangle(
                p,
                [
                    pos2(center.x, box_rect.top()),
                    pos2(box_rect.left(), center.y),
                    pos2(box_rect.right(), center.y),
                ],
                BROWN,
            );
            let body = Rect::from_min_max(
                pos2(center.x - s * 0.28, center.y),
                pos2(center.x + s * 0.28, box_rect.bottom()),
            );
            p.rect_filled(body, 0, TAN);
            p.rect_stroke(body, 0, Stroke::new(1.0, DARK), StrokeKind::Outside);
        }
        Icon::Refresh => {
            rotate(p, true);
        }
        Icon::ThumbSmall | Icon::ThumbMedium | Icon::ThumbLarge => {
            let cells = match glyph {
                Icon::ThumbSmall => 3,
                Icon::ThumbMedium => 2,
                _ => 1,
            };
            let pad = s * 0.08;
            let cell = (s - pad * (cells as f32 + 1.0)) / cells as f32;
            for y in 0..cells {
                for x in 0..cells {
                    let min = pos2(
                        box_rect.left() + pad + x as f32 * (cell + pad),
                        box_rect.top() + pad + y as f32 * (cell + pad),
                    );
                    let rect = Rect::from_min_size(min, vec2(cell, cell));
                    p.rect_filled(rect, 0, Color32::WHITE);
                    p.rect_stroke(rect, 0, Stroke::new(1.0, DARK), StrokeKind::Outside);
                }
            }
        }
        Icon::ZoomIn => magnifier(p, Some(1)),
        Icon::ZoomOut => magnifier(p, Some(-1)),
        Icon::Fit | Icon::Actual => {
            p.rect_stroke(box_rect, 0, Stroke::new(1.0, color), StrokeKind::Outside);
            let inner = if glyph == Icon::Fit {
                Rect::from_min_max(
                    pos2(box_rect.left() + s * 0.22, box_rect.top() + s * 0.22),
                    pos2(box_rect.right() - s * 0.22, box_rect.bottom() - s * 0.22),
                )
            } else {
                Rect::from_min_max(
                    pos2(box_rect.left() + s * 0.12, box_rect.top() + s * 0.12),
                    pos2(box_rect.right() - s * 0.12, box_rect.bottom() - s * 0.12),
                )
            };
            p.rect_filled(inner, 0, BLUE);
            p.rect_stroke(inner, 0, Stroke::new(1.0, DARK), StrokeKind::Outside);
        }
        Icon::RotateLeft => rotate(p, false),
        Icon::RotateRight => rotate(p, true),
        Icon::FlipH | Icon::FlipV => {
            let horizontal = glyph == Icon::FlipH;
            let (a, b) = if horizontal {
                (
                    Rect::from_min_max(
                        pos2(box_rect.left(), box_rect.top()),
                        pos2(center.x, box_rect.bottom()),
                    ),
                    Rect::from_min_max(
                        pos2(center.x, box_rect.top()),
                        pos2(box_rect.right(), box_rect.bottom()),
                    ),
                )
            } else {
                (
                    Rect::from_min_max(
                        pos2(box_rect.left(), box_rect.top()),
                        pos2(box_rect.right(), center.y),
                    ),
                    Rect::from_min_max(
                        pos2(box_rect.left(), center.y),
                        pos2(box_rect.right(), box_rect.bottom()),
                    ),
                )
            };
            p.rect_filled(a, 0, BLUE);
            p.rect_stroke(a, 0, Stroke::new(1.0, DARK), StrokeKind::Outside);
            p.rect_filled(b, 0, Color32::WHITE);
            p.rect_stroke(b, 0, Stroke::new(1.0, DARK), StrokeKind::Outside);
            let axis = if horizontal {
                [
                    pos2(center.x, box_rect.top() - 1.0),
                    pos2(center.x, box_rect.bottom() + 1.0),
                ]
            } else {
                [
                    pos2(box_rect.left() - 1.0, center.y),
                    pos2(box_rect.right() + 1.0, center.y),
                ]
            };
            p.line_segment(axis, Stroke::new(1.0, RED));
        }
        Icon::First | Icon::Prev | Icon::Next | Icon::Last => {
            let dir = if glyph == Icon::First || glyph == Icon::Prev {
                -1
            } else {
                1
            };
            arrow(p, dir);
            if glyph == Icon::First || glyph == Icon::Last {
                let x = if dir < 0 {
                    box_rect.left()
                } else {
                    box_rect.right()
                };
                p.line_segment(
                    [pos2(x, box_rect.top()), pos2(x, box_rect.bottom())],
                    Stroke::new(2.0, color),
                );
            }
        }
        Icon::Slideshow => {
            p.rect_stroke(box_rect, 0, Stroke::new(1.0, DARK), StrokeKind::Outside);
            triangle(
                p,
                [
                    pos2(box_rect.left() + s * 0.25, box_rect.top() + s * 0.2),
                    pos2(box_rect.left() + s * 0.25, box_rect.bottom() - s * 0.2),
                    pos2(box_rect.right() - s * 0.2, center.y),
                ],
                GREEN,
            );
        }
        Icon::Stop => {
            p.rect_stroke(box_rect, 0, Stroke::new(1.0, DARK), StrokeKind::Outside);
            let inner = box_rect.shrink(s * 0.25);
            p.rect_filled(inner, 0, RED);
        }
        Icon::Fullscreen => {
            let arm = s * 0.28;
            for (corner, dx, dy) in [
                (box_rect.min, 1.0, 1.0),
                (pos2(box_rect.right(), box_rect.top()), -1.0, 1.0),
                (pos2(box_rect.left(), box_rect.bottom()), 1.0, -1.0),
                (box_rect.max, -1.0, -1.0),
            ] {
                p.line_segment(
                    [corner, corner + vec2(arm * dx, 0.0)],
                    Stroke::new(1.6, color),
                );
                p.line_segment(
                    [corner, corner + vec2(0.0, arm * dy)],
                    Stroke::new(1.6, color),
                );
            }
        }
        Icon::Delete => {
            p.line_segment([box_rect.min, box_rect.max], Stroke::new(2.0, RED));
            p.line_segment(
                [
                    pos2(box_rect.right(), box_rect.top()),
                    pos2(box_rect.left(), box_rect.bottom()),
                ],
                Stroke::new(2.0, RED),
            );
        }
        Icon::Info => {
            p.circle(center, s * 0.42, Color32::WHITE, Stroke::new(1.2, DARK));
            p.text(
                center,
                Align2::CENTER_CENTER,
                "i",
                FontId::proportional(s * 0.6),
                NAVY,
            );
        }
        Icon::Image => {
            p.rect_filled(box_rect, 0, Color32::WHITE);
            p.rect_stroke(box_rect, 0, Stroke::new(1.0, DARK), StrokeKind::Outside);
            triangle(
                p,
                [
                    pos2(box_rect.left() + s * 0.1, box_rect.bottom() - s * 0.1),
                    pos2(center.x, box_rect.top() + s * 0.3),
                    pos2(box_rect.right() - s * 0.1, box_rect.bottom() - s * 0.1),
                ],
                LEAF,
            );
            p.circle(
                pos2(box_rect.left() + s * 0.25, box_rect.top() + s * 0.25),
                s * 0.12,
                SUN,
                Stroke::new(1.0, DARK),
            );
        }
        Icon::Browser => {
            p.rect_stroke(box_rect, 0, Stroke::new(1.0, DARK), StrokeKind::Outside);
            p.line_segment(
                [
                    pos2(center.x - s * 0.15, box_rect.top()),
                    pos2(center.x - s * 0.15, box_rect.bottom()),
                ],
                Stroke::new(1.0, DARK),
            );
            p.line_segment(
                [
                    pos2(center.x + s * 0.2, box_rect.top()),
                    pos2(center.x + s * 0.2, box_rect.bottom()),
                ],
                Stroke::new(1.0, DARK),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Theme
// ---------------------------------------------------------------------------

pub fn apply_theme(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();

    style.visuals.dark_mode = false;
    style.visuals.panel_fill = FACE;
    style.visuals.extreme_bg_color = Color32::WHITE;
    style.visuals.faint_bg_color = LIGHT;
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.window_fill = FACE;
    style.visuals.window_stroke = Stroke::new(1.0, DARK);
    style.visuals.window_corner_radius = egui::CornerRadius::ZERO;
    style.visuals.menu_corner_radius = egui::CornerRadius::ZERO;
    style.visuals.window_shadow = egui::Shadow {
        offset: [0, 0],
        blur: 0,
        spread: 0,
        color: Color32::from_black_alpha(90),
    };
    style.visuals.popup_shadow = style.visuals.window_shadow;
    style.visuals.selection.bg_fill = NAVY;
    style.visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    style.visuals.hyperlink_color = NAVY;
    style.visuals.widgets.noninteractive.bg_fill = FACE;
    style.visuals.widgets.noninteractive.weak_bg_fill = FACE;
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, SHADOW);
    style.visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::ZERO;
    for widget in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.open,
    ] {
        widget.bg_fill = FACE;
        widget.weak_bg_fill = FACE;
        widget.fg_stroke = Stroke::new(1.0, TEXT);
        widget.bg_stroke = Stroke::new(1.0, SHADOW);
        widget.corner_radius = egui::CornerRadius::ZERO;
        widget.expansion = 0.0;
    }
    style.visuals.widgets.hovered.weak_bg_fill = LIGHT;
    style.visuals.widgets.active.weak_bg_fill = FACE;

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

    ctx.set_style(style);
}

/// Register a CJK capable font when the system has one, so that Chinese file
/// names render instead of turning into boxes.
pub fn install_cjk_font(ctx: &egui::Context) {
    const CANDIDATES: &[&str] = &[
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        "/System/Library/Fonts/Supplemental/NotoSansCJK-Regular.ttc",
        "/Library/Fonts/Arial Unicode.ttf",
        "/usr/share/fonts/truetype/arphic/uming.ttc",
    ];
    for path in CANDIDATES {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        if bytes.len() < 1_000 {
            continue;
        }
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "cjk".to_owned(),
            std::sync::Arc::new(egui::FontData::from_owned(bytes)),
        );
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts
                .families
                .entry(family)
                .or_default()
                .push("cjk".to_owned());
        }
        ctx.set_fonts(fonts);
        return;
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
