//! Follows the open folder so files that other programs add, remove or rename
//! show up without a manual reload.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use egui::Context;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

/// Quiet time after the last event before the folder is listed again; a copy
/// emits a burst of events and one listing at the end is enough.
const SETTLE: Duration = Duration::from_millis(400);
/// A steady stream of events (a long copy) still refreshes this often.
const MAX_WAIT: Duration = Duration::from_secs(2);

pub struct DirWatcher {
    dir: Option<PathBuf>,
    watcher: Option<RecommendedWatcher>,
    tx: mpsc::Sender<()>,
    rx: mpsc::Receiver<()>,
    /// When the first and the latest unhandled event arrived.
    pending: Option<(Instant, Instant)>,
}

impl DirWatcher {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            dir: None,
            watcher: None,
            tx,
            rx,
            pending: None,
        }
    }

    /// Keep watching `dir` (re-targeting when it changed). Returns true once
    /// the folder has changed and settled, so the caller should re-list it.
    pub fn poll(&mut self, ctx: &Context, dir: &Path) -> bool {
        if self.dir.as_deref() != Some(dir) {
            self.retarget(ctx, dir);
        }
        let now = Instant::now();
        while self.rx.try_recv().is_ok() {
            let first = self.pending.map_or(now, |(first, _)| first);
            self.pending = Some((first, now));
        }
        let Some((first, last)) = self.pending else {
            return false;
        };
        if now - last >= SETTLE || now - first >= MAX_WAIT {
            self.pending = None;
            return true;
        }
        ctx.request_repaint_after(SETTLE);
        false
    }

    fn retarget(&mut self, ctx: &Context, dir: &Path) {
        self.watcher = None;
        self.pending = None;
        while self.rx.try_recv().is_ok() {}
        let tx = self.tx.clone();
        let repaint = ctx.clone();
        // A folder that cannot be watched (network share, permissions) just
        // keeps the manual reload.
        self.watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            if event.is_ok_and(|e| !matches!(e.kind, EventKind::Access(_))) {
                let _ = tx.send(());
                repaint.request_repaint();
            }
        })
        .ok()
        .and_then(|mut watcher| {
            watcher.watch(dir, RecursiveMode::NonRecursive).ok()?;
            Some(watcher)
        });
        self.dir = Some(dir.to_path_buf());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_file_in_the_folder_is_reported() {
        let dir = std::env::temp_dir().join(format!("lumiere-watch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ctx = Context::default();
        let mut watch = DirWatcher::new();
        assert!(!watch.poll(&ctx, &dir));

        std::fs::write(dir.join("new.png"), b"x").unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut seen = false;
        while !seen && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
            seen = watch.poll(&ctx, &dir);
        }
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(seen, "adding a file must be reported");
    }
}
