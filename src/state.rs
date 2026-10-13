//! Tiny session state: the last browsed folder and the window size, stored as
//! plain `key=value` lines so there is no serialization dependency.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

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
        let mut state = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = unescape(value);
            match key.trim() {
                "last_dir" => state.last_dir = Some(PathBuf::from(&value)),
                "width" => state.width = value.parse().unwrap_or(0.0),
                "height" => state.height = value.parse().unwrap_or(0.0),
                "x" => state.x = value.parse().unwrap_or(0.0),
                "y" => state.y = value.parse().unwrap_or(0.0),
                "thumb_size" => state.thumb_size = value.parse().unwrap_or(0),
                "sort" => state.sort = value,
                "sort_reverse" => state.sort_reverse = value == "1",
                "filter" => state.filter = value,
                "show_folders" => state.show_folders = value.parse().unwrap_or(-1),
                "show_sidebar" => state.show_sidebar = value.parse().unwrap_or(-1),
                "show_inspector" => state.show_inspector = value.parse().unwrap_or(-1),
                "slideshow_fullscreen" => state.slideshow_fullscreen = value.parse().unwrap_or(-1),
                key if key.starts_with("sidebar_dir_") => {
                    if let Ok(i) = key["sidebar_dir_".len()..].parse::<usize>() {
                        if i < 64 {
                            state.sidebar_dirs.push(PathBuf::from(value));
                        }
                    }
                }
                key if key.starts_with("recent_dir_") => {
                    if let Ok(i) = key["recent_dir_".len()..].parse::<usize>() {
                        if i < 64 {
                            state.recent_dirs.push(PathBuf::from(value));
                        }
                    }
                }
                _ => {}
            }
        }
        state.width = valid_size(state.width, 640.0, 16_384.0);
        state.height = valid_size(state.height, 420.0, 16_384.0);
        if !state.x.is_finite() || state.x.abs() > 100_000.0 {
            state.x = 0.0;
        }
        if !state.y.is_finite() || state.y.abs() > 100_000.0 {
            state.y = 0.0;
        }
        if state.thumb_size != 0 {
            state.thumb_size = state.thumb_size.clamp(32, 320);
        }
        state
    }

    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut text = String::new();
        if let Some(dir) = &self.last_dir {
            text.push_str(&format!(
                "last_dir={}\n",
                escape(&dir.display().to_string())
            ));
        }
        if self.width > 0.0 {
            text.push_str(&format!("width={}\n", self.width));
        }
        if self.height > 0.0 {
            text.push_str(&format!("height={}\n", self.height));
        }
        if self.x != 0.0 || self.y != 0.0 {
            text.push_str(&format!("x={}\ny={}\n", self.x, self.y));
        }
        if self.thumb_size > 0 {
            text.push_str(&format!("thumb_size={}\n", self.thumb_size));
        }
        if !self.sort.is_empty() {
            text.push_str(&format!("sort={}\n", escape(&self.sort)));
        }
        text.push_str(&format!("sort_reverse={}\n", self.sort_reverse as i32));
        if !self.filter.is_empty() {
            text.push_str(&format!("filter={}\n", escape(&self.filter)));
        }
        if self.show_folders >= 0 {
            text.push_str(&format!("show_folders={}\n", self.show_folders));
        }
        if self.show_sidebar >= 0 {
            text.push_str(&format!("show_sidebar={}\n", self.show_sidebar));
        }
        if self.show_inspector >= 0 {
            text.push_str(&format!("show_inspector={}\n", self.show_inspector));
        }
        if self.slideshow_fullscreen >= 0 {
            text.push_str(&format!(
                "slideshow_fullscreen={}\n",
                self.slideshow_fullscreen
            ));
        }
        for (i, dir) in self.sidebar_dirs.iter().enumerate() {
            text.push_str(&format!(
                "sidebar_dir_{}={}\n",
                i,
                escape(&dir.display().to_string())
            ));
        }
        for (i, dir) in self.recent_dirs.iter().enumerate() {
            text.push_str(&format!(
                "recent_dir_{}={}\n",
                i,
                escape(&dir.display().to_string())
            ));
        }
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

fn valid_size(value: f32, minimum: f32, maximum: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value.clamp(minimum, maximum)
    } else {
        0.0
    }
}

fn escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }
    out
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
