//! Application state, window chrome (menu bar, toolbars, status bar) and the
//! command dispatch used by menus, toolbar buttons and the keyboard alike.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui::{Context, TextureHandle, TextureOptions, Vec2};

use crate::fsmodel::{self, Entry, SortKey, TreeModel};
use crate::imaging::{self, Info};
use crate::loader::{Kind, Loader, Request};
use crate::menu::{Item, Key, Shortcut};
use crate::theme::{self, Icon};

pub const APP_TITLE: &str = "Lumiere";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// Three pane browser (the classic "Browser" window).
    Browser,
    /// Single image viewer.
    Viewer,
}

pub struct ThumbEntry {
    pub texture: TextureHandle,
    pub width: u32,
    pub height: u32,
}

pub struct FullEntry {
    pub texture: TextureHandle,
    pub width: u32,
    pub height: u32,
    /// Bytes of the uploaded texture (clamped), not of the decoded original.
    pub bytes: usize,
    pub info: Info,
}

/// Pending "Move to Folder" / "Copy to Folder" dialog state.
#[derive(Clone, Debug)]
pub struct Transfer {
    pub paths: Vec<PathBuf>,
    pub dest: String,
    pub copy: bool,
}

#[derive(Default)]
pub struct Caches {
    thumbs: HashMap<(PathBuf, u32), ThumbEntry>,
    thumbs_order: VecDeque<(PathBuf, u32)>,
    fulls: HashMap<PathBuf, FullEntry>,
    fulls_order: VecDeque<PathBuf>,
    infos: HashMap<PathBuf, Info>,
    thumb_bytes: usize,
    full_bytes: usize,
    pub pending: HashSet<(Kind, PathBuf, u32)>,
    /// Full-size decodes that failed: re-requesting them every frame would put
    /// the viewer into the same Loading-flicker loop as the eviction bug.
    failed_fulls: HashSet<PathBuf>,
}

const MAX_THUMBS: usize = 4000;
const MAX_FULLS: usize = 6;
/// Resident RGBA budgets, so a folder with thousands of images cannot grow the
/// texture cache without bound.
const MAX_THUMB_BYTES: usize = 96 * 1024 * 1024;
const MAX_FULL_BYTES: usize = 192 * 1024 * 1024;
/// Longest side of an uploaded full-size texture (~144 MB RGBA at the limit).
const MAX_FULL_TEXTURE_SIDE: u32 = 6000;
/// Neighbour prefetch skips pictures whose decode would dwarf the budget.
const MAX_PREFETCH_PIXELS: u64 = 50_000_000;

fn rgba_bytes(width: u32, height: u32) -> usize {
    width as usize * height as usize * 4
}

impl Caches {
    pub fn thumb(&self, path: &Path, size: u32) -> Option<&ThumbEntry> {
        self.thumbs.get(&(path.to_path_buf(), size))
    }

    pub fn full(&self, path: &Path) -> Option<&FullEntry> {
        self.fulls.get(path)
    }

    pub fn full_failed(&self, path: &Path) -> bool {
        self.failed_fulls.contains(path)
    }

    /// Forget failed decodes, e.g. after a refresh: the file may have changed.
    pub fn clear_failures(&mut self) {
        self.failed_fulls.clear();
    }

    /// Format + EXIF without decoding pixels, cached because the preview pane
    /// asks for it every frame.
    pub fn info(&mut self, path: &Path) -> Info {
        if let Some(info) = self.infos.get(path) {
            return info.clone();
        }
        let info = imaging::info(path);
        self.infos.insert(path.to_path_buf(), info.clone());
        info
    }

    /// Requests dropped by the loader never come back as replies, so their
    /// pending markers must be released or the paths are never decoded again.
    pub fn abandon(&mut self, dropped: &[Request]) {
        for request in dropped {
            self.pending
                .remove(&(request.kind, request.path.clone(), request.size));
        }
    }

    pub fn request(&mut self, loader: &Loader, kind: Kind, path: &Path, size: u32) {
        self.queue_work(loader, kind, path, size, false);
    }

    /// Same de-duplication as [`Self::request`], but the work goes to the back
    /// of the queue: used for images that are not on screen yet.
    pub fn prefetch(&mut self, loader: &Loader, kind: Kind, path: &Path, size: u32) {
        self.queue_work(loader, kind, path, size, true);
    }

    fn queue_work(&mut self, loader: &Loader, kind: Kind, path: &Path, size: u32, back: bool) {
        let key = (kind, path.to_path_buf(), size);
        if self.pending.contains(&key) {
            return;
        }
        if self.pending.len() > 512 {
            return;
        }
        if back {
            loader.submit_prefetch(kind, path.to_path_buf(), size);
        } else {
            loader.submit(kind, path.to_path_buf(), size);
        }
        self.pending.insert(key);
    }

    /// Move a full-size entry to the back of the LRU so the image actually on
    /// screen is never the one evicted by `trim` — otherwise the viewer and its
    /// neighbour prefetch chase each other and the picture flickers.
    pub fn protect_full(&mut self, path: &Path) {
        if !self.fulls.contains_key(path) {
            return;
        }
        if let Some(at) = self.fulls_order.iter().position(|k| k == path) {
            let key = self.fulls_order.remove(at).expect("position found");
            self.fulls_order.push_back(key);
        }
    }

    /// Move finished decodes into the texture caches. Returns how many arrived.
    /// `protect` is the picture currently on screen: it is moved to the back of
    /// the LRU before eviction so a prefetch landing behind it cannot push the
    /// displayed image out (the classic viewer flicker).
    pub fn pump(&mut self, ctx: &Context, loader: &Loader, protect: Option<&Path>) -> usize {
        let replies = loader.drain(24);
        let arrived = replies.len();
        for reply in replies {
            let request = reply.request;
            self.pending
                .remove(&(request.kind, request.path.clone(), request.size));
            let built = match reply.result {
                Ok(decoded) => {
                    // Never hand the GPU a texture bigger than it advertises:
                    // an oversized upload fails, the cache entry never sticks and
                    // the viewer re-requests the same picture every frame.
                    let max_side = ctx.input(|i| i.max_texture_side).max(512) as u32;
                    // A full-size texture also has to fit the resident byte
                    // budget, or one panorama evicts everything and thrashes.
                    let cap = match request.kind {
                        Kind::Full => max_side.min(MAX_FULL_TEXTURE_SIDE),
                        _ => max_side,
                    };
                    let upload = decoded.scaled(cap, cap);
                    let image = if upload.premultiplied {
                        egui::ColorImage::from_rgba_premultiplied(
                            [upload.width as usize, upload.height as usize],
                            &upload.rgba,
                        )
                    } else {
                        egui::ColorImage::from_rgba_unmultiplied(
                            [upload.width as usize, upload.height as usize],
                            &upload.rgba,
                        )
                    };
                    let name = format!(
                        "{:?}:{}",
                        request.kind,
                        request
                            .path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("")
                    );
                    let options = match request.kind {
                        Kind::Thumb => TextureOptions::LINEAR,
                        Kind::Preview => TextureOptions::LINEAR,
                        Kind::Full => TextureOptions::LINEAR,
                    };
                    let texture = ctx.load_texture(name, image, options);
                    Some((
                        texture,
                        upload.width,
                        upload.height,
                        decoded.width,
                        decoded.height,
                    ))
                }
                Err(error) => {
                    log::warn!("{}: {error}", request.path.display());
                    if request.kind == Kind::Full {
                        self.failed_fulls.insert(request.path.clone());
                    }
                    None
                }
            };
            let Some((texture, tex_w, tex_h, width, height)) = built else {
                continue;
            };
            match request.kind {
                Kind::Thumb | Kind::Preview => {
                    let key = (request.path.clone(), request.size);
                    if let Some(old) = self.thumbs.remove(&key) {
                        self.thumb_bytes -= rgba_bytes(old.width, old.height);
                        if let Some(at) = self.thumbs_order.iter().position(|k| *k == key) {
                            self.thumbs_order.remove(at);
                        }
                    }
                    self.thumb_bytes += rgba_bytes(tex_w, tex_h);
                    self.thumbs_order.push_back(key.clone());
                    self.thumbs.insert(
                        key,
                        ThumbEntry {
                            texture,
                            width,
                            height,
                        },
                    );
                }
                Kind::Full => {
                    let info = imaging::info(&request.path);
                    let key = request.path.clone();
                    self.failed_fulls.remove(&key);
                    if let Some(old) = self.fulls.remove(&key) {
                        self.full_bytes -= old.bytes;
                        if let Some(at) = self.fulls_order.iter().position(|k| *k == key) {
                            self.fulls_order.remove(at);
                        }
                    }
                    let bytes = rgba_bytes(tex_w, tex_h);
                    self.full_bytes += bytes;
                    self.fulls_order.push_back(key.clone());
                    self.fulls.insert(
                        key,
                        FullEntry {
                            texture,
                            width,
                            height,
                            bytes,
                            info,
                        },
                    );
                }
            }
        }
        self.trim(protect);
        arrived
    }

    fn trim(&mut self, protect: Option<&Path>) {
        while self.thumbs_order.len() > MAX_THUMBS || self.thumb_bytes > MAX_THUMB_BYTES {
            let Some(key) = self.thumbs_order.pop_front() else {
                break;
            };
            if let Some(old) = self.thumbs.remove(&key) {
                self.thumb_bytes -= rgba_bytes(old.width, old.height);
            }
        }
        // The displayed picture goes to the back of the LRU *before* eviction:
        // arrivals queued behind it must be the ones that fall out.
        if let Some(path) = protect {
            if let Some(at) = self.fulls_order.iter().position(|k| k == path) {
                let key = self.fulls_order.remove(at).expect("position found");
                self.fulls_order.push_back(key);
            }
        }
        while self.fulls_order.len() > 1
            && (self.fulls_order.len() > MAX_FULLS || self.full_bytes > MAX_FULL_BYTES)
        {
            let Some(key) = self.fulls_order.pop_front() else {
                break;
            };
            if let Some(old) = self.fulls.remove(&key) {
                self.full_bytes -= old.bytes;
            }
        }
    }

    pub fn clear_thumbs(&mut self) {
        self.thumbs.clear();
        self.thumbs_order.clear();
        self.thumb_bytes = 0;
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    /// Native folder chooser (macOS) / built-in browser elsewhere.
    OpenFolder,
    /// Hand the current file to the system's default application.
    OpenWithDefault,
    UpFolder,
    GoHome,
    GoPictures,
    Reload,
    MoveToTrash,
    GetInfo,
    Rename,
    MoveToFolder,
    CopyToFolder,
    Quit,

    SelectAll,
    SelectNone,
    InvertSelection,

    NextImage,
    PrevImage,
    FirstImage,
    LastImage,
    OpenViewer,
    CloseViewer,

    ZoomIn,
    ZoomOut,
    FitWindow,
    ActualSize,
    RotateCW,
    RotateCCW,
    FlipH,
    FlipV,

    Slideshow,
    StopSlideshow,
    ToggleFullscreen,

    ThumbSmall,
    ThumbMedium,
    ThumbLarge,
    SortByName,
    SortByDate,
    SortBySize,
    SortByType,
    ToggleReverse,
    ToggleFolders,
    ToggleSidebar,
    ToggleInspector,
    ToggleToolbar,
    ToggleStatus,

    Settings,
    About,
}

pub struct App {
    pub mode: Mode,
    pub dir: PathBuf,
    pub entries: Vec<Entry>,
    pub selected: usize,
    pub multi: HashSet<usize>,
    pub anchor: usize,

    pub tree: TreeModel,
    pub tree_selected: Option<usize>,

    pub caches: Caches,
    pub loader: Loader,

    pub sort: SortKey,
    pub sort_reverse: bool,
    pub filter: String,
    pub show_folders: bool,
    pub thumb_size: u32,

    pub show_sidebar: bool,
    pub show_inspector: bool,
    pub show_toolbar: bool,
    pub show_places: bool,
    pub show_status: bool,

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

    pub fullscreen: bool,

    pub settings_open: bool,
    /// Whether an egui text edit owns the keyboard right now.
    pub text_focused: bool,
    pub about_open: bool,
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
    pub toast: Option<(String, Instant)>,

    pub path_edit: String,
    pub filter_edit: String,
    pub status_override: Option<String>,
    last_title: String,
    pub scroll_to_selected: bool,
}

impl Item {
    /// Applies an enabled state to an already-built item.
    fn with_enabled(self, enabled: bool) -> Self {
        let mut item = self;
        if let Item::Command { enabled: slot, .. } = &mut item {
            *slot = enabled;
        }
        item
    }
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, start: Option<PathBuf>) -> Self {
        Self::build(&cc.egui_ctx, start)
    }

    /// Build the application state; split out from [`Self::new`] so the whole
    /// UI can be exercised head-lessly from tests.
    pub fn build(ctx: &Context, start: Option<PathBuf>) -> Self {
        crate::menu::attach(ctx);
        crate::theme::apply_theme(ctx);
        crate::theme::install_fonts(ctx);

        let start_file = start.as_ref().filter(|p| p.is_file()).cloned();
        if !cfg!(test) {
            let _ = imaging::prune_thumbnail_cache(512 * 1024 * 1024);
        }
        let saved = if cfg!(test) {
            crate::state::State::default()
        } else {
            crate::state::State::load()
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

        let mut app = Self {
            mode: Mode::Browser,
            dir: dir.clone(),
            entries: Vec::new(),
            selected: 0,
            multi: HashSet::new(),
            anchor: 0,
            tree: TreeModel::new(),
            tree_selected: None,
            caches: Caches::default(),
            loader: Loader::new(4),
            sort: SortKey::Name,
            sort_reverse: false,
            filter: "*.*".to_owned(),
            show_folders: true,
            thumb_size: 120,
            show_sidebar: true,
            show_inspector: true,
            show_toolbar: true,
            show_places: true,
            show_status: true,
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
            fullscreen: false,
            settings_open: false,
            text_focused: false,
            about_open: false,
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
            toast: None,
            path_edit: dir.display().to_string(),
            filter_edit: "*.*".to_owned(),
            status_override: None,
            last_title: String::new(),
            scroll_to_selected: false,
        };
        app.tree.reveal(&app.dir.clone());
        apply_preferences(&mut app, &saved);
        app.rescan();
        if let Some(file) = start_file {
            if let Some(index) = app.entries.iter().position(|e| e.path == file) {
                app.selected = index;
                app.anchor = index;
                app.multi.insert(index);
                app.scroll_to_selected = true;
            }
        }
        app
    }

    // -- directory handling ------------------------------------------------

    pub fn rescan(&mut self) {
        self.entries = fsmodel::read_directory(&self.dir, self.show_folders, &self.filter);
        fsmodel::sort_entries(&mut self.entries, self.sort, self.sort_reverse);
        if self.selected >= self.entries.len() {
            self.selected = self.entries.len().saturating_sub(1);
        }
        self.anchor = self.selected;
        self.multi.clear();
        self.path_edit = self.dir.display().to_string();
        let (_, dropped) = self.loader.bump_generation();
        self.caches.abandon(&dropped);
    }

    pub fn set_dir(&mut self, dir: PathBuf) {
        if !dir.is_dir() {
            return;
        }
        self.dir = dir;
        self.selected = 0;
        self.mode = Mode::Browser;
        self.state_dirty = true;
        self.rescan();
        self.tree.reveal(&self.dir.clone());
        self.tree_selected = self.tree.find(&self.dir.clone());
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

    pub fn notify(&mut self, message: impl Into<String>) {
        self.toast = Some((message.into(), Instant::now()));
    }

    // -- command dispatch --------------------------------------------------

    pub fn act(&mut self, action: Action, ctx: &Context) {
        match action {
            Action::OpenFolder => {
                match crate::panels::choose_directory("Choose a Folder", "Choose", Some(&self.dir))
                {
                    crate::panels::PanelResult::Chose(path) => self.set_dir(path),
                    crate::panels::PanelResult::Cancelled => {}
                    crate::panels::PanelResult::Unavailable => {
                        // No native panel (other OS, or a test build): the built-in
                        // folder browser.
                        self.explore_open = true;
                        self.explore_path = self.dir.clone();
                    }
                }
            }
            Action::OpenWithDefault => {
                if let Some(path) = self.current_image_path() {
                    let _ = std::process::Command::new("open").arg(&path).spawn();
                }
            }
            Action::UpFolder => {
                if let Some(parent) = self.dir.parent().map(|p| p.to_path_buf()) {
                    self.set_dir(parent);
                }
            }
            Action::GoHome => {
                if let Some(home) = dirs::home_dir() {
                    self.set_dir(home);
                }
            }
            Action::GoPictures => {
                if let Some(pics) = dirs::picture_dir() {
                    self.set_dir(pics);
                }
            }
            Action::Reload => {
                self.rescan();
                self.caches.clear_failures();
            }
            Action::MoveToTrash => self.begin_trash(self.selection_paths()),
            Action::Rename => {
                if let Some(entry) = self.current().cloned() {
                    self.rename_text = entry.name.clone();
                    self.rename_target = Some(entry.path.clone());
                }
            }
            Action::MoveToFolder => self.begin_transfer(false),
            Action::CopyToFolder => self.begin_transfer(true),
            Action::GetInfo => self.show_inspector = true,
            Action::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Action::SelectAll => {
                self.multi.clear();
                for (i, entry) in self.entries.iter().enumerate() {
                    if entry.is_image {
                        self.multi.insert(i);
                    }
                }
            }
            Action::SelectNone => self.multi.clear(),
            Action::InvertSelection => {
                let mut next = HashSet::new();
                for (i, entry) in self.entries.iter().enumerate() {
                    if entry.is_image && !self.multi.contains(&i) {
                        next.insert(i);
                    }
                }
                self.multi = next;
            }
            Action::NextImage => {
                if self.mode == Mode::Viewer {
                    let dropped = self.loader.purge_thumbs();
                    self.caches.abandon(&dropped);
                }
                self.step_image(1);
            }
            Action::PrevImage => {
                if self.mode == Mode::Viewer {
                    let dropped = self.loader.purge_thumbs();
                    self.caches.abandon(&dropped);
                }
                self.step_image(-1);
            }
            Action::FirstImage => self.jump_image(true),
            Action::LastImage => self.jump_image(false),
            Action::OpenViewer => {
                self.ensure_image_selected();
                if self.current_image_path().is_some() {
                    self.mode = Mode::Viewer;
                    let dropped = self.loader.purge_thumbs();
                    self.caches.abandon(&dropped);
                    self.fit = true;
                    self.pan = Vec2::ZERO;
                }
            }
            Action::CloseViewer => {
                self.mode = Mode::Browser;
                self.fullscreen = false;
                self.slideshow = false;
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
            }
            Action::ZoomIn => {
                self.zoom = (self.zoom * 1.25).min(64.0);
                self.fit = false;
            }
            Action::ZoomOut => {
                self.zoom = (self.zoom / 1.25).max(0.02);
                self.fit = false;
            }
            Action::FitWindow => {
                self.fit = true;
                self.pan = Vec2::ZERO;
            }
            Action::ActualSize => {
                self.zoom = 1.0;
                self.fit = false;
                self.pan = Vec2::ZERO;
            }
            Action::RotateCW => self.rotation = (self.rotation + 90) % 360,
            Action::RotateCCW => self.rotation = (self.rotation + 270) % 360,
            Action::FlipH => self.flip_h = !self.flip_h,
            Action::FlipV => self.flip_v = !self.flip_v,
            Action::Slideshow => {
                if !self.entries.iter().any(|e| e.is_image) {
                    return;
                }
                self.ensure_image_selected();
                self.mode = Mode::Viewer;
                let dropped = self.loader.purge_thumbs();
                self.caches.abandon(&dropped);
                self.slideshow = true;
                self.slideshow_at = Instant::now();
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
                self.fullscreen = true;
            }
            Action::StopSlideshow => {
                self.slideshow = false;
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
                self.fullscreen = false;
            }
            Action::ToggleFullscreen => {
                self.fullscreen = !self.fullscreen;
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
            }
            Action::ThumbSmall => self.thumb_size = 64,
            Action::ThumbMedium => self.thumb_size = 120,
            Action::ThumbLarge => self.thumb_size = 200,
            Action::SortByName => self.sort = SortKey::Name,
            Action::SortByDate => self.sort = SortKey::Date,
            Action::SortBySize => self.sort = SortKey::Size,
            Action::SortByType => self.sort = SortKey::Type,
            Action::ToggleReverse => self.sort_reverse = !self.sort_reverse,
            Action::ToggleFolders => self.show_folders = !self.show_folders,
            Action::ToggleSidebar => self.show_sidebar = !self.show_sidebar,
            Action::ToggleInspector => self.show_inspector = !self.show_inspector,
            Action::ToggleToolbar => self.show_toolbar = !self.show_toolbar,
            Action::ToggleStatus => self.show_status = !self.show_status,
            Action::Settings => self.settings_open = true,
            Action::About => self.about_open = true,
        }
        if matches!(
            action,
            Action::SortByName
                | Action::SortByDate
                | Action::SortBySize
                | Action::SortByType
                | Action::ToggleReverse
                | Action::ToggleFolders
        ) {
            self.rescan();
        }
        if matches!(
            action,
            Action::ThumbSmall
                | Action::ThumbMedium
                | Action::ThumbLarge
                | Action::SortByName
                | Action::SortByDate
                | Action::SortBySize
                | Action::SortByType
                | Action::ToggleReverse
                | Action::ToggleFolders
        ) {
            self.state_dirty = true;
        }
    }

    // -- keyboard ----------------------------------------------------------

    /// Keyboard map.
    ///
    /// On macOS the native menu bar owns the `⌘` shortcuts and AppKit consumes
    /// the key event before egui ever sees it; the `⌘` handling here is what the
    /// in-window fallback menu and the headless tests drive. Both paths read the
    /// same [`Shortcut`] values out of [`Self::menu_spec`], so the menu label and
    /// the working shortcut cannot disagree.
    fn handle_keys(&mut self, ctx: &Context) {
        if ctx.memory(|m| m.focused().is_some()) {
            return;
        }
        let shortcuts = self.shortcuts();
        let mut actions: Vec<Action> = Vec::new();
        let viewer_mode = self.mode == Mode::Viewer;
        ctx.input_mut(|i| {
            if viewer_mode {
                for key in [
                    egui::Key::Space,
                    egui::Key::ArrowRight,
                    egui::Key::ArrowDown,
                ] {
                    if i.consume_key(egui::Modifiers::NONE, key) {
                        actions.push(Action::NextImage);
                    }
                }
                for key in [
                    egui::Key::Backspace,
                    egui::Key::ArrowLeft,
                    egui::Key::ArrowUp,
                ] {
                    if i.consume_key(egui::Modifiers::NONE, key) {
                        actions.push(Action::PrevImage);
                    }
                }
            } else {
                if i.consume_key(egui::Modifiers::NONE, egui::Key::PageDown) {
                    actions.push(Action::NextImage);
                }
                if i.consume_key(egui::Modifiers::NONE, egui::Key::PageUp) {
                    actions.push(Action::PrevImage);
                }
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Home) {
                actions.push(Action::FirstImage);
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::End) {
                actions.push(Action::LastImage);
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
                if self.confirm_trash.is_some()
                    || self.rename_target.is_some()
                    || self.transfer.is_some()
                    || self.settings_open
                    || self.about_open
                    || self.explore_open
                {
                    self.confirm_trash = None;
                    self.rename_target = None;
                    self.transfer = None;
                    self.settings_open = false;
                    self.about_open = false;
                    self.explore_open = false;
                } else if self.slideshow {
                    actions.push(Action::StopSlideshow);
                } else if self.fullscreen {
                    actions.push(Action::ToggleFullscreen);
                } else if self.mode == Mode::Viewer {
                    actions.push(Action::CloseViewer);
                }
            }

            // Menu-declared shortcuts, most specific modifier set first.
            for (shortcut, action) in shortcuts {
                let modifiers = egui_modifiers(shortcut);
                let mut hit = false;
                for key in egui_keys(shortcut.key) {
                    hit |= i.consume_key(modifiers, key);
                }
                // `⌘+` is `⇧⌘=` on the keyboards that have no dedicated `+`.
                if shortcut.key == Key::Char('+') {
                    hit |= i.consume_key(modifiers | egui::Modifiers::SHIFT, egui::Key::Equals);
                }
                if hit {
                    actions.push(action);
                }
            }

            // F2 renames as well; the menu advertises the `⌥⌘R` spelling because
            // function keys are not how a macOS user is meant to find this.
            if i.consume_key(egui::Modifiers::NONE, egui::Key::F2) {
                actions.push(Action::Rename);
            }

            // Viewer-only single keys, kept because they are handy and clash
            // with nothing.
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Plus)
                || i.consume_key(egui::Modifiers::SHIFT, egui::Key::Equals)
            {
                actions.push(Action::ZoomIn);
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Minus) {
                actions.push(Action::ZoomOut);
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::F) {
                actions.push(Action::FitWindow);
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Num1) {
                actions.push(Action::ActualSize);
            }
        });
        for action in actions {
            self.act(action, ctx);
        }
    }

    fn tick_slideshow(&mut self, ctx: &Context) {
        if !self.slideshow {
            return;
        }
        if self.slideshow_at.elapsed().as_secs_f32() >= self.slideshow_delay {
            self.slideshow_at = Instant::now();
            let indices = self.image_indices();
            let position = indices
                .iter()
                .position(|&i| i == self.selected)
                .unwrap_or(0);
            if position + 1 >= indices.len() {
                self.act(Action::StopSlideshow, ctx);
                self.notify("Slideshow finished");
            } else {
                let dropped = self.loader.purge_thumbs();
                self.caches.abandon(&dropped);
                self.step_image(1);
            }
        }
        ctx.request_repaint_after(Duration::from_millis(100));
    }

    // -- layout ------------------------------------------------------------

    fn title(&self) -> String {
        match self.mode {
            Mode::Browser => format!("{APP_TITLE} - {}", self.dir.display()),
            Mode::Viewer => self
                .current()
                .map(|e| format!("{} - {}", e.name, APP_TITLE))
                .unwrap_or_else(|| APP_TITLE.to_owned()),
        }
    }

    fn pump(&mut self, ctx: &Context) {
        let protect = if self.mode == Mode::Viewer {
            self.current_image_path()
        } else {
            None
        };
        let arrived = self.caches.pump(ctx, &self.loader, protect.as_deref());
        if arrived > 0 || !self.caches.pending.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(40));
        }
    }

    /// The menu bar, described once and rendered by the platform back end.
    ///
    /// This is also the source of truth for the keyboard map: [`Self::shortcuts`]
    /// walks it, so a menu entry and the shortcut that triggers it cannot drift
    /// apart.
    pub fn menu_spec(&self) -> crate::menu::MenuBar {
        let has_entry = self.current().is_some();
        let has_image = self.current_image_path().is_some();
        let sort = self.sort;

        let app = vec![
            Item::command("About Lumiere", Action::About),
            Item::Separator,
            Item::key(
                "Settings\u{2026}",
                Action::Settings,
                Shortcut::cmd(Key::Char(',')),
            ),
        ];

        let file = vec![
            Item::key(
                "Open\u{2026}",
                Action::OpenFolder,
                Shortcut::cmd(Key::Char('o')),
            ),
            Item::Separator,
            Item::key("Get Info", Action::GetInfo, Shortcut::cmd(Key::Char('i'))),
            Item::Command {
                label: "Rename\u{2026}".into(),
                shortcut: Some(Shortcut::cmd(Key::Char('r')).option()),
                action: Action::Rename,
                enabled: has_entry,
                marked: false,
            },
            self.key_item(
                "Move to Trash",
                Action::MoveToTrash,
                Shortcut::cmd(Key::Delete),
            )
            .with_enabled(has_entry),
            Item::Separator,
            Item::command("Move to Folder\u{2026}", Action::MoveToFolder),
            Item::command("Copy to Folder\u{2026}", Action::CopyToFolder),
        ];

        let edit = vec![
            self.key_item(
                "Select All",
                Action::SelectAll,
                Shortcut::cmd(Key::Char('a')),
            ),
            self.key_item(
                "Select None",
                Action::SelectNone,
                Shortcut::cmd(Key::Char('a')).shift(),
            ),
            self.key_item(
                "Invert Selection",
                Action::InvertSelection,
                Shortcut::cmd(Key::Char('a')).option(),
            ),
            Item::Separator,
            Item::key(
                "Rotate Left",
                Action::RotateCCW,
                Shortcut::cmd(Key::Char('l')).shift(),
            ),
            Item::key(
                "Rotate Right",
                Action::RotateCW,
                Shortcut::cmd(Key::Char('r')).shift(),
            ),
            Item::toggle("Flip Horizontal", Action::FlipH, self.flip_h),
            Item::toggle("Flip Vertical", Action::FlipV, self.flip_v),
        ];

        let view = vec![
            Item::key("Reload", Action::Reload, Shortcut::cmd(Key::Char('r'))),
            Item::Separator,
            Item::key(
                "Actual Size",
                Action::ActualSize,
                Shortcut::cmd(Key::Char('0')),
            ),
            Item::key("Zoom In", Action::ZoomIn, Shortcut::cmd(Key::Char('+'))),
            Item::key("Zoom Out", Action::ZoomOut, Shortcut::cmd(Key::Char('-'))),
            Item::key(
                "Fit to Window",
                Action::FitWindow,
                Shortcut::cmd(Key::Char('f')).shift(),
            ),
            Item::Separator,
            Item::toggle_key(
                "Show Sidebar",
                Action::ToggleSidebar,
                Shortcut::cmd(Key::Char('s')).control(),
                self.show_sidebar,
            ),
            Item::toggle_key(
                "Show Toolbar",
                Action::ToggleToolbar,
                Shortcut::cmd(Key::Char('t')).option(),
                self.show_toolbar,
            ),
            Item::toggle_key(
                "Show Inspector",
                Action::ToggleInspector,
                Shortcut::cmd(Key::Char('i')).option(),
                self.show_inspector,
            ),
            Item::toggle("Show Status Bar", Action::ToggleStatus, self.show_status),
            Item::Separator,
            Item::toggle(
                "Small Thumbnails",
                Action::ThumbSmall,
                self.thumb_size <= 80,
            ),
            Item::toggle(
                "Medium Thumbnails",
                Action::ThumbMedium,
                self.thumb_size > 80 && self.thumb_size < 160,
            ),
            Item::toggle(
                "Large Thumbnails",
                Action::ThumbLarge,
                self.thumb_size >= 160,
            ),
            Item::submenu(
                "Sort",
                vec![
                    Item::toggle("by Name", Action::SortByName, sort == SortKey::Name),
                    Item::toggle("by Date", Action::SortByDate, sort == SortKey::Date),
                    Item::toggle("by Size", Action::SortBySize, sort == SortKey::Size),
                    Item::toggle("by Type", Action::SortByType, sort == SortKey::Type),
                    Item::Separator,
                    Item::toggle("Reverse order", Action::ToggleReverse, self.sort_reverse),
                ],
            ),
            Item::toggle("Show Folders", Action::ToggleFolders, self.show_folders),
            Item::Separator,
            Item::key(
                if self.fullscreen {
                    "Exit Full Screen"
                } else {
                    "Enter Full Screen"
                },
                Action::ToggleFullscreen,
                Shortcut::cmd(Key::Char('f')).control(),
            ),
        ];

        let go = vec![
            Item::key(
                "Up One Level",
                Action::UpFolder,
                Shortcut::cmd(Key::Char('u')).shift(),
            ),
            Item::key(
                "Home",
                Action::GoHome,
                Shortcut::cmd(Key::Char('h')).shift(),
            ),
            Item::key(
                "Pictures",
                Action::GoPictures,
                Shortcut::cmd(Key::Char('p')).shift(),
            ),
            Item::Separator,
            Item::command("First Image", Action::FirstImage),
            Item::command("Previous Image", Action::PrevImage),
            Item::command("Next Image", Action::NextImage),
            Item::command("Last Image", Action::LastImage),
            Item::Separator,
            self.key_item(
                "View Image",
                Action::OpenViewer,
                Shortcut::plain(Key::Return),
            )
            .with_enabled(has_image),
            Item::Command {
                label: "Back to Browser".into(),
                shortcut: None,
                action: Action::CloseViewer,
                enabled: self.mode == Mode::Viewer,
                marked: false,
            },
            Item::Separator,
            if self.slideshow {
                Item::key(
                    "Stop Slideshow",
                    Action::StopSlideshow,
                    Shortcut::cmd(Key::Char('y')).shift(),
                )
            } else {
                Item::Command {
                    label: "Play Slideshow".into(),
                    shortcut: Some(Shortcut::cmd(Key::Char('y')).shift()),
                    action: Action::Slideshow,
                    enabled: self.entries.iter().any(|e| e.is_image),
                    marked: false,
                }
            },
        ];

        crate::menu::MenuBar {
            app,
            menus: vec![
                ("File".into(), file),
                ("Edit".into(), edit),
                ("View".into(), view),
                ("Go".into(), go),
            ],
        }
    }

    /// A menu item whose key equivalent is withdrawn while a text field owns the
    /// keyboard: `⌘A` has to select the text, `⌘\u{232B}` delete to the start of the
    /// line, and Return has to confirm the edit.
    fn key_item(&self, label: &str, action: Action, shortcut: Shortcut) -> Item {
        if self.text_focused {
            Item::command(label, action)
        } else {
            Item::key(label, action, shortcut)
        }
    }

    /// Every shortcut the menus declare, most specific modifier set first:
    /// `consume_key` ignores extra Shift/Alt unless the pattern asks for them,
    /// so `⇧⌘R` has to be matched before `⌘R`.
    fn shortcuts(&self) -> Vec<(Shortcut, Action)> {
        fn walk(items: &[Item], out: &mut Vec<(Shortcut, Action)>) {
            for item in items {
                match item {
                    Item::Command {
                        shortcut: Some(shortcut),
                        action,
                        ..
                    } => out.push((*shortcut, *action)),
                    Item::Submenu { items, .. } => walk(items, out),
                    Item::Command { .. } | Item::Separator => {}
                }
            }
        }
        let spec = self.menu_spec();
        let mut out = Vec::new();
        walk(&spec.app, &mut out);
        for (_, items) in &spec.menus {
            walk(items, &mut out);
        }
        out.sort_by_key(|(shortcut, _)| std::cmp::Reverse(shortcut_modifier_count(shortcut)));
        out
    }

    /// The in-window menu bar. Only platforms without a native menu bar draw it.
    #[cfg(not(target_os = "macos"))]
    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        let spec = self.menu_spec();
        let mut actions: Vec<Action> = Vec::new();
        egui::MenuBar::new().ui(ui, |ui| {
            top_menu(ui, APP_TITLE, |ui| {
                menu_items(ui, &spec.app, &mut actions);
                ui.separator();
                menu(ui, &mut actions, Action::Quit, "Quit Lumiere", "\u{2318}Q");
            });
            for (title, items) in &spec.menus {
                top_menu(ui, title, |ui| menu_items(ui, items, &mut actions));
            }
        });
        let ctx = ui.ctx().clone();
        for action in actions {
            self.act(action, &ctx);
        }
    }

    /// Draws the in-window menu bar. macOS has a real one at the top of the
    /// screen, so there the whole thing is a no-op.
    #[cfg(target_os = "macos")]
    fn menu_bar_panel(&mut self, _ctx: &Context) {}

    #[cfg(not(target_os = "macos"))]
    fn menu_bar_panel(&mut self, ctx: &Context) {
        egui::TopBottomPanel::top("menubar")
            .exact_height(22.0)
            .show(ctx, |ui| {
                self.menu_bar(ui);
            });
    }

    fn tool_row(&mut self, ui: &mut egui::Ui) {
        let mut pending: Vec<Action> = Vec::new();
        let mut open_dialog = false;
        ui.horizontal(|ui| {
            if theme::tool_button(ui, Icon::FolderUp, "Up one level", true).clicked() {
                pending.push(Action::UpFolder);
            }
            if theme::tool_button(ui, Icon::Home, "Home folder", true).clicked() {
                pending.push(Action::GoHome);
            }
            if theme::tool_button(ui, Icon::Refresh, "Refresh", true).clicked() {
                pending.push(Action::Reload);
            }
            theme::vtool_separator(ui);
            if theme::tool_button(ui, Icon::ThumbSmall, "Small thumbnails", true).clicked() {
                pending.push(Action::ThumbSmall);
            }
            if theme::tool_button(ui, Icon::ThumbMedium, "Medium thumbnails", true).clicked() {
                pending.push(Action::ThumbMedium);
            }
            if theme::tool_button(ui, Icon::ThumbLarge, "Large thumbnails", true).clicked() {
                pending.push(Action::ThumbLarge);
            }
            theme::vtool_separator(ui);
            if theme::tool_button(ui, Icon::First, "First image", true).clicked() {
                pending.push(Action::FirstImage);
            }
            if theme::tool_button(ui, Icon::Prev, "Previous image", true).clicked() {
                pending.push(Action::PrevImage);
            }
            if theme::tool_button(ui, Icon::Next, "Next image", true).clicked() {
                pending.push(Action::NextImage);
            }
            if theme::tool_button(ui, Icon::Last, "Last image", true).clicked() {
                pending.push(Action::LastImage);
            }
            theme::vtool_separator(ui);
            if theme::tool_button(ui, Icon::Slideshow, "Slideshow", true).clicked() {
                pending.push(Action::Slideshow);
            }
            if theme::tool_button(ui, Icon::Stop, "Stop slideshow", self.slideshow).clicked() {
                pending.push(Action::StopSlideshow);
            }
            theme::vtool_separator(ui);
            if theme::tool_button(ui, Icon::ZoomOut, "Zoom out", true).clicked() {
                pending.push(Action::ZoomOut);
            }
            if theme::tool_button(ui, Icon::ZoomIn, "Zoom in", true).clicked() {
                pending.push(Action::ZoomIn);
            }
            if theme::tool_button(ui, Icon::Fit, "Fit to window", true).clicked() {
                pending.push(Action::FitWindow);
            }
            if theme::tool_button(ui, Icon::Actual, "Actual size", true).clicked() {
                pending.push(Action::ActualSize);
            }
            theme::vtool_separator(ui);
            if theme::tool_button(ui, Icon::RotateLeft, "Rotate left", true).clicked() {
                pending.push(Action::RotateCCW);
            }
            if theme::tool_button(ui, Icon::RotateRight, "Rotate right", true).clicked() {
                pending.push(Action::RotateCW);
            }
            if theme::tool_button(ui, Icon::FlipH, "Flip horizontal", true).clicked() {
                pending.push(Action::FlipH);
            }
            if theme::tool_button(ui, Icon::FlipV, "Flip vertical", true).clicked() {
                pending.push(Action::FlipV);
            }
            theme::vtool_separator(ui);
            if theme::tool_button(ui, Icon::Fullscreen, "Full screen", true).clicked() {
                pending.push(Action::ToggleFullscreen);
            }
            if theme::tool_button(
                ui,
                Icon::Delete,
                "Move to Trash",
                self.current_image_path().is_some(),
            )
            .clicked()
            {
                pending.push(Action::MoveToTrash);
            }
            if theme::tool_button(ui, Icon::Info, "Get Info", true).clicked() {
                open_dialog = true;
                pending.push(Action::GetInfo);
            }
            let _ = open_dialog;
        });

        for action in pending {
            self.act(action, ui.ctx());
        }
    }

    fn address_row(&mut self, ui: &mut egui::Ui) {
        let mut navigate: Option<PathBuf> = None;
        let mut rescan = false;

        ui.horizontal(|ui| {
            ui.label("Address:");
            let response = ui.add_sized(
                [(ui.available_width() - 190.0).max(120.0), 20.0],
                egui::TextEdit::singleline(&mut self.path_edit).font(egui::FontId::monospace(12.0)),
            );
            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                navigate = Some(PathBuf::from(self.path_edit.trim()));
            }
            let go = ui.button("Go");
            theme::note_rect(ui, "Go", go.rect);
            if go.clicked() {
                navigate = Some(PathBuf::from(self.path_edit.trim()));
            }
            ui.label("Files:");
            let filter_response = ui.add_sized(
                [90.0, 20.0],
                egui::TextEdit::singleline(&mut self.filter_edit),
            );
            theme::note_rect(ui, "Files filter", filter_response.rect);
            let filter_lost_focus = filter_response.lost_focus();
            if filter_lost_focus && self.filter_edit != self.filter {
                self.filter = self.filter_edit.clone();
                self.state_dirty = true;
                rescan = true;
            }
            let apply = ui.button("Apply");
            theme::note_rect(ui, "Apply", apply.rect);
            if apply.clicked() {
                self.filter = self.filter_edit.clone();
                self.state_dirty = true;
                rescan = true;
            }
        });

        if let Some(path) = navigate {
            if path.is_dir() {
                self.set_dir(path);
            } else {
                self.notify(format!("Cannot open {}", self.path_edit));
            }
        }
        if rescan {
            self.rescan();
        }
    }

    fn places_row(&mut self, ui: &mut egui::Ui) {
        let mut goto: Option<PathBuf> = None;
        ui.horizontal(|ui| {
            ui.label("Places:");
            let mut places: Vec<(String, PathBuf)> = Vec::new();
            if let Some(home) = dirs::home_dir() {
                places.push(("Home".to_owned(), home));
            }
            for (label, dir) in [
                ("Desktop", dirs::desktop_dir()),
                ("Documents", dirs::document_dir()),
                ("Downloads", dirs::download_dir()),
                ("Pictures", dirs::picture_dir()),
                ("Movies", dirs::video_dir()),
            ] {
                if let Some(path) = dir.filter(|p| p.is_dir()) {
                    places.push((label.to_owned(), path));
                }
            }
            places.push(("Macintosh HD".to_owned(), PathBuf::from("/")));
            for (label, path) in places {
                let response = ui.selectable_label(self.dir == path, &label);
                theme::note_rect(ui, &label, response.rect);
                if response.clicked() {
                    goto = Some(path);
                }
            }
        });
        if let Some(path) = goto {
            self.set_dir(path);
        }
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let images = self.entries.iter().filter(|e| e.is_image).count();
        let folders = self.entries.iter().filter(|e| e.is_dir).count();
        let left = format!(
            "{} image(s), {} folder(s), {} selected",
            images,
            folders,
            self.multi.len().max(1)
        );

        let middle = self
            .current()
            .map(|e| {
                format!(
                    "{}  {}  {}",
                    e.name,
                    imaging::human_bytes(e.size),
                    imaging::format_time(e.modified)
                )
            })
            .unwrap_or_default();

        let right = if self.mode == Mode::Viewer {
            format!("{:.0}%", self.zoom * 100.0)
        } else {
            format!("{} x {}", self.thumb_size, self.thumb_size)
        };

        let status = self
            .status_override
            .take()
            .or_else(|| self.toast.as_ref().map(|(m, _)| m.clone()))
            .unwrap_or(left);

        ui.horizontal(|ui| {
            theme::status_field(ui, &status, 320.0);
            theme::status_field(ui, &middle, (ui.available_width() - 120.0).max(80.0));
            theme::status_field(ui, &right, 90.0);
        });
    }

    // -- dialogs -----------------------------------------------------------

    fn options_window(&mut self, ctx: &Context) {
        let mut clear_cache = false;
        let mut rescan = false;
        let mut open = self.settings_open;
        egui::Window::new("Options")
            .open(&mut open)
            .resizable(false)
            .default_pos([320.0, 200.0])
            .show(ctx, |ui| {
                theme::section_label(ui, "Thumbnails");
                ui.horizontal(|ui| {
                    ui.label("Size:");
                    if ui
                        .add(egui::Slider::new(&mut self.thumb_size, 32..=320).suffix(" px"))
                        .changed()
                    {
                        self.caches.clear_thumbs();
                    }
                });
                if ui
                    .checkbox(&mut self.show_folders, "Show folders in browser")
                    .changed()
                {
                    rescan = true;
                }
                theme::section_label(ui, "Viewer");
                if ui
                    .checkbox(&mut self.canvas_dark, "Black background")
                    .changed()
                {
                    ctx.request_repaint();
                }
                theme::section_label(ui, "Slideshow");
                ui.horizontal(|ui| {
                    ui.label("Delay:");
                    ui.add(
                        egui::Slider::new(&mut self.slideshow_delay, 0.5..=30.0)
                            .suffix(" s")
                            .step_by(0.5),
                    );
                });
                theme::section_label(ui, "Maintenance");
                if ui.button("Clear thumbnail cache").clicked() {
                    clear_cache = true;
                }
                ui.label(format!("Cache folder: {}", imaging::cache_root().display()));
            });
        self.settings_open = open;

        if clear_cache {
            let removed = imaging::clear_thumbnail_cache().unwrap_or(0);
            self.caches.clear_thumbs();
            let (_, dropped) = self.loader.bump_generation();
            self.caches.abandon(&dropped);
            self.notify(format!("Cleared {removed} cached thumbnail(s)"));
        }
        if rescan {
            self.rescan();
        }
    }

    fn about_window(&mut self, ctx: &Context) {
        let mut open = self.about_open;
        egui::Window::new("About Lumiere")
            .open(&mut open)
            .resizable(false)
            .default_pos([360.0, 220.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(40.0, 40.0), egui::Sense::hover());
                    theme::draw_icon(
                        ui.painter(),
                        rect.center(),
                        36.0,
                        Icon::Image,
                        ui.visuals().text_color(),
                    );
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("Lumiere").strong());
                        ui.label("An image browser and viewer for macOS, written in Rust");
                        ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                        ui.label(
                            egui::RichText::new("egui + image + Apple ImageIO (HEIC/AVIF)").small(),
                        );
                    });
                });
                ui.separator();
                ui.label(
                    "Keys: Space/Right next, Backspace/Left previous, Enter open, Esc close, \
                     +/- zoom, F fit, 1 actual size, \u{2303}\u{2318}F full screen, \
                     \u{2325}\u{2318}Y slideshow.",
                );
            });
        self.about_open = open;
    }

    fn explore_window(&mut self, ctx: &Context) {
        let mut open = self.explore_open;
        let mut choose: Option<PathBuf> = None;
        let mut browse: Option<PathBuf> = None;
        let current = self.explore_path.clone();
        egui::Window::new("Explore Folder")
            .open(&mut open)
            .default_pos([340.0, 180.0])
            .default_size([420.0, 380.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Up").clicked() {
                        if let Some(parent) = current.parent() {
                            browse = Some(parent.to_path_buf());
                        }
                    }
                    ui.label(egui::RichText::new(current.display().to_string()).monospace());
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let dirs = fsmodel::read_directory(&current, true, "*.*");
                        for entry in dirs.iter().filter(|e| e.is_dir) {
                            let response = ui.selectable_label(false, &entry.name);
                            if response.double_clicked() || response.clicked() {
                                browse = Some(entry.path.clone());
                            }
                        }
                        if dirs.iter().all(|e| !e.is_dir) {
                            ui.label("(no folders)");
                        }
                    });
                ui.separator();
                if ui.button("Select this folder").clicked() {
                    choose = Some(current.clone());
                }
            });
        self.explore_open = open;
        if let Some(path) = browse {
            self.explore_path = path;
        }
        if let Some(path) = choose {
            self.set_dir(path);
        }
    }

    fn rename_window(&mut self, ctx: &Context) {
        let Some(path) = self.rename_target.clone() else {
            return;
        };
        let mut commit = false;
        let mut cancel = false;
        egui::Window::new("Rename")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                if let Some(parent) = path.parent() {
                    ui.label(parent.display().to_string());
                }
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.rename_text)
                        .desired_width(320.0)
                        .cursor_at_end(true),
                );
                response.request_focus();
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Rename").clicked() {
                        commit = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    commit = true;
                }
            });

        if cancel {
            self.rename_target = None;
        }
        if commit {
            self.rename_target = None;
            let name = self.rename_text.clone();
            self.apply_rename(&path, &name);
        }
    }

    /// The files/folders a batch operation acts on: the multi-selection, in
    /// grid order, or the current entry when nothing is selected.
    fn selection_paths(&self) -> Vec<PathBuf> {
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
            crate::panels::PanelResult::Chose(dest) => self.apply_transfer(&paths, &dest, copy),
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

    /// Move or copy `paths` into `dest`, skipping no-ops and reporting failures.
    pub fn apply_transfer(&mut self, paths: &[PathBuf], dest: &Path, copy: bool) {
        if !dest.is_dir() {
            self.notify(format!("Not a folder: {}", dest.display()));
            return;
        }
        let mut done = 0;
        for path in paths {
            let Some(name) = path.file_name() else {
                continue;
            };
            let target = dest.join(name);
            if target == *path {
                continue;
            }
            let result = if copy {
                copy_path(path, &target)
            } else {
                std::fs::rename(path, &target)
            };
            match result {
                Ok(()) => done += 1,
                Err(error) => self.notify(format!("Transfer failed: {error}")),
            }
        }
        if done > 0 {
            let verb = if copy { "Copied" } else { "Moved" };
            self.notify(format!("{verb} {done} item(s)"));
            self.multi.clear();
            self.rescan();
        }
    }

    /// The "move to Trash" flow for `paths`: confirm first, then apply. On
    /// macOS the confirmation is a native alert; elsewhere — and in a test
    /// build — the built-in dialog confirms.
    pub fn begin_trash(&mut self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        match crate::alerts::confirm_trash(&paths) {
            crate::alerts::Confirmation::Confirmed => self.apply_trash(&paths),
            crate::alerts::Confirmation::Cancelled => {}
            crate::alerts::Confirmation::Unavailable => {
                self.confirm_trash = Some(paths);
            }
        }
    }

    /// Move `paths` to the Trash with the `trash` crate — the items stay in
    /// the Finder's Trash, so the operation is recoverable — and report what
    /// happened for a multi-selection.
    pub fn apply_trash(&mut self, paths: &[PathBuf]) {
        let names = crate::alerts::trash_names(paths);
        let mut moved = 0usize;
        let mut failures: Vec<(String, String)> = Vec::new();
        for (path, name) in paths.iter().zip(&names) {
            match trash::delete(path) {
                Ok(()) => moved += 1,
                Err(error) => failures.push((name.clone(), error.to_string())),
            }
        }
        if moved > 0 {
            self.multi.clear();
            self.rescan();
        }
        let message = match (paths.len(), moved, failures.is_empty()) {
            (1, 1, _) => format!("Moved {} to the Trash", names[0]),
            (1, 0, _) => {
                format!("Couldn't move {} to the Trash: {}", names[0], failures[0].1)
            }
            (_, _, true) => format!("Moved {moved} items to the Trash"),
            (_, 0, _) => {
                format!(
                    "Couldn't move {} items to the Trash: {}",
                    paths.len(),
                    failures[0].0
                )
            }
            (_, _, _) => {
                format!(
                    "Moved {moved} of {} items to the Trash: {} — {}",
                    paths.len(),
                    failures[0].0,
                    failures[0].1
                )
            }
        };
        self.notify(message);
    }

    fn transfer_window(&mut self, ctx: &Context) {
        let Some(pending) = self.transfer.clone() else {
            return;
        };
        let mut apply = false;
        let mut cancel = false;
        let title = if pending.copy {
            "Copy to Folder"
        } else {
            "Move to Folder"
        };
        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(format!("{} item(s)", pending.paths.len()));
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.transfer.as_mut().unwrap().dest)
                        .desired_width(320.0)
                        .cursor_at_end(true),
                );
                response.request_focus();
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let label = if pending.copy { "Copy" } else { "Move" };
                    if ui.button(label).clicked() {
                        apply = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    apply = true;
                }
            });

        if cancel {
            self.transfer = None;
        }
        if apply {
            let dest = self
                .transfer
                .as_ref()
                .map(|pending| pending.dest.clone())
                .unwrap_or_default();
            self.transfer = None;
            let dest = PathBuf::from(dest.trim());
            self.apply_transfer(&pending.paths, &dest, pending.copy);
        }
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
        let target = path.with_file_name(name);
        match std::fs::rename(path, &target) {
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

    /// The built-in confirmation dialog, shown where no native alert is
    /// available. It reuses the alert's own text, so the fallback and the
    /// native alert cannot drift apart.
    fn confirm_trash_window(&mut self, ctx: &Context) {
        let Some(paths) = self.confirm_trash.clone() else {
            return;
        };
        let detail = crate::alerts::trash_alert_text(&paths).1;
        egui::Window::new("Move to Trash")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(&detail);
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Move to Trash").clicked() {
                        self.confirm_trash = None;
                        self.apply_trash(&paths);
                    }
                    if ui.button("Cancel").clicked() {
                        self.confirm_trash = None;
                    }
                });
            });
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

impl App {
    /// One frame of UI, usable from tests as well as the eframe backend.
    pub fn draw(&mut self, ctx: &Context) {
        let title = self.title();
        if title != self.last_title {
            self.last_title = title.clone();
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        }
        self.pump(ctx);
        if let Some(target) = self.restore_pos {
            self.restore_frames += 1;
            if self.restore_frames == 1 {
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(target));
            } else if self.restore_frames >= 3 {
                let reported = ctx.input(|i| {
                    i.viewport()
                        .outer_rect
                        .map(|rect| rect.min)
                        .unwrap_or_default()
                });
                let delta = target - reported;
                if delta.length() > 2.0 {
                    ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(target + delta));
                }
                self.restore_pos = None;
            }
        }
        let viewport_size = ctx.input(|i| {
            i.viewport()
                .inner_rect
                .map(|rect| rect.size())
                .unwrap_or_default()
        });
        let viewport_pos = ctx.input(|i| {
            i.viewport()
                .outer_rect
                .map(|rect| rect.min)
                .unwrap_or_default()
        });
        if viewport_size.x > 0.0 && (viewport_size - self.last_size).length() > 2.0 {
            self.last_size = viewport_size;
            self.state_dirty = true;
        }
        if viewport_pos.x != 0.0 && (viewport_pos - self.last_pos).length() > 2.0 {
            self.last_pos = viewport_pos;
            self.state_dirty = true;
        }
        if self.state_dirty && !cfg!(test) {
            self.state_dirty = false;
            let state = crate::state::State {
                last_dir: Some(self.dir.clone()),
                width: viewport_size.x,
                height: viewport_size.y,
                x: viewport_pos.x,
                y: viewport_pos.y,
                thumb_size: self.thumb_size,
                sort: sort_key_name(self.sort).to_owned(),
                sort_reverse: self.sort_reverse,
                filter: self.filter.clone(),
                show_folders: i32::from(self.show_folders),
            };
            let _ = state.save();
        }
        // AppKit offers key equivalents to the menu bar before the window ever
        // sees the key, so a menu that claims `⌘A` or Return would take them away
        // from an egui text field that is open. Track who owns the keyboard.
        self.text_focused = ctx.memory(|m| m.focused().is_some());
        self.handle_keys(ctx);
        self.tick_slideshow(ctx);

        // The native menu bar is the app's only menu on macOS: push the current
        // description to it and pick up whatever the user clicked.
        if crate::menu::NATIVE {
            crate::menu::sync(&self.menu_spec());
            for action in crate::menu::take_events() {
                self.act(action, ctx);
            }
        }

        if let Some((_, at)) = self.toast {
            if at.elapsed().as_secs_f32() > 3.0 {
                self.toast = None;
            }
        }

        let chrome = !self.fullscreen && self.mode == Mode::Browser;

        if chrome {
            self.menu_bar_panel(ctx);
            if self.show_toolbar {
                egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
                    self.tool_row(ui);
                });
            }
            if self.show_places {
                egui::TopBottomPanel::top("places").show(ctx, |ui| {
                    self.places_row(ui);
                });
            }
            egui::TopBottomPanel::top("address").show(ctx, |ui| {
                self.address_row(ui);
            });
            if self.show_status {
                egui::TopBottomPanel::bottom("statusbar")
                    .exact_height(24.0)
                    .show(ctx, |ui| {
                        self.status_bar(ui);
                    });
            }
            if self.show_sidebar {
                egui::SidePanel::left("foldertree")
                    .resizable(true)
                    .default_width(240.0)
                    .min_width(140.0)
                    .show(ctx, |ui| {
                        crate::browser::tree_pane(self, ui);
                    });
            }
            if self.show_inspector {
                egui::SidePanel::right("previewpane")
                    .resizable(true)
                    .default_width(280.0)
                    .min_width(160.0)
                    .show(ctx, |ui| {
                        crate::browser::preview_pane(self, ui);
                    });
            }
        } else if !self.fullscreen {
            self.menu_bar_panel(ctx);
            if self.show_status {
                egui::TopBottomPanel::bottom("statusbar")
                    .exact_height(24.0)
                    .show(ctx, |ui| {
                        self.status_bar(ui);
                    });
            }
        }

        let fill = if self.mode == Mode::Viewer {
            canvas_color(self, ctx)
        } else {
            ctx.style().visuals.panel_fill
        };
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(fill)
                    .inner_margin(egui::Margin::same(0)),
            )
            .show(ctx, |ui| match self.mode {
                Mode::Browser => crate::browser::grid(self, ui),
                Mode::Viewer => crate::viewer::view(self, ui),
            });

        self.options_window(ctx);
        self.about_window(ctx);
        self.explore_window(ctx);
        self.rename_window(ctx);
        self.transfer_window(ctx);
        self.confirm_trash_window(ctx);
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
            Item::Separator => ui.separator(),
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
                    node.set_label(label);
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

fn copy_path(from: &Path, to: &Path) -> std::io::Result<()> {
    if from.is_dir() {
        std::fs::create_dir_all(to)?;
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            copy_path(&entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        std::fs::copy(from, to).map(|_| ())
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
mod tests {
    use super::*;

    fn raw_input() -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_max(
                egui::pos2(0.0, 0.0),
                egui::pos2(1280.0, 820.0),
            )),
            focused: true,
            ..Default::default()
        }
    }

    /// Warm the pointer up on `pos` for one frame (egui hit-tests the previous
    /// pass), then press and release there.
    fn click_at(ctx: &Context, app: &mut App, pos: egui::Pos2, time: &mut f64) {
        let press = vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ];
        for (frame, events) in [vec![egui::Event::PointerMoved(pos)], press]
            .into_iter()
            .enumerate()
        {
            let mut input = raw_input();
            input.time = Some(*time + frame as f64 * 0.4);
            input.events = events;
            let _ = ctx.run(input, |ctx| app.draw(ctx));
        }
        *time += 2.0;
    }

    fn click_label(ctx: &Context, app: &mut App, label: &str, time: &mut f64) -> bool {
        let Some(rect) = theme::rect_of(ctx, label) else {
            return false;
        };
        click_at(ctx, app, rect.center(), time);
        true
    }

    /// Paints frames until the given text shows up (windows need a sizing pass
    /// before their content is drawn).
    fn painted_once(ctx: &Context, app: &mut App, needle: &str) -> String {
        let mut text = String::new();
        for _ in 0..40 {
            let output = ctx.run(raw_input(), |ctx| app.draw(ctx));
            text = painted_text(&output.shapes);
            if text.contains(needle) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        text
    }

    fn settle(ctx: &Context, app: &mut App, time: &mut f64) {
        // The registry must only hold controls that are on screen right now,
        // otherwise a closed menu leaves a stale rect behind.
        theme::clear_rects(ctx);
        let mut input = raw_input();
        input.time = Some(*time);
        let _ = ctx.run(input, |ctx| app.draw(ctx));
        *time += 0.4;
    }

    /// Looks a command up in the menu description exactly as a menu back end
    /// does, and returns what clicking it would do.
    fn menu_action(app: &App, path: &[&str]) -> Action {
        let spec = app.menu_spec();
        let mut items: &[Item] = match path[0] {
            "App" => &spec.app,
            title => {
                &spec
                    .menus
                    .iter()
                    .find(|(name, _)| name == title)
                    .unwrap_or_else(|| panic!("no {title} menu"))
                    .1
            }
        };
        for title in &path[1..path.len() - 1] {
            items = match items
                .iter()
                .find(|item| matches!(item, Item::Submenu { label, .. } if label == title))
            {
                Some(Item::Submenu {
                    items: children, ..
                }) => children,
                Some(_) => panic!("{title} is a command, not a submenu"),
                None => panic!("no {title} submenu"),
            };
        }
        let title = path.last().unwrap();
        match items.iter().find(|item| match item {
            Item::Command { label, .. } | Item::Submenu { label, .. } => label == title,
            Item::Separator => false,
        }) {
            Some(Item::Command {
                action, enabled, ..
            }) => {
                assert!(*enabled, "{title} is disabled");
                *action
            }
            Some(_) => panic!("{title} is a submenu, not a command"),
            None => panic!("no {title} menu item"),
        }
    }

    /// The menu bar has to read like every other macOS app: the verbs and the
    /// `\u{2318}` spellings a user already knows, and none of the Windows-era
    /// habits this app shipped with.
    #[test]
    fn menu_uses_macos_names_and_shortcuts() {
        let ctx = egui::Context::default();
        let app = App::build(&ctx, Some(fixture_dir("menuspec")));

        let spec = app.menu_spec();
        let titles: Vec<&str> = spec.menus.iter().map(|(title, _)| title.as_str()).collect();
        assert_eq!(titles, ["File", "Edit", "View", "Go"]);

        let app_menu: Vec<&str> = spec
            .app
            .iter()
            .map(|item| match item {
                Item::Command { label, .. } | Item::Submenu { label, .. } => label.as_str(),
                Item::Separator => "",
            })
            .collect();
        assert_eq!(app_menu, ["About Lumiere", "", "Settings\u{2026}"]);

        let named = |path: &[&str]| -> String {
            let items = match path[0] {
                "App" => &spec.app,
                title => &spec.menus.iter().find(|(name, _)| name == title).unwrap().1,
            };
            for item in items {
                if let Item::Command {
                    label, shortcut, ..
                } = item
                {
                    if label == path[1] {
                        return shortcut
                            .map(|shortcut| shortcut.display())
                            .unwrap_or_default();
                    }
                }
            }
            panic!("no {} menu item", path[1]);
        };
        assert_eq!(named(&["App", "Settings\u{2026}"]), "\u{2318},");
        assert_eq!(named(&["File", "Open\u{2026}"]), "\u{2318}O");
        assert_eq!(named(&["File", "Get Info"]), "\u{2318}I");
        assert_eq!(named(&["File", "Move to Trash"]), "\u{2318}\u{232B}");
        assert_eq!(named(&["View", "Reload"]), "\u{2318}R");
        assert_eq!(named(&["View", "Enter Full Screen"]), "\u{2303}\u{2318}F");
        assert_eq!(named(&["Go", "Up One Level"]), "\u{21E7}\u{2318}U");

        // The Windows-era vocabulary must be gone from every level.
        let banned = [
            "Exit",
            "Options",
            "Properties",
            "Delete",
            "Refresh",
            "Explore Folder",
            "Tools",
            "Folder Tree",
            "Preview Pane",
        ];
        fn labels(items: &[Item], out: &mut Vec<String>) {
            for item in items {
                match item {
                    Item::Command { label, .. } | Item::Submenu { label, .. } => {
                        out.push(label.clone())
                    }
                    Item::Separator => {}
                }
            }
        }
        let mut seen = Vec::new();
        labels(&spec.app, &mut seen);
        for (_, items) in &spec.menus {
            labels(items, &mut seen);
        }
        for word in banned {
            assert!(
                !seen.iter().any(|label| label == word),
                "{word} is still in the menu bar: {seen:?}"
            );
        }
    }

    /// A menu entry and the key that triggers it come from the same
    /// [`Shortcut`], so the two cannot drift apart.
    #[test]
    fn menu_shortcuts_are_the_keyboard_map() {
        let dir = fixture_dir("shortcutmap");
        let ctx = egui::Context::default();
        let mut app = App::build(&ctx, Some(dir.clone()));
        let _ = ctx.run(raw_input(), |ctx| app.draw(ctx));

        let cases: &[(&[&str], egui::Key, egui::Modifiers)] = &[
            (
                &["Edit", "Rotate Left"],
                egui::Key::L,
                egui::Modifiers::SHIFT | egui::Modifiers::COMMAND,
            ),
            (
                &["Edit", "Rotate Right"],
                egui::Key::R,
                egui::Modifiers::SHIFT | egui::Modifiers::COMMAND,
            ),
            (
                &["File", "Move to Trash"],
                egui::Key::Backspace,
                egui::Modifiers::COMMAND,
            ),
            (
                &["View", "Show Sidebar"],
                egui::Key::S,
                egui::Modifiers::COMMAND | egui::Modifiers::CTRL,
            ),
            (
                &["Go", "Pictures"],
                egui::Key::P,
                egui::Modifiers::SHIFT | egui::Modifiers::COMMAND,
            ),
        ];
        for (path, key, modifiers) in cases {
            let before = (
                app.rotation,
                app.confirm_trash.clone(),
                app.show_sidebar,
                app.dir.clone(),
            );
            let mut input = raw_input();
            input.events.push(egui::Event::Key {
                key: *key,
                pressed: true,
                modifiers: *modifiers,
                physical_key: None,
                repeat: false,
            });
            let _ = ctx.run(input, |ctx| app.draw(ctx));
            let after = (
                app.rotation,
                app.confirm_trash.clone(),
                app.show_sidebar,
                app.dir.clone(),
            );
            assert_ne!(before, after, "{path:?} did nothing");

            // And the menu advertises the very same combination.
            let action = menu_action(&app, path);
            let declared = app
                .shortcuts()
                .into_iter()
                .find(|(_, declared)| *declared == action)
                .map(|(shortcut, _)| (shortcut.key, shortcut))
                .expect("the shortcut is not declared in the menu bar");
            assert_eq!(
                egui_modifiers(declared.1),
                *modifiers,
                "{path:?} is advertised with different modifiers"
            );
            assert!(
                egui_keys(declared.0).contains(key),
                "{path:?} is advertised on a different key"
            );
        }

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The grid is painted, not built from widgets, so without an explicit
    /// accessible name a screen reader hears nothing at all.
    #[test]
    fn thumbnails_are_readable_by_accessibility() {
        let dir = fixture_dir("a11ygrid");
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = App::build(&ctx, Some(dir.clone()));
        let frame = ctx.run(raw_input(), |ctx| app.draw(ctx));

        // The tree is handed to the platform in the frame's output, so this is
        // what a screen reader would have been told.
        let names: Vec<String> = frame
            .platform_output
            .accesskit_update
            .as_ref()
            .map(|update| {
                update
                    .nodes
                    .iter()
                    .filter_map(|(_, node)| node.label().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        for name in ["one.png", "two.png", "three.png"] {
            assert!(
                names.iter().any(|read| read.starts_with(name)),
                "{name} is not announced; the tree says {names:?}"
            );
        }

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn sort_menu_reorders_the_grid() {
        let dir = std::env::temp_dir().join("lumiere_ui_test_sortmenu");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.png"), vec![b'x'; 10]).unwrap();
        std::fs::write(dir.join("b.png"), vec![b'x'; 900]).unwrap();
        std::fs::write(dir.join("c.png"), vec![b'x'; 100]).unwrap();

        let ctx = egui::Context::default();
        let mut app = App::build(&ctx, Some(dir.clone()));

        app.act(menu_action(&app, &["View", "Sort", "by Size"]), &ctx);
        assert_eq!(app.sort, SortKey::Size);
        let names: Vec<String> = app.entries.iter().map(|e| e.name.clone()).collect();
        assert_eq!(names, ["a.png", "c.png", "b.png"]);

        app.act(menu_action(&app, &["View", "Sort", "Reverse order"]), &ctx);
        let names: Vec<String> = app.entries.iter().map(|e| e.name.clone()).collect();
        assert_eq!(names, ["b.png", "c.png", "a.png"]);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn view_menu_toggles_panels_and_thumb_size() {
        let dir = fixture_dir("viewmenu");
        let ctx = egui::Context::default();
        let mut app = App::build(&ctx, Some(dir.clone()));

        assert!(app.show_sidebar && app.thumb_size == 120);
        app.act(menu_action(&app, &["View", "Show Sidebar"]), &ctx);
        assert!(!app.show_sidebar);

        app.act(menu_action(&app, &["View", "Small Thumbnails"]), &ctx);
        assert_eq!(app.thumb_size, 64);

        for path in [
            &["View", "Show Inspector"][..],
            &["View", "Show Toolbar"][..],
            &["View", "Show Status Bar"][..],
        ] {
            app.act(menu_action(&app, path), &ctx);
        }
        assert!(!app.show_inspector && !app.show_toolbar && !app.show_status);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn places_row_navigates_to_the_selected_place() {
        let dir = fixture_dir("places");
        let ctx = egui::Context::default();
        let mut app = App::build(&ctx, Some(dir.clone()));
        let mut time = 1.0;

        settle(&ctx, &mut app, &mut time);
        assert!(click_label(&ctx, &mut app, "Macintosh HD", &mut time));
        assert_eq!(app.dir, PathBuf::from("/"));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn clicking_a_tree_folder_navigates_to_it() {
        let home = dirs::home_dir().expect("home dir");
        let ctx = egui::Context::default();
        let mut app = App::build(&ctx, Some(home.clone()));
        let mut time = 1.0;

        let home_label = home.file_name().unwrap().to_string_lossy().to_string();
        settle(&ctx, &mut app, &mut time);
        assert!(click_label(
            &ctx,
            &mut app,
            &format!("tree:{home_label}"),
            &mut time
        ));
        assert_eq!(app.dir, home);
        assert!(app.tree_selected.is_some());

        settle(&ctx, &mut app, &mut time);
        let child_label = {
            let index = app.tree.find(&home).expect("home node");
            app.tree
                .node(index)
                .children
                .first()
                .map(|&c| app.tree.node(c).label.clone())
                .expect("a child folder")
        };
        let child_path = {
            let index = app.tree.find(&home).expect("home node");
            app.tree.node(app.tree.node(index).children[0]).path.clone()
        };
        assert!(click_label(
            &ctx,
            &mut app,
            &format!("tree:{child_label}"),
            &mut time
        ));
        assert_eq!(app.dir, child_path);
    }

    #[test]
    fn files_filter_applies_from_the_address_row() {
        let dir = std::env::temp_dir().join("lumiere_ui_test_filter");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("one.png"), vec![b'x'; 10]).unwrap();
        std::fs::write(dir.join("two.jpg"), vec![b'x'; 10]).unwrap();

        let ctx = egui::Context::default();
        let mut app = App::build(&ctx, Some(dir.clone()));
        let mut time = 1.0;

        settle(&ctx, &mut app, &mut time);
        assert!(click_label(&ctx, &mut app, "Files filter", &mut time));

        let mut input = raw_input();
        input.time = Some(time);
        input.events = vec![
            egui::Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            },
            egui::Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            },
            egui::Event::Text("*.jpg".to_owned()),
        ];
        let _ = ctx.run(input, |ctx| app.draw(ctx));
        time += 0.4;

        settle(&ctx, &mut app, &mut time);
        assert!(click_label(&ctx, &mut app, "Apply", &mut time));

        assert_eq!(app.filter, "*.jpg");
        let names: Vec<String> = app.entries.iter().map(|e| e.name.clone()).collect();
        assert_eq!(names, ["two.jpg"]);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn about_and_explore_dialogs_render_their_content() {
        let dir = fixture_dir("dialogs");
        let ctx = egui::Context::default();
        let mut app = App::build(&ctx, Some(dir.clone()));

        app.act(Action::About, &ctx);
        let text = painted_once(&ctx, &mut app, "Version");
        assert!(text.contains("Lumiere"));
        assert!(text.contains("Keys:"));

        app.about_open = false;
        app.act(Action::OpenFolder, &ctx);
        let text = painted_once(&ctx, &mut app, "Explore");
        eprintln!("DBG explore<<{text}>>END");
        assert!(text.contains("Explore"));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn viewer_keeps_the_current_image_when_the_full_cache_overflows() {
        let dir = std::env::temp_dir().join("lumiere_ui_test_fullcache");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // 5000x5000 RGBA is ~100 MB each: three of them blow the full budget.
        for name in ["a.png", "b.png", "c.png"] {
            image::RgbaImage::from_pixel(5000, 5000, image::Rgba([9, 99, 199, 255]))
                .save(dir.join(name))
                .unwrap();
        }

        let ctx = egui::Context::default();
        let mut app = App::build(&ctx, Some(dir.clone()));
        app.act(Action::NextImage, &ctx);
        app.act(Action::OpenViewer, &ctx);
        let current = app.current_image_path().expect("current image");

        // A backend that can hold the 5000x5000 uploads, so the byte budget
        // (not the texture clamp) is what overflows here.
        let big_input = || {
            let mut input = raw_input();
            input.max_texture_side = Some(16384);
            input
        };

        let mut arrived = false;
        for frame in 0..120 {
            let mut input = big_input();
            input.time = Some(1.0 + frame as f64 * 0.1);
            let _ = ctx.run(input, |ctx| app.draw(ctx));
            if app.caches.full(&current).is_some() {
                arrived = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        assert!(arrived, "current image never made it into the full cache");

        // Let the neighbour prefetches land; they overflow the budget and must
        // evict the neighbours, not the image on screen.
        for frame in 0..120 {
            let mut input = big_input();
            input.time = Some(20.0 + frame as f64 * 0.1);
            let _ = ctx.run(input, |ctx| app.draw(ctx));
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        assert!(
            app.caches.full(&current).is_some(),
            "the displayed image was evicted: the viewer would flicker"
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn oversized_images_are_clamped_to_the_texture_limit() {
        let dir = std::env::temp_dir().join("lumiere_ui_test_texclamp");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        image::RgbaImage::from_pixel(3000, 3000, image::Rgba([9, 99, 199, 255]))
            .save(dir.join("big.png"))
            .unwrap();

        // Default test context advertises max_texture_side = 2048.
        let ctx = egui::Context::default();
        let mut app = App::build(&ctx, Some(dir.clone()));
        app.act(Action::OpenViewer, &ctx);
        let current = app.current_image_path().expect("current image");

        for frame in 0..120 {
            let mut input = raw_input();
            input.time = Some(1.0 + frame as f64 * 0.1);
            let _ = ctx.run(input, |ctx| app.draw(ctx));
            if app.caches.full(&current).is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        let entry = app.caches.full(&current).expect("clamped texture cached");
        assert_eq!((entry.width, entry.height), (3000, 3000));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn real_photo_1jpg_stays_cached() {
        let dir = PathBuf::from("/Users/james/Pictures/Photo");
        if !dir.join("1.jpg").is_file() {
            return;
        }
        let ctx = egui::Context::default();
        let mut app = App::build(&ctx, Some(dir.clone()));
        let idx = app
            .entries
            .iter()
            .position(|e| e.name == "1.jpg")
            .expect("1.jpg listed");
        app.selected = idx;
        app.act(Action::OpenViewer, &ctx);
        let current = dir.join("1.jpg");

        let mut missing_after_arrival = 0;
        let mut arrived = false;
        for frame in 0..300 {
            let mut input = raw_input();
            input.time = Some(1.0 + frame as f64 * 0.1);
            let _ = ctx.run(input, |ctx| app.draw(ctx));
            if app.caches.full(&current).is_some() {
                arrived = true;
            } else if arrived {
                missing_after_arrival += 1;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(arrived, "1.jpg never made it into the full cache");
        assert_eq!(
            missing_after_arrival, 0,
            "1.jpg disappeared from the cache after arriving: flicker"
        );
    }

    #[test]
    fn real_photo_huge_image_stays_cached() {
        let dir = PathBuf::from("/Users/james/Pictures/Photo");
        let huge = dir.join("影像.jpg");
        if !huge.is_file() {
            return;
        }
        let ctx = egui::Context::default();
        let mut app = App::build(&ctx, Some(dir.clone()));
        let idx = app
            .entries
            .iter()
            .position(|e| e.path == huge)
            .expect("影像.jpg listed");
        app.selected = idx;
        app.act(Action::OpenViewer, &ctx);

        let mut arrived = false;
        let mut frames_after_arrival = 0;
        let mut missing_after_arrival = 0;
        for frame in 0..600 {
            let mut input = raw_input();
            input.time = Some(1.0 + frame as f64 * 0.1);
            let _ = ctx.run(input, |ctx| app.draw(ctx));
            if app.caches.full(&huge).is_some() {
                arrived = true;
                frames_after_arrival += 1;
                if frames_after_arrival > 40 {
                    break;
                }
            } else if arrived {
                missing_after_arrival += 1;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(arrived, "影像.jpg never decoded into the full cache");
        assert_eq!(
            missing_after_arrival, 0,
            "the huge image was evicted while displayed: flicker"
        );
        let entry = app.caches.full(&huge).expect("cached entry");
        assert!(entry.bytes <= MAX_FULL_BYTES, "texture over budget");
        assert_eq!((entry.width, entry.height), (10200, 14031));
    }

    fn fixture_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lumiere_ui_test_{tag}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("nested")).unwrap();
        for name in ["one.png", "two.png", "three.png"] {
            let img = image::RgbaImage::from_pixel(64, 48, image::Rgba([9, 99, 199, 255]));
            img.save(dir.join(name)).unwrap();
        }
        dir
    }

    #[test]
    fn browser_viewer_and_slideshow_frames_render() {
        let dir = fixture_dir("frames");
        let ctx = egui::Context::default();
        let mut app = App::build(&ctx, Some(dir.clone()));
        assert!(app.entries.iter().any(|e| e.is_dir && e.name == "nested"));
        assert_eq!(app.image_indices().len(), 3);

        for _ in 0..3 {
            let output = ctx.run(raw_input(), |ctx| app.draw(ctx));
            assert!(!output.shapes.is_empty());
        }

        app.act(Action::NextImage, &ctx);
        assert!(app.current_image_path().is_some());
        app.act(Action::OpenViewer, &ctx);
        assert_eq!(app.mode, Mode::Viewer);
        for _ in 0..3 {
            let output = ctx.run(raw_input(), |ctx| app.draw(ctx));
            assert!(!output.shapes.is_empty());
        }

        app.act(Action::NextImage, &ctx);
        app.act(Action::RotateCW, &ctx);
        app.act(Action::ZoomIn, &ctx);
        let output = ctx.run(raw_input(), |ctx| app.draw(ctx));
        assert!(!output.shapes.is_empty());

        app.act(Action::Slideshow, &ctx);
        assert!(app.slideshow && app.fullscreen);
        let output = ctx.run(raw_input(), |ctx| app.draw(ctx));
        assert!(!output.shapes.is_empty());

        app.act(Action::StopSlideshow, &ctx);
        app.act(Action::CloseViewer, &ctx);
        assert_eq!(app.mode, Mode::Browser);

        // Options / about / explore / delete confirmation windows.
        let plain = ctx.run(raw_input(), |ctx| app.draw(ctx)).shapes.len();
        app.act(Action::Settings, &ctx);
        app.act(Action::About, &ctx);
        app.act(Action::OpenFolder, &ctx);
        app.act(Action::MoveToFolder, &ctx);
        assert!(app.transfer.is_some());
        app.confirm_trash = Some(vec![dir.join("one.png")]);
        let output = ctx.run(raw_input(), |ctx| app.draw(ctx));
        assert!(output.shapes.len() > plain);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn keyboard_navigation_moves_the_selection() {
        let dir = fixture_dir("keys");
        let ctx = egui::Context::default();
        let mut app = App::build(&ctx, Some(dir.clone()));
        let mut input = raw_input();
        input.events.push(egui::Event::Key {
            key: egui::Key::PageDown,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
            physical_key: None,
            repeat: false,
        });
        let _ = ctx.run(input, |ctx| app.draw(ctx));
        assert_eq!(app.selected, app.image_indices()[1]);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn escape_closes_open_dialogs() {
        let dir = fixture_dir("escape");
        let ctx = egui::Context::default();
        let mut app = App::build(&ctx, Some(dir.clone()));
        app.confirm_trash = Some(vec![dir.join("one.png")]);
        app.act(Action::Settings, &ctx);
        let _ = ctx.run(raw_input(), |ctx| app.draw(ctx));
        assert!(app.confirm_trash.is_some() && app.settings_open);

        let mut input = raw_input();
        input.events.push(egui::Event::Key {
            key: egui::Key::Escape,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
            physical_key: None,
            repeat: false,
        });
        let _ = ctx.run(input, |ctx| app.draw(ctx));
        assert!(app.confirm_trash.is_none() && !app.settings_open);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn command_shortcuts_read_event_modifiers() {
        let dir = fixture_dir("cmdkeys");
        let ctx = egui::Context::default();
        let mut app = App::build(&ctx, Some(dir.clone()));
        let mut input = raw_input();
        input.events.push(egui::Event::Key {
            key: egui::Key::F,
            pressed: true,
            modifiers: egui::Modifiers::COMMAND | egui::Modifiers::CTRL,
            physical_key: None,
            repeat: false,
        });
        let _ = ctx.run(input, |ctx| app.draw(ctx));
        assert!(app.fullscreen);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn slideshow_starts_from_first_image_when_folder_selected() {
        let dir = fixture_dir("slide");
        let ctx = egui::Context::default();
        let mut app = App::build(&ctx, Some(dir.clone()));
        let folder = app
            .entries
            .iter()
            .position(|e| e.is_dir)
            .expect("fixture has a folder");
        app.selected = folder;
        app.act(Action::Slideshow, &ctx);
        assert!(app.slideshow);
        assert!(app.current_image_path().is_some());

        app.act(Action::StopSlideshow, &ctx);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn fullscreen_canvas_is_black() {
        let dir = fixture_dir("canvas");
        let ctx = egui::Context::default();
        let mut app = App::build(&ctx, Some(dir.clone()));
        // The canvas follows the current appearance, so assert against it.
        let expected = if ctx.style().visuals.dark_mode {
            egui::Color32::from_gray(0x26)
        } else {
            egui::Color32::from_gray(0x80)
        };
        assert_eq!(canvas_color(&app, &ctx), expected);
        app.act(Action::ToggleFullscreen, &ctx);
        assert_eq!(canvas_color(&app, &ctx), egui::Color32::BLACK);
        app.act(Action::ToggleFullscreen, &ctx);
        assert_eq!(canvas_color(&app, &ctx), expected);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn delete_action_targets_folders_too() {
        let dir = fixture_dir("del");
        let ctx = egui::Context::default();
        let mut app = App::build(&ctx, Some(dir.clone()));
        let folder = app
            .entries
            .iter()
            .position(|e| e.is_dir)
            .expect("fixture has a folder");
        app.selected = folder;
        app.act(Action::MoveToTrash, &ctx);
        assert_eq!(
            app.confirm_trash.as_deref(),
            Some([dir.join("nested")].as_slice())
        );

        app.confirm_trash = None;
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// `⌘⌫` on a multi-selection confirms the whole selection, not just the
    /// current entry, in grid order.
    #[test]
    fn trash_confirm_covers_the_whole_selection() {
        let dir = fixture_dir("trashmulti");
        let ctx = egui::Context::default();
        let mut app = App::build(&ctx, Some(dir.clone()));
        let images = app.image_indices();
        assert!(images.len() >= 2, "fixture has several images");
        let (first, last) = (images[0], *images.last().unwrap());
        app.selected = last;
        app.multi.clear();
        app.multi.insert(last);
        app.multi.insert(first);
        app.act(Action::MoveToTrash, &ctx);
        let paths = app.confirm_trash.clone().unwrap_or_default();
        assert_eq!(
            paths,
            vec![
                app.entries[first].path.clone(),
                app.entries[last].path.clone()
            ]
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Applying a trashed set removes the files and reports the result;
    /// nothing is reported when every item fails.
    #[test]
    fn apply_trash_reports_the_result() {
        let dir = fixture_dir("trashapply");
        let ctx = egui::Context::default();
        let mut app = App::build(&ctx, Some(dir.clone()));
        let image = app.image_indices()[0];
        let path = app.entries[image].path.clone();
        app.selected = image;

        app.apply_trash(std::slice::from_ref(&path));
        assert!(!path.exists(), "the file must be gone");
        let (message, _) = app.toast.clone().unwrap();
        assert!(message.contains("to the Trash"), "{message}");
        assert!(
            message.contains(&crate::alerts::trash_names(std::slice::from_ref(&path))[0]),
            "{message}"
        );
        assert!(app.multi.is_empty());

        // A file that is already gone cannot be moved: that is reported, not
        // silently swallowed.
        app.apply_trash(std::slice::from_ref(&path));
        let (message, _) = app.toast.clone().unwrap();
        assert!(message.contains("Couldn't move"), "{message}");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn menu_shortcuts_open_dialogs() {
        let dir = fixture_dir("menukeys");
        let ctx = egui::Context::default();
        let mut app = App::build(&ctx, Some(dir.clone()));
        let mut input = raw_input();
        for key in [egui::Key::Comma, egui::Key::I] {
            input.events.push(egui::Event::Key {
                key,
                pressed: true,
                modifiers: egui::Modifiers::COMMAND,
                physical_key: None,
                repeat: false,
            });
        }
        let _ = ctx.run(input, |ctx| app.draw(ctx));
        assert!(app.settings_open);
        assert!(app.show_inspector);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rename_updates_disk_and_selection() {
        let dir = fixture_dir("rename");
        let ctx = egui::Context::default();
        let mut app = App::build(&ctx, Some(dir.clone()));
        let image = dir.join("one.png");
        app.selected = app
            .entries
            .iter()
            .position(|e| e.path == image)
            .expect("fixture has one.png");

        app.act(Action::Rename, &ctx);
        assert_eq!(app.rename_target.as_deref(), Some(image.as_path()));
        assert_eq!(app.rename_text, "one.png");

        app.apply_rename(&image, "renamed.png");
        assert!(dir.join("renamed.png").exists());
        assert!(!image.exists());
        assert_eq!(app.current().map(|e| e.name.as_str()), Some("renamed.png"));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn opening_a_file_selects_it() {
        let dir = fixture_dir("openfile");
        let ctx = egui::Context::default();
        let app = App::build(&ctx, Some(dir.join("two.png")));
        assert_eq!(app.dir, dir);
        assert_eq!(app.current().map(|e| e.name.as_str()), Some("two.png"));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn move_and_copy_between_folders() {
        let dir = fixture_dir("transfer");
        let ctx = egui::Context::default();
        let mut app = App::build(&ctx, Some(dir.clone()));
        let other = dir.join("nested");

        app.apply_transfer(&[dir.join("one.png")], &other, true);
        assert!(dir.join("one.png").exists());
        assert!(other.join("one.png").exists());

        app.apply_transfer(&[dir.join("two.png")], &other, false);
        assert!(!dir.join("two.png").exists());
        assert!(other.join("two.png").exists());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn viewer_prefetches_the_neighbouring_images() {
        let dir = fixture_dir("prefetch");
        let ctx = egui::Context::default();
        let mut app = App::build(&ctx, Some(dir.clone()));
        app.mode = Mode::Viewer;
        app.selected = app.image_indices()[0];
        app.prefetch_neighbours();

        let queued: Vec<PathBuf> = app
            .caches
            .pending
            .iter()
            .filter(|(kind, _, _)| *kind == Kind::Full)
            .map(|(_, path, _)| path.clone())
            .collect();
        assert_eq!(queued.len(), 2);
        assert!(queued.contains(&dir.join("two.png")));
        assert!(queued.contains(&dir.join("three.png")));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn grid_queues_thumbnails_wider_than_the_viewport() {
        let dir = std::env::temp_dir().join("lumiere_ui_test_prefetch_grid");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let img = image::RgbaImage::from_pixel(64, 48, image::Rgba([9, 99, 199, 255]));
        for index in 0..200 {
            img.save(dir.join(format!("img{index:03}.png"))).unwrap();
        }

        let ctx = egui::Context::default();
        let mut app = App::build(&ctx, Some(dir.clone()));
        let _ = ctx.run(raw_input(), |ctx| app.draw(ctx));

        let thumbs = app
            .caches
            .pending
            .iter()
            .filter(|(kind, _, _)| *kind == Kind::Thumb)
            .count();
        assert!(thumbs > 8, "only {thumbs} thumbnails queued");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn saved_preferences_are_restored() {
        let dir = fixture_dir("prefs");
        let path = std::env::temp_dir().join("lumiere_prefs_test.ini");
        crate::state::State {
            last_dir: Some(dir.clone()),
            thumb_size: 200,
            sort: "size".to_owned(),
            sort_reverse: true,
            filter: "*.jpg".to_owned(),
            show_folders: 0,
            ..crate::state::State::default()
        }
        .save_to(&path)
        .unwrap();

        let saved = crate::state::State::load_from(&path);
        let ctx = egui::Context::default();
        let mut app = App::build(&ctx, Some(dir.clone()));
        apply_preferences(&mut app, &saved);

        assert_eq!(app.thumb_size, 200);
        assert_eq!(app.sort, SortKey::Size);
        assert!(app.sort_reverse);
        assert_eq!(app.filter, "*.jpg");
        assert_eq!(app.filter_edit, "*.jpg");
        assert!(!app.show_folders);

        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn purged_thumbnail_work_clears_the_pending_marker() {
        let loader = Loader::new(0);
        let mut caches = Caches::default();
        let path = Path::new("/tmp/lumiere_pending.png");

        caches.request(&loader, Kind::Thumb, path, 100);
        assert_eq!(loader.queued(), 1);

        let dropped = loader.purge_thumbs();
        caches.abandon(&dropped);
        assert_eq!(loader.queued(), 0);
        assert!(caches.pending.is_empty());

        caches.request(&loader, Kind::Thumb, path, 100);
        assert_eq!(loader.queued(), 1);
        loader.shutdown();
    }

    #[test]
    fn stale_generation_work_is_dropped_and_releasable() {
        let loader = Loader::new(0);
        let mut caches = Caches::default();

        caches.request(
            &loader,
            Kind::Preview,
            Path::new("/tmp/lumiere_stale.png"),
            100,
        );
        let (generation, dropped) = loader.bump_generation();
        assert_eq!(generation, 2);
        assert_eq!(dropped.len(), 1);

        caches.abandon(&dropped);
        assert!(caches.pending.is_empty());
        loader.shutdown();
    }

    #[test]
    fn thumb_cache_evicts_to_stay_within_budget() {
        let ctx = egui::Context::default();
        let mut caches = Caches::default();
        let image = egui::ColorImage::filled([1000, 1000], egui::Color32::WHITE);

        for index in 0..60 {
            let texture =
                ctx.load_texture(format!("thumb{index}"), image.clone(), Default::default());
            let key = (
                PathBuf::from(format!("/tmp/lumiere_budget_{index}.png")),
                100,
            );
            caches.thumb_bytes += rgba_bytes(1000, 1000);
            caches.thumbs_order.push_back(key.clone());
            caches.thumbs.insert(
                key,
                ThumbEntry {
                    texture,
                    width: 1000,
                    height: 1000,
                },
            );
        }
        caches.trim(None);

        let expected = MAX_THUMB_BYTES / rgba_bytes(1000, 1000);
        assert_eq!(caches.thumbs.len(), expected);
        assert_eq!(caches.thumbs_order.len(), expected);
        assert!(caches.thumb_bytes <= MAX_THUMB_BYTES);
        assert!(caches
            .thumbs
            .contains_key(&(PathBuf::from("/tmp/lumiere_budget_59.png"), 100)));
    }

    fn painted_text(shapes: &[egui::epaint::ClippedShape]) -> String {
        let mut out = String::new();
        for clipped in shapes {
            if let egui::Shape::Text(text) = &clipped.shape {
                out.push_str(text.galley.text());
                out.push('\n');
            }
        }
        out
    }

    #[test]
    fn preview_panel_lists_exif_fields() {
        let dir = std::env::temp_dir().join("lumiere_ui_test_exif");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let shot = dir.join("shot.jpg");
        imaging::write_exif_jpeg(&shot).unwrap();

        let ctx = egui::Context::default();
        let mut app = App::build(&ctx, Some(dir.clone()));
        app.selected = app.image_indices()[0];

        // Only the first rows fit in the panel without scrolling.
        let text = painted_once(&ctx, &mut app, "Make");
        assert!(text.contains("Format"));
        assert!(text.contains("JPEG"));
        assert!(text.contains("LumiereTest"));
        assert!(text.contains("CAM-1"));

        let exif: std::collections::HashMap<String, String> =
            app.caches.info(&shot).exif.into_iter().collect();
        assert_eq!(
            exif.get("Date/Time").map(String::as_str),
            Some("2026-10-11 09:30:00")
        );
        assert_eq!(exif.get("Exposure").map(String::as_str), Some("1/125"));
        assert_eq!(exif.get("ISO").map(String::as_str), Some("200"));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn preview_panel_lists_file_metadata() {
        let dir = fixture_dir("info");
        let ctx = egui::Context::default();
        let mut app = App::build(&ctx, Some(dir.clone()));
        app.selected = app.image_indices()[0];

        let mut text = String::new();
        for _ in 0..40 {
            let output = ctx.run(raw_input(), |ctx| app.draw(ctx));
            text = painted_text(&output.shapes);
            if text.contains("Format") {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }

        // The name appears in the header, not as a grid row.
        assert!(text.contains("one.png"));
        assert!(text.contains("Type"));
        assert!(text.contains("Path"));
        assert!(text.contains("Dimensions"));
        assert!(text.contains("PNG"));

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
