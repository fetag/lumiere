//! Tiny session state: the last browsed folder and the window size, stored as
//! plain `key=value` lines so there is no serialization dependency.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub use lumiere_core::state::{parse, serialize};

#[derive(Clone, Debug, PartialEq)]
pub struct State {
    pub last_dir: Option<PathBuf>,
    pub width: f32,
    pub height: f32,
    pub x: f32,
    pub y: f32,
    /// Browsing preferences. Zero / empty means "not stored yet", and
    /// `show_folders` uses -1 for "not stored yet" because `false` is a value.
    pub thumb_size: u32,
    pub sort: String,
    pub sort_reverse: bool,
    pub filter: String,
    pub show_folders: i32,
    /// Sidebar / Inspector visibility; -1 means "not stored yet".
    pub show_sidebar: i32,
    pub show_inspector: i32,
    /// Whether a slideshow should start in full screen; -1 = "not stored yet".
    pub slideshow_fullscreen: i32,
    /// Folders the user pinned into the sidebar's Locations section.
    pub sidebar_dirs: Vec<PathBuf>,
    /// Recently visited folders, most recent first.
    pub recent_dirs: Vec<PathBuf>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            last_dir: None,
            width: 0.0,
            height: 0.0,
            x: 0.0,
            y: 0.0,
            thumb_size: 0,
            sort: String::new(),
            sort_reverse: false,
            filter: String::new(),
            show_folders: -1,
            show_sidebar: -1,
            show_inspector: -1,
            slideshow_fullscreen: -1,
            sidebar_dirs: Vec::new(),
            recent_dirs: Vec::new(),
        }
    }
}

impl State {
    pub fn file() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("Lumiere")
            .join("state.ini")
    }

    pub fn load() -> Self {
        Self::load_from(&Self::file())
    }

    pub fn save(&self) -> io::Result<()> {
        self.save_to(&Self::file())
    }

    pub fn load_from(path: &Path) -> Self {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(_) => return Self::default(),
        };
        parse(&text).into()
    }

    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = serialize(&self.clone().into());
        // A session file is user-private history (recent folders included).
        // Write then rename so a crash cannot leave a half-written state file.
        static NEXT_TEMP: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let (tmp, mut file) = loop {
            let sequence = NEXT_TEMP.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let tmp = path.with_extension(format!("ini.{}.{}.tmp", std::process::id(), sequence));
            let mut options = fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            match options.open(&tmp) {
                Ok(file) => break (tmp, file),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        };
        let result = (|| {
            file.write_all(text.as_bytes())?;
            file.sync_all()?;
            fs::rename(&tmp, path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        result
    }
}

impl From<lumiere_core::state::State> for State {
    fn from(state: lumiere_core::state::State) -> Self {
        Self {
            last_dir: state.last_dir,
            width: state.width,
            height: state.height,
            x: state.x,
            y: state.y,
            thumb_size: state.thumb_size,
            sort: state.sort,
            sort_reverse: state.sort_reverse,
            filter: state.filter,
            show_folders: state.show_folders,
            show_sidebar: state.show_sidebar,
            show_inspector: state.show_inspector,
            slideshow_fullscreen: state.slideshow_fullscreen,
            sidebar_dirs: state.sidebar_dirs,
            recent_dirs: state.recent_dirs,
        }
    }
}

impl From<State> for lumiere_core::state::State {
    fn from(state: State) -> Self {
        Self {
            last_dir: state.last_dir,
            width: state.width,
            height: state.height,
            x: state.x,
            y: state.y,
            thumb_size: state.thumb_size,
            sort: state.sort,
            sort_reverse: state.sort_reverse,
            filter: state.filter,
            show_folders: state.show_folders,
            show_sidebar: state.show_sidebar,
            show_inspector: state.show_inspector,
            slideshow_fullscreen: state.slideshow_fullscreen,
            sidebar_dirs: state.sidebar_dirs,
            recent_dirs: state.recent_dirs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_round_trips() {
        let path = std::env::temp_dir().join("lumiere_state_test.ini");
        let state = State {
            last_dir: Some(PathBuf::from("/tmp/lumiere_demo")),
            width: 1120.0,
            height: 740.0,
            x: 60.0,
            y: 80.0,
            thumb_size: 200,
            sort: "date".to_owned(),
            sort_reverse: true,
            filter: "*.jpg".to_owned(),
            show_folders: 0,
            show_sidebar: 1,
            show_inspector: 0,
            slideshow_fullscreen: 0,
            sidebar_dirs: vec![
                PathBuf::from("/Volumes/Work/exports"),
                PathBuf::from("/tmp/lumiere_demo"),
            ],
            recent_dirs: vec![
                PathBuf::from("/tmp/lumiere_demo"),
                PathBuf::from("/tmp/other"),
            ],
        };
        state.save_to(&path).unwrap();
        assert_eq!(State::load_from(&path), state);

        fs::remove_file(&path).unwrap();
        assert_eq!(State::load_from(&path), State::default());
    }

    #[test]
    fn state_escapes_line_breaks_and_backslashes() {
        let path = std::env::temp_dir().join("lumiere_state_escape_test.ini");
        let state = State {
            filter: "line1\nline2\\pattern".to_owned(),
            recent_dirs: vec![PathBuf::from("/tmp/line\nbreak")],
            ..Default::default()
        };
        state.save_to(&path).unwrap();
        assert_eq!(State::load_from(&path), state);
        fs::remove_file(&path).unwrap();
    }
}
