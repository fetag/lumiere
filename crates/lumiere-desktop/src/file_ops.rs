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
            rename_without_replace(path, &target)
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
    let names = crate::trash_names(paths);
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

/// Move one selection to the system Trash in a single backend call. On macOS
/// the Finder backend treats that as one delete action, so a multi-selection
/// produces one Finder sound and one undoable operation.
pub fn trash_all<E>(
    paths: &[PathBuf],
    progress: &mut dyn FnMut(usize, String),
    delete_all: impl FnOnce(&[PathBuf]) -> Result<(), E>,
) -> Outcome
where
    E: std::fmt::Display,
{
    let names = crate::trash_names(paths);
    match delete_all(paths) {
        Ok(()) => {
            for (index, name) in names.into_iter().enumerate() {
                progress(index + 1, name);
            }
            Outcome {
                moved: paths.len(),
                failures: Vec::new(),
            }
        }
        Err(error) => Outcome {
            moved: 0,
            failures: paths
                .iter()
                .cloned()
                .map(|path| (path, error.to_string()))
                .collect(),
        },
    }
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
        std::fs::create_dir(to)?;
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            copy_path(&entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        let mut source = std::fs::File::open(from)?;
        // Exclusive creation rejects both existing files and dangling symlinks.
        let mut destination = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(to)?;
        std::io::copy(&mut source, &mut destination)?;
        destination.set_permissions(source_meta.permissions())
    }
}

/// macOS provides an atomic no-replace rename, including directories.
pub fn rename_without_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;
        unsafe extern "C" {
            fn renamex_np(
                from: *const std::ffi::c_char,
                to: *const std::ffi::c_char,
                flags: u32,
            ) -> i32;
        }
        let source = CString::new(from.as_os_str().as_bytes())?;
        let target = CString::new(to.as_os_str().as_bytes())?;
        // SAFETY: both paths are live NUL-terminated strings. RENAME_EXCL (4)
        // makes the filesystem reject any existing destination atomically.
        if unsafe { renamex_np(source.as_ptr(), target.as_ptr(), 4) } == 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (from, to);
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "atomic no-replace rename is unavailable on this platform",
        ))
    }
}

/// Native Trash service; errors retain the backend error type and wording.
pub use trash::delete_all as trash_delete_all;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trash_all_calls_the_backend_once_for_a_selection() {
        let paths = vec![PathBuf::from("/tmp/one.jpg"), PathBuf::from("/tmp/two.jpg")];
        let mut calls = 0;
        let mut progress = Vec::new();
        let outcome = trash_all(
            &paths,
            &mut |index, name| progress.push((index, name)),
            |received| {
                calls += 1;
                assert_eq!(received, paths.as_slice());
                Ok::<_, String>(())
            },
        );

        assert_eq!(calls, 1);
        assert_eq!(outcome.moved, 2);
        assert!(outcome.failures.is_empty());
        assert_eq!(progress.len(), 2);
    }
}
