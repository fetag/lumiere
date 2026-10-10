//! Application state, window chrome (menu bar, toolbars, status bar) and the
//! command dispatch used by menus, toolbar buttons and the keyboard alike.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui::{Context, TextureHandle, TextureOptions, Vec2};

use crate::classic::{self, Icon};
use crate::fsmodel::{self, Entry, SortKey, TreeModel};
use crate::imaging::{self, Info};
use crate::loader::{Kind, Loader, Request};

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
    thumb_bytes: usize,
    full_bytes: usize,
    pub pending: HashSet<(Kind, PathBuf, u32)>,
}

const MAX_THUMBS: usize = 4000;
const MAX_FULLS: usize = 6;
/// Resident RGBA budgets, so a folder with thousands of images cannot grow the
/// texture cache without bound.
const MAX_THUMB_BYTES: usize = 96 * 1024 * 1024;
const MAX_FULL_BYTES: usize = 192 * 1024 * 1024;

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

    /// Requests dropped by the loader never come back as replies, so their
    /// pending markers must be released or the paths are never decoded again.
    pub fn abandon(&mut self, dropped: &[Request]) {
        for request in dropped {
            self.pending
                .remove(&(request.kind, request.path.clone(), request.size));
        }
    }

    pub fn request(&mut self, loader: &Loader, kind: Kind, path: &Path, size: u32) {
        let key = (kind, path.to_path_buf(), size);
        if self.pending.contains(&key) {
            return;
        }
        if self.pending.len() > 512 {
            return;
        }
        loader.submit(kind, path.to_path_buf(), size);
        self.pending.insert(key);
    }

    /// Move finished decodes into the texture caches. Returns how many arrived.
    pub fn pump(&mut self, ctx: &Context, loader: &Loader) -> usize {
        let replies = loader.drain(24);
        let arrived = replies.len();
        for reply in replies {
            let request = reply.request;
            self.pending
                .remove(&(request.kind, request.path.clone(), request.size));
            let built = match reply.result {
                Ok(decoded) => {
                    let image = if decoded.premultiplied {
                        egui::ColorImage::from_rgba_premultiplied(
                            [decoded.width as usize, decoded.height as usize],
                            &decoded.rgba,
                        )
                    } else {
                        egui::ColorImage::from_rgba_unmultiplied(
                            [decoded.width as usize, decoded.height as usize],
                            &decoded.rgba,
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
                    Some((texture, decoded.width, decoded.height))
                }
                Err(error) => {
                    log::warn!("{}: {error}", request.path.display());
                    None
                }
            };
            let Some((texture, width, height)) = built else {
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
                    self.thumb_bytes += rgba_bytes(width, height);
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
                    if let Some(old) = self.fulls.remove(&key) {
                        self.full_bytes -= rgba_bytes(old.width, old.height);
                        if let Some(at) = self.fulls_order.iter().position(|k| *k == key) {
                            self.fulls_order.remove(at);
                        }
                    }
                    self.full_bytes += rgba_bytes(width, height);
                    self.fulls_order.push_back(key.clone());
                    self.fulls.insert(
                        key,
                        FullEntry {
                            texture,
                            width,
                            height,
                            info,
                        },
                    );
                }
            }
        }
        self.trim();
        arrived
    }

    fn trim(&mut self) {
        while self.thumbs_order.len() > MAX_THUMBS || self.thumb_bytes > MAX_THUMB_BYTES {
            let Some(key) = self.thumbs_order.pop_front() else {
                break;
            };
            if let Some(old) = self.thumbs.remove(&key) {
                self.thumb_bytes -= rgba_bytes(old.width, old.height);
            }
        }
        while self.fulls_order.len() > MAX_FULLS || self.full_bytes > MAX_FULL_BYTES {
            let Some(key) = self.fulls_order.pop_front() else {
                break;
            };
            if let Some(old) = self.fulls.remove(&key) {
                self.full_bytes -= rgba_bytes(old.width, old.height);
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
    Open,
    ExploreFolder,
    NextFolder,
    PrevFolder,
    UpFolder,
    GoHome,
    GoPictures,
    Refresh,
    Delete,
    Properties,
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
    ToggleFolders,
    ToggleTree,
    TogglePreview,
    ToggleToolbar,
    ToggleStatus,

    Options,
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

    pub show_tree: bool,
    pub show_preview: bool,
    pub show_toolbar: bool,
    pub show_places: bool,
    pub show_status: bool,
    pub preview_tab: usize,

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

    pub options_open: bool,
    pub about_open: bool,
    pub explore_open: bool,
    pub explore_path: PathBuf,
    pub confirm_delete: Option<PathBuf>,
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

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, start: Option<PathBuf>) -> Self {
        Self::build(&cc.egui_ctx, start)
    }

    /// Build the application state; split out from [`Self::new`] so the whole
    /// UI can be exercised head-lessly from tests.
    pub fn build(ctx: &Context, start: Option<PathBuf>) -> Self {
        classic::apply_theme(ctx);
        classic::install_cjk_font(ctx);

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
        } else {
            Some(egui::pos2(saved.x, saved.y))
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
            show_tree: true,
            show_preview: true,
            show_toolbar: true,
            show_places: true,
            show_status: true,
            preview_tab: 0,
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
            options_open: false,
            about_open: false,
            explore_open: false,
            explore_path: dir.clone(),
            confirm_delete: None,
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
            Action::Open => {
                if let Some(path) = self.current_image_path() {
                    let _ = std::process::Command::new("open").arg(&path).spawn();
                }
            }
            Action::ExploreFolder => {
                self.explore_open = true;
                self.explore_path = self.dir.clone();
            }
            Action::NextFolder => {
                if let Some(next) = fsmodel::sibling_folder(&self.dir.clone(), true) {
                    self.set_dir(next);
                }
            }
            Action::PrevFolder => {
                if let Some(prev) = fsmodel::sibling_folder(&self.dir.clone(), false) {
                    self.set_dir(prev);
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
            Action::Refresh => self.rescan(),
            Action::Delete => {
                if let Some(entry) = self.current() {
                    self.confirm_delete = Some(entry.path.clone());
                }
            }
            Action::Rename => {
                if let Some(entry) = self.current().cloned() {
                    self.rename_text = entry.name.clone();
                    self.rename_target = Some(entry.path.clone());
                }
            }
            Action::MoveToFolder => self.start_transfer(false),
            Action::CopyToFolder => self.start_transfer(true),
            Action::Properties => {
                self.preview_tab = 1;
                self.show_preview = true;
            }
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
            Action::ToggleFolders => self.show_folders = !self.show_folders,
            Action::ToggleTree => self.show_tree = !self.show_tree,
            Action::TogglePreview => self.show_preview = !self.show_preview,
            Action::ToggleToolbar => self.show_toolbar = !self.show_toolbar,
            Action::ToggleStatus => self.show_status = !self.show_status,
            Action::Options => self.options_open = true,
            Action::About => self.about_open = true,
        }
        if matches!(
            action,
            Action::SortByName
                | Action::SortByDate
                | Action::SortBySize
                | Action::SortByType
                | Action::ToggleFolders
        ) {
            self.rescan();
        }
    }

    // -- keyboard ----------------------------------------------------------

    fn handle_keys(&mut self, ctx: &Context) {
        if ctx.memory(|m| m.focused().is_some()) {
            return;
        }
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
                if i.consume_key(egui::Modifiers::NONE, egui::Key::Space) {
                    actions.push(Action::OpenViewer);
                }
                if i.consume_key(egui::Modifiers::NONE, egui::Key::PageDown) {
                    actions.push(Action::NextImage);
                }
                if i.consume_key(egui::Modifiers::NONE, egui::Key::PageUp) {
                    actions.push(Action::PrevImage);
                }
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Enter) {
                actions.push(Action::OpenViewer);
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
                if self.confirm_delete.is_some()
                    || self.rename_target.is_some()
                    || self.transfer.is_some()
                    || self.options_open
                    || self.about_open
                    || self.explore_open
                {
                    self.confirm_delete = None;
                    self.rename_target = None;
                    self.transfer = None;
                    self.options_open = false;
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
            if i.consume_key(egui::Modifiers::NONE, egui::Key::F2) {
                actions.push(Action::Rename);
            }
            if i.consume_key(egui::Modifiers::COMMAND, egui::Key::Comma) {
                actions.push(Action::Options);
            }
            if i.consume_key(egui::Modifiers::COMMAND, egui::Key::I) {
                actions.push(Action::Properties);
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::F11) {
                actions.push(Action::ToggleFullscreen);
            }
            if i.consume_key(egui::Modifiers::COMMAND, egui::Key::F) {
                actions.push(Action::ToggleFullscreen);
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Home) {
                actions.push(Action::FirstImage);
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::End) {
                actions.push(Action::LastImage);
            }
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
            if i.consume_key(
                egui::Modifiers::SHIFT | egui::Modifiers::COMMAND,
                egui::Key::R,
            ) {
                actions.push(Action::RotateCCW);
            }
            if i.consume_key(egui::Modifiers::COMMAND, egui::Key::R) {
                actions.push(Action::RotateCW);
            }
            if i.consume_key(
                egui::Modifiers::SHIFT | egui::Modifiers::COMMAND,
                egui::Key::A,
            ) {
                actions.push(Action::SelectNone);
            }
            if i.consume_key(egui::Modifiers::COMMAND, egui::Key::A) {
                actions.push(Action::SelectAll);
            }
            if i.consume_key(egui::Modifiers::COMMAND, egui::Key::Delete) {
                actions.push(Action::Delete);
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Delete) {
                actions.push(Action::Delete);
            }
            if i.consume_key(
                egui::Modifiers::SHIFT | egui::Modifiers::COMMAND,
                egui::Key::Y,
            ) {
                actions.push(if self.slideshow {
                    Action::StopSlideshow
                } else {
                    Action::Slideshow
                });
            }
            if i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab) {
                actions.push(Action::PrevFolder);
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Tab) {
                actions.push(Action::NextFolder);
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
        let arrived = self.caches.pump(ctx, &self.loader);
        if arrived > 0 || !self.caches.pending.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(40));
        }
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        let mut actions: Vec<Action> = Vec::new();
        let sort = self.sort;
        let (toolbar, tree, preview, status, folders, _reverse, slideshow) = (
            self.show_toolbar,
            self.show_tree,
            self.show_preview,
            self.show_status,
            self.show_folders,
            self.sort_reverse,
            self.slideshow,
        );
        egui::MenuBar::new().ui(ui, |ui| {
            top_menu(ui, "File", |ui| {
                menu(ui, &mut actions, Action::Open, "Open", "Enter");
                menu(
                    ui,
                    &mut actions,
                    Action::ExploreFolder,
                    "Explore Folder...",
                    "",
                );
                ui.separator();
                menu(ui, &mut actions, Action::NextFolder, "Next Folder", "Tab");
                menu(
                    ui,
                    &mut actions,
                    Action::PrevFolder,
                    "Previous Folder",
                    "Shift+Tab",
                );
                menu(ui, &mut actions, Action::UpFolder, "Up One Level", "");
                ui.separator();
                menu(ui, &mut actions, Action::Delete, "Delete", "Del");
                menu(
                    ui,
                    &mut actions,
                    Action::Properties,
                    "Properties...",
                    "Cmd+I",
                );
                menu(ui, &mut actions, Action::Rename, "Rename...", "F2");
                menu(
                    ui,
                    &mut actions,
                    Action::MoveToFolder,
                    "Move to Folder...",
                    "",
                );
                menu(
                    ui,
                    &mut actions,
                    Action::CopyToFolder,
                    "Copy to Folder...",
                    "",
                );
                ui.separator();
                menu(ui, &mut actions, Action::Quit, "Exit", "Cmd+Q");
            });
            top_menu(ui, "Edit", |ui| {
                menu(ui, &mut actions, Action::SelectAll, "Select All", "Cmd+A");
                menu(
                    ui,
                    &mut actions,
                    Action::SelectNone,
                    "Select None",
                    "Cmd+Shift+A",
                );
                menu(
                    ui,
                    &mut actions,
                    Action::InvertSelection,
                    "Invert Selection",
                    "",
                );
                ui.separator();
                menu(ui, &mut actions, Action::RotateCW, "Rotate Right", "Cmd+R");
                menu(
                    ui,
                    &mut actions,
                    Action::RotateCCW,
                    "Rotate Left",
                    "Cmd+Shift+R",
                );
                menu(ui, &mut actions, Action::FlipH, "Flip Horizontal", "");
                menu(ui, &mut actions, Action::FlipV, "Flip Vertical", "");
            });
            top_menu(ui, "View", |ui| {
                check(ui, &mut actions, Action::ToggleToolbar, "Toolbar", toolbar);
                check(ui, &mut actions, Action::ToggleTree, "Folder Tree", tree);
                check(
                    ui,
                    &mut actions,
                    Action::TogglePreview,
                    "Preview Pane",
                    preview,
                );
                check(ui, &mut actions, Action::ToggleStatus, "Status Bar", status);
                ui.separator();
                menu(ui, &mut actions, Action::ThumbSmall, "Small Thumbnails", "");
                menu(
                    ui,
                    &mut actions,
                    Action::ThumbMedium,
                    "Medium Thumbnails",
                    "",
                );
                menu(ui, &mut actions, Action::ThumbLarge, "Large Thumbnails", "");
                ui.separator();
                top_menu(ui, "Sort", |ui| {
                    check(
                        ui,
                        &mut actions,
                        Action::SortByName,
                        "by Name",
                        sort == SortKey::Name,
                    );
                    check(
                        ui,
                        &mut actions,
                        Action::SortByDate,
                        "by Date",
                        sort == SortKey::Date,
                    );
                    check(
                        ui,
                        &mut actions,
                        Action::SortBySize,
                        "by Size",
                        sort == SortKey::Size,
                    );
                    check(
                        ui,
                        &mut actions,
                        Action::SortByType,
                        "by Type",
                        sort == SortKey::Type,
                    );
                    let reverse = ui.checkbox(&mut self.sort_reverse, "Reverse order");
                    classic::note_rect(ui, "Reverse order", reverse.rect);
                    if reverse.changed() {
                        actions.push(Action::Refresh);
                    }
                });
                check(
                    ui,
                    &mut actions,
                    Action::ToggleFolders,
                    "Show Folders",
                    folders,
                );
                ui.separator();
                menu(ui, &mut actions, Action::ZoomIn, "Zoom In", "+");
                menu(ui, &mut actions, Action::ZoomOut, "Zoom Out", "-");
                menu(ui, &mut actions, Action::FitWindow, "Fit to Window", "F");
                menu(ui, &mut actions, Action::ActualSize, "Actual Size", "1");
                ui.separator();
                menu(
                    ui,
                    &mut actions,
                    Action::ToggleFullscreen,
                    "Full Screen",
                    "F11 / Cmd+F",
                );
                menu(ui, &mut actions, Action::Refresh, "Refresh", "F5");
            });
            top_menu(ui, "Go", |ui| {
                menu(ui, &mut actions, Action::GoHome, "Home Folder", "");
                menu(ui, &mut actions, Action::GoPictures, "Pictures", "");
                ui.separator();
                menu(ui, &mut actions, Action::FirstImage, "First Image", "Home");
                menu(
                    ui,
                    &mut actions,
                    Action::PrevImage,
                    "Previous Image",
                    "Backspace",
                );
                menu(ui, &mut actions, Action::NextImage, "Next Image", "Space");
                menu(ui, &mut actions, Action::LastImage, "Last Image", "End");
                ui.separator();
                menu(ui, &mut actions, Action::OpenViewer, "View Image", "Enter");
                menu(
                    ui,
                    &mut actions,
                    Action::CloseViewer,
                    "Back to Browser",
                    "Esc",
                );
                ui.separator();
                menu(
                    ui,
                    &mut actions,
                    if slideshow {
                        Action::StopSlideshow
                    } else {
                        Action::Slideshow
                    },
                    "Slideshow...",
                    "Cmd+Shift+Y",
                );
            });
            top_menu(ui, "Tools", |ui| {
                menu(ui, &mut actions, Action::Options, "Options...", "Cmd+,");
            });
            top_menu(ui, "Help", |ui| {
                menu(ui, &mut actions, Action::About, "About Lumiere", "");
            });
        });
        let ctx = ui.ctx().clone();
        for action in actions {
            self.act(action, &ctx);
        }
    }

    fn tool_row(&mut self, ui: &mut egui::Ui) {
        let mut pending: Vec<Action> = Vec::new();
        let mut open_dialog = false;
        ui.horizontal(|ui| {
            if classic::tool_button(ui, Icon::FolderUp, "Up one level", true).clicked() {
                pending.push(Action::UpFolder);
            }
            if classic::tool_button(ui, Icon::Home, "Home folder", true).clicked() {
                pending.push(Action::GoHome);
            }
            if classic::tool_button(ui, Icon::Refresh, "Refresh", true).clicked() {
                pending.push(Action::Refresh);
            }
            classic::vtool_separator(ui);
            if classic::tool_button(ui, Icon::ThumbSmall, "Small thumbnails", true).clicked() {
                pending.push(Action::ThumbSmall);
            }
            if classic::tool_button(ui, Icon::ThumbMedium, "Medium thumbnails", true).clicked() {
                pending.push(Action::ThumbMedium);
            }
            if classic::tool_button(ui, Icon::ThumbLarge, "Large thumbnails", true).clicked() {
                pending.push(Action::ThumbLarge);
            }
            classic::vtool_separator(ui);
            if classic::tool_button(ui, Icon::First, "First image", true).clicked() {
                pending.push(Action::FirstImage);
            }
            if classic::tool_button(ui, Icon::Prev, "Previous image", true).clicked() {
                pending.push(Action::PrevImage);
            }
            if classic::tool_button(ui, Icon::Next, "Next image", true).clicked() {
                pending.push(Action::NextImage);
            }
            if classic::tool_button(ui, Icon::Last, "Last image", true).clicked() {
                pending.push(Action::LastImage);
            }
            classic::vtool_separator(ui);
            if classic::tool_button(ui, Icon::Slideshow, "Slideshow", true).clicked() {
                pending.push(Action::Slideshow);
            }
            if classic::tool_button(ui, Icon::Stop, "Stop slideshow", self.slideshow).clicked() {
                pending.push(Action::StopSlideshow);
            }
            classic::vtool_separator(ui);
            if classic::tool_button(ui, Icon::ZoomOut, "Zoom out", true).clicked() {
                pending.push(Action::ZoomOut);
            }
            if classic::tool_button(ui, Icon::ZoomIn, "Zoom in", true).clicked() {
                pending.push(Action::ZoomIn);
            }
            if classic::tool_button(ui, Icon::Fit, "Fit to window", true).clicked() {
                pending.push(Action::FitWindow);
            }
            if classic::tool_button(ui, Icon::Actual, "Actual size", true).clicked() {
                pending.push(Action::ActualSize);
            }
            classic::vtool_separator(ui);
            if classic::tool_button(ui, Icon::RotateLeft, "Rotate left", true).clicked() {
                pending.push(Action::RotateCCW);
            }
            if classic::tool_button(ui, Icon::RotateRight, "Rotate right", true).clicked() {
                pending.push(Action::RotateCW);
            }
            if classic::tool_button(ui, Icon::FlipH, "Flip horizontal", true).clicked() {
                pending.push(Action::FlipH);
            }
            if classic::tool_button(ui, Icon::FlipV, "Flip vertical", true).clicked() {
                pending.push(Action::FlipV);
            }
            classic::vtool_separator(ui);
            if classic::tool_button(ui, Icon::Fullscreen, "Full screen (F11)", true).clicked() {
                pending.push(Action::ToggleFullscreen);
            }
            if classic::tool_button(
                ui,
                Icon::Delete,
                "Delete",
                self.current_image_path().is_some(),
            )
            .clicked()
            {
                pending.push(Action::Delete);
            }
            if classic::tool_button(ui, Icon::Info, "Properties", true).clicked() {
                open_dialog = true;
                pending.push(Action::Properties);
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
            classic::note_rect(ui, "Go", go.rect);
            if go.clicked() {
                navigate = Some(PathBuf::from(self.path_edit.trim()));
            }
            ui.label("Files:");
            let filter_response = ui.add_sized(
                [90.0, 20.0],
                egui::TextEdit::singleline(&mut self.filter_edit),
            );
            classic::note_rect(ui, "Files filter", filter_response.rect);
            let filter_lost_focus = filter_response.lost_focus();
            if filter_lost_focus && self.filter_edit != self.filter {
                self.filter = self.filter_edit.clone();
                rescan = true;
            }
            let apply = ui.button("Apply");
            classic::note_rect(ui, "Apply", apply.rect);
            if apply.clicked() {
                self.filter = self.filter_edit.clone();
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
                classic::note_rect(ui, &label, response.rect);
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
            classic::status_field(ui, &status, 320.0);
            classic::status_field(ui, &middle, (ui.available_width() - 120.0).max(80.0));
            classic::status_field(ui, &right, 90.0);
        });
    }

    // -- dialogs -----------------------------------------------------------

    fn options_window(&mut self, ctx: &Context) {
        let mut clear_cache = false;
        let mut rescan = false;
        let mut open = self.options_open;
        egui::Window::new("Options")
            .open(&mut open)
            .resizable(false)
            .default_pos([320.0, 200.0])
            .show(ctx, |ui| {
                classic::section_label(ui, "Thumbnails");
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
                classic::section_label(ui, "Viewer");
                if ui
                    .checkbox(&mut self.canvas_dark, "Black background")
                    .changed()
                {
                    ctx.request_repaint();
                }
                classic::section_label(ui, "Slideshow");
                ui.horizontal(|ui| {
                    ui.label("Delay:");
                    ui.add(
                        egui::Slider::new(&mut self.slideshow_delay, 0.5..=30.0)
                            .suffix(" s")
                            .step_by(0.5),
                    );
                });
                classic::section_label(ui, "Maintenance");
                if ui.button("Clear thumbnail cache").clicked() {
                    clear_cache = true;
                }
                ui.label(format!("Cache folder: {}", imaging::cache_root().display()));
            });
        self.options_open = open;

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
                    classic::draw_icon(
                        ui.painter(),
                        rect.center(),
                        36.0,
                        Icon::Image,
                        classic::TEXT,
                    );
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("Lumiere").strong());
                        ui.label("Classic browser / viewer, rebuilt in Rust");
                        ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                        ui.label(
                            egui::RichText::new("egui + image + Apple ImageIO (HEIC/AVIF)").small(),
                        );
                    });
                });
                ui.separator();
                ui.label(
                    "Keys: Space/Right next, Backspace/Left previous, Enter open, Esc close, \
                     +/- zoom, F fit, 1 actual size, F11 full screen, Cmd+Shift+Y slideshow.",
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

    fn start_transfer(&mut self, copy: bool) {
        let mut paths: Vec<PathBuf> = self
            .multi
            .iter()
            .filter_map(|&index| self.entries.get(index))
            .map(|entry| entry.path.clone())
            .collect();
        if paths.is_empty() {
            if let Some(entry) = self.current() {
                paths.push(entry.path.clone());
            }
        }
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

    fn confirm_delete_window(&mut self, ctx: &Context) {
        let Some(path) = self.confirm_delete.clone() else {
            return;
        };
        egui::Window::new("Confirm Delete")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                let noun = if path.is_dir() { "folder" } else { "file" };
                ui.label(format!("Move this {noun} to the Trash?"));
                ui.label(egui::RichText::new(path.display().to_string()).monospace());
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Yes").clicked() {
                        match trash::delete(&path) {
                            Ok(()) => self.notify(format!("Trashed {}", path.display())),
                            Err(error) => {
                                self.notify(format!("Delete failed: {error}"));
                            }
                        }
                        self.confirm_delete = None;
                        self.rescan();
                    }
                    if ui.button("No").clicked() {
                        self.confirm_delete = None;
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
            };
            let _ = state.save();
        }
        self.handle_keys(ctx);
        self.tick_slideshow(ctx);

        if let Some((_, at)) = self.toast {
            if at.elapsed().as_secs_f32() > 3.0 {
                self.toast = None;
            }
        }

        let chrome = !self.fullscreen && self.mode == Mode::Browser;

        if chrome {
            egui::TopBottomPanel::top("menubar")
                .exact_height(22.0)
                .show(ctx, |ui| {
                    self.menu_bar(ui);
                });
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
            if self.show_tree {
                egui::SidePanel::left("foldertree")
                    .resizable(true)
                    .default_width(240.0)
                    .min_width(140.0)
                    .show(ctx, |ui| {
                        crate::browser::tree_pane(self, ui);
                    });
            }
            if self.show_preview {
                egui::SidePanel::right("previewpane")
                    .resizable(true)
                    .default_width(280.0)
                    .min_width(160.0)
                    .show(ctx, |ui| {
                        crate::browser::preview_pane(self, ui);
                    });
            }
        } else if !self.fullscreen {
            egui::TopBottomPanel::top("menubar")
                .exact_height(22.0)
                .show(ctx, |ui| {
                    self.menu_bar(ui);
                });
            if self.show_status {
                egui::TopBottomPanel::bottom("statusbar")
                    .exact_height(24.0)
                    .show(ctx, |ui| {
                        self.status_bar(ui);
                    });
            }
        }

        let fill = if self.mode == Mode::Viewer {
            canvas_color(self)
        } else {
            egui::Color32::WHITE
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
        self.confirm_delete_window(ctx);
    }
}

fn menu_row(label: &str, shortcut: &str, checked: Option<bool>) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let mark = egui::TextFormat {
        font_id: egui::FontId::proportional(13.0),
        color: classic::TEXT,
        ..Default::default()
    };
    let prefix = match checked {
        Some(true) => "\u{2713} ",
        Some(false) => "   ",
        None => "",
    };
    job.append(&format!("{prefix}{label}"), 0.0, mark.clone());
    if !shortcut.is_empty() {
        let weak = egui::TextFormat {
            font_id: egui::FontId::proportional(11.0),
            color: classic::SHADOW,
            ..Default::default()
        };
        job.append(&format!("    {shortcut}"), 24.0, weak);
    }
    job
}

fn top_menu(ui: &mut egui::Ui, label: &str, contents: impl FnOnce(&mut egui::Ui)) {
    let response = ui.menu_button(label, contents);
    classic::note_rect(ui, label, response.response.rect);
}

fn menu(ui: &mut egui::Ui, actions: &mut Vec<Action>, action: Action, label: &str, shortcut: &str) {
    let response =
        ui.add(egui::Button::new(menu_row(label, shortcut, None)).min_size(egui::vec2(240.0, 0.0)));
    classic::note_rect(ui, label, response.rect);
    if response.clicked() {
        actions.push(action);
        ui.close();
    }
}

fn check(ui: &mut egui::Ui, actions: &mut Vec<Action>, action: Action, label: &str, checked: bool) {
    let response = ui.add(
        egui::Button::new(menu_row(label, "", Some(checked))).min_size(egui::vec2(240.0, 0.0)),
    );
    classic::note_rect(ui, label, response.rect);
    if response.clicked() {
        actions.push(action);
        ui.close();
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

pub fn canvas_color(app: &App) -> egui::Color32 {
    if app.canvas_dark || app.fullscreen {
        egui::Color32::from_gray(0)
    } else {
        classic::CANVAS
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
        let Some(rect) = classic::rect_of(ctx, label) else {
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
        classic::clear_rects(ctx);
        let mut input = raw_input();
        input.time = Some(*time);
        let _ = ctx.run(input, |ctx| app.draw(ctx));
        *time += 0.4;
    }

    fn hover(ctx: &Context, app: &mut App, pos: egui::Pos2, time: &mut f64) {
        let mut input = raw_input();
        input.time = Some(*time);
        input.events = vec![egui::Event::PointerMoved(pos)];
        let _ = ctx.run(input, |ctx| app.draw(ctx));
        *time += 0.4;
    }

    /// Walks a menu path such as ["View", "Sort"] and leaves the deepest submenu
    /// open. Only the first level needs a click; egui opens nested submenus as
    /// soon as the pointer hovers the parent entry.
    fn open_menu(ctx: &Context, app: &mut App, path: &[&str], time: &mut f64) -> bool {
        click_at(ctx, app, egui::pos2(600.0, 700.0), time);
        for (depth, label) in path.iter().enumerate() {
            settle(ctx, app, time);
            let Some(rect) = classic::rect_of(ctx, label) else {
                return false;
            };
            if depth == 0 {
                click_at(ctx, app, rect.center(), time);
            } else {
                hover(ctx, app, rect.center(), time);
            }
        }
        settle(ctx, app, time);
        true
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
        let mut time = 1.0;
        assert!(open_menu(&ctx, &mut app, &["View", "Sort"], &mut time));
        assert!(click_label(&ctx, &mut app, "by Size", &mut time));

        assert_eq!(app.sort, SortKey::Size);
        let names: Vec<String> = app.entries.iter().map(|e| e.name.clone()).collect();
        assert_eq!(names, ["a.png", "c.png", "b.png"]);

        assert!(open_menu(&ctx, &mut app, &["View", "Sort"], &mut time));
        assert!(click_label(&ctx, &mut app, "Reverse order", &mut time));
        let names: Vec<String> = app.entries.iter().map(|e| e.name.clone()).collect();
        assert_eq!(names, ["b.png", "c.png", "a.png"]);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn view_menu_toggles_panels_and_thumb_size() {
        let dir = fixture_dir("viewmenu");
        let ctx = egui::Context::default();
        let mut app = App::build(&ctx, Some(dir.clone()));
        let mut time = 1.0;

        assert!(app.show_tree && app.thumb_size == 120);
        assert!(open_menu(&ctx, &mut app, &["View"], &mut time));
        assert!(click_label(&ctx, &mut app, "Folder Tree", &mut time));
        assert!(!app.show_tree);

        assert!(open_menu(&ctx, &mut app, &["View"], &mut time));
        assert!(click_label(&ctx, &mut app, "Small Thumbnails", &mut time));
        assert_eq!(app.thumb_size, 64);

        for label in ["Preview Pane", "Toolbar", "Status Bar"] {
            assert!(open_menu(&ctx, &mut app, &["View"], &mut time));
            assert!(click_label(&ctx, &mut app, label, &mut time));
        }
        assert!(!app.show_preview && !app.show_toolbar && !app.show_status);

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
        app.act(Action::ExploreFolder, &ctx);
        let text = painted_once(&ctx, &mut app, "Explore");
        eprintln!("DBG explore<<{text}>>END");
        assert!(text.contains("Explore"));

        std::fs::remove_dir_all(&dir).unwrap();
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
        app.act(Action::Options, &ctx);
        app.act(Action::About, &ctx);
        app.act(Action::ExploreFolder, &ctx);
        app.act(Action::MoveToFolder, &ctx);
        assert!(app.transfer.is_some());
        app.confirm_delete = Some(dir.join("one.png"));
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
        app.confirm_delete = Some(dir.join("one.png"));
        app.act(Action::Options, &ctx);
        let _ = ctx.run(raw_input(), |ctx| app.draw(ctx));
        assert!(app.confirm_delete.is_some() && app.options_open);

        let mut input = raw_input();
        input.events.push(egui::Event::Key {
            key: egui::Key::Escape,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
            physical_key: None,
            repeat: false,
        });
        let _ = ctx.run(input, |ctx| app.draw(ctx));
        assert!(app.confirm_delete.is_none() && !app.options_open);

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
            modifiers: egui::Modifiers::COMMAND,
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
        assert_eq!(canvas_color(&app), classic::CANVAS);
        app.act(Action::ToggleFullscreen, &ctx);
        assert_eq!(canvas_color(&app), egui::Color32::BLACK);
        app.act(Action::ToggleFullscreen, &ctx);
        assert_eq!(canvas_color(&app), classic::CANVAS);

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
        app.act(Action::Delete, &ctx);
        assert_eq!(
            app.confirm_delete.as_deref(),
            Some(dir.join("nested").as_path())
        );

        app.confirm_delete = None;
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
        assert!(app.options_open);
        assert_eq!(app.preview_tab, 1);

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
        caches.trim();

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
    fn information_tab_lists_file_metadata() {
        let dir = fixture_dir("info");
        let ctx = egui::Context::default();
        let mut app = App::build(&ctx, Some(dir.clone()));
        app.selected = app.image_indices()[0];
        app.preview_tab = 1;

        let mut text = String::new();
        for _ in 0..40 {
            let output = ctx.run(raw_input(), |ctx| app.draw(ctx));
            text = painted_text(&output.shapes);
            if text.contains("Format") {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }

        assert!(text.contains("Name"));
        assert!(text.contains("one.png"));
        assert!(text.contains("Path"));
        assert!(text.contains("Dimensions"));
        assert!(text.contains("PNG"));

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
