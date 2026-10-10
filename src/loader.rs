//! Background decode pipeline: a small worker pool plus a reply channel that the
//! UI drains once per frame. Nothing ever blocks the UI thread.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

use crate::imaging::{self, Decoded};

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
    pub result: Result<Decoded, String>,
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
        let (tx, rx) = channel();
        let shutdown = Arc::new(AtomicBool::new(false));

        for _ in 0..workers.max(1) {
            let queue = Arc::clone(&queue);
            let tx = tx.clone();
            let shutdown = Arc::clone(&shutdown);
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

                        let result = produce(&request);
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

    pub fn bump_generation(&self) -> u64 {
        let value = self.generation.fetch_add(1, Ordering::Relaxed) + 1;
        self.purge_stale();
        value
    }

    /// Drop queued work that belongs to an older visible set.
    fn purge_stale(&self) {
        let generation = self.generation();
        let (lock, _) = &*self.queue;
        let mut pending = lock.lock().unwrap();
        pending.retain(|request| request.generation >= generation);
    }

    /// Drop queued thumbnail work, used when the grid is not visible so the
    /// viewer's full-size requests are not stuck behind a wall of thumbnails.
    pub fn purge_thumbs(&self) {
        let (lock, _) = &*self.queue;
        let mut pending = lock.lock().unwrap();
        pending.retain(|request| request.kind != Kind::Thumb);
    }

    pub fn submit(&self, kind: Kind, path: PathBuf, size: u32) {
        let request = Request {
            kind,
            path,
            size,
            generation: self.generation(),
        };
        let (lock, cvar) = &*self.queue;
        let mut pending = lock.lock().unwrap();
        if matches!(kind, Kind::Full | Kind::Preview) {
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

fn produce(request: &Request) -> Result<Decoded, String> {
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
