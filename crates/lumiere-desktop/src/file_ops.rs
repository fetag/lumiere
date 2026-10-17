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

/// Rename without replacing an existing destination, atomically where the
/// platform provides that operation. This applies to both files and folders.
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
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::ffi::OsStrExt;
        use std::os::windows::io::{AsRawHandle, FromRawHandle, RawHandle};
        use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::Storage::FileSystem::{
            CreateFileW, FileRenameInfoEx, SetFileInformationByHandle, DELETE,
            FILE_FLAG_BACKUP_SEMANTICS, FILE_RENAME_INFO, FILE_RENAME_INFO_0, FILE_SHARE_DELETE,
            FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
        };

        let source: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
        // A null RootDirectory requires an absolute destination path.
        let destination = std::path::absolute(to)?;
        let destination: Vec<u16> = destination.as_os_str().encode_wide().collect();
        let destination_len = u32::try_from(destination.len() * std::mem::size_of::<u16>())
            .map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "path is too long")
            })?;

        // DELETE access is required for FileRenameInfoEx. BACKUP_SEMANTICS lets
        // CreateFileW open directories too; the returned handle is owned by File.
        let handle = unsafe {
            CreateFileW(
                source.as_ptr(),
                DELETE,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(std::io::Error::last_os_error());
        }
        let handle = unsafe { std::fs::File::from_raw_handle(handle as RawHandle) };

        // FILE_RENAME_INFO has a one-u16 trailing array. Reserve exactly enough
        // room for the full UTF-16 destination and initialize its fixed header.
        let header_len = std::mem::size_of::<FILE_RENAME_INFO>() - std::mem::size_of::<u16>();
        let mut buffer = vec![0_u8; header_len + destination.len() * std::mem::size_of::<u16>()];
        let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFO>();
        unsafe {
            (*info).Anonymous = FILE_RENAME_INFO_0 { Flags: 0 };
            (*info).RootDirectory = std::ptr::null_mut();
            (*info).FileNameLength = destination_len;
            std::ptr::copy_nonoverlapping(
                destination.as_ptr(),
                (*info).FileName.as_mut_ptr(),
                destination.len(),
            );
        }

        // Flags = 0 deliberately omits FILE_RENAME_FLAG_REPLACE_IF_EXISTS, so
        // Windows rejects an existing destination without a check-then-rename race.
        let result = unsafe {
            SetFileInformationByHandle(
                handle.as_raw_handle() as HANDLE,
                FileRenameInfoEx,
                info.cast(),
                u32::try_from(buffer.len()).expect("rename buffer length fits in u32"),
            )
        };
        if result == 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
    #[cfg(target_os = "linux")]
    {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;

        let source = CString::new(from.as_os_str().as_bytes())?;
        let target = CString::new(to.as_os_str().as_bytes())?;
        // Invoke the syscall directly rather than relying on a recent glibc
        // export. RENAME_NOREPLACE makes this an atomic no-overwrite rename.
        if unsafe {
            libc::syscall(
                libc::SYS_renameat2,
                libc::AT_FDCWD,
                source.as_ptr(),
                libc::AT_FDCWD,
                target.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        } == 0
        {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
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
