//! 實驗性影片檢視器：mpv 負責解碼、音訊、同步與 seek。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use eframe::egui::{self, ColorImage, Context, TextureHandle, TextureOptions, Ui};
use mpv_engine::{Engine, PlaybackEvent};

use crate::app::{Action, App};

#[derive(Default)]
struct Status {
    duration: f64,
    position: f64,
    paused: bool,
}

struct Frame {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
}

enum Command {
    Pause(bool),
    Seek(f64),
}

/// 只有按下 Play 且已播到結尾時，才需要先回到開頭。
fn restart_before_resume(paused: bool, eof_reached: bool) -> bool {
    !paused && eof_reached
}

enum Event {
    Status(Status),
    Error(String),
}

#[derive(Default)]
pub struct VideoPlayer {
    path: Option<PathBuf>,
    cancel: Option<Arc<AtomicBool>>,
    commands: Option<mpsc::Sender<Command>>,
    events: Option<mpsc::Receiver<Event>>,
    frames: Option<mpsc::Receiver<Frame>>,
    texture: Option<TextureHandle>,
    size: [usize; 2],
    status: Status,
    scrub_position: Option<f64>,
    error: Option<String>,
}

impl Drop for VideoPlayer {
    fn drop(&mut self) {
        if let Some(cancel) = &self.cancel {
            cancel.store(true, Ordering::Relaxed);
        }
    }
}

impl VideoPlayer {
    pub fn clear(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        *self = Self::default();
    }

    fn open(&mut self, path: &Path, ctx: &Context) {
        if self.path.as_deref() == Some(path) {
            return;
        }
        self.clear();
        self.path = Some(path.to_owned());
        let (command_tx, command_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let (frame_tx, frame_rx) = mpsc::sync_channel(2);
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel = Some(Arc::clone(&cancel));
        self.commands = Some(command_tx);
        self.events = Some(event_rx);
        self.frames = Some(frame_rx);
        let path = path.to_owned();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = play(&path, &ctx, &cancel, &command_rx, &event_tx, &frame_tx);
            if let Err(error) = result {
                let _ = event_tx.send(Event::Error(error));
                ctx.request_repaint();
            }
        });
    }

    fn pump(&mut self, ctx: &Context) {
        if let Some(events) = &self.events {
            while let Ok(event) = events.try_recv() {
                match event {
                    Event::Status(status) => self.status = status,
                    Event::Error(error) => self.error = Some(error),
                }
            }
        }
        if let Some(frames) = &self.frames {
            let mut latest = None;
            while let Ok(frame) = frames.try_recv() {
                latest = Some(frame);
            }
            if let Some(frame) = latest {
                self.size = [frame.width, frame.height];
                let image = ColorImage::from_rgba_unmultiplied(self.size, &frame.pixels);
                if let Some(texture) = &mut self.texture {
                    texture.set(image, TextureOptions::LINEAR);
                } else {
                    self.texture = Some(ctx.load_texture("video", image, TextureOptions::LINEAR));
                }
            }
        }
    }

    fn send(&self, command: Command) {
        if let Some(commands) = &self.commands {
            let _ = commands.send(command);
        }
    }
}

fn play(
    path: &Path,
    ctx: &Context,
    cancel: &AtomicBool,
    commands: &mpsc::Receiver<Command>,
    events: &mpsc::Sender<Event>,
    frames: &mpsc::SyncSender<Frame>,
) -> Result<(), String> {
    let source = path
        .to_str()
        .ok_or("Unable to open a non-Unicode video path")?;
    let engine = Engine::video().build().map_err(|e| e.to_string())?;
    let changed = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&changed);
    let repaint = ctx.clone();
    engine
        .attach_sw_render(move || {
            flag.store(true, Ordering::Release);
            repaint.request_repaint();
        })
        .map_err(|e| e.to_string())?;
    engine.load(source).map_err(|e| e.to_string())?;
    let mut last_status = Instant::now() - Duration::from_secs(1);
    let mut last_frame = Instant::now() - Duration::from_secs(1);
    let mut pixels = Vec::new();
    while !cancel.load(Ordering::Relaxed) {
        while let Ok(command) = commands.try_recv() {
            let result = match command {
                Command::Pause(paused) => {
                    let eof = engine.get_property::<bool>("eof-reached").unwrap_or(false);
                    if restart_before_resume(paused, eof) {
                        engine
                            .seek_absolute(0.0)
                            .and_then(|()| engine.set_paused(false))
                    } else {
                        engine.set_paused(paused)
                    }
                }
                Command::Seek(position) => engine.seek_absolute(position),
            };
            if let Err(error) = result {
                let _ = events.send(Event::Error(error.to_string()));
            }
        }
        for event in engine.pump_events() {
            if let PlaybackEvent::Failed { code, message } = event {
                return Err(format!("Playback failed ({code}): {message}"));
            }
        }
        let now = Instant::now();
        if now.duration_since(last_status) >= Duration::from_millis(150) {
            let _ = events.send(Event::Status(Status {
                duration: engine.duration().unwrap_or(0.0),
                position: engine.position().unwrap_or(0.0),
                paused: engine.is_paused(),
            }));
            ctx.request_repaint();
            last_status = now;
        }
        if changed.load(Ordering::Acquire)
            && now.duration_since(last_frame) >= Duration::from_millis(33)
        {
            changed.store(false, Ordering::Release);
            if engine.render_update() {
                let width = engine.get_property::<i64>("dwidth").unwrap_or(0);
                let height = engine.get_property::<i64>("dheight").unwrap_or(0);
                if width > 0 && height > 0 {
                    // 限制 CPU 畫面與 GPU texture；不改變原始影片或音訊。
                    let scale = (1280.0 / width.max(height) as f64).min(1.0);
                    let w = (width as f64 * scale).round().max(1.0) as i32;
                    let h = (height as f64 * scale).round().max(1.0) as i32;
                    engine
                        .render_sw(w, h, &mut pixels)
                        .map_err(|e| e.to_string())?;
                    if pixels.len() == w as usize * h as usize * 4 {
                        let _ = frames.try_send(Frame {
                            width: w as usize,
                            height: h as usize,
                            pixels: pixels.clone(),
                        });
                        ctx.request_repaint();
                    }
                }
            }
            last_frame = now;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    engine.detach_render();
    Ok(())
}

pub fn view(app: &mut App, ui: &mut Ui, path: &Path) {
    app.gif_player.clear();
    app.video_player.open(path, ui.ctx());
    app.video_player.pump(ui.ctx());
    ui.vertical_centered(|ui| {
        if let Some(texture) = &app.video_player.texture {
            let [w, h] = app.video_player.size;
            let available = ui.available_size();
            let scale = (available.x / w as f32).min((available.y - 80.0).max(100.0) / h as f32);
            ui.add(egui::Image::new((
                texture.id(),
                egui::vec2(w as f32 * scale, h as f32 * scale),
            )));
        } else {
            ui.add_space(40.0);
            ui.label(
                app.video_player
                    .error
                    .as_deref()
                    .unwrap_or("Loading video…"),
            );
            ui.add_space(40.0);
        }
        ui.horizontal(|ui| {
            if ui.button("Previous").clicked() {
                app.act(Action::PrevImage, ui.ctx());
            }
            let paused = app.video_player.status.paused;
            let button = egui::Button::new(if paused { "Play" } else { "Pause" });
            if ui
                .add_sized([64.0, ui.spacing().interact_size.y], button)
                .clicked()
            {
                app.video_player.send(Command::Pause(!paused));
            }
            let duration = app.video_player.status.duration.max(0.0);
            let mut position = app
                .video_player
                .scrub_position
                .unwrap_or(app.video_player.status.position)
                .clamp(0.0, duration);
            if duration > 0.0 {
                let slider = ui.add(egui::Slider::new(&mut position, 0.0..=duration).suffix(" s"));
                if slider.dragged() {
                    app.video_player.scrub_position = Some(position);
                }
                if slider.drag_stopped() || (slider.changed() && !slider.dragged()) {
                    app.video_player.send(Command::Seek(position));
                    app.video_player.scrub_position = None;
                }
            }
            ui.label(format!("{position:.1} / {duration:.1} s"));
            if ui.button("Next").clicked() {
                app.act(Action::NextImage, ui.ctx());
            }
            if ui.button("Back").clicked() {
                app.act(Action::CloseViewer, ui.ctx());
            }
        });
        if app.video_player.texture.is_some() {
            if let Some(error) = &app.video_player.error {
                ui.label(error);
            }
        }
    });
    ui.ctx().request_repaint_after(Duration::from_millis(100));
}

#[cfg(test)]
mod tests {
    use super::restart_before_resume;

    #[test]
    fn only_play_at_eof_restarts() {
        assert!(restart_before_resume(false, true));
        assert!(!restart_before_resume(false, false));
        assert!(!restart_before_resume(true, true));
        assert!(!restart_before_resume(true, false));
    }
}
