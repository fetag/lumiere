//! Image decoding, thumbnail generation, disk thumbnail cache and file metadata.

use std::fs;
use std::io::{BufReader, Cursor};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{error::Error, fmt};

use image::{ImageDecoder, ImageFormat, ImageReader, RgbaImage};

use lumiere_core::imaging::{apply_orientation, decode_pcx, decode_with_image};
/// Decoding happens before a texture can be downscaled. Keep untrusted images
/// from making a worker allocate an unbounded RGBA buffer (four workers may
/// decode concurrently).
pub use lumiere_core::imaging::{check_decode_dimensions, Decoded, MAX_DECODE_PIXELS};

/// A decode failure with stable semantics for callers that need to decide
/// whether retrying can help.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeError {
    ResourceLimit(String),
    Io(String),
    Corrupt(String),
    Panic(String),
}

impl DecodeError {
    pub fn from_message(message: impl Into<String>) -> Self {
        let message = message.into();
        if message.starts_with("resource limit:") || message.starts_with("image is too large") {
            Self::ResourceLimit(message)
        } else if message.starts_with("open failed:") || message.starts_with("pcx:") {
            Self::Io(message)
        } else {
            Self::Corrupt(message)
        }
    }

    pub fn is_resource_limit(&self) -> bool {
        matches!(self, Self::ResourceLimit(_))
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::ResourceLimit(message)
            | Self::Io(message)
            | Self::Corrupt(message)
            | Self::Panic(message) => message,
        };
        f.write_str(message)
    }
}

impl Error for DecodeError {}

#[cfg(target_os = "macos")]
use crate::platform::image_io as macos;

/// Extensions we know how to display.
pub const SUPPORTED_EXTS: &[&str] = &[
    "jpg", "jpeg", "jpe", "jfif", "png", "bmp", "rle", "gif", "webp", "tif", "tiff", "tga", "vda",
    "icb", "vst", "ico", "cur", "pcx", "qoi", "pnm", "pbm", "pgm", "ppm", "pam", "heic", "heif",
    "hif", "avif", "svg",
    // 3D and game pipelines: the `image` crate has pure-Rust decoders for these.
    "dds", "exr", "hdr", "farbfeld", "ff",
    // Camera RAW: the `image` crate cannot decode these, but macOS ImageIO can
    // (decode() already falls back to it).
    "dng", "cr2", "cr3", "nef", "nrw", "arw", "srf", "sr2", "orf", "rw2", "raf", "pef", "raw",
];

#[cfg(test)]
mod bundle_tests {
    use super::{DecodeError, SUPPORTED_EXTS};

    #[test]
    fn bundle_declares_every_supported_extension() {
        let bundle = include_str!("../../../bundle.sh");
        for extension in SUPPORTED_EXTS {
            assert!(
                bundle.contains(&format!("<string>{extension}</string>")),
                "bundle.sh is missing the {extension} document extension"
            );
        }
    }

    #[test]
    fn bundle_uses_cargo_version_metadata() {
        let bundle = include_str!("../../../bundle.sh");
        assert!(bundle.contains("cargo metadata --no-deps --format-version 1"));
        assert!(bundle.contains("<string>$VERSION</string>"));
    }

    #[test]
    fn decode_error_preserves_resource_limit_semantics() {
        let error = DecodeError::from_message("resource limit: too many pixels");
        assert!(error.is_resource_limit());
        assert_eq!(error.to_string(), "resource limit: too many pixels");
        assert!(!DecodeError::from_message("decode failed: truncated PNG").is_resource_limit());
    }
}

pub fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default()
}

/// 影片支援僅在啟用實驗性 video feature 時開放。
pub fn is_video(path: &Path) -> bool {
    cfg!(feature = "video") && matches!(extension(path).as_str(), "mp4" | "avi")
}

/// 3D 模型（.glb）支援僅在啟用實驗性 models feature 時開放。
///
/// 與 video 同樣的 feature-gate 先例：glb 不進 `SUPPORTED_EXTS`（那些是
/// `image` crate / ImageIO 解得到的影像格式），feature 關閉時 `.glb`
/// 對 `is_image` 不成立，browser 不列出、viewer 拒絕開啟。
pub fn is_model(path: &Path) -> bool {
    cfg!(feature = "models") && extension(path) == "glb"
}

pub fn is_image(path: &Path) -> bool {
    let ext = extension(path);
    (!ext.is_empty() && SUPPORTED_EXTS.contains(&ext.as_str())) || is_video(path) || is_model(path)
}

/// Decode an image, applying EXIF orientation when needed.
pub fn decode(path: &Path) -> Result<Decoded, DecodeError> {
    let ext = extension(path);

    if ext == "svg" {
        return match decode_svg(path) {
            Ok(img) => Ok(img),
            // resvg is strict about XML; give ImageIO a chance on macOS before
            // declaring the document corrupt.
            #[cfg(target_os = "macos")]
            Err(primary) => macos::decode(path)
                .map_err(|fallback| DecodeError::Corrupt(format!("{primary}; {fallback}"))),
            #[cfg(not(target_os = "macos"))]
            Err(primary) => Err(DecodeError::from_message(primary)),
        };
    }

    // Reject known oversized dimensions before considering a native fallback.
    if let Some((width, height)) = pixel_size(path) {
        if let Err(error) = check_decode_dimensions(width, height) {
            #[cfg(target_os = "macos")]
            if width > 0 && height > 0 {
                return macos::decode(path).map_err(DecodeError::from_message);
            }
            return Err(DecodeError::ResourceLimit(error));
        }
    }

    if ext == "pcx" {
        return decode_pcx(path).map_err(DecodeError::from_message);
    }

    // Native ImageIO first for the formats Apple handles best (HEIC/AVIF/TIFF).
    #[cfg(target_os = "macos")]
    {
        if matches!(ext.as_str(), "heic" | "heif" | "hif" | "avif") {
            return macos::decode(path)
                .map(|img| apply_orientation(path, img))
                .map_err(DecodeError::from_message);
        }
    }

    match decode_with_image(path) {
        Ok(img) => Ok(apply_orientation(path, img)),
        Err(primary) if DecodeError::from_message(primary.clone()).is_resource_limit() => {
            Err(DecodeError::from_message(primary))
        }
        #[cfg(target_os = "macos")]
        Err(primary) => macos::decode(path)
            .map(|img| apply_orientation(path, img))
            .map_err(|fallback| DecodeError::Corrupt(format!("{primary}; {fallback}"))),
        #[cfg(not(target_os = "macos"))]
        Err(primary) => Err(DecodeError::from_message(primary)),
    }
}

/// Render a vector SVG with resvg. Unlike raster formats, an oversized
/// document is not rejected but rendered at a reduced size: vector content
/// can be drawn at any scale, so the pixel budget becomes a render target.
/// Animated (SMIL) documents render their first frame only.
pub fn decode_svg(path: &Path) -> Result<Decoded, String> {
    decode_svg_at_size(path, 0)
}

/// 依最長邊的目標像素數渲染 SVG；0 表示使用原始尺寸。
pub fn decode_svg_at_size(path: &Path, max_side: u32) -> Result<Decoded, String> {
    let data = fs::read(path).map_err(|e| format!("open failed: {e}"))?;
    let tree = usvg::Tree::from_data(&data, &usvg::Options::default())
        .map_err(|e| format!("decode failed: {e}"))?;
    let size = tree.size();
    // SVG 可使用小數尺寸或英吋等單位；必須保留小數計算畫布，
    // 否則截斷後的 pixmap 會裁掉右側與底部。
    let (width, height) = (f64::from(size.width()), f64::from(size.height()));
    let intrinsic = (
        width.round().max(1.0) as u32,
        height.round().max(1.0) as u32,
    );
    // Oversized documents are shrunk to fit the pixel budget instead of being
    // rejected; `original_size` then carries the true document dimensions.
    let requested_scale = if max_side == 0 {
        1.0
    } else {
        f64::from(max_side) / width.max(height)
    };
    let budget_scale = (MAX_DECODE_PIXELS as f64 / (width * height)).sqrt();
    let scale = requested_scale.min(budget_scale);
    let mut target_w = ((width * scale).round() as u32).max(1);
    let mut target_h = ((height * scale).round() as u32).max(1);
    if u64::from(target_w) * u64::from(target_h) > MAX_DECODE_PIXELS {
        if target_w >= target_h {
            target_w -= 1;
        } else {
            target_h -= 1;
        }
    }
    check_decode_dimensions(target_w, target_h)?;
    let transform = resvg::tiny_skia::Transform::from_scale(
        (f64::from(target_w) / width) as f32,
        (f64::from(target_h) / height) as f32,
    );
    let mut pixmap = resvg::tiny_skia::Pixmap::new(target_w, target_h)
        .ok_or_else(|| format!("resource limit: SVG render surface {target_w} × {target_h}"))?;
    {
        let mut surface = pixmap.as_mut();
        resvg::render(&tree, transform, &mut surface);
    }
    // tiny-skia stores premultiplied RGBA; the rest of the pipeline (including
    // the PNG thumbnail cache) expects straight alpha, so demultiply here.
    let rgba = pixmap.take_demultiplied();
    Ok(Decoded {
        width: target_w,
        height: target_h,
        rgba,
        premultiplied: false,
        original_size: Some(intrinsic),
    })
}

/// Metadata shown in the preview pane.
#[derive(Clone)]
pub struct Info {
    pub format: String,
    pub width: u32,
    pub height: u32,
    pub exif: Vec<(String, String)>,
}

/// Header-only dimensions: no pixels are decoded. SVG is a text format the
/// `image` crate cannot sniff, so it goes through usvg (parse only, no render).
pub fn pixel_size(path: &Path) -> Option<(u32, u32)> {
    if extension(path) == "svg" {
        let data = fs::read(path).ok()?;
        let size = usvg::Tree::from_data(&data, &usvg::Options::default())
            .ok()?
            .size();
        return Some((size.width() as u32, size.height() as u32));
    }
    let reader = ImageReader::open(path).ok()?.with_guessed_format().ok()?;
    let decoder = reader.into_decoder().ok()?;
    let (w, h) = decoder.dimensions();
    (w > 0 && h > 0).then_some((w, h))
}

pub fn info(path: &Path) -> Info {
    let mut format = extension(path).to_uppercase();
    if let Ok(reader) = ImageReader::open(path).and_then(|r| r.with_guessed_format()) {
        if let Some(fmt) = reader.format() {
            format = format!("{fmt:?}").to_uppercase();
        }
    }
    let (width, height) = pixel_size(path).unwrap_or((0, 0));
    Info {
        format,
        width,
        height,
        exif: exif_summary(path),
    }
}

pub fn exif_summary(path: &Path) -> Vec<(String, String)> {
    use exif::{In, Reader, Tag};

    let Ok(file) = fs::File::open(path) else {
        return Vec::new();
    };
    let mut reader = BufReader::new(file);
    let Ok(exif) = Reader::new().read_from_container(&mut reader) else {
        return Vec::new();
    };

    let wanted = [
        (Tag::Make, "Make"),
        (Tag::Model, "Model"),
        (Tag::Software, "Software"),
        (Tag::DateTimeOriginal, "Date/Time"),
        (Tag::ExposureTime, "Exposure"),
        (Tag::FNumber, "F-number"),
        (Tag::PhotographicSensitivity, "ISO"),
        (Tag::ISOSpeed, "ISO speed"),
        (Tag::FocalLength, "Focal length"),
        (Tag::Flash, "Flash"),
        (Tag::WhiteBalance, "White balance"),
        (Tag::MeteringMode, "Metering"),
        (Tag::ExposureProgram, "Exposure program"),
        (Tag::ExposureBiasValue, "Exposure bias"),
        (Tag::LensModel, "Lens"),
        (Tag::Artist, "Artist"),
        (Tag::ImageDescription, "Description"),
        (Tag::Orientation, "Orientation"),
    ];

    let mut out = Vec::new();
    for (tag, label) in wanted {
        if let Some(field) = exif.get_field(tag, In::PRIMARY) {
            let value = field.display_value().to_string();
            let value = value.trim_matches('"').to_string();
            if !value.is_empty() {
                out.push((label.to_string(), value));
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Test fixtures
// ---------------------------------------------------------------------------

#[cfg(any(test, feature = "test-support"))]
fn ascii_entry(tag: u16, value: &str, data: &mut Vec<u8>, data_start: usize) -> [u8; 12] {
    let bytes = format!("{value}\0").into_bytes();
    let mut entry = [0u8; 12];
    entry[0..2].copy_from_slice(&tag.to_le_bytes());
    entry[2..4].copy_from_slice(&2u16.to_le_bytes());
    entry[4..8].copy_from_slice(&(bytes.len() as u32).to_le_bytes());
    if bytes.len() <= 4 {
        entry[8..8 + bytes.len()].copy_from_slice(&bytes);
    } else {
        let offset = (data_start + data.len()) as u32;
        entry[8..12].copy_from_slice(&offset.to_le_bytes());
        data.extend_from_slice(&bytes);
    }
    entry
}

#[cfg(any(test, feature = "test-support"))]
fn rational_entry(
    tag: u16,
    numerator: u32,
    denominator: u32,
    data: &mut Vec<u8>,
    data_start: usize,
) -> [u8; 12] {
    let mut entry = [0u8; 12];
    entry[0..2].copy_from_slice(&tag.to_le_bytes());
    entry[2..4].copy_from_slice(&5u16.to_le_bytes());
    entry[4..8].copy_from_slice(&1u32.to_le_bytes());
    let offset = (data_start + data.len()) as u32;
    entry[8..12].copy_from_slice(&offset.to_le_bytes());
    data.extend_from_slice(&numerator.to_le_bytes());
    data.extend_from_slice(&denominator.to_le_bytes());
    entry
}

#[cfg(any(test, feature = "test-support"))]
fn short_entry(tag: u16, value: u16) -> [u8; 12] {
    let mut entry = [0u8; 12];
    entry[0..2].copy_from_slice(&tag.to_le_bytes());
    entry[2..4].copy_from_slice(&3u16.to_le_bytes());
    entry[4..8].copy_from_slice(&1u32.to_le_bytes());
    entry[8..10].copy_from_slice(&value.to_le_bytes());
    entry
}

/// Writes a real JPEG with an APP1/EXIF block, so the preview pane can be
/// tested against actual metadata instead of an empty tag list.
#[cfg(any(test, feature = "test-support"))]
pub fn write_exif_jpeg(path: &Path) -> std::io::Result<()> {
    let image = image::RgbImage::from_pixel(32, 24, image::Rgb([9, 99, 199]));
    image
        .save(path)
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    let jpeg = std::fs::read(path)?;

    // IFD0: 4 strings, Orientation and the Exif sub-IFD pointer.
    let strings = [
        (0x010Fu16, "LumiereTest"),
        (0x0110, "CAM-1"),
        (0x0131, "Lumiere test"),
        (0x013B, "J. Lin"),
    ];
    let count = strings.len() + 2;
    let data_start = 8 + 2 + count * 12 + 4;

    let mut data = Vec::new();
    let mut entries: Vec<(u16, [u8; 12])> = Vec::new();
    for (tag, value) in strings {
        entries.push((tag, ascii_entry(tag, value, &mut data, data_start)));
    }
    entries.push((0x0112, short_entry(0x0112, 1)));

    // Camera settings belong in the Exif sub-IFD, not in IFD0.
    let sub_ifd_at = data_start + data.len();
    let sub_strings = [(0x9003u16, "2026:10:11 09:30:00")];
    let sub_rationals = [(0x829Au16, 1u32, 125u32), (0x829D, 28, 10), (0x920A, 35, 1)];
    let sub_count = sub_strings.len() + sub_rationals.len() + 1;
    let sub_data_start = sub_ifd_at + 2 + sub_count * 12 + 4;

    let mut sub_data = Vec::new();
    let mut sub_entries: Vec<(u16, [u8; 12])> = Vec::new();
    for (tag, value) in sub_strings {
        sub_entries.push((tag, ascii_entry(tag, value, &mut sub_data, sub_data_start)));
    }
    for (tag, numerator, denominator) in sub_rationals {
        sub_entries.push((
            tag,
            rational_entry(tag, numerator, denominator, &mut sub_data, sub_data_start),
        ));
    }
    sub_entries.push((0x8827, short_entry(0x8827, 200)));
    sub_entries.sort_by_key(|(tag, _)| *tag);

    let mut exif_pointer = [0u8; 12];
    exif_pointer[0..2].copy_from_slice(&0x8769u16.to_le_bytes());
    exif_pointer[2..4].copy_from_slice(&4u16.to_le_bytes());
    exif_pointer[4..8].copy_from_slice(&1u32.to_le_bytes());
    exif_pointer[8..12].copy_from_slice(&(sub_ifd_at as u32).to_le_bytes());
    entries.push((0x8769, exif_pointer));
    entries.sort_by_key(|(tag, _)| *tag);

    let mut tiff = Vec::new();
    tiff.extend_from_slice(b"II");
    tiff.extend_from_slice(&42u16.to_le_bytes());
    tiff.extend_from_slice(&8u32.to_le_bytes());
    tiff.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    for (_, entry) in &entries {
        tiff.extend_from_slice(entry);
    }
    tiff.extend_from_slice(&0u32.to_le_bytes());
    tiff.extend_from_slice(&data);

    tiff.extend_from_slice(&(sub_entries.len() as u16).to_le_bytes());
    for (_, entry) in &sub_entries {
        tiff.extend_from_slice(entry);
    }
    tiff.extend_from_slice(&0u32.to_le_bytes());
    tiff.extend_from_slice(&sub_data);

    let payload = [b"Exif\0\0".to_vec(), tiff].concat();
    let mut segment = vec![0xFF, 0xE1];
    segment.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
    segment.extend_from_slice(&payload);

    let mut with_exif = vec![0xFF, 0xD8];
    with_exif.extend_from_slice(&segment);
    with_exif.extend_from_slice(&jpeg[2..]);
    std::fs::write(path, with_exif)
}

// ---------------------------------------------------------------------------
// On-disk thumbnail cache (the classic "thumbnail database" idea)
// ---------------------------------------------------------------------------

pub fn cache_root() -> PathBuf {
    let base = dirs::cache_dir().unwrap_or_else(std::env::temp_dir);
    // QuickLook-style: ~/Library/Caches/<bundle-id>/thumbs on macOS.
    #[cfg(target_os = "macos")]
    let app = "net.fetag.lumiere";
    #[cfg(not(target_os = "macos"))]
    let app = "Lumiere";
    base.join(app).join("thumbs")
}

/// True when the two app-owned components of the cache root (the `<app>`
/// directory and `thumbs`) are not symbolic links. A local user who can
/// pre-create one of them as a symlink before the first launch would
/// otherwise redirect cache writes and pruning to a directory of their
/// choice; the check runs once per process. Higher prefixes are deliberately
/// not inspected: platform symlinks such as macOS `/var -> /private/var`
/// are legitimate, and the app-owned components are the ones the app itself
/// would create.
pub fn cache_root_safe(root: &Path) -> bool {
    [
        Some(root.to_path_buf()),
        root.parent().map(|parent| parent.to_path_buf()),
    ]
    .into_iter()
    .flatten()
    .all(|prefix| match fs::symlink_metadata(&prefix) {
        Ok(meta) => !meta.file_type().is_symlink(),
        // Not there yet; the cache write will create it.
        Err(_) => true,
    })
}

/// The once-per-process result of [`cache_root_safe`] for the actual root.
/// When it fails the on-disk thumbnail cache is disabled for this session.
fn disk_cache_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        let safe = cache_root_safe(&cache_root());
        if !safe {
            log::warn!(
                "cache root {} contains a symbolic link; the on-disk thumbnail \
                 cache is disabled for this session",
                cache_root().display()
            );
        }
        safe
    })
}

fn fnv1a(bytes: &[u8], seed: u64) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325 ^ seed;
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn cache_key(path: &Path, size: u32) -> PathBuf {
    let lossy = path.to_string_lossy();
    let mtime = fs::metadata(path)
        .and_then(|m| m.modified())
        .map(|t| {
            t.duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        })
        .unwrap_or(0);
    let h = fnv1a(lossy.as_bytes(), mtime);
    PathBuf::from(format!("{h:016x}_{size}.png"))
}

fn cached_thumbnail_in(root: &Path, path: &Path, size: u32) -> Option<PathBuf> {
    let p = root.join(cache_key(path, size));
    p.is_file().then_some(p)
}

fn store_thumbnail_in(root: &Path, path: &Path, size: u32, img: &Decoded) -> std::io::Result<()> {
    fs::create_dir_all(root)?;
    let Some(rgba) = RgbaImage::from_raw(img.width, img.height, img.rgba.clone()) else {
        return Ok(());
    };
    let mut buf = Cursor::new(Vec::new());
    rgba.write_to(&mut buf, ImageFormat::Png)
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    fs::write(root.join(cache_key(path, size)), buf.into_inner())
}

pub fn cached_thumbnail(path: &Path, size: u32) -> Option<PathBuf> {
    disk_cache_enabled()
        .then(|| cached_thumbnail_in(&cache_root(), path, size))
        .flatten()
}

pub fn store_thumbnail(path: &Path, size: u32, img: &Decoded) {
    if !disk_cache_enabled() {
        return;
    }
    if let Err(error) = store_thumbnail_in(&cache_root(), path, size, img) {
        log::warn!("could not store thumbnail for {}: {error}", path.display());
    }
}

pub fn read_cached_thumbnail(file: &Path) -> Option<Decoded> {
    decode_with_image(file).ok()
}

/// Delete the oldest cached thumbnails until the cache fits in `max_bytes`.
/// Returns the number of files removed.
pub fn prune_cache(dir: &Path, max_bytes: u64) -> std::io::Result<u64> {
    let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = Vec::new();
    let mut total = 0u64;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = fs::symlink_metadata(&path) else {
                continue;
            };
            // Cache contents are disposable, but must never turn a symlink
            // into permission to walk or delete outside the cache root.
            if meta.file_type().is_symlink() {
                continue;
            }
            if meta.is_dir() {
                stack.push(path);
                continue;
            }
            let mtime = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
            total += meta.len();
            files.push((mtime, meta.len(), path));
        }
    }
    if total <= max_bytes {
        return Ok(0);
    }
    files.sort_by_key(|(mtime, _, _)| *mtime);
    let mut removed = 0u64;
    for (_, size, path) in files {
        if total <= max_bytes {
            break;
        }
        if fs::remove_file(&path).is_ok() {
            total -= size;
            removed += 1;
        }
    }
    Ok(removed)
}

pub fn prune_thumbnail_cache(max_bytes: u64) -> std::io::Result<u64> {
    if !disk_cache_enabled() {
        return Ok(0);
    }
    prune_cache(&cache_root(), max_bytes)
}

pub fn clear_thumbnail_cache() -> std::io::Result<u64> {
    if !disk_cache_enabled() {
        return Ok(0);
    }
    let dir = cache_root();
    let mut count = 0u64;
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            if entry
                .path()
                .extension()
                .map(|e| e == "png")
                .unwrap_or(false)
                && entry
                    .file_type()
                    .map(|kind| !kind.is_symlink())
                    .unwrap_or(false)
                && fs::remove_file(entry.path()).is_ok()
            {
                count += 1;
            }
        }
    }
    Ok(count)
}

pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

pub fn format_time(time: SystemTime) -> String {
    chrono::DateTime::<chrono::Local>::from(time)
        .format("%Y/%m/%d %H:%M:%S")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_png(path: &Path, w: u32, h: u32) {
        let mut img = RgbaImage::new(w, h);
        for (x, y, pixel) in img.enumerate_pixels_mut() {
            *pixel = image::Rgba([
                (x % 255) as u8,
                (y % 255) as u8,
                128,
                if (x + y) % 7 == 0 { 0 } else { 255 },
            ]);
        }
        img.save(path).unwrap();
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lumiere_img_test_{tag}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn exif_summary_reads_a_real_jpeg_block() {
        let dir = temp_dir("exif");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("shot.jpg");
        write_exif_jpeg(&path).unwrap();

        let summary = exif_summary(&path);
        let found: std::collections::HashMap<String, String> = summary.into_iter().collect();
        assert_eq!(found.get("Make").map(String::as_str), Some("LumiereTest"));
        assert_eq!(found.get("Model").map(String::as_str), Some("CAM-1"));
        assert_eq!(
            found.get("Date/Time").map(String::as_str),
            Some("2026-10-11 09:30:00")
        );
        assert_eq!(found.get("Exposure").map(String::as_str), Some("1/125"));
        assert_eq!(found.get("ISO").map(String::as_str), Some("200"));
        assert_eq!(found.get("Focal length").map(String::as_str), Some("35"));

        assert_eq!(info(&path).format, "JPEG");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn decodes_png_and_reports_size() {
        let dir = temp_dir("png");
        let path = dir.join("shot.png");
        write_png(&path, 40, 24);
        let decoded = decode(&path).expect("png decode");
        assert_eq!((decoded.width, decoded.height), (40, 24));
        assert_eq!(decoded.rgba.len(), 40 * 24 * 4);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn write_svg(path: &Path, body: &str) {
        std::fs::write(path, body).unwrap();
    }

    #[test]
    fn decodes_svg_at_intrinsic_size() {
        let dir = temp_dir("svg");
        let path = dir.join("square.svg");
        write_svg(
            &path,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"40\" height=\"24\">\n\
             <rect width=\"40\" height=\"24\" fill=\"#f00\"/>\n</svg>",
        );
        let decoded = decode(&path).expect("svg decode");
        assert_eq!((decoded.width, decoded.height), (40, 24));
        assert!(!decoded.premultiplied);
        // Centre pixel: straight-alpha red.
        let centre = (12 * 40 + 19) as usize * 4;
        assert_eq!(&decoded.rgba[centre..centre + 4], &[255, 0, 0, 255]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn svg_renders_at_requested_resolution_without_losing_intrinsic_size() {
        let dir = temp_dir("svg_target");
        let path = dir.join("icon.svg");
        write_svg(&path, "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"12\"><circle cx=\"12\" cy=\"6\" r=\"5\" fill=\"red\"/></svg>");
        let image = decode_svg_at_size(&path, 600).unwrap();
        assert_eq!((image.width, image.height), (600, 300));
        assert_eq!(image.original_size, Some((24, 12)));
        assert_eq!(&image.rgba[(150 * 600 + 300) * 4..][..4], &[255, 0, 0, 255]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn svg_fractional_dimensions_preserve_right_and_bottom_edges() {
        let dir = temp_dir("svg_fractional");
        let path = dir.join("fractional.svg");
        write_svg(&path, "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"10.8\" height=\"8.8\" viewBox=\"0 0 10.8 8.8\"><rect width=\"10.8\" height=\"8.8\" fill=\"red\"/></svg>");
        let image = decode_svg_at_size(&path, 108).unwrap();
        assert_eq!((image.width, image.height), (108, 88));
        let pixel = ((87 * 108 + 107) * 4) as usize;
        assert_eq!(&image.rgba[pixel..pixel + 4], &[255, 0, 0, 255]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn oversized_svg_is_shrunk_to_fit_the_pixel_budget() {
        let dir = temp_dir("svg_large");
        let path = dir.join("huge.svg");
        write_svg(
            &path,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"30000\" height=\"30000\">\n\
             <rect width=\"30000\" height=\"30000\" fill=\"#00f\"/>\n</svg>",
        );
        let decoded = decode(&path).expect("oversized svg should render, not fail");
        let pixels = u64::from(decoded.width) * u64::from(decoded.height);
        assert!(pixels <= MAX_DECODE_PIXELS, "rendered {pixels} px");
        // The true document size is preserved for the information pane.
        assert_eq!(decoded.original_size, Some((30000, 30000)));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn svg_without_dimensions_uses_the_default_canvas() {
        let dir = temp_dir("svg_nosize");
        let path = dir.join("bare.svg");
        write_svg(
            &path,
            "<svg xmlns=\"http://www.w3.org/2000/svg\"><rect width=\"10\" height=\"10\"/></svg>",
        );
        let decoded = decode(&path).expect("bare svg decode");
        assert!(decoded.width > 0 && decoded.height > 0);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn corrupt_svg_is_a_decode_error() {
        let dir = temp_dir("svg_corrupt");
        let path = dir.join("broken.svg");
        write_svg(&path, "<svg xmlns=\"http://www.w3.org/2000/svg\"><rect");
        assert!(decode(&path).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn pixel_size_reads_svg_without_rendering() {
        let dir = temp_dir("svg_meta");
        let path = dir.join("meta.svg");
        write_svg(
            &path,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"40\" height=\"24\"></svg>",
        );
        assert_eq!(pixel_size(&path), Some((40, 24)));
        let info = info(&path);
        assert_eq!((info.width, info.height), (40, 24));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn scaled_keeps_aspect_ratio_and_never_upscales() {
        let dir = temp_dir("scale");
        let path = dir.join("wide.png");
        write_png(&path, 200, 100);
        let decoded = decode(&path).unwrap();
        let small = decoded.scaled(50, 50);
        assert_eq!((small.width, small.height), (50, 25));
        let same = decoded.scaled(800, 800);
        assert_eq!((same.width, same.height), (200, 100));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn thumbnail_cache_round_trip() {
        let dir = temp_dir("cache");
        let cache = dir.join("cache-root");
        let path = dir.join("cached.png");
        write_png(&path, 120, 80);
        let decoded = decode(&path).unwrap();
        let thumb = decoded.scaled(32, 32);
        assert!(cached_thumbnail_in(&cache, &path, 32).is_none());
        store_thumbnail_in(&cache, &path, 32, &thumb).expect("cache write");
        let file = cached_thumbnail_in(&cache, &path, 32).expect("cache entry");
        let reread = read_cached_thumbnail(&file).expect("cache read");
        assert_eq!((reread.width, reread.height), (32, 21));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn video_extensions_are_feature_gated() {
        for path in ["/tmp/video.mp4", "/tmp/video.AVI"] {
            assert_eq!(is_video(Path::new(path)), cfg!(feature = "video"));
            assert_eq!(is_image(Path::new(path)), cfg!(feature = "video"));
        }
        assert!(!is_video(Path::new("/tmp/image.gif")));
    }

    #[test]
    fn model_extensions_are_feature_gated() {
        for path in ["/tmp/model.glb", "/tmp/model.GLB"] {
            assert_eq!(is_model(Path::new(path)), cfg!(feature = "models"));
            assert_eq!(is_image(Path::new(path)), cfg!(feature = "models"));
        }
        assert!(!is_model(Path::new("/tmp/image.gif")));
        // 影像格式不受 models feature 影響。
        assert!(is_image(Path::new("/tmp/a.png")));
    }

    #[test]
    fn supported_extensions_cover_classic_formats() {
        assert!(is_image(Path::new("/tmp/a.HEIC")));
        assert!(is_image(Path::new("/tmp/a.pcx")));
        assert!(is_image(Path::new("/tmp/a.tga")));
        assert!(is_image(Path::new("/tmp/a.dng")));
        assert!(is_image(Path::new("/tmp/a.NEF")));
        assert!(is_image(Path::new("/tmp/a.svg")));
        assert!(is_image(Path::new("/tmp/a.pam")));
        assert!(is_image(Path::new("/tmp/a.dds")));
        assert!(is_image(Path::new("/tmp/a.exr")));
        assert!(is_image(Path::new("/tmp/a.hdr")));
        assert!(is_image(Path::new("/tmp/a.farbfeld")));
        assert!(!is_image(Path::new("/tmp/a.txt")));
    }

    fn write_dxt1_dds(path: &Path) {
        // Minimal uncompressed-pixel DDS is rejected by the image crate, so
        // the fixture is a 4x4 DXT1 surface: both color endpoints are solid
        // red and every index selects the first endpoint, making each pixel
        // opaque red.
        let mut bytes: Vec<u8> = vec![b'D', b'D', b'S', b' '];
        for value in [124u32, 0x1007, 4, 4, 0, 0, 0] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend([0u8; 44]); // dwReserved1
        bytes.extend_from_slice(&32u32.to_le_bytes()); // pixel format size
        bytes.extend_from_slice(&4u32.to_le_bytes()); // DDPF_FOURCC
        bytes.extend_from_slice(b"DXT1");
        bytes.extend([0u8; 20]); // bit count and channel masks
        for value in [0x1000u32, 0, 0, 0, 0] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        // DXT1 block: color0 0xF800 (red in 565), color1 0x0000, indices 0.
        bytes.extend_from_slice(&[0x00, 0xF8, 0x00, 0x00, 0, 0, 0, 0]);
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn decodes_dxt1_dds_fixture() {
        let dir = temp_dir("dds");
        let path = dir.join("tex.dds");
        write_dxt1_dds(&path);
        let decoded = decode(&path).expect("dds decode");
        assert_eq!((decoded.width, decoded.height), (4, 4));
        assert_eq!(&decoded.rgba[..8], &[255, 0, 0, 255, 255, 0, 0, 255]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn decodes_hdr_fixture() {
        let dir = temp_dir("hdr");
        let path = dir.join("shot.hdr");
        // Powers of two round-trip Radiance RGBE exactly.
        image::Rgb32FImage::from_pixel(8, 6, image::Rgb([0.5, 0.25, 0.125]))
            .save(&path)
            .unwrap();
        let decoded = decode(&path).expect("hdr decode");
        assert_eq!((decoded.width, decoded.height), (8, 6));
        assert_eq!(decoded.rgba.len(), 8 * 6 * 4);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn decodes_exr_fixture() {
        let dir = temp_dir("exr");
        let path = dir.join("shot.exr");
        image::Rgba32FImage::from_pixel(8, 6, image::Rgba([0.5, 0.25, 0.125, 1.0]))
            .save(&path)
            .unwrap();
        let decoded = decode(&path).expect("exr decode");
        assert_eq!((decoded.width, decoded.height), (8, 6));
        assert_eq!(decoded.rgba.len(), 8 * 6 * 4);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn decodes_farbfeld_fixture() {
        let dir = temp_dir("farbfeld");
        let path = dir.join("shot.ff");
        // `Rgba16Image` has no crate-root alias in image 0.25.
        image::ImageBuffer::from_pixel(8, 6, image::Rgba([1234u16, 5678, 9012, 40960]))
            .save(&path)
            .unwrap();
        let decoded = decode(&path).expect("farbfeld decode");
        assert_eq!((decoded.width, decoded.height), (8, 6));
        assert_eq!(decoded.rgba.len(), 8 * 6 * 4);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn decodes_pam_fixture() {
        let dir = temp_dir("pam");
        let path = dir.join("shot.pam");
        // PnmEncoder defaults to P6; PAM is the P7 (arbitrary) subtype.
        // Samples are flat RGBA8 bytes, so a plain buffer is the fixture.
        let rgba: Vec<u8> = std::iter::repeat_n([10u8, 20, 30, 255], 8 * 6)
            .flatten()
            .collect();
        let mut encoder =
            image::codecs::pnm::PnmEncoder::new(std::fs::File::create(&path).unwrap())
                .with_subtype(image::codecs::pnm::PnmSubtype::ArbitraryMap);
        encoder
            .encode(&*rgba, 8, 6, image::ColorType::Rgba8.into())
            .unwrap();
        let decoded = decode(&path).expect("pam decode");
        assert_eq!((decoded.width, decoded.height), (8, 6));
        assert_eq!(&decoded.rgba[..4], &[10, 20, 30, 255]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn imageio_decodes_heic_when_available() {
        let candidate = std::fs::read_dir("/System/Library/Desktop Pictures")
            .ok()
            .and_then(|entries| {
                entries
                    .flatten()
                    .map(|e| e.path())
                    .find(|p| extension(p) == "heic")
            });
        let Some(path) = candidate else {
            return;
        };
        let decoded = decode(&path).expect("heic decode");
        assert!(decoded.width > 32 && decoded.height > 32);
        assert_eq!(
            decoded.rgba.len(),
            decoded.width as usize * decoded.height as usize * 4
        );
    }
    #[test]
    fn prune_cache_removes_oldest_first() {
        let dir = std::env::temp_dir().join("lumiere_prune_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        for name in ["a.png", "b.png", "c.png"] {
            fs::write(dir.join(name), vec![0u8; 1000]).unwrap();
        }
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        fs::File::open(dir.join("a.png"))
            .unwrap()
            .set_modified(old)
            .unwrap();

        assert_eq!(prune_cache(&dir, 2500).unwrap(), 1);
        assert!(!dir.join("a.png").exists());
        assert!(dir.join("b.png").exists());
        assert!(dir.join("c.png").exists());

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn cache_root_safe_flags_symlinked_app_or_thumbs_dirs() {
        let base = std::env::temp_dir().join("lumiere_cachesafe_test");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();

        // A root that does not exist yet is safe: the first cache write
        // creates it.
        assert!(cache_root_safe(&base.join("app").join("thumbs")));

        // Plain directories are safe.
        fs::create_dir_all(base.join("real").join("thumbs")).unwrap();
        assert!(cache_root_safe(&base.join("real").join("thumbs")));

        // A symlinked leaf or parent would redirect cache writes (and
        // pruning) elsewhere, so it is unsafe. Symlink creation needs
        // privileges on some platforms; only assert when it succeeded.
        #[cfg(unix)]
        {
            fs::create_dir_all(base.join("target")).unwrap();
            std::os::unix::fs::symlink(base.join("target"), base.join("linked-leaf")).unwrap();
            assert!(!cache_root_safe(&base.join("linked-leaf")));

            fs::create_dir_all(base.join("linked-parent")).unwrap();
            fs::remove_dir(base.join("linked-parent")).unwrap();
            std::os::unix::fs::symlink(base.join("target"), base.join("linked-parent")).unwrap();
            assert!(!cache_root_safe(&base.join("linked-parent").join("thumbs")));
        }

        let _ = fs::remove_dir_all(&base);
    }
}
