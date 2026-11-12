//! OS default-handler registration for the "File Types" settings section.
//!
//! Each format group below can be made the system-wide (per-user) default
//! handler for its file types, so double-clicking such a file opens Lumiere.
//! The OS is the source of truth: `bound_groups` reads the live OS state and
//! `set_binding` writes it; nothing is persisted in Lumiere's own
//! `state.ini`.
//!
//! - **macOS** — `LSSetDefaultRoleHandlerForContentType` (LaunchServices) per
//!   concrete UTI, by the app's bundle identifier. The deprecated C API is
//!   used deliberately: it works on the macOS 11 deployment target, unlike
//!   the `NSWorkspace` replacement (macOS 12+). The call returns as soon as
//!   the change is accepted, but on modern macOS the change then sits
//!   *pending* until the user confirms it in the system prompt — verified on
//!   macOS 27 — so `bound_groups` (which reports the committed state, never
//!   a queued one) only agrees with the new checkbox once the user has
//!   confirmed. Each group is submitted as one batch (all of its UTIs back
//!   to back); groups with two UTIs (HEIC/HEIF, Video) therefore produce up
//!   to two prompts. The UI tells the user to expect the prompt.
//!   Only concrete system UTIs are bound — never the `public.image`
//!   supertype — and only UTIs the app's `Info.plist` claims, which
//!   `bundle.sh` keeps in sync with `SUPPORTED_EXTS`. The group table was
//!   verified against `lsregister -dump` on macOS 27: `public.heic`,
//!   `public.heif`, `public.avif` and `model.gltf-binary` are system
//!   CoreTypes; the Other-images and Camera-RAW groups have no system UTI, so
//!   they are hidden there (dragging and opening such files still works — the
//!   `Info.plist` extension claims are untouched). The LaunchServices API has
//!   no "reset to the previous default", so the handler that owned each UTI
//!   at bind time is remembered in `filetypes.ini` (beside `state.ini`) and
//!   restored when the user unbinds.
//! - **Windows** — per-user `HKCU\Software\Classes` keys (no admin rights,
//!   matching the portable-zip packaging): a `Lumiere.<id>` ProgID with an
//!   open command quoting the real executable path, `.\<ext>` default values
//!   that shadow the machine default for this user, and an
//!   `Applications\Lumiere.exe\SupportedFileTypes` marker so Lumiere shows up
//!   in the "Open with" list. Unbinding only removes the `.\<ext>` default
//!   when it is Lumiere's ProgID — a pre-existing choice is never clobbered.
//! - **Linux** — the user desktop file `applications/lumiere-filetypes.desktop`
//!   (distinct from the packaged `lumiere.desktop`, with `Exec` always the
//!   running binary and `NoDisplay=true`) plus `[Default Applications]`
//!   entries in `~/.config/mimeapps.list`, merged and rewritten atomically.
//!
//! In a test build (`test-support` feature) the OS layer is a no-op that
//! records the calls in a drainable log, so the settings dialog wiring can
//! be exercised headlessly.

use std::collections::HashSet;

/// One bindable format group: every extension of one family (JPEG, PNG, …).
pub struct FileTypeGroup {
    /// Stable identifier, valid as a Windows ProgID suffix (`Lumiere.<id>`).
    pub id: &'static str,
    /// Row label in the Settings window.
    pub label: &'static str,
    /// Every extension in the group (all platforms; the Windows binding key).
    pub extensions: &'static [&'static str],
    /// Concrete macOS UTIs the group binds; empty when the platform cannot
    /// name this family, in which case the group is hidden on macOS.
    pub utis: &'static [&'static str],
    /// Linux MIME types the group binds (best effort: camera RAW has no
    /// canonical freedesktop MIME, so distribution MIME tables decide).
    pub mimes: &'static [&'static str],
    /// Whether the build includes support for this family at all.
    pub enabled: bool,
}

impl FileTypeGroup {
    /// Whether this build can offer the group on the current platform.
    pub fn available(&self) -> bool {
        if !self.enabled {
            return false;
        }
        #[cfg(target_os = "macos")]
        {
            !self.utis.is_empty()
        }
        #[cfg(not(target_os = "macos"))]
        {
            true
        }
    }
}

/// The format groups, in Settings-window order. The `other` and `raw` groups
/// carry no macOS UTIs (the OS cannot name those families) and no `video` /
/// `models` feature build carries the last two rows; `available` encodes
/// both.
const GROUPS: &[FileTypeGroup] = &[
    FileTypeGroup {
        id: "jpeg",
        label: "JPEG",
        extensions: &["jpg", "jpeg", "jpe", "jfif"],
        utis: &["public.jpeg"],
        mimes: &["image/jpeg"],
        enabled: true,
    },
    FileTypeGroup {
        id: "png",
        label: "PNG",
        extensions: &["png"],
        utis: &["public.png"],
        mimes: &["image/png"],
        enabled: true,
    },
    FileTypeGroup {
        id: "gif",
        label: "GIF",
        extensions: &["gif"],
        utis: &["com.compuserve.gif"],
        mimes: &["image/gif"],
        enabled: true,
    },
    FileTypeGroup {
        id: "bmp",
        label: "Bitmap (BMP)",
        extensions: &["bmp", "rle"],
        utis: &["com.microsoft.bmp"],
        mimes: &["image/bmp"],
        enabled: true,
    },
    FileTypeGroup {
        id: "tiff",
        label: "TIFF",
        extensions: &["tif", "tiff"],
        utis: &["public.tiff"],
        mimes: &["image/tiff"],
        enabled: true,
    },
    FileTypeGroup {
        id: "webp",
        label: "WebP",
        extensions: &["webp"],
        utis: &["org.webmproject.webp"],
        mimes: &["image/webp"],
        enabled: true,
    },
    FileTypeGroup {
        id: "heic",
        label: "HEIC / HEIF",
        extensions: &["heic", "heif", "hif"],
        utis: &["public.heic", "public.heif"],
        mimes: &["image/heic", "image/heif"],
        enabled: true,
    },
    FileTypeGroup {
        id: "avif",
        label: "AVIF",
        extensions: &["avif"],
        utis: &["public.avif"],
        mimes: &["image/avif"],
        enabled: true,
    },
    FileTypeGroup {
        id: "svg",
        label: "SVG",
        extensions: &["svg"],
        utis: &["org.w3c.svg"],
        mimes: &["image/svg+xml"],
        enabled: true,
    },
    FileTypeGroup {
        id: "icon",
        label: "Icons (ICO / CUR)",
        extensions: &["ico", "cur"],
        utis: &["com.microsoft.ico"],
        mimes: &["image/x-icon"],
        enabled: true,
    },
    FileTypeGroup {
        id: "pnm",
        label: "PNM",
        extensions: &["pnm", "pbm", "pgm", "ppm", "pam"],
        utis: &["public.pnm"],
        mimes: &["image/x-pnm"],
        enabled: true,
    },
    FileTypeGroup {
        id: "other",
        label: "Other images",
        extensions: &[
            "tga", "vda", "icb", "vst", "pcx", "qoi", "dds", "exr", "hdr", "farbfeld", "ff",
        ],
        utis: &[],
        mimes: &[
            "image/x-tga",
            "image/x-pcx",
            "image/qoi",
            "image/x-dds",
            "image/x-exr",
            "image/vnd.radiance",
            "image/x-farbfeld",
        ],
        enabled: true,
    },
    FileTypeGroup {
        id: "raw",
        label: "Camera RAW",
        extensions: &[
            "dng", "cr2", "cr3", "nef", "nrw", "arw", "srf", "sr2", "orf", "rw2", "raf", "pef",
            "raw",
        ],
        utis: &[],
        mimes: &["image/x-raw"],
        enabled: true,
    },
    FileTypeGroup {
        id: "video",
        label: "Video (MP4 / AVI)",
        extensions: &["mp4", "avi"],
        utis: &["public.mpeg-4", "public.avi"],
        mimes: &["video/mp4", "video/x-msvideo"],
        enabled: cfg!(feature = "video"),
    },
    FileTypeGroup {
        id: "model",
        label: "3D models (GLB)",
        extensions: &["glb"],
        utis: &["model.gltf-binary"],
        mimes: &["model/gltf-binary"],
        enabled: cfg!(feature = "models"),
    },
];

/// All format groups, in Settings-window order.
pub fn groups() -> &'static [FileTypeGroup] {
    GROUPS
}

/// The ids of the groups currently bound to this app in the OS.
pub fn bound_groups() -> HashSet<&'static str> {
    if cfg!(any(test, feature = "test-support")) {
        return HashSet::new();
    }
    #[cfg(target_os = "macos")]
    {
        macos_bound_groups()
    }
    #[cfg(target_os = "windows")]
    {
        windows_bound_groups()
    }
    #[cfg(target_os = "linux")]
    {
        linux_bound_groups()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        HashSet::new()
    }
}

/// Bind (or unbind) one group's types in the OS.
pub fn set_binding(id: &str, on: bool) -> Result<(), String> {
    if cfg!(any(test, feature = "test-support")) {
        #[cfg(any(test, feature = "test-support"))]
        record_test_binding(id, on);
        return Ok(());
    }
    let group = find_group(id)?;
    if !group.available() {
        return Err(format!(
            "{label} cannot be bound on this platform",
            label = group.label
        ));
    }
    #[cfg(target_os = "macos")]
    {
        macos_set_binding(group, on)
    }
    #[cfg(target_os = "windows")]
    {
        windows_set_binding(group, on)
    }
    #[cfg(target_os = "linux")]
    {
        linux_set_binding(group, on)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = (group, on);
        Err("file-type binding is not supported on this platform".to_owned())
    }
}

fn find_group(id: &str) -> Result<&'static FileTypeGroup, String> {
    GROUPS
        .iter()
        .find(|group| group.id == id)
        .ok_or_else(|| format!("unknown file type {id:?}"))
}

// -- macOS -----------------------------------------------------------------

#[cfg(target_os = "macos")]
fn bundle_id() -> Option<String> {
    use objc2_foundation::NSBundle;
    NSBundle::mainBundle()
        .bundleIdentifier()
        .map(|id| id.to_string())
}

#[cfg(target_os = "macos")]
fn macos_bound_groups() -> HashSet<&'static str> {
    let Some(ours) = bundle_id().map(|id| id.to_string()) else {
        // A bare `cargo run` executable has no bundle identifier, so nothing
        // can be (or is) bound.
        return HashSet::new();
    };
    GROUPS
        .iter()
        .filter(|group| group.available())
        .filter(|group| {
            group
                .utis
                .iter()
                .copied()
                .all(|uti| macos_current_handler(uti).as_deref() == Some(ours.as_str()))
        })
        .map(|group| group.id)
        .collect()
}

// Deprecated by Apple but synchronous and available on the macOS 11
// deployment target; the `NSWorkspace` replacements need macOS 12.
#[cfg(target_os = "macos")]
#[allow(deprecated)]
fn set_handler(uti: &str, handler: &str) -> Result<(), String> {
    use objc2_core_foundation::CFString;
    use objc2_core_services::{LSRolesMask, LSSetDefaultRoleHandlerForContentType};
    let uti = CFString::from_str(uti);
    let handler = CFString::from_str(handler);
    // Set by bundle identifier, so moving or rebuilding the .app never pins a
    // stale path. (If this ever fails with a not-found status, re-register the
    // bundle with `LSRegisterURL(bundle_url, true)` — kept out of the hot path
    // on purpose.)
    //
    // SAFETY: the arguments are valid, non-null CFString references for the
    // duration of the call; `LSSetDefaultRoleHandlerForContentType` is a
    // documented CoreServices entry point.
    let status =
        unsafe { LSSetDefaultRoleHandlerForContentType(&uti, LSRolesMask::Viewer, &handler) };
    if status != 0 {
        return Err(format!("LaunchServices rejected {uti}: status {status}"));
    }
    Ok(())
}

/// Remember which handler owned each UTI before Lumiere bound it, so
/// unbinding can restore it (LaunchServices has no "reset to previous").
#[cfg(target_os = "macos")]
fn restore_file() -> std::path::PathBuf {
    crate::state::file().with_file_name("filetypes.ini")
}

#[cfg(target_os = "macos")]
fn read_restore_points() -> std::collections::BTreeMap<String, String> {
    let Ok(text) = std::fs::read_to_string(restore_file()) else {
        return std::collections::BTreeMap::new();
    };
    text.lines()
        .filter_map(|line| line.split_once('='))
        .map(|(uti, handler)| (uti.trim().to_owned(), handler.trim().to_owned()))
        .filter(|(uti, handler)| !uti.is_empty() && !handler.is_empty())
        .collect()
}

#[cfg(target_os = "macos")]
fn write_restore_points(points: &std::collections::BTreeMap<String, String>) -> Result<(), String> {
    let mut text = String::new();
    for (uti, handler) in points {
        text.push_str(uti);
        text.push('=');
        text.push_str(handler);
        text.push('\n');
    }
    write_atomic(&restore_file(), &text)
}

#[cfg(target_os = "macos")]
fn macos_set_binding(group: &FileTypeGroup, on: bool) -> Result<(), String> {
    let Some(ours) = bundle_id().map(|id| id.to_string()) else {
        return Err(
            "file-type binding needs the bundled app (Lumiere.app), not the raw executable"
                .to_owned(),
        );
    };
    if on {
        let mut points = read_restore_points();
        let mut remembered = false;
        for uti in group.utis.iter().copied() {
            // Remember the current handler once, so a later unbind restores
            // exactly what the user had before the first bind.
            if !points.contains_key(uti) {
                if let Some(previous) = macos_current_handler(uti) {
                    if previous != ours {
                        points.insert(uti.to_owned(), previous);
                        remembered = true;
                    }
                }
            }
            set_handler(uti, &ours)?;
        }
        if remembered {
            write_restore_points(&points)?;
        }
        return Ok(());
    }
    let mut points = read_restore_points();
    let mut restored = false;
    for uti in group.utis.iter().copied() {
        if let Some(previous) = points.remove(uti) {
            if previous != ours {
                set_handler(uti, &previous)?;
            }
            restored = true;
        }
    }
    if restored {
        write_restore_points(&points)?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
#[allow(deprecated)]
fn macos_current_handler(uti: &str) -> Option<String> {
    use objc2_core_foundation::CFString;
    use objc2_core_services::{LSCopyDefaultRoleHandlerForContentType, LSRolesMask};
    let uti = CFString::from_str(uti);
    //
    // SAFETY: `uti` is a valid CFString reference for the duration of the
    // call.
    unsafe { LSCopyDefaultRoleHandlerForContentType(&uti, LSRolesMask::Viewer) }
        .map(|handler| handler.to_string())
}

// -- Windows ---------------------------------------------------------------

/// The per-user ProgID for a group.
#[cfg(any(target_os = "windows", test))]
fn progid(id: &str) -> String {
    format!("Lumiere.{id}")
}

/// The `HKCU\Software\Classes` subkey holding one extension's default.
#[cfg(any(target_os = "windows", test))]
fn class_ext_subkey(ext: &str) -> String {
    format!(r"Software\Classes\.{}", ext.to_ascii_lowercase())
}

/// The `Applications\<exe>\SupportedFileTypes` subkey for one extension.
#[cfg(any(target_os = "windows", test))]
fn app_ext_subkey(exe_name: &str, ext: &str) -> String {
    format!(
        r"Software\Classes\Applications\{}\SupportedFileTypes\.{}",
        exe_name,
        ext.to_ascii_lowercase()
    )
}

/// The ProgID's open command; the executable path is quoted (the archive can
/// sit in a directory with spaces) while `%1` stays unquoted.
#[cfg(any(target_os = "windows", test))]
fn open_command(exe: &str) -> String {
    format!("\"{exe}\" \"%1\"")
}

/// UTF-16 encoding with the trailing NUL the registry APIs expect.
#[cfg(any(target_os = "windows", test))]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(target_os = "windows")]
fn exe_path() -> std::path::PathBuf {
    std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("Lumiere.exe"))
}

#[cfg(target_os = "windows")]
fn windows_bound_groups() -> HashSet<&'static str> {
    use windows_sys::Win32::Foundation::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    GROUPS
        .iter()
        .filter(|group| group.available())
        .filter(|group| {
            let pid = progid(group.id);
            group.extensions.iter().all(|ext| {
                let key = class_ext_subkey(ext);
                // The per-user value shadows the machine default; fall back to
                // it when the user has not chosen anything yet.
                read_registry_value(HKEY_CURRENT_USER, &key)
                    .or_else(|| read_registry_value(HKEY_LOCAL_MACHINE, &key))
                    .map(|value| value.eq_ignore_ascii_case(&pid))
                    .unwrap_or(false)
            })
        })
        .map(|group| group.id)
        .collect()
}

#[cfg(target_os = "windows")]
fn windows_set_binding(group: &FileTypeGroup, on: bool) -> Result<(), String> {
    use windows_sys::Win32::Foundation::HKEY_CURRENT_USER;
    use windows_sys::Win32::Storage::FileSystem::KEY_WRITE;
    let exe = exe_path();
    let pid = progid(group.id);
    if on {
        let exe_str = exe.to_string_lossy().into_owned();
        let exe_name = exe
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Lumiere.exe".to_owned());
        write_registry_value(
            HKEY_CURRENT_USER,
            &format!(r"Software\Classes\{pid}"),
            Some(&format!("Lumiere {} files", group.label)),
        )?;
        write_registry_value(
            HKEY_CURRENT_USER,
            &format!(r"Software\Classes\{pid}\DefaultIcon"),
            Some(&exe_str),
        )?;
        write_registry_value(
            HKEY_CURRENT_USER,
            &format!(r"Software\Classes\{pid}\shell\open\command"),
            Some(&open_command(&exe_str)),
        )?;
        for ext in group.extensions {
            write_registry_value(HKEY_CURRENT_USER, &class_ext_subkey(ext), Some(&pid))?;
            // The empty REG_SZ marks Lumiere as a supported app for the
            // extension so it appears in "Open with → choose another app".
            write_registry_value(HKEY_CURRENT_USER, &app_ext_subkey(&exe_name, ext), Some(""))?;
        }
        return Ok(());
    }
    for ext in group.extensions {
        let key = class_ext_subkey(ext);
        // Only remove the default when it is ours: a pre-existing user or
        // machine choice must survive.
        if read_registry_value(HKEY_CURRENT_USER, &key)
            .map(|value| value.eq_ignore_ascii_case(&pid))
            .unwrap_or(false)
        {
            let status = unsafe {
                windows_sys::Win32::System::Registry::RegDeleteValueW(
                    HKEY_CURRENT_USER,
                    wide(&key).as_ptr(),
                )
            };
            if status != 0 && status != 2 {
                return Err(format!("could not unbind .{ext}: status {status}"));
            }
        }
        // Our marker key holds nothing but the empty default value.
        let status = unsafe {
            windows_sys::Win32::System::Registry::RegDeleteKeyExW(
                HKEY_CURRENT_USER,
                wide(&app_ext_subkey(
                    exe.file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .as_deref()
                        .unwrap_or("Lumiere.exe"),
                    ext,
                ))
                .as_ptr(),
                KEY_WRITE,
                0,
            )
        };
        if status != 0 && status != 2 {
            return Err(format!(
                "could not clear the .{ext} support marker: status {status}"
            ));
        }
    }
    // The ProgID block is intentionally left in place: at most a handful of
    // small keys, and deleting it mid-way leaves a less understandable state.
    Ok(())
}

#[cfg(target_os = "windows")]
fn read_registry_value(hive: windows_sys::Win32::Foundation::HKEY, subkey: &str) -> Option<String> {
    use windows_sys::Win32::Foundation::HKEY;
    use windows_sys::Win32::Storage::FileSystem::KEY_READ;
    use windows_sys::Win32::System::Registry::{RegCloseKey, RegOpenKeyExW};
    let mut hkey: HKEY = std::ptr::null_mut();
    // SAFETY: `subkey` is NUL-terminated wide text; the handle is closed on
    // every path below.
    let status = unsafe {
        RegOpenKeyExW(
            hive,
            wide(subkey).as_ptr(),
            0,
            KEY_READ,
            &mut hkey as *mut HKEY,
        )
    };
    if status != 0 {
        return None;
    }
    let value = query_default_string(hkey);
    unsafe {
        RegCloseKey(hkey);
    }
    value
}

#[cfg(target_os = "windows")]
fn query_default_string(hkey: windows_sys::Win32::Foundation::HKEY) -> Option<String> {
    use windows_sys::Win32::System::Registry::{RegQueryValueExW, REG_SZ};
    // First pass: the value's size (including the trailing NUL).
    let mut size = 0u32;
    let mut value_type = 0u32;
    // SAFETY: `size`/`value_type` are writable and `RegQueryValueExW` fills
    // them when the data pointer is null.
    let status = unsafe {
        RegQueryValueExW(
            hkey,
            None,
            None,
            &mut value_type,
            std::ptr::null_mut(),
            &mut size,
        )
    };
    if status != 0 {
        return None;
    }
    if value_type != REG_SZ || size == 0 {
        return None;
    }
    let mut buffer = vec![0u8; size as usize];
    // SAFETY: `buffer` has exactly `size` bytes, as the first pass reported.
    let status = unsafe {
        RegQueryValueExW(
            hkey,
            None,
            None,
            &mut value_type,
            buffer.as_mut_ptr(),
            &mut size,
        )
    };
    if status != 0 {
        return None;
    }
    let utf16: Vec<u16> =
        unsafe { std::slice::from_raw_parts(buffer.as_ptr() as *const u16, (size / 2) as usize) }
            .to_vec();
    String::from_utf16_lossy(&utf16)
        .trim_end_matches('\0')
        .to_owned()
        .into()
}

#[cfg(target_os = "windows")]
fn write_registry_value(
    hive: windows_sys::Win32::Foundation::HKEY,
    subkey: &str,
    value: Option<&str>,
) -> Result<(), String> {
    use windows_sys::Win32::Storage::FileSystem::{KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ};
    use windows_sys::Win32::System::Registry::{RegCloseKey, RegCreateKeyExW, RegSetValueExW};
    let mut hkey: windows_sys::Win32::Foundation::HKEY = std::ptr::null_mut();
    let mut disposition = 0u32;
    // SAFETY: `subkey` is a NUL-terminated wide string; the handle is closed
    // on every path below.
    let status = unsafe {
        RegCreateKeyExW(
            hive,
            wide(subkey).as_ptr(),
            0,
            std::ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut hkey,
            &mut disposition,
        )
    };
    if status != 0 {
        return Err(format!("could not open {subkey}: status {status}"));
    }
    let result = write_default_string(hkey, value);
    unsafe {
        RegCloseKey(hkey);
    }
    result
}

#[cfg(target_os = "windows")]
fn write_default_string(
    hkey: windows_sys::Win32::Foundation::HKEY,
    value: Option<&str>,
) -> Result<(), String> {
    use windows_sys::Win32::System::Registry::{RegSetValueExW, REG_SZ};
    let (data, len): (&[u8], u32) = match value {
        Some(text) => {
            // REG_SZ is UTF-16 with a trailing NUL (an empty string is the
            // two zero bytes).
            let mut utf16 = wide(text);
            let len = (utf16.len() * 2) as u32;
            utf16.push(0);
            (&utf16, len)
        }
        None => (b"", 0),
    };
    // SAFETY: `data` outlives the call and its size is passed explicitly.
    let status = unsafe { RegSetValueExW(hkey, None, 0, REG_SZ, data.as_ptr(), len) };
    if status != 0 {
        return Err(format!(
            "could not write the registry value: status {status}"
        ));
    }
    Ok(())
}

// -- Linux -----------------------------------------------------------------

/// The per-user desktop file that owns the user's Lumiere bindings.
#[cfg(any(target_os = "linux", test))]
const USER_DESKTOP: &str = "lumiere-filetypes.desktop";

/// The `[Default Applications]` section of a `mimeapps.list`: MIME type to
/// the list of desktop files, in preference order.
#[cfg(any(target_os = "linux", test))]
fn parse_defaults(text: &str) -> std::collections::BTreeMap<String, Vec<String>> {
    let mut defaults = std::collections::BTreeMap::new();
    let mut in_defaults = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(section) = trimmed.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            in_defaults = section == "Default Applications";
            continue;
        }
        if !in_defaults || trimmed.is_empty() || trimmed.starts_with([';', '#']) {
            continue;
        }
        let Some((mime, apps)) = trimmed.split_once('=') else {
            continue;
        };
        let apps = apps
            .split(';')
            .map(|app| app.trim().to_owned())
            .filter(|app| !app.is_empty())
            .collect::<Vec<_>>();
        defaults.insert(mime.trim().to_owned(), apps);
    }
    defaults
}

/// Rewrite `mimeapps.list`: `set` entries become the first (default) app for
/// their MIME type, `remove` entries have this app stripped from their list
/// (restoring the previous default the desktop recorded there), everything
/// else is preserved verbatim.
#[cfg(any(target_os = "linux", test))]
fn merge_mimeapps(existing: Option<&str>, set: &[(&str, &str)], remove: &[&str]) -> String {
    let ours = USER_DESKTOP;
    let existing = existing.unwrap_or("");
    let mut out = String::new();
    let mut in_defaults = false;
    let mut saw_defaults = false;
    let mut handled: HashSet<&str> = HashSet::new();
    for line in existing.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if in_defaults {
                // The Default section ends here: append the `set` entries that
                // had no line of their own.
                append_missing(&mut out, set, &handled);
            }
            in_defaults = trimmed == "[Default Applications]";
            saw_defaults |= in_defaults;
            out.push_str(trimmed);
            out.push('\n');
            continue;
        }
        if in_defaults {
            if let Some((mime, apps)) = trimmed.split_once('=') {
                let mime = mime.trim();
                if let Some((_, name)) = set.iter().find(|(candidate, _)| *candidate == mime) {
                    out.push_str(&format!("{mime}={name}\n"));
                    handled.insert(mime);
                    continue;
                }
                if remove.contains(&mime) {
                    let rest: Vec<&str> = apps
                        .split(';')
                        .map(str::trim)
                        .filter(|app| !app.is_empty() && *app != ours)
                        .collect();
                    if !rest.is_empty() {
                        out.push_str(&format!("{mime}={}\n", rest.join(";")));
                    }
                    handled.insert(mime);
                    continue;
                }
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    if in_defaults {
        append_missing(&mut out, set, &handled);
    } else if !saw_defaults && !set.is_empty() {
        out.push('\n');
        out.push_str("[Default Applications]\n");
        for (mime, name) in set {
            out.push_str(&format!("{mime}={name}\n"));
        }
    }
    out
}

#[cfg(any(target_os = "linux", test))]
fn append_missing(out: &mut String, set: &[(&str, &str)], handled: &HashSet<&str>) {
    for (mime, name) in set {
        if !handled.contains(mime) {
            out.push_str(&format!("{mime}={name}\n"));
        }
    }
}

#[cfg(target_os = "linux")]
fn group_bound(
    defaults: &std::collections::BTreeMap<String, Vec<String>>,
    group: &FileTypeGroup,
) -> bool {
    group.mimes.iter().all(|mime| {
        defaults
            .get(*mime)
            .and_then(|apps| apps.first())
            .is_some_and(|app| app == USER_DESKTOP)
    })
}

#[cfg(target_os = "linux")]
fn linux_bound_groups() -> HashSet<&'static str> {
    let Some(config_dir) = dirs::config_dir() else {
        return HashSet::new();
    };
    let Ok(text) = std::fs::read_to_string(config_dir.join("mimeapps.list")) else {
        return HashSet::new();
    };
    let defaults = parse_defaults(&text);
    GROUPS
        .iter()
        .filter(|group| group.available())
        .filter(|group| group_bound(&defaults, group))
        .map(|group| group.id)
        .collect()
}

#[cfg(target_os = "linux")]
fn linux_set_binding(group: &FileTypeGroup, on: bool) -> Result<(), String> {
    let data_dir = dirs::data_dir().ok_or("no user data directory")?;
    let config_dir = dirs::config_dir().ok_or("no user config directory")?;
    let apps_dir = data_dir.join("applications");
    std::fs::create_dir_all(&apps_dir).map_err(|error| error.to_string())?;
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let desktop_file = apps_dir.join(USER_DESKTOP);
    let mimeapps_path = config_dir.join("mimeapps.list");

    // The set of groups bound after this change, as recorded by the OS.
    let existing = std::fs::read_to_string(&mimeapps_path).unwrap_or_default();
    let defaults = parse_defaults(&existing);
    let mut bound: HashSet<&str> = GROUPS
        .iter()
        .filter(|candidate| candidate.available())
        .filter(|candidate| group_bound(&defaults, candidate))
        .map(|candidate| candidate.id)
        .collect();
    if on {
        bound.insert(group.id);
    } else {
        bound.remove(group.id);
    }

    // The desktop file lists the MIME types of everything bound, so it is
    // regenerated whole (or removed when nothing is bound any more).
    let mimes: Vec<&str> = GROUPS
        .iter()
        .filter(|candidate| bound.contains(candidate.id))
        .flat_map(|candidate| candidate.mimes)
        .collect();
    if mimes.is_empty() {
        match std::fs::remove_file(&desktop_file) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
    } else {
        let exec = exe.to_string_lossy();
        let mut desktop = String::from("[Desktop Entry]\nType=Application\nName=Lumiere\n");
        desktop.push_str(&format!("Exec={exec} %F\n"));
        desktop.push_str("Terminal=false\nNoDisplay=true\n");
        desktop.push_str(&format!("MimeType={}\n", mimes.join(";")));
        std::fs::write(&desktop_file, desktop).map_err(|error| error.to_string())?;
    }

    // The MIME types this change flips: added ones become ours, removed ones
    // have our token stripped (the previous default, if the desktop recorded
    // it, comes back to first place).
    let set: Vec<(&str, &str)> = if on {
        group
            .mimes
            .iter()
            .map(|mime| (*mime, USER_DESKTOP))
            .collect()
    } else {
        Vec::new()
    };
    let remove: Vec<&str> = if on { Vec::new() } else { group.mimes.to_vec() };
    let merged = merge_mimeapps(Some(&existing), &set, &remove);
    write_atomic(&mimeapps_path, &merged)?;
    Ok(())
}

/// Write `path` atomically: a same-directory temp file that is renamed into
/// place, the same discipline as `state.ini`.
fn write_atomic(path: &std::path::Path, content: &str) -> Result<(), String> {
    use std::io::Write;
    let temp = path.with_file_name(format!(
        "{}.tmp-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("mimeapps.list"),
        std::process::id()
    ));
    let mut file = std::fs::File::create(&temp).map_err(|error| error.to_string())?;
    if let Err(error) = file.write_all(content.as_bytes()) {
        let _ = std::fs::remove_file(&temp);
        return Err(error.to_string());
    }
    if let Err(error) = file.sync_all() {
        let _ = std::fs::remove_file(&temp);
        return Err(error.to_string());
    }
    std::fs::rename(&temp, path).map_err(|error| {
        let _ = std::fs::remove_file(&temp);
        error.to_string()
    })
}

// -- test log ---------------------------------------------------------------

#[cfg(any(test, feature = "test-support"))]
static TEST_LOG: std::sync::Mutex<Vec<(String, bool)>> = std::sync::Mutex::new(Vec::new());

#[cfg(any(test, feature = "test-support"))]
fn record_test_binding(id: &str, on: bool) {
    TEST_LOG
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push((id.to_owned(), on));
}

/// The `set_binding` calls recorded since the last drain (test builds only).
#[cfg(any(test, feature = "test-support"))]
pub fn test_bindings_drain() -> Vec<(String, bool)> {
    std::mem::take(
        &mut *TEST_LOG
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    )
}

#[cfg(test)]
mod filetypes_tests {
    use super::*;
    use crate::imaging::SUPPORTED_EXTS;

    #[test]
    fn groups_cover_every_supported_extension() {
        let mut seen = HashSet::new();
        for group in GROUPS {
            for ext in group.extensions {
                assert!(
                    seen.insert(*ext),
                    "extension {ext} belongs to two file type groups"
                );
            }
        }
        for ext in SUPPORTED_EXTS {
            assert!(seen.contains(ext), "no file type group claims {ext}");
        }
        // The video/model rows are always in the table (availability is a
        // runtime property of the feature flags), so their extensions are
        // always claimed.
        assert!(seen.contains("mp4") && seen.contains("avi") && seen.contains("glb"));
        assert!(
            seen.len() == SUPPORTED_EXTS.len() + 2 + 1,
            "file type groups claim {} extensions, SUPPORTED_EXTS has {}",
            seen.len(),
            SUPPORTED_EXTS.len()
        );
    }

    #[test]
    fn group_ids_are_valid_progids() {
        for group in GROUPS {
            assert!(
                group
                    .id
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()),
                "ProgID-unsafe id {:?}",
                group.id
            );
            assert!(!group.id.is_empty());
        }
        let labels: HashSet<&str> = GROUPS.iter().map(|group| group.label).collect();
        assert_eq!(
            labels.len(),
            GROUPS.len(),
            "row labels must be unique (headless tests click them by name)"
        );
    }

    #[test]
    fn headless_tests_short_circuit_the_os_layer() {
        assert!(bound_groups().is_empty());
        assert!(set_binding("jpeg", true).is_ok());
    }

    #[test]
    fn test_log_records_set_binding_calls() {
        test_bindings_drain(); // drop anything a parallel test recorded
        set_binding("jpeg", true).unwrap();
        set_binding("png", false).unwrap();
        let recorded = test_bindings_drain();
        assert!(
            recorded.contains(&("jpeg".to_owned(), true))
                && recorded.contains(&("png".to_owned(), false))
        );
    }

    #[test]
    fn open_command_quotes_paths_with_spaces() {
        assert_eq!(
            open_command(r"C:\My Apps\Lumiere.exe"),
            "\"C:\\My Apps\\Lumiere.exe\" \"%1\""
        );
    }

    #[test]
    fn registry_subkeys_are_lowercase_and_quoted() {
        assert_eq!(progid("jpeg"), "Lumiere.jpeg");
        assert_eq!(class_ext_subkey("JPG"), r"Software\Classes\.jpg");
        assert_eq!(
            app_ext_subkey("Lumiere.exe", "jpg"),
            r"Software\Classes\Applications\Lumiere.exe\SupportedFileTypes\.jpg"
        );
        assert_eq!(wide("ab"), vec![0x61, 0x62, 0x00]);
    }

    #[test]
    fn merge_preserves_foreign_entries_and_sections() {
        let existing = "\
[Default Applications]
image/png=org.other.desktop
; a comment
image/gif=org.other.desktop;second.desktop

[Added Applications]
image/png=second.desktop
";
        let merged = merge_mimeapps(Some(existing), &[("image/jpeg", USER_DESKTOP)], &[]);
        let mut lines = merged.lines();
        assert!(lines.any(|line| line == "[Default Applications]"));
        assert!(merged.contains("image/png=org.other.desktop"));
        assert!(merged.contains("image/gif=org.other.desktop;second.desktop"));
        assert!(merged.contains("[Added Applications]"));
        assert!(merged.contains("image/png=second.desktop"));
        assert!(merged.contains("image/jpeg=lumiere-filetypes.desktop"));
        assert!(merged.contains("; a comment"));
    }

    #[test]
    fn merge_replaces_default_and_restores_previous() {
        let existing =
            "[Default Applications]\nimage/png=org.other.desktop;lumiere-filetypes.desktop\n";
        let merged = merge_mimeapps(Some(existing), &[], &["image/png"]);
        assert_eq!(
            merged.lines().collect::<Vec<_>>().join("\n"),
            "[Default Applications]\nimage/png=org.other.desktop"
        );
    }

    #[test]
    fn merge_drops_line_when_only_ours_remained() {
        let existing = "[Default Applications]\nimage/png=lumiere-filetypes.desktop\n";
        let merged = merge_mimeapps(Some(existing), &[], &["image/png"]);
        assert_eq!(
            merged.lines().collect::<Vec<_>>(),
            vec!["[Default Applications]"]
        );
    }

    #[test]
    fn merge_appends_new_mimes_and_creates_section_when_absent() {
        let merged = merge_mimeapps(Some(""), &[("image/jpeg", USER_DESKTOP)], &[]);
        let lines: Vec<&str> = merged.lines().collect();
        assert!(lines.contains(&"[Default Applications]"));
        assert!(lines.contains(&"image/jpeg=lumiere-filetypes.desktop"));
    }

    #[test]
    fn merge_appends_missing_mimes_inside_existing_section() {
        let existing = "[Default Applications]\nimage/png=org.other.desktop\n";
        let merged = merge_mimeapps(Some(existing), &[("image/jpeg", USER_DESKTOP)], &[]);
        let lines: Vec<&str> = merged.lines().collect();
        let section = lines
            .iter()
            .position(|line| *line == "[Default Applications]")
            .unwrap();
        let added = lines
            .iter()
            .position(|line| *line == "image/jpeg=lumiere-filetypes.desktop")
            .unwrap();
        assert!(added > section);
    }

    #[test]
    fn parse_defaults_ignores_other_sections() {
        let defaults = parse_defaults(
            "[Default Applications]\nimage/png=org.other.desktop;lumiere-filetypes.desktop\n[Added Applications]\nimage/png=lumiere-filetypes.desktop\n",
        );
        assert_eq!(
            defaults.get("image/png"),
            Some(&vec![
                "org.other.desktop".to_owned(),
                "lumiere-filetypes.desktop".to_owned()
            ])
        );
        assert!(!defaults.contains_key("image/jpeg"));
    }
}
