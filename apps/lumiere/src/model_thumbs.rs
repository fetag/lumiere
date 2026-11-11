//! GPU thumbnail service for `.glb` models.
//!
//! The image loader cannot decode `.glb` (it is a GPU model, not a pixel
//! image), so thumbnails and previews for models are produced here by a
//! dedicated single thread that owns one wgpu device. Each request renders
//! the model offscreen at the requested size with a fixed camera, then feeds
//! the pixels through the **same** `Decoded -> texture -> ThumbEntry` path the
//! image pipeline uses (`Caches::pump`), and persists them to the shared disk
//! thumbnail cache so later launches need no GPU work.

use std::path::PathBuf;
use std::sync::mpsc;
use std::thread::JoinHandle;

use crate::imaging::{self, DecodeError, Decoded};
use crate::loader::{Kind, Reply, Request};

/// One render job: `kind` (Thumb or Preview), target `path`, `size` (px).
pub struct ModelThumbJob {
    pub kind: Kind,
    pub path: PathBuf,
    pub size: u32,
}

/// Background GPU renderer. Created once; `submit` is cheap (channel send).
pub struct ModelThumbService {
    tx: mpsc::Sender<ModelThumbJob>,
    reply_rx: mpsc::Receiver<Reply>,
    _thread: JoinHandle<()>,
}

impl ModelThumbService {
    /// Spawn the render thread. The wgpu device is created once on that
    /// thread and reused for every job.
    pub fn start() -> Self {
        let (tx, rx) = mpsc::channel::<ModelThumbJob>();
        let (reply_tx, reply_rx) = mpsc::channel::<Reply>();
        let thread = std::thread::Builder::new()
            .name("model-thumb".into())
            .spawn(move || run_loop(rx, reply_tx))
            .expect("spawn model thumb thread");
        Self {
            tx,
            reply_rx,
            _thread: thread,
        }
    }

    /// Queue a render. Fails silently if the service has shut down.
    pub fn submit(&self, kind: Kind, path: PathBuf, size: u32) {
        let _ = self.tx.send(ModelThumbJob { kind, path, size });
    }

    /// Collect finished replies (up to `max`), non-blocking.
    pub fn drain(&self, max: usize) -> Vec<Reply> {
        let mut out = Vec::new();
        while out.len() < max {
            match self.reply_rx.try_recv() {
                Ok(reply) => out.push(reply),
                Err(_) => break,
            }
        }
        out
    }
}

/// Thread body: create one wgpu device, then render jobs as they arrive.
fn run_loop(rx: mpsc::Receiver<ModelThumbJob>, reply_tx: mpsc::Sender<Reply>) {
    // Device setup is best-effort: on a machine with no GPU we still answer
    // every job (with an error) so the cache marks the thumbnail as failed
    // instead of leaving a permanent `pending` marker.
    let gpu = match setup_gpu() {
        Ok(g) => Some(g),
        Err(e) => {
            log::warn!("model thumb service: {e}");
            None
        }
    };

    // The loop exits when the sender (held by the UI) is dropped and
    // rx.recv() returns Err — that is the shutdown signal.
    while let Ok(job) = rx.recv() {
        let request = Request {
            kind: job.kind,
            path: job.path.clone(),
            size: job.size,
            generation: 0,
        };
        let result = render_job(gpu.as_ref(), &job);
        let reply = Reply {
            request,
            result: result.map_err(DecodeError::from_message),
        };
        if reply_tx.send(reply).is_err() {
            break; // UI gone
        }
    }
}

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
}

fn setup_gpu() -> Result<Gpu, String> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
    }))
    .map_err(|e| format!("no GPU adapter: {e}"))?;
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .map_err(|e| format!("request_device failed: {e}"))?;
    Ok(Gpu { device, queue })
}

fn render_job(gpu: Option<&Gpu>, job: &ModelThumbJob) -> Result<Decoded, String> {
    let path = job.path.as_path();
    let size = job.size.max(16);

    // 1. Disk cache: a previous render (this or an earlier launch) is reused.
    if let Some(file) = imaging::cached_thumbnail(path, size) {
        if let Some(img) = imaging::read_cached_thumbnail(&file) {
            return Ok(img);
        }
    }

    // 2. Read + render.
    let Some(gpu) = gpu else {
        return Err("no GPU for model thumbnail".to_string());
    };
    let bytes = std::fs::read(path).map_err(|_| format!("open failed: {}", path.display()))?;
    let (pixels, rendered) =
        crate::models::render_offscreen_with_device(&gpu.device, &gpu.queue, &bytes, size)?;

    // The GPU renders at `rendered` (a multiple of 64); Lanczos-downscale to
    // the exact requested size for the cache + display.
    let full = Decoded {
        width: rendered,
        height: rendered,
        rgba: pixels,
        premultiplied: false,
        original_size: None,
    };
    let decoded = full.scaled(size, size);

    // 3. Persist to the shared disk thumbnail cache.
    imaging::store_thumbnail(path, size, &decoded);
    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A thumbnail for a model that does not exist is reported as a failure
    /// (not a panic, and not an indefinite pending).
    #[test]
    fn missing_model_errors() {
        let job = ModelThumbJob {
            kind: Kind::Thumb,
            path: PathBuf::from("/nonexistent/model.glb"),
            size: 128,
        };
        let res = render_job(None, &job);
        assert!(res.is_err());
    }

    /// A real GLB fixture renders to a square RGBA buffer of the requested size.
    #[test]
    fn render_fixture_square() {
        let fixture =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/model-test.glb");
        let bytes = std::fs::read(&fixture).expect("read fixture");
        let gpu = setup_gpu().expect("gpu");
        let size = 128;
        let (pixels, rendered) =
            crate::models::render_offscreen_with_device(&gpu.device, &gpu.queue, &bytes, size)
                .expect("render");
        // Size is normalized to a multiple of 64 and the buffer matches it.
        assert_eq!(rendered % 64, 0);
        assert_eq!(pixels.len(), (rendered * rendered * 4) as usize);
    }

    /// Full service path: start -> submit -> drain yields a square `Decoded`,
    /// and the result is persisted to the disk thumbnail cache (a second
    /// request is served from that cache, no re-render).
    #[test]
    fn service_renders_and_persists() {
        let fixture =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/model-test.glb");
        let size = 96;
        let service = ModelThumbService::start();
        service.submit(Kind::Thumb, fixture.clone(), size);

        // The render happens on another thread; poll the (non-blocking) reply
        // channel with a bounded timeout, sleeping so we actually wait for the
        // GPU work to finish rather than spinning.
        let reply = (0..300)
            .find_map(|_| {
                let r = service.drain(1).into_iter().next();
                std::thread::sleep(std::time::Duration::from_millis(20));
                r
            })
            .unwrap_or_else(|| panic!("timed out waiting for model thumbnail"));
        let decoded = reply.result.expect("model thumbnail should render");
        assert_eq!((decoded.width, decoded.height), (size, size));
        assert_eq!(decoded.rgba.len(), (size * size * 4) as usize);

        // Persisted to the shared disk cache.
        let cached =
            imaging::cached_thumbnail(&fixture, size).expect("thumbnail should be stored on disk");
        assert!(imaging::read_cached_thumbnail(&cached).is_some());

        // A second request is answered from the disk cache (still a square).
        service.submit(Kind::Thumb, fixture.clone(), size);
        let reply = (0..300)
            .find_map(|_| {
                let r = service.drain(1).into_iter().next();
                std::thread::sleep(std::time::Duration::from_millis(20));
                r
            })
            .expect("second request should complete");
        let decoded = reply.result.expect("cached thumbnail should decode");
        assert_eq!((decoded.width, decoded.height), (size, size));
    }
}
