//! Background decode pipeline: a small worker pool plus a reply channel that the
//! UI drains once per frame. Nothing ever blocks the UI thread.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

use crate::imaging::{self, DecodeError, Decoded};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Kind {
    /// Grid thumbnail.
    Thumb,
    /// Right-hand preview pane.
    Preview,
    /// Full size image for the viewer.
    Full,
}

#[derive(Clone, Debug)]
pub struct Request {
    pub kind: Kind,
    pub path: PathBuf,
    pub size: u32,
    /// Bumped whenever the visible set changes, so queued work can be dropped.
    pub generation: u64,
}

pub struct Reply {
    pub request: Request,
    pub result: Result<Decoded, DecodeError>,
}

type Queue = Arc<(Mutex<VecDeque<Request>>, Condvar)>;

pub struct Loader {
    queue: Queue,
    rx: Receiver<Reply>,
    generation: AtomicU64,
    shutdown: Arc<AtomicBool>,
}

impl Loader {
    pub fn new(workers: usize) -> Self {
        let queue: Queue = Arc::new((Mutex::new(VecDeque::new()), Condvar::new()));
        // Backpressure prevents decoded pixel buffers accumulating without
        // bound while the main thread is busy or a native dialog is open.
        let (tx, rx) = sync_channel(2);
        let shutdown = Arc::new(AtomicBool::new(false));
        // Serializes the pixel work of this loader's workers so that at most
        // one full-resolution buffer is alive at a time. The production app
        // runs a single loader; test suites run many in parallel, so the gate
        // must be per-loader rather than a process-wide static or unrelated
        // tests would queue behind each other's decodes and time out. The
        // workers' clones keep it alive for as long as they run.
        let pixel_work = Arc::new(Mutex::new(()));

        for _ in 0..workers {
            let queue = Arc::clone(&queue);
            let tx = tx.clone();
            let shutdown = Arc::clone(&shutdown);
            let pixel_work = Arc::clone(&pixel_work);
            let _ = thread::Builder::new()
                .name("lumiere-decode".into())
                .spawn(move || {
                    let (lock, cvar) = &*queue;
                    loop {
                        let mut pending = lock.lock().unwrap();
                        let request = loop {
                            if let Some(request) = pending.pop_front() {
                                break request;
                            }
                            if shutdown.load(Ordering::Relaxed) {
                                return;
                            }
                            pending = cvar.wait(pending).unwrap();
                        };
                        drop(pending);

                        // Include scaling and cache encoding in the gate:
                        // serializing only decode would still let all workers
                        // retain full-resolution buffers together. The guard
                        // is dropped before the (possibly blocking) send so a
                        // busy UI does not also stall the other workers.
                        let result = {
                            let _pixel_work =
                                pixel_work.lock().unwrap_or_else(|error| error.into_inner());
                            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                produce(&request)
                            }))
                            .unwrap_or_else(|panic| {
                                let msg = panic
                                    .downcast_ref::<String>()
                                    .cloned()
                                    .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                                    .unwrap_or_else(|| "panic".to_string());
                                Err(DecodeError::Panic(format!("decoder panicked: {msg}")))
                            })
                        };
                        let _ = tx.send(Reply { request, result });
                    }
                });
        }

        Self {
            queue,
            rx,
            generation: AtomicU64::new(1),
            shutdown,
        }
    }

    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }

    pub fn bump_generation(&self) -> (u64, Vec<Request>) {
        let value = self.generation.fetch_add(1, Ordering::Relaxed) + 1;
        (value, self.purge_stale())
    }

    /// Drop queued work that belongs to an older visible set. The dropped
    /// requests never produce a reply, so callers must release their pending
    /// markers or those paths would never be decoded again.
    fn purge_stale(&self) -> Vec<Request> {
        let generation = self.generation();
        let (lock, _) = &*self.queue;
        let mut pending = lock.lock().unwrap();
        let mut dropped = Vec::new();
        pending.retain(|request| {
            if request.generation >= generation {
                true
            } else {
                dropped.push(request.clone());
                false
            }
        });
        dropped
    }

    /// Drop queued thumbnail work, used when the grid is not visible so the
    /// viewer's full-size requests are not stuck behind a wall of thumbnails.
    pub fn purge_thumbs(&self) -> Vec<Request> {
        let (lock, _) = &*self.queue;
        let mut pending = lock.lock().unwrap();
        let mut dropped = Vec::new();
        pending.retain(|request| {
            if request.kind != Kind::Thumb {
                true
            } else {
                dropped.push(request.clone());
                false
            }
        });
        dropped
    }

    /// How many requests are still waiting for a worker. Test-only seam: the
    /// queue is otherwise opaque and timing-dependent.
    #[cfg(any(test, feature = "test-support"))]
    pub fn queued(&self) -> usize {
        let (lock, _) = &*self.queue;
        lock.lock().unwrap().len()
    }

    pub fn submit(&self, kind: Kind, path: PathBuf, size: u32) {
        let front = matches!(kind, Kind::Full | Kind::Preview);
        self.enqueue(kind, path, size, front);
    }

    /// Queue work at the back even when it would normally cut the line, so
    /// prefetching never puts an image behind the one actually on screen.
    pub fn submit_prefetch(&self, kind: Kind, path: PathBuf, size: u32) {
        self.enqueue(kind, path, size, false);
    }

    fn enqueue(&self, kind: Kind, path: PathBuf, size: u32, front: bool) {
        let request = Request {
            kind,
            path,
            size,
            generation: self.generation(),
        };
        let (lock, cvar) = &*self.queue;
        let mut pending = lock.lock().unwrap();
        if front {
            pending.push_front(request);
        } else {
            pending.push_back(request);
        }
        cvar.notify_one();
    }

    /// Collect finished work without blocking.
    pub fn drain(&self, limit: usize) -> Vec<Reply> {
        let mut out = Vec::new();
        while out.len() < limit {
            match self.rx.try_recv() {
                Ok(reply) => out.push(reply),
                Err(_) => break,
            }
        }
        out
    }

    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
        let (_, cvar) = &*self.queue;
        cvar.notify_all();
    }
}

fn produce(request: &Request) -> Result<Decoded, DecodeError> {
    match request.kind {
        Kind::Thumb => {
            if let Some(cached) = imaging::cached_thumbnail(&request.path, request.size) {
                if let Some(decoded) = imaging::read_cached_thumbnail(&cached) {
                    return Ok(decoded);
                }
            }
            let full = imaging::decode(&request.path)?;
            let thumb = full.scaled(request.size, request.size);
            imaging::store_thumbnail(&request.path, request.size, &thumb);
            Ok(thumb)
        }
        Kind::Preview => {
            let full = imaging::decode(&request.path)?;
            Ok(full.scaled(request.size, request.size))
        }
        Kind::Full => imaging::decode(&request.path),
    }
}
