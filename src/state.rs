//! Tiny session state: the last browsed folder and the window size, stored as
//! plain `key=value` lines so there is no serialization dependency.

use std::fs;
use std::io;
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
            let value = value.trim();
            match key.trim() {
                "last_dir" => state.last_dir = Some(PathBuf::from(value)),
                "width" => state.width = value.parse().unwrap_or(0.0),
                "height" => state.height = value.parse().unwrap_or(0.0),
                "x" => state.x = value.parse().unwrap_or(0.0),
                "y" => state.y = value.parse().unwrap_or(0.0),
                "thumb_size" => state.thumb_size = value.parse().unwrap_or(0),
                "sort" => state.sort = value.to_owned(),
                "sort_reverse" => state.sort_reverse = value == "1",
                "filter" => state.filter = value.to_owned(),
                "show_folders" => state.show_folders = value.parse().unwrap_or(-1),
                _ => {}
            }
        }
        state
    }

    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut text = String::new();
        if let Some(dir) = &self.last_dir {
            text.push_str(&format!("last_dir={}\n", dir.display()));
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
            text.push_str(&format!("sort={}\n", self.sort));
        }
        text.push_str(&format!("sort_reverse={}\n", self.sort_reverse as i32));
        if !self.filter.is_empty() {
            text.push_str(&format!("filter={}\n", self.filter));
        }
        if self.show_folders >= 0 {
            text.push_str(&format!("show_folders={}\n", self.show_folders));
        }
        fs::write(path, text)
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
        };
        state.save_to(&path).unwrap();
        assert_eq!(State::load_from(&path), state);

        fs::remove_file(&path).unwrap();
        assert_eq!(State::load_from(&path), State::default());
    }
}
