//! Directory model: entries, sorting, the classic wildcard filter and the
//! lazy-loaded folder tree shown in the left pane.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::imaging;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SortKey {
    Name,
    Date,
    Size,
    Type,
}

#[derive(Clone)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub is_image: bool,
    pub size: u64,
    pub modified: SystemTime,
    pub ext: String,
}

/// Very small wildcard matcher (`*`, `?`, case insensitive) supporting
/// Classic semicolon separated wildcard filters such as `*.jpg;*.gif`.
pub fn matches_filter(name: &str, filter: &str) -> bool {
    let filter = filter.trim();
    if filter.is_empty() || filter == "*.*" || filter == "*" {
        return true;
    }
    let lowered = name.to_lowercase();
    filter
        .split(';')
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .any(|pattern| glob(&lowered, &pattern.to_lowercase()))
}

fn glob(text: &str, pattern: &str) -> bool {
    let text: Vec<char> = text.chars().collect();
    let pattern: Vec<char> = pattern.chars().collect();
    let (mut t, mut p) = (0usize, 0usize);
    let (mut star, mut star_match) = (None, 0usize);
    while t < text.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == text[t]) {
            t += 1;
            p += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            star_match = t;
            p += 1;
        } else if let Some(sp) = star {
            p = sp + 1;
            star_match += 1;
            t = star_match;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == '*' {
        p += 1;
    }
    p == pattern.len()
}

pub fn read_directory(dir: &Path, show_folders: bool, filter: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    let Ok(read_dir) = fs::read_dir(dir) else {
        return out;
    };
    for item in read_dir.flatten() {
        let path = item.path();
        let file_name = item.file_name().to_string_lossy().to_string();
        if file_name.starts_with('.') {
            continue;
        }
        let Ok(meta) = item.metadata() else {
            continue;
        };
        let is_dir = meta.is_dir();
        let is_image = !is_dir && imaging::is_image(&path);
        if is_dir {
            if !show_folders {
                continue;
            }
        } else if !is_image || !matches_filter(&file_name, filter) {
            continue;
        }
        out.push(Entry {
            name: file_name,
            ext: imaging::extension(&path),
            modified: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            size: meta.len(),
            path,
            is_dir,
            is_image,
        });
    }
    out
}

pub fn sort_entries(entries: &mut [Entry], key: SortKey, reverse: bool) {
    entries.sort_by(|a, b| {
        // Folders always stay on top, exactly like the classic browser.
        match (a.is_dir, b.is_dir) {
            (true, false) => return std::cmp::Ordering::Less,
            (false, true) => return std::cmp::Ordering::Greater,
            _ => {}
        }
        let ordering = match key {
            SortKey::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            SortKey::Date => a.modified.cmp(&b.modified),
            SortKey::Size => a.size.cmp(&b.size),
            SortKey::Type => a
                .ext
                .cmp(&b.ext)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
        };
        let ordering = ordering.then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        if reverse {
            ordering.reverse()
        } else {
            ordering
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_wildcard_filter() {
        assert!(matches_filter("photo.jpg", "*.*"));
        assert!(matches_filter("photo.JPG", "*.jpg"));
        assert!(!matches_filter("photo.png", "*.jpg"));
        assert!(matches_filter("a.gif", "*.jpg;*.gif"));
        assert!(matches_filter("readme.txt", "read??.txt"));
        assert!(!matches_filter("readme.txt", "read?.md"));
        assert!(matches_filter("anything.png", ""));
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lumiere_test_{tag}"));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn folders_come_first_and_respect_sort_key() {
        let base = temp_dir("sort");
        std::fs::create_dir_all(base.join("B folder")).unwrap();
        std::fs::create_dir_all(base.join("a folder")).unwrap();
        std::fs::write(base.join("beta.jpg"), vec![b'x'; 10]).unwrap();
        std::fs::write(base.join("alpha.jpg"), vec![b'x'; 900]).unwrap();

        let mut entries = read_directory(&base, true, "*.*");
        sort_entries(&mut entries, SortKey::Name, false);
        let names: Vec<String> = entries.iter().map(|e| e.name.clone()).collect();
        assert_eq!(names, ["a folder", "B folder", "alpha.jpg", "beta.jpg"]);

        sort_entries(&mut entries, SortKey::Size, false);
        assert!(entries[0].is_dir && entries[1].is_dir);
        assert_eq!(entries.last().unwrap().name, "alpha.jpg");

        sort_entries(&mut entries, SortKey::Name, true);
        assert!(entries[0].is_dir && entries[1].is_dir);
        assert_eq!(entries[entries.len() - 2].name, "beta.jpg");
        assert_eq!(entries.last().unwrap().name, "alpha.jpg");

        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn non_images_are_never_listed() {
        let base = temp_dir("filter");
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(base.join("notes.txt"), b"hello").unwrap();
        std::fs::write(base.join("pic.heic"), b"hello").unwrap();
        let entries = read_directory(&base, true, "*.*");
        assert_eq!(entries.len(), 1);
        assert!(entries[0].is_image);
        std::fs::remove_dir_all(&base).unwrap();
    }
}
