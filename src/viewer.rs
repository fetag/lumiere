//! The single image viewer: zoom, pan, rotate, flip, full screen and slideshow.

use egui::epaint::Vertex;
use egui::{
    pos2, vec2, Align2, Color32, FontId, Mesh, Pos2, Rect, Sense, Shape, TextureId, Ui, Vec2,
};

use crate::app::{Action, App};
use crate::loader::Kind;
use crate::theme::{self, Icon};

pub fn view(app: &mut App, ui: &mut Ui) {
    let Some(path) = app.current_image_path() else {
        status_label(
            ui,
            ui.available_rect_before_wrap(),
            "No image selected",
            app,
        );
        return;
    };

    let cached = app
        .caches
        .full(&path)
        .map(|entry| (entry.texture.id(), entry.width, entry.height));
    if cached.is_some() {
        app.caches.protect_full(&path);
    }
    if cached.is_none() && !app.caches.full_failed(&path) {
        app.caches.request(&app.loader, Kind::Full, &path, 0);
    }
    app.prefetch_neighbours();

    let canvas = ui.available_rect_before_wrap();
    let response = ui.allocate_rect(canvas, Sense::click_and_drag());

    // The canvas is one big drawn picture; tell the accessibility tree what
    // it is and what is on it, the way a native image well would.
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_owned();

    let Some((texture, width, height)) = cached else {
        status_label(
            ui,
            canvas,
            if app.caches.full_failed(&path) {
                "Failed to load image"
            } else {
                "Loading..."
            },
            app,
        );
        ui.ctx().accesskit_node_builder(response.id, |node| {
            node.set_role(egui::accesskit::Role::Image);
            node.set_label(file_name.as_str());
            if app.caches.full_failed(&path) {
                node.set_value("failed to load");
            } else {
                node.set_value("loading");
            }
        });
        draw_toolbar(app, ui, canvas);
        return;
    };

    // ---- interaction ------------------------------------------------------

    let rotated = app.rotation % 180 != 0;
    let (base_w, base_h) = if rotated {
        (height as f32, width as f32)
    } else {
        (width as f32, height as f32)
    };
    let fit_zoom = (canvas.width() / base_w)
        .min(canvas.height() / base_h)
        .max(0.001);

    if response.dragged() {
        app.pan += response.drag_delta();
        app.fit = false;
    }
    if response.double_clicked() {
        app.fit = !app.fit;
        app.pan = Vec2::ZERO;
    }

    let scroll: f32 = ui.input(|i| i.raw_scroll_delta.y);
    if scroll.abs() > 0.0 {
        let old_zoom = if app.fit { fit_zoom } else { app.zoom };
        let factor = (1.0f32 + scroll.abs() * 0.0015).powf(if scroll > 0.0 { 1.0 } else { -1.0 });
        let new_zoom = (old_zoom * factor).clamp(0.01, 64.0);
        if let Some(pointer) = ui.input(|i| i.pointer.hover_pos()) {
            let center = canvas.center() + app.pan;
            let k = new_zoom / old_zoom;
            let new_center = pointer - (pointer - center) * k;
            app.pan = new_center - canvas.center();
        }
        app.zoom = new_zoom;
        app.fit = false;
    }

    let zoom = if app.fit {
        app.zoom = fit_zoom;
        fit_zoom
    } else {
        app.zoom
    };

    // Now that the size and zoom are known, complete the canvas' description.
    let value = format!("{width} x {height} px, {:.0}%", zoom * 100.0);
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_role(egui::accesskit::Role::Image);
        node.set_label(file_name.as_str());
        node.set_value(value.as_str());
    });

    // ---- draw -------------------------------------------------------------

    let center = canvas.center() + app.pan;
    paint_rotated(
        ui.painter(),
        texture,
        center,
        vec2(width as f32 * zoom, height as f32 * zoom),
        app.rotation as f32,
        app.flip_h,
        app.flip_v,
    );

    draw_toolbar(app, ui, canvas);

    let label = format!(
        "{}  {} x {}  {:.0}%{}",
        path.file_name().and_then(|n| n.to_str()).unwrap_or(""),
        width,
        height,
        zoom * 100.0,
        if app.slideshow { "  [slideshow]" } else { "" }
    );
    ui.painter().text(
        pos2(canvas.right() - 8.0, canvas.bottom() - 8.0),
        Align2::RIGHT_BOTTOM,
        label,
        FontId::proportional(12.0),
        if app.canvas_dark {
            Color32::from_gray(200)
        } else {
            Color32::WHITE
        },
    );
}

/// A status line as a real, accessible `Label` centred on the canvas, so a
/// screen reader hears "Loading..." or "Failed to load image" instead of a
/// painted patch of pixels.
fn status_label(ui: &mut Ui, rect: Rect, text: &str, app: &App) {
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::centered_and_justified(
                egui::Direction::TopDown,
            ))
            .id_salt("viewer_status"),
    );
    let color = if app.canvas_dark {
        Color32::from_gray(200)
    } else {
        Color32::WHITE
    };
    child.label(egui::RichText::new(text).color(color).size(14.0));
}

fn paint_rotated(
    painter: &egui::Painter,
    texture: TextureId,
    center: Pos2,
    size: Vec2,
    degrees: f32,
    flip_h: bool,
    flip_v: bool,
) {
    let angle = degrees.to_radians();
    let (cos, sin) = (angle.cos(), angle.sin());
    let half = size / 2.0;

    let corners = [
        pos2(center.x - half.x, center.y - half.y),
        pos2(center.x + half.x, center.y - half.y),
        pos2(center.x - half.x, center.y + half.y),
        pos2(center.x + half.x, center.y + half.y),
    ];
    let rotated: Vec<Pos2> = corners
        .iter()
        .map(|p| {
            let dx = p.x - center.x;
            let dy = p.y - center.y;
            pos2(
                center.x + dx * cos - dy * sin,
                center.y + dx * sin + dy * cos,
            )
        })
        .collect();

    let mut uv: [Pos2; 4] = [
        pos2(0.0, 0.0),
        pos2(1.0, 0.0),
        pos2(0.0, 1.0),
        pos2(1.0, 1.0),
    ];
    for uv in uv.iter_mut() {
        if flip_h {
            uv.x = 1.0 - uv.x;
        }
        if flip_v {
            uv.y = 1.0 - uv.y;
        }
    }

    let mut mesh = Mesh::with_texture(texture);
    for (pos, uv) in rotated.iter().zip(uv.iter()) {
        mesh.vertices.push(Vertex {
            pos: *pos,
            uv: *uv,
            color: Color32::WHITE,
        });
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(2, 1, 3);
    painter.add(Shape::mesh(mesh));
}

/// Floating toolbar drawn on top of the canvas: a rounded, lightly
/// translucent panel that matches the current light/dark appearance (like a
/// native media HUD), built from the same buttons as the rest of the chrome.
fn draw_toolbar(app: &mut App, ui: &mut Ui, canvas: Rect) {
    let buttons: [(Icon, &str, Action, bool); 12] = [
        (Icon::First, "First", Action::FirstImage, true),
        (Icon::Prev, "Previous", Action::PrevImage, true),
        (Icon::Next, "Next", Action::NextImage, true),
        (Icon::Last, "Last", Action::LastImage, true),
        (Icon::ZoomOut, "Zoom out", Action::ZoomOut, true),
        (Icon::ZoomIn, "Zoom in", Action::ZoomIn, true),
        (Icon::Fit, "Fit to window", Action::FitWindow, true),
        (Icon::Actual, "Actual size", Action::ActualSize, true),
        (Icon::RotateLeft, "Rotate left", Action::RotateCCW, true),
        (Icon::RotateRight, "Rotate right", Action::RotateCW, true),
        (
            Icon::Stop,
            "Stop slideshow",
            Action::StopSlideshow,
            app.slideshow,
        ),
        (Icon::Browser, "Back to browser", Action::CloseViewer, true),
    ];

    let width = buttons.len() as f32 * 34.0 + 12.0;
    let rect = Rect::from_min_size(
        pos2(canvas.left() + 8.0, canvas.bottom() - 40.0),
        vec2(width, 32.0),
    );
    let visuals = ui.visuals();
    // A little translucent, so the image underneath stays visible.
    let base = visuals.window_fill;
    let fill = Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), 235);
    ui.painter().rect(
        rect,
        visuals.window_corner_radius.nw,
        fill,
        visuals.window_stroke,
        egui::StrokeKind::Middle,
    );

    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink(4.0))
            .layout(egui::Layout::left_to_right(egui::Align::Center))
            .id_salt("viewer_toolbar"),
    );
    let ctx = ui.ctx().clone();
    let mut actions: Vec<Action> = Vec::new();
    for (glyph, title, action, enabled) in buttons {
        if theme::tool_button(&mut child, glyph, title, enabled).clicked() {
            actions.push(action);
        }
    }
    for action in actions {
        app.act(action, &ctx);
    }
}
