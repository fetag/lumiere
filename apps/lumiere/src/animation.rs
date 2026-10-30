//! GIF 動畫：使用 zengif 逐幀解碼，以有限佇列控制記憶體。

use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use egui::{ColorImage, Context, TextureHandle, TextureOptions};
use zengif::{Decoder, Limits, Repeat, Unstoppable};

const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_PIXELS: u64 = 8_000_000;
const MAX_DECODE_MEMORY: u64 = 64 * 1024 * 1024;
const QUEUE_FRAMES: usize = 2;

struct Frame {
    width: usize,
    height: usize,
    rgba: Vec<u8>,
    delay: Duration,
}

enum Message {
    Frame(Frame),
    Finished,
    Error(String),
}

/// 當佇列滿載時可取消等待，切換圖片或關閉檢視器不會留下阻塞的 worker。
fn send(
    tx: &mpsc::SyncSender<Message>,
    mut message: Message,
    cancelled: &AtomicBool,
    ctx: &Context,
) -> bool {
    loop {
        if cancelled.load(Ordering::Relaxed) {
            return false;
        }
        match tx.try_send(message) {
            Ok(()) => {
                ctx.request_repaint();
                return true;
            }
            Err(mpsc::TrySendError::Full(returned)) => {
                message = returned;
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(mpsc::TrySendError::Disconnected(_)) => return false,
        }
    }
}

fn decode_stream(
    path: &Path,
    tx: &mpsc::SyncSender<Message>,
    cancelled: &AtomicBool,
    ctx: &Context,
) -> Result<(), String> {
    let mut completed: u32 = 0;
    loop {
        if cancelled.load(Ordering::Relaxed) {
            return Ok(());
        }
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        if file.metadata().map_err(|e| e.to_string())?.len() > MAX_FILE_BYTES {
            return Err("GIF exceeds the 64 MiB file size limit; showing a static image".into());
        }
        let limits = Limits::default()
            .max_total_pixels(MAX_PIXELS)
            .max_memory(MAX_DECODE_MEMORY)
            .max_file_size(MAX_FILE_BYTES)
            .max_frame_count(10_000);
        let mut decoder =
            Decoder::new(BufReader::new(file), limits, &Unstoppable).map_err(|e| e.to_string())?;
        let mut count = 0usize;
        loop {
            if cancelled.load(Ordering::Relaxed) {
                return Ok(());
            }
            let width = usize::from(decoder.width());
            let height = usize::from(decoder.height());
            // next_frame() 的輸出計入 zengif 0.6 的累積配置統計；
            // 使用原地 callback 才能以固定預算播放長動畫。
            let message = match decoder
                .with_next_frame(|_, delay, pixels| Frame {
                    width,
                    height,
                    rgba: pixels.iter().flat_map(|p| [p.r, p.g, p.b, p.a]).collect(),
                    delay: Duration::from_millis((u64::from(delay) * 10).max(20)),
                })
                .map_err(|e| e.to_string())?
            {
                Some(frame) => frame,
                None => break,
            };
            count += 1;
            if !send(tx, Message::Frame(message), cancelled, ctx) {
                return Ok(());
            }
        }
        if count == 0 {
            return Err("GIF contains no frames".into());
        }
        completed = completed.saturating_add(1);
        let finished = match decoder.repeat() {
            Repeat::Once => true,
            Repeat::Infinite => false,
            Repeat::Count(n) => completed > u32::from(n),
            _ => true,
        };
        if finished || count == 1 {
            send(tx, Message::Finished, cancelled, ctx);
            return Ok(());
        }
    }
}

#[derive(Default)]
pub struct GifPlayer {
    path: Option<PathBuf>,
    cancelled: Option<Arc<AtomicBool>>,
    rx: Option<mpsc::Receiver<Message>>,
    texture: Option<TextureHandle>,
    next: Option<Instant>,
    paused: bool,
    finished: bool,
    frame_count: usize,
    error: Option<String>,
}

impl GifPlayer {
    pub fn clear(&mut self) {
        if let Some(cancelled) = self.cancelled.take() {
            cancelled.store(true, Ordering::Relaxed);
        }
        *self = Self::default();
    }

    pub fn update(&mut self, ctx: &Context, path: &Path) -> Option<egui::TextureId> {
        if path
            .extension()
            .and_then(|e| e.to_str())
            .is_none_or(|e| !e.eq_ignore_ascii_case("gif"))
        {
            self.clear();
            return None;
        }
        if self.path.as_deref() != Some(path) {
            self.clear();
            self.path = Some(path.to_owned());
            let cancelled = Arc::new(AtomicBool::new(false));
            let worker_cancelled = cancelled.clone();
            self.cancelled = Some(cancelled);
            let (tx, rx) = mpsc::sync_channel(QUEUE_FRAMES);
            self.rx = Some(rx);
            let path = path.to_owned();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                static DECODE_GATE: OnceLock<Mutex<()>> = OnceLock::new();
                let _guard = DECODE_GATE
                    .get_or_init(|| Mutex::new(()))
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                if worker_cancelled.load(Ordering::Relaxed) {
                    return;
                }
                if let Err(error) = decode_stream(&path, &tx, &worker_cancelled, &ctx) {
                    send(&tx, Message::Error(error), &worker_cancelled, &ctx);
                }
            });
        }
        let now = Instant::now();
        if !self.paused && self.next.is_none_or(|next| now >= next) {
            let message = self.rx.as_ref().and_then(|rx| rx.try_recv().ok());
            match message {
                Some(Message::Frame(frame)) => {
                    let max_side = ctx.input(|i| i.max_texture_side);
                    if frame.width > max_side || frame.height > max_side {
                        self.error = Some(
                            "GIF exceeds the GPU texture size limit; showing a static image".into(),
                        );
                        if let Some(cancelled) = &self.cancelled {
                            cancelled.store(true, Ordering::Relaxed);
                        }
                        self.rx = None;
                    } else {
                        let image = ColorImage::from_rgba_unmultiplied(
                            [frame.width, frame.height],
                            &frame.rgba,
                        );
                        if let Some(texture) = &mut self.texture {
                            texture.set(image, TextureOptions::LINEAR);
                        } else {
                            self.texture = Some(ctx.load_texture(
                                "gif-animation",
                                image,
                                TextureOptions::LINEAR,
                            ));
                        }
                        self.frame_count += 1;
                        self.next = Some(now + frame.delay);
                    }
                }
                Some(Message::Finished) => {
                    self.finished = true;
                    self.paused = true;
                    self.rx = None;
                }
                Some(Message::Error(error)) => {
                    self.error = Some(error);
                    self.rx = None;
                }
                None => {}
            }
        }
        if !self.paused && self.rx.is_some() {
            let wait = self.next.map_or(Duration::from_millis(50), |next| {
                next.saturating_duration_since(now)
                    .max(Duration::from_millis(10))
            });
            ctx.request_repaint_after(wait);
        }
        self.texture.as_ref().map(TextureHandle::id)
    }

    pub fn is_animated(&self) -> bool {
        self.frame_count > 1
    }
    pub fn paused(&self) -> bool {
        self.paused
    }
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    pub fn toggle(&mut self) {
        if self.finished {
            // 下次 update 重新開啟檔案，從第一幀開始播放。
            self.clear();
        } else {
            self.paused = !self.paused;
            if !self.paused {
                self.next = Some(Instant::now());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;

    fn fixture(repeat: Option<gif::Repeat>) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder =
                gif::Encoder::new(&mut bytes, 2, 1, &[0, 0, 0, 255, 0, 0, 0, 0, 255]).unwrap();
            if let Some(repeat) = repeat {
                encoder.set_repeat(repeat).unwrap();
            }
            let first = gif::Frame {
                width: 2,
                height: 1,
                buffer: Cow::Borrowed(&[1, 1]),
                delay: 5,
                ..Default::default()
            };
            encoder.write_frame(&first).unwrap();
            let second = gif::Frame {
                width: 2,
                height: 1,
                buffer: Cow::Borrowed(&[0, 2]),
                transparent: Some(0),
                delay: 12,
                dispose: gif::DisposalMethod::Previous,
                ..Default::default()
            };
            encoder.write_frame(&second).unwrap();
            let third = gif::Frame {
                width: 1,
                height: 1,
                buffer: Cow::Borrowed(&[2]),
                delay: 7,
                ..Default::default()
            };
            encoder.write_frame(&third).unwrap();
        }
        bytes
    }

    #[test]
    fn stream_respects_timing_transparency_disposal_and_repeat() {
        for (repeat, expected) in [(None, 3), (Some(gif::Repeat::Finite(2)), 9)] {
            let path = std::env::temp_dir().join(format!(
                "lumiere-zengif-test-{}-{expected}.gif",
                std::process::id()
            ));
            std::fs::write(&path, fixture(repeat)).unwrap();
            let (tx, rx) = mpsc::sync_channel(16);
            decode_stream(&path, &tx, &AtomicBool::new(false), &Context::default()).unwrap();
            let mut frames = Vec::new();
            while let Ok(message) = rx.try_recv() {
                match message {
                    Message::Frame(frame) => frames.push(frame),
                    Message::Finished => break,
                    Message::Error(e) => panic!("{e}"),
                }
            }
            assert_eq!(frames.len(), expected);
            assert_eq!(frames[0].delay, Duration::from_millis(50));
            assert_eq!(frames[1].delay, Duration::from_millis(120));
            assert_eq!(&frames[1].rgba, &[255, 0, 0, 255, 0, 0, 255, 255]);
            assert_eq!(&frames[2].rgba, &[0, 0, 255, 255, 255, 0, 0, 255]);
            std::fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn long_gif_streams_without_accumulating_decoded_frames() {
        let path =
            std::env::temp_dir().join(format!("lumiere-long-gif-{}.gif", std::process::id()));
        {
            let file = std::fs::File::create(&path).unwrap();
            let mut encoder = gif::Encoder::new(file, 640, 640, &[0, 0, 0, 255, 0, 0]).unwrap();
            let pixels = vec![1u8; 640 * 640];
            let frame = gif::Frame {
                width: 640,
                height: 640,
                buffer: Cow::Borrowed(&pixels),
                delay: 7,
                ..Default::default()
            };
            for _ in 0..101 {
                encoder.write_frame(&frame).unwrap();
            }
        }
        let (tx, rx) = mpsc::sync_channel(QUEUE_FRAMES);
        let ctx = Context::default();
        std::thread::scope(|scope| {
            let worker = scope.spawn(|| decode_stream(&path, &tx, &AtomicBool::new(false), &ctx));
            let mut count = 0;
            loop {
                match rx.recv_timeout(Duration::from_secs(30)).unwrap() {
                    Message::Frame(_) => count += 1,
                    Message::Finished => break,
                    Message::Error(e) => panic!("{e}"),
                }
            }
            assert_eq!(count, 101);
            worker.join().unwrap().unwrap();
        });
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn player_pauses_finishes_and_restarts() {
        let ctx = Context::default();
        let path = PathBuf::from("sample.gif");
        let (tx, rx) = mpsc::sync_channel(4);
        for _ in 0..2 {
            tx.send(Message::Frame(Frame {
                width: 1,
                height: 1,
                rgba: vec![255, 0, 0, 255],
                delay: Duration::from_millis(50),
            }))
            .unwrap();
        }
        tx.send(Message::Finished).unwrap();
        let mut player = GifPlayer {
            path: Some(path.clone()),
            rx: Some(rx),
            ..Default::default()
        };
        assert!(player.update(&ctx, &path).is_some());
        assert_eq!(player.frame_count, 1);
        player.toggle();
        player.next = Some(Instant::now());
        player.update(&ctx, &path);
        assert_eq!(player.frame_count, 1);
        player.toggle();
        player.update(&ctx, &path);
        assert_eq!(player.frame_count, 2);
        player.next = Some(Instant::now());
        player.update(&ctx, &path);
        assert!(player.finished && player.paused());
        player.toggle();
        assert!(player.path.is_none());
    }

    #[test]
    fn full_queue_cancels_without_blocking() {
        let (tx, _rx) = mpsc::sync_channel(1);
        tx.send(Message::Finished).unwrap();
        let cancel = AtomicBool::new(true);
        assert!(!send(&tx, Message::Finished, &cancel, &Context::default()));
    }
}
