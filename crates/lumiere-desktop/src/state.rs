//! Tiny session state: the last browsed folder and the window size, stored as
//! plain `key=value` lines so there is no serialization dependency.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub use lumiere_core::state::{parse, serialize, State};

pub fn file() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("Lumiere")
        .join("state.ini")
}

pub fn load() -> State {
    load_from(&file())
}

pub fn save(state: &State) -> io::Result<()> {
    save_to(state, &file())
}

pub fn load_from(path: &Path) -> State {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(_) => return State::default(),
    };
    parse(&text)
}

pub fn save_to(state: &State, path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = serialize(state);
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
        save_to(&state, &path).unwrap();
        assert_eq!(load_from(&path), state);

        fs::remove_file(&path).unwrap();
        assert_eq!(load_from(&path), State::default());
    }

    #[test]
    fn state_escapes_line_breaks_and_backslashes() {
        let path = std::env::temp_dir().join("lumiere_state_escape_test.ini");
        let state = State {
            filter: "line1\nline2\\pattern".to_owned(),
            recent_dirs: vec![PathBuf::from("/tmp/line\nbreak")],
            ..Default::default()
        };
        save_to(&state, &path).unwrap();
        assert_eq!(load_from(&path), state);
        fs::remove_file(&path).unwrap();
    }
}
