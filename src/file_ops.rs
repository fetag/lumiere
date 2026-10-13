//! Filesystem-only operations used by the application command layer.
//!
//! Keeping these operations independent from `App` makes their safety rules
//! testable without constructing UI state or starting worker threads.

use std::path::{Component, Path, PathBuf};

pub struct Outcome {
    pub moved: usize,
    pub failures: Vec<(PathBuf, String)>,
}

pub fn transfer(
    paths: &[PathBuf],
    dest: &Path,
    copy: bool,
    progress: &mut dyn FnMut(usize, String),
) -> Outcome {
    if !dest.is_dir() {
        return Outcome {
            moved: 0,
            failures: paths
                .iter()
                .map(|path| (path.clone(), format!("Not a folder: {}", dest.display())))
                .collect(),
        };
    }
    let mut moved = 0;
    let mut failures = Vec::new();
    for (index, path) in paths.iter().enumerate() {
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
            Ok(()) => moved += 1,
            Err(error) => failures.push((path.clone(), error.to_string())),
        }
        progress(index + 1, name.to_string_lossy().into_owned());
    }
    Outcome { moved, failures }
}

pub fn trash<E>(
    paths: &[PathBuf],
    progress: &mut dyn FnMut(usize, String),
    mut delete: impl FnMut(&Path) -> Result<(), E>,
) -> Outcome
where
    E: std::fmt::Display,
{
    let names = crate::alerts::trash_names(paths);
    let mut moved = 0;
    let mut failures = Vec::new();
    for (index, (path, name)) in paths.iter().zip(&names).enumerate() {
        match delete(path) {
            Ok(()) => moved += 1,
            Err(error) => failures.push((path.clone(), error.to_string())),
        }
        progress(index + 1, name.clone());
    }
    Outcome { moved, failures }
}

pub fn is_single_file_name(name: &str) -> bool {
    matches!(
        Path::new(name).components().next(),
        Some(Component::Normal(_))
    ) && Path::new(name).components().count() == 1
}

pub fn copy_path(from: &Path, to: &Path) -> std::io::Result<()> {
    let source_meta = std::fs::symlink_metadata(from)?;
    if source_meta.file_type().is_symlink() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "copying symbolic links is not supported",
        ));
    }
    if source_meta.is_dir() {
        let source = from.canonicalize()?;
        let destination = to
            .parent()
            .unwrap_or(to)
            .canonicalize()
            .unwrap_or_else(|_| to.to_path_buf());
        if destination.starts_with(&source) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "cannot copy a folder into itself or one of its subfolders",
            ));
        }
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
