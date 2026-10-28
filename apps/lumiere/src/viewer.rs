//! The single image viewer: trackpad gestures (pinch zoom, two-finger pan,
//! swipe to browse, double-click fit/100%), rotate, flip and slideshow.

use std::time::{Duration, Instant};

use egui::epaint::Vertex;
use egui::{
    pos2, vec2, Align2, Color32, FontId, Mesh, Pos2, Rect, Sense, Shape, TextureId, Ui, Vec2,
};

use crate::app::{Action, App};
use crate::loader::Kind;
use crate::menu::{Key, Shortcut};
use crate::theme::{self, Icon};

/// The zoom the viewer allows, shared by the HUD buttons, the keyboard and
/// the pinch gesture so they cannot disagree.
pub const ZOOM_MIN: f32 = 0.02;
pub const ZOOM_MAX: f32 = 64.0;

/// How far the trackpad must travel horizontally before a swipe pages to the
/// neighbouring image. Once a swipe has paged, the gesture is locked: no
/// matter how far the same gesture keeps travelling (the flick's momentum
/// included) it cannot page a second time — only a pause, an equal swipe
/// the other way, or the flick's tail dying out starts a fresh one.
const SWIPE_PX: f32 = 120.0;
/// A pause longer than this starts a fresh swipe instead of extending the
/// last one. Trackpad events arrive every 8–16 ms, so the pause must be far
/// larger than the event cadence (one gesture, however long its momentum,
/// can never break); but it must stay short enough that two quick swipes —
/// typically 100–300 ms apart once the momentum dies — still count as two.
const SWIPE_GAP: Duration = Duration::from_millis(150);
/// A per-event horizontal delta below this counts as "the gesture is
/// dying" while a swipe is locked. A flick's momentum decays smoothly to
/// nothing, while a fresh swipe re-accelerates; once the tail is dying,
/// the lock releases right away so the next swipe works immediately.
const SWIPE_DEAD: f32 = 8.0;
/// Consecutive dying deltas required to release the lock, so a single noisy
/// event cannot split a live gesture.
const SWIPE_DEAD_EVENTS: u32 = 4;

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
        // Name the picture: "what is being processed" is only useful when
        // it says which one.
        let status = if app.caches.full_failed(&path) {
            format!("Failed to load {file_name}")
        } else {
            format!("Loading {file_name}…")
        };
        status_label(ui, canvas, &status, app);
        ui.ctx().accesskit_node_builder(response.id, |node| {
            node.set_role(egui::accesskit::Role::Image);
            node.set_label(file_name.as_str());
            if app.caches.full_failed(&path) {
                node.set_value(format!("failed to load {file_name}"));
            } else {
                node.set_value(format!("loading {file_name}"));
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
    let effective_zoom = if app.fit { fit_zoom } else { app.zoom };
    let drawn = vec2(base_w * effective_zoom, base_h * effective_zoom);
    // When the picture overflows the canvas there is room to pan, so a
    // two-finger scroll pans it. While it fits, a horizontal swipe browses
    // to the neighbouring image instead.
    let overflows = drawn.x > canvas.width() || drawn.y > canvas.height();

    // A pinch magnifies about the gesture centre (the pointer position): the
    // point under the two fingers stays put while the picture grows or
    // shrinks. A plain two-finger scroll must never zoom — that is the
    // trackpad's pan / browse gesture.
    let pinch: f32 = ui.ctx().input(|i| {
        i.events.iter().fold(1.0, |factor, event| match event {
            egui::Event::Zoom(scale) => factor * scale,
            _ => factor,
        })
    });
    if (pinch - 1.0).abs() > 0.001 {
        let old_zoom = effective_zoom;
        let new_zoom = (old_zoom * pinch).clamp(ZOOM_MIN, ZOOM_MAX);
        if let Some(pointer) = ui.input(|i| i.pointer.hover_pos()) {
            let center = canvas.center() + app.pan;
            let k = new_zoom / old_zoom;
            let new_center = pointer - (pointer - center) * k;
            app.pan = new_center - canvas.center();
        }
        app.zoom = new_zoom;
        app.fit = false;
    }

    let scroll = ui.input(|i| i.raw_scroll_delta);
    if scroll.x != 0.0 || scroll.y != 0.0 {
        if overflows {
            // One-to-one with the trackpad: the picture follows the content
            // direction of the scroll.
            app.pan -= scroll;
        } else if scroll.x.abs() > scroll.y.abs() {
            let now = Instant::now();
            if now.duration_since(app.swipe_last) > SWIPE_GAP {
                // A long pause ends the gesture: the next input starts a
                // fresh swipe.
                app.swipe_x = 0.0;
                app.swipe_against = 0.0;
                app.swipe_decay = 0;
            }
            app.swipe_last = now;
            if app.swipe_x.abs() >= SWIPE_PX {
                // A swipe already paged within this gesture. Same-direction
                // input — the flick's momentum, however far it travels — is
                // discarded; only an equal swipe against the locked
                // direction pages back.
                if scroll.x.signum() != app.swipe_x.signum() {
                    app.swipe_against += scroll.x.abs();
                    if app.swipe_against >= SWIPE_PX {
                        app.swipe_against = 0.0;
                        app.swipe_decay = 0;
                        app.swipe_x = -app.swipe_x;
                        let action = if app.swipe_x < 0.0 {
                            Action::PrevImage
                        } else {
                            Action::NextImage
                        };
                        app.act(action, ui.ctx());
                    }
                } else {
                    // The flick's tail dies out smoothly: once the deltas
                    // are small enough for a while, the gesture is over and
                    // the lock releases right away, so the user's next
                    // swipe works immediately instead of waiting for the
                    // tail to stop delivering events.
                    app.swipe_decay = if scroll.x.abs() < SWIPE_DEAD {
                        app.swipe_decay + 1
                    } else {
                        0
                    };
                    if app.swipe_decay >= SWIPE_DEAD_EVENTS {
                        app.swipe_x = 0.0;
                        app.swipe_against = 0.0;
                        app.swipe_decay = 0;
                    }
                }
            } else {
                app.swipe_x = (app.swipe_x + scroll.x).clamp(-SWIPE_PX, SWIPE_PX);
                if app.swipe_x == SWIPE_PX {
                    // Swiping left (content moves left) reveals the next
                    // picture, the way a photo carousel does.
                    app.act(Action::NextImage, ui.ctx());
                } else if app.swipe_x == -SWIPE_PX {
                    app.act(Action::PrevImage, ui.ctx());
                }
            }
        }
        // A vertical scroll while the picture fits does nothing: there is
        // nothing to scroll.
    }

    if response.dragged() {
        app.pan += response.drag_delta();
        app.fit = false;
    }
    // Double click bounces between "fit" and actual size (100%), the way a
    // macOS viewer does.
    if response.double_clicked() {
        if app.fit {
            app.fit = false;
            app.zoom = 1.0;
        } else {
            app.fit = true;
        }
        app.pan = Vec2::ZERO;
    }

    // Holding Space is the hand tool: the grab cursor advertises that the
    // picture can be dragged while Space stays down.
    if response.hovered() && ui.input(|i| i.key_down(egui::Key::Space)) {
        ui.ctx().set_cursor_icon(if response.dragged() {
            egui::CursorIcon::Grabbing
        } else {
            egui::CursorIcon::Grab
        });
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
/// screen reader hears "Loading one.png" or "Failed to load one.png"
/// instead of a painted patch of pixels.
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
    let buttons: [(Icon, &str, Action, bool); 13] = [
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
        (Icon::Delete, "Move to Trash", Action::MoveToTrash, true),
        (Icon::Browser, "Back to browser", Action::CloseViewer, true),
    ];
    let trash_key = Shortcut::plain(Key::Delete).display();

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
        let response = if action == Action::MoveToTrash {
            theme::tool_button_with_shortcut(&mut child, glyph, title, &trash_key, enabled)
        } else {
            theme::tool_button(&mut child, glyph, title, enabled)
        };
        if response.clicked() {
            actions.push(action);
        }
    }
    for action in actions {
        app.act(action, &ctx);
    }
}
