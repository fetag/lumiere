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

// ---------------------------------------------------------------------------
// Folder tree
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct TreeNode {
    pub path: PathBuf,
    pub label: String,
    pub expanded: bool,
    pub loaded: bool,
    pub has_children: bool,
    pub children: Vec<usize>,
}

pub struct TreeModel {
    nodes: Vec<TreeNode>,
    pub roots: Vec<usize>,
}

fn volume_label(path: &Path) -> String {
    let name = path.file_name().map(|n| n.to_string_lossy().to_string());
    match name {
        Some(ref n) if n == "/" || n.is_empty() => "Macintosh HD".to_string(),
        Some(n) => n,
        None => path.to_string_lossy().to_string(),
    }
}

impl TreeModel {
    pub fn new() -> Self {
        let mut model = Self {
            nodes: Vec::new(),
            roots: Vec::new(),
        };

        let mut roots: Vec<PathBuf> = Vec::new();
        if let Some(home) = dirs::home_dir() {
            roots.push(home.clone());
            for name in ["Desktop", "Documents", "Downloads", "Pictures", "Movies"] {
                let path = home.join(name);
                if path.is_dir() {
                    roots.push(path);
                }
            }
        }
        roots.push(PathBuf::from("/"));
        if let Ok(entries) = fs::read_dir("/Volumes") {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    roots.push(path);
                }
            }
        }

        for path in roots {
            let label = if path == Path::new("/") {
                volume_label(&path)
            } else {
                path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.to_string_lossy().to_string())
            };
            let index = model.add(path, label);
            model.roots.push(index);
        }
        model
    }

    fn add(&mut self, path: PathBuf, label: String) -> usize {
        self.nodes.push(TreeNode {
            path,
            label,
            expanded: false,
            loaded: false,
            has_children: false,
            children: Vec::new(),
        });
        self.nodes.len() - 1
    }

    pub fn node(&self, index: usize) -> &TreeNode {
        &self.nodes[index]
    }

    pub fn ensure_loaded(&mut self, index: usize) {
        let (loaded, path) = {
            let node = &self.nodes[index];
            (node.loaded, node.path.clone())
        };
        if loaded {
            return;
        }
        let mut children: Vec<(PathBuf, String)> = fs::read_dir(&path)
            .map(|entries| {
                entries
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.is_dir())
                    .filter(|p| {
                        p.file_name()
                            .map(|n| !n.to_string_lossy().starts_with('.'))
                            .unwrap_or(false)
                    })
                    .map(|p| {
                        let label = p
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| p.to_string_lossy().to_string());
                        (p, label)
                    })
                    .collect()
            })
            .unwrap_or_default();
        children.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));

        let mut indices = Vec::with_capacity(children.len());
        for (path, label) in children {
            indices.push(self.add(path, label));
        }
        let node = &mut self.nodes[index];
        node.children = indices;
        node.has_children = !node.children.is_empty();
        node.loaded = true;
    }

    pub fn toggle(&mut self, index: usize) {
        self.ensure_loaded(index);
        let node = &mut self.nodes[index];
        node.expanded = !node.expanded;
    }

    /// Depth first list of `(node index, depth)` for rendering.
    pub fn visible(&self) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        let mut stack: Vec<(usize, usize)> = self.roots.iter().rev().map(|&r| (r, 0)).collect();
        while let Some((index, depth)) = stack.pop() {
            out.push((index, depth));
            let node = &self.nodes[index];
            if node.expanded {
                for &child in node.children.iter().rev() {
                    stack.push((child, depth + 1));
                }
            }
        }
        out
    }

    /// Expand every ancestor of `path` and return the node index if present.
    pub fn reveal(&mut self, path: &Path) -> Option<usize> {
        let mut current: Option<usize> = None;
        for candidate in path.ancestors() {
            if let Some(index) = self.find(candidate) {
                current = Some(index);
                break;
            }
        }
        let mut index = current?;
        // Walk down from the found ancestor towards `path`, creating the chain.
        let components: Vec<_> = path
            .strip_prefix(self.nodes[index].path.clone())
            .ok()
            .map(|rest| {
                rest.components()
                    .map(|c| c.as_os_str().to_string_lossy().to_string())
                    .collect()
            })
            .unwrap_or_default();
        for component in components {
            self.ensure_loaded(index);
            let next = self.nodes[index]
                .children
                .iter()
                .copied()
                .find(|&c| self.nodes[c].label == component);
            match next {
                Some(child) => index = child,
                None => break,
            }
        }
        self.nodes[index].expanded = true;
        Some(index)
    }

    pub fn find(&self, path: &Path) -> Option<usize> {
        self.nodes
            .iter()
            .position(|n| n.path == path)
            .filter(|_| true)
    }

    pub fn set_expanded(&mut self, index: usize, expanded: bool) {
        self.nodes[index].expanded = expanded;
    }
}

/// Sibling folder navigation (Go ▸ Next Folder / Previous Folder).
pub fn sibling_folder(current: &Path, forward: bool) -> Option<PathBuf> {
    let parent = current.parent()?;
    let mut dirs: Vec<PathBuf> = fs::read_dir(parent)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    let position = dirs.iter().position(|p| p == current)?;
    if forward {
        dirs.get(position + 1).cloned()
    } else {
        position.checked_sub(1).map(|i| dirs[i].clone())
    }
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

    #[test]
    fn sibling_folder_navigation() {
        let base = temp_dir("siblings");
        std::fs::create_dir_all(base.join("one/two/three")).unwrap();
        std::fs::create_dir_all(base.join("one/four")).unwrap();
        let current = base.join("one/four");
        assert_eq!(sibling_folder(&current, true), Some(base.join("one/two")));
        assert_eq!(
            sibling_folder(&base.join("one/two"), false),
            Some(current.clone())
        );
        assert_eq!(sibling_folder(&current, false), None);
        std::fs::remove_dir_all(&base).unwrap();
    }
}
