//! Application state, window chrome (menu bar, toolbars, status bar) and the
//! command dispatch used by menus, toolbar buttons and the keyboard alike.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use egui::{Context, TextureHandle, TextureOptions, Vec2};

use crate::file_ops::{self, Outcome as OpOutcome};
use crate::fsmodel::{self, Entry, SortKey};
use crate::imaging::{self, Info};
use crate::loader::{Kind, Loader, Request};
use crate::menu::{Item, Key, Shortcut};
use crate::theme::{self, Icon};

mod cache;
mod commands;
mod dialogs;
mod menus;
mod open_with;
mod watch;

use cache::*;
mod view;

pub const APP_TITLE: &str = "Lumiere";

/// Pending "Move to Folder" / "Copy to Folder" dialog state.
#[derive(Clone, Debug)]
pub struct Transfer {
    pub paths: Vec<PathBuf>,
    pub dest: String,
    pub copy: bool,
}

/// The zoom, pan and full-screen state captured when a slideshow starts, so
/// stopping the show (or closing the viewer) puts the window and the picture
/// back exactly the way the user had them.
#[derive(Clone, Copy, Debug)]
pub struct SlideshowView {
    pub fullscreen: bool,
    pub fit: bool,
    pub zoom: f32,
    pub pan: Vec2,
}

impl Default for SlideshowView {
    fn default() -> Self {
        Self {
            fullscreen: false,
            fit: true,
            zoom: 1.0,
            pan: Vec2::ZERO,
        }
    }
}

pub struct App {
    pub mode: Mode,
    pub dir: PathBuf,
    pub entries: Vec<Entry>,
    pub selected: usize,
    pub multi: HashSet<usize>,
    pub anchor: usize,

    /// The grid cell / sidebar row that has keyboard focus, if any; kept in
    /// sync with egui's focus each frame so arrow keys move selection and
    /// focus together.
    pub grid_focus: Option<usize>,
    /// The sidebar row the keyboard is on, if any (index into the flat row
    /// list drawn by `browser::sidebar_pane`).
    pub sidebar_selected: Option<usize>,
    pub sidebar_focus: Option<usize>,
    /// Folders pinned into the sidebar's Locations section.
    pub sidebar_dirs: Vec<PathBuf>,
    /// Recently visited folders, most recent first.
    pub recent_dirs: Vec<PathBuf>,

    pub caches: Caches,
    pub loader: Loader,
    listing_tx: mpsc::Sender<ListingReply>,
    listing_rx: mpsc::Receiver<ListingReply>,
    listing_generation: u64,
    dir_watch: watch::DirWatcher,
    /// File to select after the directory worker has supplied its entries.
    pending_select: Option<(PathBuf, bool)>,

    pub sort: SortKey,
    pub sort_reverse: bool,
    pub filter: String,
    pub show_folders: bool,
    pub thumb_size: u32,

    pub show_sidebar: bool,
    pub show_inspector: bool,
    pub show_toolbar: bool,
    pub show_status: bool,
    /// The wildcard-filter popover on the address row, when it is open.
    pub filter_open: bool,

    pub zoom: f32,
    pub fit: bool,
    pub pan: Vec2,
    pub rotation: i32,
    pub flip_h: bool,
    pub flip_v: bool,
    pub canvas_dark: bool,

    pub slideshow: bool,
    pub slideshow_delay: f32,
    pub slideshow_at: Instant,
    /// Whether a slideshow should start in full screen (Settings ▸ Slideshow).
    pub slideshow_fullscreen: bool,
    /// The view to restore when the slideshow ends, captured at its start.
    pub slideshow_saved: Option<SlideshowView>,

    /// Horizontal trackpad input accumulated toward a left/right image swipe
    /// (see `viewer::view`); once it reaches the threshold the image switches
    /// and the gesture is locked, so no matter how far the same swipe keeps
    /// travelling (the flick's momentum included) it cannot page a second
    /// time.
    pub swipe_x: f32,
    /// Opposite-direction input accumulated against a locked swipe; reaching
    /// the threshold here pages back and re-locks the other way.
    pub swipe_against: f32,
    /// Consecutive "dying" deltas seen while a swipe is locked (see
    /// `viewer::SWIPE_DEAD`); the lock releases once the flick's tail has
    /// died out, so the next swipe works immediately.
    pub swipe_decay: u32,
    pub swipe_last: Instant,

    pub fullscreen: bool,

    pub settings_open: bool,
    /// Whether an egui text edit owns the keyboard right now.
    pub text_focused: bool,
    /// The text fields laid out during the current frame; `text_focused` is
    /// recomputed from these at the end of the frame, so a focused grid cell
    /// or toolbar button does not withdraw the menu's key equivalents.
    pub text_field_ids: Vec<egui::Id>,
    pub about_open: bool,
    /// The about panel's icon texture, built once from the shipped
    /// `AppIcon.png` so the fallback panel shows the real bundle artwork
    /// instead of a drawn glyph.
    app_icon: Option<TextureHandle>,
    pub explore_open: bool,
    pub explore_path: PathBuf,
    /// Paths pending the built-in "move to Trash" confirmation; on macOS the
    /// native alert is shown instead and this stays `None`.
    pub confirm_trash: Option<Vec<PathBuf>>,
    pub rename_target: Option<PathBuf>,
    pub rename_text: String,
    pub transfer: Option<Transfer>,
    pub state_dirty: bool,
    pub last_size: Vec2,
    pub last_pos: egui::Pos2,
    pub restore_pos: Option<egui::Pos2>,
    pub restore_frames: u32,
    /// The floating feedback card, when one is on screen.
    pub notice: Option<Notice>,
    /// The batch file operation on its worker thread, when one is running.
    job: Option<RunningJob>,
    pending_system_action: Option<PendingSystemAction>,
    pub(crate) open_with_cache: open_with::OpenWithCache,
    /// The drag-in state machine: a file drag is recognized from the
    /// pasteboard and the physical button state, so the decision lives in a
    /// pure, testable watch rather than in the native glue.
    drop_watch: crate::dnd::DropWatch,
    /// A native drag session this app started is in flight: its payload
    /// also lands on the pasteboard, so the drag-in watch must stay off
    /// while it is.
    drag_out_active: bool,
    /// A held pointer that started on a grid cell: (entry, press position).
    /// Once it moves far enough, a native drag of the file(s) starts.
    drag_out: Option<(usize, egui::Pos2)>,
    /// The entry whose drag-out gesture has crossed the motion threshold;
    /// started by `pump_dnd`, which runs before the grid layout.
    pending_drag: Option<usize>,
    /// (Tests only) the files the most recent drag-out gesture carried, so
    /// the gesture itself can be asserted without a window.
    #[cfg(test)]
    pub last_drag: Option<Vec<PathBuf>>,

    pub path_edit: String,
    pub filter_edit: String,
    last_title: String,
    pub scroll_to_selected: bool,
}

/// A completed directory listing. The generation makes results from a folder
/// left behind by a fast navigation harmless.
struct ListingReply {
    generation: u64,
    entries: Vec<Entry>,
    /// A refresh of the folder already on screen: selection is kept.
    soft: bool,
}

static NEXT_LISTING_GENERATION: AtomicU64 = AtomicU64::new(1);

/// A short, non-modal report about something the app just did. It is shown
/// as a floating card until it expires on its own or the user dismisses it,
/// which is the whole of the app's standing feedback: there is no permanent
/// status bar any more.
#[derive(Clone)]
pub struct Notice {
    /// The one-line headline ("Moved 3 items", "Couldn't move 2 items").
    pub title: String,
    /// What exactly failed and why, when anything did.
    pub detail: Option<String>,
    /// Next steps worth offering ("Show in Finder", "Retry").
    pub actions: Vec<NoticeAction>,
    born: Instant,
}

/// A suggested next step a notice can offer.
#[derive(Clone)]
pub enum NoticeAction {
    /// Reveal the file in Finder so the user can inspect it (macOS).
    Reveal(PathBuf),
    /// Run the failed operation again, on the failed items only.
    Retry(FileOp),
}

/// A batch file operation, kept whole so a failed run can be retried on
/// just the items that failed.
#[derive(Clone)]
pub enum FileOp {
    Transfer {
        paths: Vec<PathBuf>,
        dest: PathBuf,
        copy: bool,
    },
    Trash {
        paths: Vec<PathBuf>,
    },
}

enum PendingSystemAction {
    Share(Vec<PathBuf>),
    Reveal(Vec<PathBuf>),
    OpenWith {
        paths: Vec<PathBuf>,
        application: Option<PathBuf>,
    },
}

/// A batch file operation running on its own thread, so a long copy never
/// freezes the window. The worker reports progress ticks and one final
/// outcome over the channel; `pump_job` drains it every frame.
struct RunningJob {
    /// "Moving" or "Copying".
    prefix: &'static str,
    /// " to the Trash" for trash runs, "" otherwise.
    tail: &'static str,
    /// The operation itself, kept so its failures can be retried.
    op: FileOp,
    total: usize,
    done: usize,
    last_name: String,
    rx: mpsc::Receiver<JobMessage>,
}

/// One report from the file-operation worker.
enum JobMessage {
    /// One more item finished: the running count and the item's name.
    Progress(usize, String),
    /// The whole operation finished.
    Done(OpOutcome),
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, start: Option<PathBuf>) -> Self {
        // X11 panels read `_NET_WM_ICON`; winit wrote only the single 512px
        // size, which they smear down to ~48px. Hand them a size ladder.
        #[cfg(target_os = "linux")]
        {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if let Ok(handle) = cc.window_handle() {
                if let RawWindowHandle::Xlib(xlib) = handle.as_raw() {
                    // X11 window ids are 32-bit; the handle just widens them.
                    let window_id = (xlib.window & 0xFFFF_FFFF) as u32;
                    lumiere_desktop::platform::icon::set_taskbar_icon(
                        window_id,
                        include_bytes!("../../../assets/AppIcon.png"),
                    );
                }
            }
        }
        // eframe creates the AppKit application and its delegate immediately
        // before calling the app factory. Register after that setup: doing it
        // earlier lets AppKit replace the Apple-event handler while starting.
        crate::macos_open::install();
        crate::macos_open::attach_context(cc.egui_ctx.clone());
        Self::build(&cc.egui_ctx, start)
    }

    /// Build the application state; split out from [`Self::new`] so the whole
    /// UI can be exercised head-lessly from tests.
    pub fn build(ctx: &Context, start: Option<PathBuf>) -> Self {
        crate::menu::attach(ctx);
        crate::theme::apply_theme(ctx);
        crate::theme::install_fonts(ctx);
        // Bootstrap Icons are SVGs. Register egui's cached SVG image loader
        // once so toolbar icons stay crisp on Retina displays.
        egui_extras::install_image_loaders(ctx);

        let start_file = start.as_ref().filter(|p| p.is_file()).cloned();
        if !cfg!(test) {
            let _ = imaging::prune_thumbnail_cache(512 * 1024 * 1024);
        }
        let saved = if cfg!(test) {
            crate::state::State::default()
        } else {
            crate::state::load()
        };
        let restore_pos = if saved.x == 0.0 && saved.y == 0.0 {
            None
        } else if crate::window::position_is_on_screen(saved.x, saved.y, saved.width, saved.height)
        {
            Some(egui::pos2(saved.x, saved.y))
        } else {
            // Left behind by a display that is no longer there.
            None
        };
        let dir = start
            .filter(|p| p.is_dir())
            .or_else(|| {
                start_file
                    .as_ref()
                    .and_then(|f| f.parent().map(|p| p.to_path_buf()))
            })
            .or_else(dirs::picture_dir)
            .filter(|p| p.is_dir())
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| PathBuf::from("/"));

        let (listing_tx, listing_rx) = mpsc::channel();
        let mut app = Self {
            mode: Mode::Browser,
            dir: dir.clone(),
            entries: Vec::new(),
            selected: 0,
            multi: HashSet::new(),
            anchor: 0,
            grid_focus: None,
            sidebar_selected: None,
            sidebar_focus: None,
            sidebar_dirs: Vec::new(),
            recent_dirs: Vec::new(),
            caches: Caches::default(),
            loader: Loader::new(4),
            listing_tx,
            listing_rx,
            listing_generation: 0,
            dir_watch: watch::DirWatcher::new(),
            pending_select: start_file.clone().map(|file| (file, false)),
            sort: SortKey::Name,
            sort_reverse: false,
            filter: "*.*".to_owned(),
            show_folders: true,
            thumb_size: 120,
            show_sidebar: true,
            show_inspector: true,
            show_toolbar: true,
            // Off by default: feedback rides the floating notice card, and
            // the legacy three-cell bar is an opt-in from the View menu.
            show_status: false,
            filter_open: false,
            zoom: 1.0,
            fit: true,
            pan: Vec2::ZERO,
            rotation: 0,
            flip_h: false,
            flip_v: false,
            canvas_dark: false,
            slideshow: false,
            slideshow_delay: 3.0,
            slideshow_at: Instant::now(),
            // Full screen by default, the way a show is expected to run;
            // the user can opt out from Settings ▸ Slideshow.
            slideshow_fullscreen: true,
            slideshow_saved: None,
            swipe_x: 0.0,
            swipe_against: 0.0,
            swipe_decay: 0,
            swipe_last: Instant::now(),
            fullscreen: false,
            settings_open: false,
            text_focused: false,
            text_field_ids: Vec::new(),
            about_open: false,
            app_icon: None,
            explore_open: false,
            explore_path: dir.clone(),
            confirm_trash: None,
            rename_target: None,
            rename_text: String::new(),
            transfer: None,
            state_dirty: false,
            last_size: Vec2::ZERO,
            last_pos: egui::Pos2::default(),
            restore_pos,
            restore_frames: 0,
            notice: None,
            job: None,
            pending_system_action: None,
            open_with_cache: open_with::OpenWithCache::default(),
            drop_watch: crate::dnd::DropWatch::new(),
            drag_out_active: false,
            drag_out: None,
            pending_drag: None,
            #[cfg(test)]
            last_drag: None,
            path_edit: dir.display().to_string(),
            filter_edit: "*.*".to_owned(),
            last_title: String::new(),
            scroll_to_selected: false,
        };
        apply_preferences(&mut app, &saved);
        app.rescan();
        app
    }

    // -- directory handling ------------------------------------------------

    pub fn rescan(&mut self) {
        let generation = NEXT_LISTING_GENERATION.fetch_add(1, Ordering::Relaxed);
        self.listing_generation = generation;
        self.entries.clear();
        self.selected = 0;
        self.anchor = 0;
        self.multi.clear();
        self.path_edit = self.dir.display().to_string();
        let (_, dropped) = self.loader.bump_generation();
        self.caches.abandon(&dropped);

        let dir = self.dir.clone();
        let show_folders = self.show_folders;
        let filter = self.filter.clone();
        if cfg!(test) {
            self.apply_listing(fsmodel::read_directory(&dir, show_folders, &filter), false);
        } else {
            // Listings can involve thousands of metadata calls on networked
            // volumes. Keep that work out of egui's frame loop.
            let tx = self.listing_tx.clone();
            std::thread::spawn(move || {
                let entries = fsmodel::read_directory(&dir, show_folders, &filter);
                let _ = tx.send(ListingReply {
                    generation,
                    entries,
                    soft: false,
                });
            });
        }
    }

    /// List the folder again after it changed on disk, keeping the selection
    /// and the cached thumbnails (unlike `rescan`, which starts over).
    fn refresh_listing(&mut self) {
        let generation = NEXT_LISTING_GENERATION.fetch_add(1, Ordering::Relaxed);
        self.listing_generation = generation;
        let dir = self.dir.clone();
        let show_folders = self.show_folders;
        let filter = self.filter.clone();
        if cfg!(test) {
            self.apply_listing(fsmodel::read_directory(&dir, show_folders, &filter), true);
        } else {
            let tx = self.listing_tx.clone();
            std::thread::spawn(move || {
                let entries = fsmodel::read_directory(&dir, show_folders, &filter);
                let _ = tx.send(ListingReply {
                    generation,
                    entries,
                    soft: true,
                });
            });
        }
    }

    fn pump_listing(&mut self) {
        while let Ok(reply) = self.listing_rx.try_recv() {
            if reply.generation == self.listing_generation {
                self.apply_listing(reply.entries, reply.soft);
            }
        }
    }

    fn apply_listing(&mut self, mut entries: Vec<Entry>, soft: bool) {
        fsmodel::sort_entries(&mut entries, self.sort, self.sort_reverse);
        // A refresh keeps what the user has selected, found again by path
        // since additions and removals shift the indices.
        let kept = soft.then(|| {
            let path_at = |i: usize| self.entries.get(i).map(|e| e.path.clone());
            let multi: Vec<PathBuf> = self.multi.iter().filter_map(|&i| path_at(i)).collect();
            (path_at(self.selected), multi)
        });
        self.entries = entries;
        if let Some((selected, multi)) = kept {
            let find = |path: &PathBuf| self.entries.iter().position(|e| &e.path == path);
            let found = selected.as_ref().and_then(find);
            self.multi = multi.iter().filter_map(find).collect();
            self.selected =
                found.unwrap_or(self.selected.min(self.entries.len().saturating_sub(1)));
            self.anchor = self.selected;
            if found.is_none() && self.mode == Mode::Viewer {
                // The picture on screen vanished: this is a different image.
                self.fit = true;
                self.zoom = 1.0;
                self.pan = Vec2::ZERO;
            }
        }
        if self
            .pending_select
            .as_ref()
            .is_some_and(|(path, _)| path.parent() == Some(self.dir.as_path()))
        {
            let (path, open_viewer) = self
                .pending_select
                .take()
                .expect("pending selection exists");
            if let Some(index) = self.entries.iter().position(|entry| entry.path == path) {
                self.selected = index;
                self.anchor = index;
                self.multi.insert(index);
                self.scroll_to_selected = true;
                if open_viewer {
                    self.mode = Mode::Viewer;
                    let dropped = self.loader.purge_thumbs();
                    self.caches.abandon(&dropped);
                    self.fit = true;
                    self.zoom = 1.0;
                    self.pan = Vec2::ZERO;
                }
            }
        }
    }

    pub fn set_dir(&mut self, dir: PathBuf) {
        if !dir.is_dir() {
            return;
        }
        self.pending_select = None;
        self.dir = dir;
        self.selected = 0;
        self.mode = Mode::Browser;
        self.state_dirty = true;
        self.rescan();
        // The sidebar's Recents: most recent first, no duplicates, capped.
        self.recent_dirs.retain(|p| p != &self.dir);
        self.recent_dirs.insert(0, self.dir.clone());
        self.recent_dirs.truncate(5);
    }

    /// Open a document received from Finder/LaunchServices in the viewer.
    /// This mirrors pressing Enter on the selected thumbnail.
    pub fn open_document(&mut self, file: PathBuf) {
        if !file.is_file() {
            return;
        }
        let Some(parent) = file.parent().map(Path::to_path_buf) else {
            return;
        };
        self.set_dir(parent);
        self.pending_select = Some((file, true));
        // Tests apply listings synchronously; in the application the pending
        // selection is resolved by `pump_listing` on the next frame.
        if cfg!(test) {
            self.apply_listing(self.entries.clone(), false);
        }
    }

    /// Pin a folder into the sidebar's Locations section.
    pub fn sidebar_add(&mut self, path: PathBuf) {
        if path.is_dir() && !self.sidebar_dirs.contains(&path) {
            self.sidebar_dirs.push(path);
            self.state_dirty = true;
        }
    }

    /// Unpin a folder from the sidebar's Locations section.
    pub fn sidebar_remove(&mut self, path: &Path) {
        if let Some(i) = self.sidebar_dirs.iter().position(|p| p == path) {
            self.sidebar_dirs.remove(i);
            self.state_dirty = true;
        }
    }

    pub fn image_indices(&self) -> Vec<usize> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.is_image)
            .map(|(i, _)| i)
            .collect()
    }

    pub fn current(&self) -> Option<&Entry> {
        self.entries.get(self.selected)
    }

    pub fn current_image_path(&self) -> Option<PathBuf> {
        self.current()
            .filter(|e| e.is_image)
            .map(|e| e.path.clone())
    }

    /// Move the selection to the next/previous image entry, wrapping.
    /// Decode the images on either side of the one being viewed, so stepping
    /// through a folder does not stall waiting for the next full decode.
    pub fn prefetch_neighbours(&mut self) {
        if self.mode != Mode::Viewer {
            return;
        }
        let indices = self.image_indices();
        if indices.len() < 2 {
            return;
        }
        let position = indices
            .iter()
            .position(|&i| i == self.selected)
            .unwrap_or(0);
        let count = indices.len();
        let mut neighbours = Vec::with_capacity(2);
        for delta in [1isize, -1] {
            let next = (position as isize + delta + count as isize) as usize % count;
            neighbours.push(self.entries[indices[next]].path.clone());
        }
        for path in neighbours {
            if self.caches.full(&path).is_none() && !self.caches.full_failed(&path) {
                let info = self.caches.info(&path);
                if (info.width as u64) * (info.height as u64) > MAX_PREFETCH_PIXELS {
                    continue;
                }
                self.caches.prefetch(&self.loader, Kind::Full, &path, 0);
            }
        }
    }

    pub fn step_image(&mut self, delta: isize) -> Option<PathBuf> {
        let indices = self.image_indices();
        if indices.is_empty() {
            return None;
        }
        let position = indices
            .iter()
            .position(|&i| i == self.selected)
            .unwrap_or(0);
        let next = if delta > 0 {
            (position + 1) % indices.len()
        } else {
            (position + indices.len() - 1) % indices.len()
        };
        self.selected = indices[next];
        self.multi.clear();
        self.multi.insert(self.selected);
        self.scroll_to_selected = true;
        self.fit = true;
        self.zoom = 1.0;
        self.pan = Vec2::ZERO;
        self.current_image_path()
    }

    pub fn ensure_image_selected(&mut self) {
        if self.current_image_path().is_some() {
            return;
        }
        if let Some(&first) = self.image_indices().first() {
            self.selected = first;
            self.anchor = first;
            self.multi.clear();
            self.multi.insert(first);
            self.scroll_to_selected = true;
        }
    }

    pub fn jump_image(&mut self, first: bool) {
        let indices = self.image_indices();
        if indices.is_empty() {
            return;
        }
        self.selected = if first {
            indices[0]
        } else {
            indices[indices.len() - 1]
        };
        self.multi.clear();
        self.multi.insert(self.selected);
        self.scroll_to_selected = true;
        self.fit = true;
        self.pan = Vec2::ZERO;
    }

    /// Report an event briefly and non-modally as the floating card.
    pub fn notify(&mut self, message: impl Into<String>) {
        self.set_notice(message.into(), None, Vec::new());
    }

    /// The files/folders a batch operation acts on: the multi-selection, in
    /// grid order, or the current entry when nothing is selected.
    pub(crate) fn selection_paths(&self) -> Vec<PathBuf> {
        let mut indices: Vec<usize> = self.multi.iter().copied().collect();
        indices.sort_unstable();
        let mut paths: Vec<PathBuf> = indices
            .iter()
            .filter_map(|&index| self.entries.get(index))
            .map(|entry| entry.path.clone())
            .collect();
        if paths.is_empty() {
            if let Some(entry) = self.current() {
                paths.push(entry.path.clone());
            }
        }
        paths
    }

    // -- macOS file workflow: share, reveal, open with, drag ---------------

    /// The Share item: show the system share picker for `paths`.
    pub fn share_selection(&mut self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        if !crate::panels::share_paths(&paths) {
            self.notify("Sharing is unavailable");
        }
    }

    pub(crate) fn defer_share_selection(&mut self, paths: Vec<PathBuf>, ctx: &Context) {
        if !paths.is_empty() {
            self.pending_system_action = Some(PendingSystemAction::Share(paths));
            ctx.request_repaint();
        }
    }

    pub(crate) fn defer_open_with(
        &mut self,
        paths: Vec<PathBuf>,
        application: Option<PathBuf>,
        ctx: &Context,
    ) {
        self.pending_system_action = Some(PendingSystemAction::OpenWith { paths, application });
        ctx.request_repaint();
    }

    /// The "Reveal in Finder" item: open a Finder window with the file
    /// selected.
    pub fn reveal_selection(&mut self, paths: Vec<PathBuf>) {
        let Some(first) = paths.first() else {
            return;
        };
        if !crate::panels::reveal_in_finder(first) {
            self.notify("Reveal in Finder is unavailable");
        }
    }

    pub(crate) fn defer_reveal_selection(&mut self, paths: Vec<PathBuf>, ctx: &Context) {
        if !paths.is_empty() {
            self.pending_system_action = Some(PendingSystemAction::Reveal(paths));
            ctx.request_repaint();
        }
    }

    /// Files dropped onto the window: a folder becomes the browsed
    /// directory, an image opens in the viewer (navigating to its folder
    /// first when it comes from outside the one on screen).
    pub fn handle_dropped_paths(&mut self, paths: Vec<PathBuf>) {
        for path in paths {
            if path.is_dir() {
                self.set_dir(path);
                return;
            }
            if !path.is_file() || !imaging::is_image(&path) {
                continue;
            }
            let Some(parent) = path.parent().map(PathBuf::from) else {
                continue;
            };
            self.set_dir(parent);
            // The rescan cleared the selection; put it back on the dropped
            // image and open it, the way a double-click would.
            if let Some(index) = self.entries.iter().position(|e| e.path == path) {
                self.selected = index;
                self.anchor = index;
                self.multi.clear();
                self.multi.insert(index);
                self.mode = Mode::Viewer;
                self.fit = true;
                self.pan = Vec2::ZERO;
                let dropped = self.loader.purge_thumbs();
                self.caches.abandon(&dropped);
            }
            return;
        }
    }

    /// The grid calls this every frame while the primary button is held,
    /// with the raw pointer position. `pressed_here` is true on the frames
    /// where egui still counts the press as a click on `index`, and those
    /// frames are how the gesture's origin is captured: the payload will be
    /// the file under the press, wherever the pointer later travels.
    pub fn drag_out_candidate(&mut self, index: usize, pos: egui::Pos2, pressed_here: bool) {
        // A native session is already running; its own motion must not
        // re-trigger the gesture.
        if self.drag_out_active {
            return;
        }
        if pressed_here {
            // The press (or a pre-threshold wobble) still lands on the cell:
            // remember where the gesture started.
            if self.drag_out.is_none() {
                self.drag_out = Some((index, pos));
            }
        }
        if let Some((i, down)) = self.drag_out {
            if down.distance(pos) >= 6.0 {
                // The grid now keeps entries available during layout. Start
                // within the pointer event frame while AppKit still has the
                // genuine mouse event, rather than waiting for a repaint.
                self.begin_drag_out(i);
            }
        }
    }

    /// Start the native drag session carrying the files at `index` (or the
    /// selection it belongs to).
    fn begin_drag_out(&mut self, index: usize) {
        self.drag_out = None;
        let paths = if self.multi.contains(&index) || self.selected == index {
            self.selection_paths()
        } else {
            self.entries
                .get(index)
                .map(|entry| vec![entry.path.clone()])
                .unwrap_or_default()
        };
        // The preview is square, the size of the cell the drag leaves from.
        let started = crate::dnd::begin_file_drag(&paths, self.thumb_size as f64);
        if started {
            self.drag_out_active = true;
        }
        #[cfg(test)]
        {
            self.last_drag = Some(paths);
        }
    }

    /// Ask for a Move/Copy destination. On macOS a native directory chooser is
    /// shown and the transfer happens as soon as the user picks a folder;
    /// anywhere else the built-in dialog is shown and applied on confirm.
    fn begin_transfer(&mut self, copy: bool) {
        let paths = self.selection_paths();
        if paths.is_empty() {
            return;
        }
        let (title, prompt) = if copy {
            ("Copy to Folder", "Copy")
        } else {
            ("Move to Folder", "Move")
        };
        match crate::panels::choose_directory(title, prompt, Some(&self.dir)) {
            crate::panels::PanelResult::Chose(dest) => {
                self.start_job(FileOp::Transfer { paths, dest, copy })
            }
            crate::panels::PanelResult::Cancelled => {}
            crate::panels::PanelResult::Unavailable => self.start_transfer(copy),
        }
    }

    /// Fallback (non-native) transfer: remember the pending paths and the
    /// destination the user will type, applied when the dialog confirms.
    fn start_transfer(&mut self, copy: bool) {
        let paths = self.selection_paths();
        if paths.is_empty() {
            return;
        }
        self.transfer = Some(Transfer {
            paths,
            dest: self.dir.display().to_string(),
            copy,
        });
    }

    /// Move or copy `paths` into `dest` right here, on the UI thread, and
    /// report the result — the direct form the tests use; the interactive
    /// paths go through `start_job`, which runs the same core on a worker
    /// thread so a long copy never freezes the window.
    #[cfg(test)]
    pub fn apply_transfer(&mut self, paths: &[PathBuf], dest: &Path, copy: bool) {
        let outcome = run_transfer(paths, dest, copy, None);
        self.report_transfer(dest.to_path_buf(), copy, outcome);
    }

    /// The "move to Trash" flow for `paths`: confirm first, then apply. On
    /// macOS the confirmation is a native alert; elsewhere — and in a test
    /// build — the built-in dialog confirms.
    pub fn begin_trash(&mut self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        match crate::alerts::confirm_trash(&paths) {
            crate::alerts::Confirmation::Confirmed => self.start_job(FileOp::Trash { paths }),
            crate::alerts::Confirmation::Cancelled => {}
            crate::alerts::Confirmation::Unavailable => {
                self.confirm_trash = Some(paths);
            }
        }
    }

    /// The image to select once `paths` are gone: the first image after the
    /// last trashed entry, falling back to the nearest one before it.
    fn image_after_trash(&self, paths: &[PathBuf]) -> Option<PathBuf> {
        let last = self.entries.iter().rposition(|e| paths.contains(&e.path))?;
        let survivor = |e: &&Entry| e.is_image && !paths.contains(&e.path);
        self.entries[last + 1..]
            .iter()
            .find(survivor)
            .or_else(|| self.entries[..last].iter().rev().find(survivor))
            .map(|e| e.path.clone())
    }

    /// Move `paths` to the Trash right here (see `apply_transfer`) and
    /// report what happened, item by item.
    #[cfg(test)]
    pub fn apply_trash(&mut self, paths: &[PathBuf]) {
        // Result reporting is unit-tested without mutating the developer's
        // real Trash; native trash integration remains in `run_trash`.
        let outcome = run_trash_with(paths, None, |path| std::fs::remove_file(path));
        self.report_trash(paths, outcome);
    }

    /// Run `op` on its own worker thread: the window keeps responding while
    /// the files move, the floating card reports the progress ("Moving 2 of
    /// 4 — sunset.jpg"), and the finished outcome becomes the result card.
    pub fn start_job(&mut self, op: FileOp) {
        let (prefix, tail) = match &op {
            FileOp::Transfer { copy, .. } => (if *copy { "Copying" } else { "Moving" }, ""),
            FileOp::Trash { .. } => ("Moving", " to the Trash"),
        };
        let total = match &op {
            FileOp::Transfer { paths, .. } | FileOp::Trash { paths } => paths.len(),
        };
        let (tx, rx) = mpsc::channel();
        let worker = op.clone();
        std::thread::spawn(move || {
            let outcome = match &worker {
                FileOp::Transfer { paths, dest, copy } => {
                    run_transfer(paths, dest, *copy, Some(&tx))
                }
                FileOp::Trash { paths } => run_trash(paths, Some(&tx)),
            };
            let _ = tx.send(JobMessage::Done(outcome));
        });
        self.job = Some(RunningJob {
            prefix,
            tail,
            op,
            total,
            done: 0,
            last_name: String::new(),
            rx,
        });
        self.notice = None;
    }

    /// Pick up whatever the file-operation worker reported since the last
    /// frame: a progress tick repaints, a finished outcome becomes the
    /// result card.
    fn pump_job(&mut self, ctx: &Context) {
        let drained: Vec<JobMessage> = match self.job.as_ref() {
            Some(job) => {
                let mut out = Vec::new();
                while let Ok(msg) = job.rx.try_recv() {
                    out.push(msg);
                }
                out
            }
            None => return,
        };
        let mut outcome = None;
        let mut progressed = false;
        for msg in drained {
            match msg {
                JobMessage::Progress(done, name) => {
                    progressed = true;
                    if let Some(job) = self.job.as_mut() {
                        job.done = done;
                        job.last_name = name;
                    }
                }
                JobMessage::Done(done) => outcome = Some(done),
            }
        }
        if progressed {
            ctx.request_repaint();
        }
        if let Some(outcome) = outcome {
            let Some(job) = self.job.take() else {
                return;
            };
            ctx.request_repaint();
            self.finish_op(job.op, outcome);
        }
    }

    /// Watch for files being dragged onto the window, and end drag-out
    /// bookkeeping the moment the button physically comes up.
    ///
    /// A foreign drag delivers no pointer events to this window, so while
    /// the button is held (or a drop is being tracked) the app keeps a slow
    /// repaint going — otherwise an idle window would never sample the
    /// pasteboard and the release would go unobserved.
    fn pump_dnd(&mut self, ctx: &Context) {
        // Prefer the backend's actual drop event whenever it is available.
        // Reading it explicitly also avoids relying on pasteboard polling
        // for quick drags whose press and release occur between frames.
        let native_paths = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .filter_map(|file| file.path.clone())
                .collect::<Vec<_>>()
        });
        if !native_paths.is_empty() {
            self.drop_watch = crate::dnd::DropWatch::new();
            self.handle_dropped_paths(native_paths);
            return;
        }
        // A release of the primary button, however it reached us, ends the
        // pending drag-out gesture (a click is not a drag).
        if !ctx.input(|i| i.pointer.primary_down()) {
            self.drag_out = None;
            self.pending_drag = None;
        } else if let Some(index) = self.pending_drag.take() {
            // The gesture crossed the threshold during the last layout; the
            // entry list is whole again, so the session can start now.
            self.begin_drag_out(index);
        }
        let Some(obs) = crate::dnd::observe_drag() else {
            return; // no native environment: nothing to watch.
        };
        // An idle egui window otherwise never samples the beginning of an
        // external drag. Poll before it arrives as well as while it is held.
        ctx.request_repaint_after(Duration::from_millis(100));
        if !obs.button_down {
            // Any release — native or egui — ends both our own drag session
            // and a pending drag-out candidate.
            self.drag_out_active = false;
            self.drag_out = None;
            self.pending_drag = None;
        }
        if self.drag_out_active {
            // Our own session owns the drag: the payload is what we put on
            // the pasteboard ourselves, so the drop watch stays idle —
            // otherwise the session's own drop would be ingested again.
            return;
        }
        let dropped = self
            .drop_watch
            .tick(Some(obs.count), obs.button_down, obs.cursor_in_window);
        if obs.button_down || self.drop_watch.in_flight() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if dropped {
            let paths = crate::dnd::pasteboard_file_paths();
            if !paths.is_empty() {
                self.handle_dropped_paths(paths);
            }
        }
    }

    fn finish_op(&mut self, op: FileOp, outcome: OpOutcome) {
        match op {
            FileOp::Transfer { dest, copy, .. } => self.report_transfer(dest, copy, outcome),
            FileOp::Trash { paths } => self.report_trash(&paths, outcome),
        }
    }

    /// The floating-card report of a finished transfer: a headline, what
    /// failed (if anything), and the next steps worth offering.
    fn report_transfer(&mut self, dest: PathBuf, copy: bool, outcome: OpOutcome) {
        if outcome.moved > 0 {
            self.multi.clear();
            self.rescan();
        }
        let total = outcome.moved + outcome.failures.len();
        if total == 0 {
            // Every item already lived in that folder: there was nothing to do.
            return;
        }
        let verb = if copy { "Copied" } else { "Moved" };
        if outcome.failures.is_empty() {
            self.set_notice(format!("{verb} {total} item(s)"), None, Vec::new());
            return;
        }
        let title = if outcome.moved == 0 {
            format!(
                "Couldn't {} {total} item(s)",
                if copy { "copy" } else { "move" }
            )
        } else {
            format!("{verb} {} of {total} items", outcome.moved)
        };
        let retry = FileOp::Transfer {
            paths: outcome
                .failures
                .iter()
                .map(|(path, _)| path.clone())
                .collect(),
            dest,
            copy,
        };
        self.set_notice(
            title,
            Some(failure_detail(&outcome.failures)),
            op_actions(&outcome.failures, retry),
        );
    }

    /// The floating-card report of a finished trash run, keeping the
    /// friendly single-file wording.
    fn report_trash(&mut self, paths: &[PathBuf], outcome: OpOutcome) {
        if outcome.moved > 0 {
            let was_viewing = self.mode == Mode::Viewer;
            // Pick the survivor to land on before the listing is rebuilt: the
            // next image after the trashed ones, else the one before them.
            let next = self.image_after_trash(paths);
            self.multi.clear();
            self.pending_select = next.clone().map(|path| (path, was_viewing));
            self.rescan();
            // A deletion in the viewer shows the next picture. It is a new
            // image, so a zoom/pan carried over from the deleted one would
            // be surprising; `apply_listing` resets them when it lands.
            if was_viewing && next.is_none() {
                self.mode = Mode::Browser;
            }
        }
        let total = outcome.moved + outcome.failures.len();
        if total == 0 {
            return;
        }
        if outcome.failures.is_empty() {
            let title = if total == 1 {
                format!("Moved {} to the Trash", file_name_of(paths.first()))
            } else {
                format!("Moved {total} item(s) to the Trash")
            };
            self.set_notice(title, None, Vec::new());
            return;
        }
        let title = if outcome.moved == 0 {
            if total == 1 {
                format!("Couldn't move {} to the Trash", file_name_of(paths.first()))
            } else {
                format!("Couldn't move {total} items to the Trash")
            }
        } else {
            format!("Moved {} of {total} items to the Trash", outcome.moved)
        };
        let retry = FileOp::Trash {
            paths: outcome
                .failures
                .iter()
                .map(|(path, _)| path.clone())
                .collect(),
        };
        self.set_notice(
            title,
            Some(failure_detail(&outcome.failures)),
            op_actions(&outcome.failures, retry),
        );
    }

    /// Show `title` (and `detail`, when there is one) as the floating card,
    /// offering `actions` as next steps.
    fn set_notice(&mut self, title: String, detail: Option<String>, actions: Vec<NoticeAction>) {
        self.notice = Some(Notice {
            title,
            detail,
            actions,
            born: Instant::now(),
        });
    }

    pub fn apply_rename(&mut self, path: &Path, name: &str) {
        let current = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let name = name.trim();
        if name.is_empty() || name == current {
            return;
        }
        if !file_ops::is_single_file_name(name) {
            self.notify("Rename failed: enter a single file name, not a path");
            return;
        }
        let target = path.with_file_name(name);
        match file_ops::rename_without_replace(path, &target) {
            Ok(()) => {
                self.notify(format!("Renamed to {name}"));
                self.rescan();
                if let Some(index) = self.entries.iter().position(|e| e.path == target) {
                    self.selected = index;
                    self.anchor = index;
                    self.multi.clear();
                    self.multi.insert(index);
                    self.scroll_to_selected = true;
                }
            }
            Err(error) => self.notify(format!("Rename failed: {error}")),
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.draw(ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.loader.shutdown();
    }
}

#[cfg(not(target_os = "macos"))]
fn menu_row(
    ui: &egui::Ui,
    label: &str,
    shortcut: Option<&Shortcut>,
    checked: Option<bool>,
) -> egui::text::LayoutJob {
    // `Visuals` is not `Copy`, so read the two adaptive colours we need.
    let mark_color = ui.ctx().style().visuals.text_color();
    let weak_color = ui.ctx().style().visuals.weak_text_color();
    let mut job = egui::text::LayoutJob::default();
    let mark = egui::TextFormat {
        font_id: egui::FontId::proportional(13.0),
        color: mark_color,
        ..Default::default()
    };
    let prefix = match checked {
        Some(true) => "\u{2713} ",
        Some(false) => "   ",
        None => "",
    };
    job.append(&format!("{prefix}{label}"), 0.0, mark.clone());
    if let Some(shortcut) = shortcut {
        let weak = egui::TextFormat {
            font_id: egui::FontId::proportional(11.0),
            color: weak_color,
            ..Default::default()
        };
        job.append(&format!("    {}", shortcut.display()), 24.0, weak);
    }
    job
}

pub fn sort_key_name(key: SortKey) -> &'static str {
    match key {
        SortKey::Name => "name",
        SortKey::Date => "date",
        SortKey::Size => "size",
        SortKey::Type => "type",
    }
}

fn sort_key_from(name: &str) -> Option<SortKey> {
    match name {
        "name" => Some(SortKey::Name),
        "date" => Some(SortKey::Date),
        "size" => Some(SortKey::Size),
        "type" => Some(SortKey::Type),
        _ => None,
    }
}

/// Restores the browsing preferences stored by the previous session, so a
/// restart comes back to the same thumbnail size, sort order and filter.
fn apply_preferences(app: &mut App, saved: &crate::state::State) {
    if saved.thumb_size > 0 {
        app.thumb_size = saved.thumb_size;
    }
    if let Some(key) = sort_key_from(saved.sort.as_str()) {
        app.sort = key;
    }
    app.sort_reverse = saved.sort_reverse;
    if !saved.filter.is_empty() {
        app.filter = saved.filter.clone();
        app.filter_edit = saved.filter.clone();
    }
    if saved.show_folders >= 0 {
        app.show_folders = saved.show_folders == 1;
    }
    if saved.show_sidebar >= 0 {
        app.show_sidebar = saved.show_sidebar == 1;
    }
    if saved.show_inspector >= 0 {
        app.show_inspector = saved.show_inspector == 1;
    }
    if saved.slideshow_fullscreen >= 0 {
        app.slideshow_fullscreen = saved.slideshow_fullscreen == 1;
    }
    app.sidebar_dirs = saved.sidebar_dirs.clone();
    app.recent_dirs = saved.recent_dirs.clone();
}

fn shortcut_modifier_count(shortcut: &Shortcut) -> u32 {
    u32::from(shortcut.command)
        + u32::from(shortcut.control)
        + u32::from(shortcut.option)
        + u32::from(shortcut.shift)
}

fn egui_modifiers(shortcut: Shortcut) -> egui::Modifiers {
    let mut modifiers = egui::Modifiers::NONE;
    if shortcut.command {
        modifiers |= egui::Modifiers::COMMAND;
    }
    if shortcut.control {
        modifiers |= egui::Modifiers::CTRL;
    }
    if shortcut.option {
        modifiers |= egui::Modifiers::ALT;
    }
    if shortcut.shift {
        modifiers |= egui::Modifiers::SHIFT;
    }
    modifiers
}

/// The egui key a menu shortcut character arrives as.
fn egui_key_for_char(c: char) -> Option<egui::Key> {
    match c {
        'a' => Some(egui::Key::A),
        'b' => Some(egui::Key::B),
        'c' => Some(egui::Key::C),
        'd' => Some(egui::Key::D),
        'e' => Some(egui::Key::E),
        'f' => Some(egui::Key::F),
        'g' => Some(egui::Key::G),
        'h' => Some(egui::Key::H),
        'i' => Some(egui::Key::I),
        'j' => Some(egui::Key::J),
        'k' => Some(egui::Key::K),
        'l' => Some(egui::Key::L),
        'm' => Some(egui::Key::M),
        'n' => Some(egui::Key::N),
        'o' => Some(egui::Key::O),
        'p' => Some(egui::Key::P),
        'q' => Some(egui::Key::Q),
        'r' => Some(egui::Key::R),
        's' => Some(egui::Key::S),
        't' => Some(egui::Key::T),
        'u' => Some(egui::Key::U),
        'v' => Some(egui::Key::V),
        'w' => Some(egui::Key::W),
        'x' => Some(egui::Key::X),
        'y' => Some(egui::Key::Y),
        'z' => Some(egui::Key::Z),
        '0' => Some(egui::Key::Num0),
        '1' => Some(egui::Key::Num1),
        '2' => Some(egui::Key::Num2),
        '3' => Some(egui::Key::Num3),
        '4' => Some(egui::Key::Num4),
        '5' => Some(egui::Key::Num5),
        '6' => Some(egui::Key::Num6),
        '7' => Some(egui::Key::Num7),
        '8' => Some(egui::Key::Num8),
        '9' => Some(egui::Key::Num9),
        ',' => Some(egui::Key::Comma),
        '.' => Some(egui::Key::Period),
        '/' => Some(egui::Key::Slash),
        '-' => Some(egui::Key::Minus),
        '+' | '=' => Some(egui::Key::Plus),
        _ => None,
    }
}

/// The egui keys a menu shortcut can arrive as. macOS draws backspace as
/// `\u{232B}`, which winit reports as `Key::Backspace`; `Key::Delete` is the
/// forward delete an external keyboard sends.
fn egui_keys(key: Key) -> Vec<egui::Key> {
    match key {
        Key::Char(c) => egui_key_for_char(c).into_iter().collect(),
        Key::Return => vec![egui::Key::Enter],
        Key::Delete => vec![egui::Key::Backspace, egui::Key::Delete],
    }
}

#[cfg(not(target_os = "macos"))]
fn top_menu(ui: &mut egui::Ui, label: &str, contents: impl FnOnce(&mut egui::Ui)) {
    let response = ui.menu_button(label, contents);
    theme::note_rect(ui, label, response.response.rect);
}

/// Renders one level of the menu description with egui widgets. Only used where
/// the platform has no native menu bar.
#[cfg(not(target_os = "macos"))]
fn menu_items(ui: &mut egui::Ui, items: &[Item], actions: &mut Vec<Action>) {
    for item in items {
        match item {
            Item::Separator => {
                ui.separator();
            }
            Item::Submenu { label, items } => {
                top_menu(ui, label, |ui| menu_items(ui, items, actions));
            }
            Item::Command {
                label,
                shortcut,
                action,
                enabled,
                marked,
            } => {
                let response = ui.add_enabled(
                    *enabled,
                    egui::Button::new(menu_row(ui, label, shortcut.as_ref(), Some(*marked)))
                        .min_size(egui::vec2(240.0, 0.0)),
                );
                // The row a human reads is a tick gutter, the command name and
                // the shortcut glyphs, which is noise for VoiceOver. Say just
                // the command, and offer the shortcut as its own property.
                ui.ctx().accesskit_node_builder(response.id, |node| {
                    node.set_label(label.as_str());
                    if let Some(shortcut) = shortcut {
                        node.set_keyboard_shortcut(shortcut.display());
                    }
                });
                theme::note_rect(ui, label, response.rect);
                if response.clicked() {
                    actions.push(*action);
                    ui.close();
                }
            }
        }
    }
}

/// Move or copy `paths` into `dest`, skipping items that already live
/// there. Pure filesystem work, so it is safe on the worker thread;
/// `progress` gets the running count and each item's name as the work
/// moves along.
fn run_transfer(
    paths: &[PathBuf],
    dest: &Path,
    copy: bool,
    progress: Option<&mpsc::Sender<JobMessage>>,
) -> OpOutcome {
    let mut report = |index, name| {
        if let Some(tx) = progress {
            let _ = tx.send(JobMessage::Progress(index, name));
        }
    };
    file_ops::transfer(paths, dest, copy, &mut report)
}

/// Move `paths` to the Trash with the `trash` crate — the items stay in the
/// Finder's Trash, so the operation is recoverable. Pure filesystem work,
/// so it is safe on the worker thread.
fn run_trash(paths: &[PathBuf], progress: Option<&mpsc::Sender<JobMessage>>) -> OpOutcome {
    run_trash_all_with(paths, progress, |paths| file_ops::trash_delete_all(paths))
}

fn run_trash_all_with<F, E>(
    paths: &[PathBuf],
    progress: Option<&mpsc::Sender<JobMessage>>,
    delete_all: F,
) -> OpOutcome
where
    F: FnOnce(&[PathBuf]) -> Result<(), E>,
    E: std::fmt::Display,
{
    let mut report = |index, name| {
        if let Some(tx) = progress {
            let _ = tx.send(JobMessage::Progress(index, name));
        }
    };
    file_ops::trash_all(paths, &mut report, delete_all)
}

#[cfg(test)]
fn run_trash_with<F, E>(
    paths: &[PathBuf],
    progress: Option<&mpsc::Sender<JobMessage>>,
    delete: F,
) -> OpOutcome
where
    F: FnMut(&Path) -> Result<(), E>,
    E: std::fmt::Display,
{
    let mut report = |index, name| {
        if let Some(tx) = progress {
            let _ = tx.send(JobMessage::Progress(index, name));
        }
    };
    file_ops::trash(paths, &mut report, delete)
}

/// The failed items, as a line the user can act on: "name: error", a few at
/// most, then how many more.
fn failure_detail(failures: &[(PathBuf, String)]) -> String {
    let mut parts: Vec<String> = failures
        .iter()
        .take(3)
        .map(|(path, error)| format!("{}: {}", file_name_of(Some(path)), error))
        .collect();
    let extra = failures.len().saturating_sub(3);
    if extra > 0 {
        parts.push(format!("and {extra} more"));
    }
    parts.join(", ")
}

/// The buttons a failed operation earns: look at the file in Finder, and
/// try the operation again on the items that failed.
fn op_actions(failures: &[(PathBuf, String)], retry: FileOp) -> Vec<NoticeAction> {
    let mut actions = Vec::new();
    if let Some((path, _)) = failures.first() {
        actions.push(NoticeAction::Reveal(path.clone()));
    }
    actions.push(NoticeAction::Retry(retry));
    actions
}

/// The file's name for a message, or its full path when it has none.
fn file_name_of(path: Option<&PathBuf>) -> String {
    match path {
        Some(path) => path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string()),
        None => String::new(),
    }
}

/// The backdrop behind the image in the viewer: black when asked for or in
/// full screen, otherwise a neutral grey that suits whichever appearance the
/// system is currently in.
pub fn canvas_color(app: &App, ctx: &Context) -> egui::Color32 {
    if app.canvas_dark || app.fullscreen {
        egui::Color32::from_gray(0)
    } else if ctx.style().visuals.dark_mode {
        egui::Color32::from_gray(0x26)
    } else {
        egui::Color32::from_gray(0x80)
    }
}

#[cfg(test)]
mod tests;

pub use lumiere_core::command::{Action, Mode};
