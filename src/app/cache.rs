//! Texture and metadata caches, decode scheduling, and eviction budgets.

use super::*;

pub struct ThumbEntry {
    pub texture: TextureHandle,
    pub width: u32,
    pub height: u32,
}

pub struct FullEntry {
    pub texture: TextureHandle,
    pub width: u32,
    pub height: u32,
    /// Bytes of the uploaded texture (clamped), not of the decoded original.
    pub bytes: usize,
}

#[derive(Default)]
pub struct Caches {
    thumbs: HashMap<(PathBuf, u32), ThumbEntry>,
    thumbs_order: VecDeque<(PathBuf, u32)>,
    fulls: HashMap<PathBuf, FullEntry>,
    fulls_order: VecDeque<PathBuf>,
    infos: HashMap<PathBuf, Info>,
    thumb_bytes: usize,
    full_bytes: usize,
    pub pending: HashSet<(Kind, PathBuf, u32)>,
    /// Full-size decodes that failed: re-requesting them every frame would put
    /// the viewer into the same Loading-flicker loop as the eviction bug.
    failed_fulls: HashSet<PathBuf>,
    /// Thumbnail and preview decodes that failed: the grid and the preview
    /// pane would otherwise re-request the broken picture on every frame.
    failed_thumbs: HashSet<(PathBuf, u32)>,
}

pub(super) const MAX_THUMBS: usize = 4000;
pub(super) const MAX_FULLS: usize = 6;
/// Resident RGBA budgets, so a folder with thousands of images cannot grow the
/// texture cache without bound.
pub(super) const MAX_THUMB_BYTES: usize = 96 * 1024 * 1024;
pub(super) const MAX_FULL_BYTES: usize = 192 * 1024 * 1024;
/// Longest side of an uploaded full-size texture (~144 MB RGBA at the limit).
pub(super) const MAX_FULL_TEXTURE_SIDE: u32 = 6000;
/// Neighbour prefetch skips pictures whose decode would dwarf the budget.
pub(super) const MAX_PREFETCH_PIXELS: u64 = 50_000_000;

pub(super) fn rgba_bytes(width: u32, height: u32) -> usize {
    width as usize * height as usize * 4
}

impl Caches {
    pub fn thumb(&self, path: &Path, size: u32) -> Option<&ThumbEntry> {
        self.thumbs.get(&(path.to_path_buf(), size))
    }

    pub fn full(&self, path: &Path) -> Option<&FullEntry> {
        self.fulls.get(path)
    }

    pub fn full_failed(&self, path: &Path) -> bool {
        self.failed_fulls.contains(path)
    }

    pub fn thumb_failed(&self, path: &Path, size: u32) -> bool {
        self.failed_thumbs.contains(&(path.to_path_buf(), size))
    }

    /// Forget failed decodes, e.g. after a refresh: the file may have changed.
    pub fn clear_failures(&mut self) {
        self.failed_fulls.clear();
        self.failed_thumbs.clear();
    }

    /// Format + EXIF without decoding pixels, cached because the preview pane
    /// asks for it every frame.
    pub fn info(&mut self, path: &Path) -> Info {
        if let Some(info) = self.infos.get(path) {
            return info.clone();
        }
        let info = imaging::info(path);
        self.infos.insert(path.to_path_buf(), info.clone());
        info
    }

    /// Requests dropped by the loader never come back as replies, so their
    /// pending markers must be released or the paths are never decoded again.
    pub fn abandon(&mut self, dropped: &[Request]) {
        for request in dropped {
            self.pending
                .remove(&(request.kind, request.path.clone(), request.size));
        }
    }

    pub fn request(&mut self, loader: &Loader, kind: Kind, path: &Path, size: u32) {
        self.queue_work(loader, kind, path, size, false);
    }

    /// Same de-duplication as [`Self::request`], but the work goes to the back
    /// of the queue: used for images that are not on screen yet.
    pub fn prefetch(&mut self, loader: &Loader, kind: Kind, path: &Path, size: u32) {
        self.queue_work(loader, kind, path, size, true);
    }

    fn queue_work(&mut self, loader: &Loader, kind: Kind, path: &Path, size: u32, back: bool) {
        let key = (kind, path.to_path_buf(), size);
        if self.pending.contains(&key) {
            return;
        }
        // A thumbnail or preview that already failed is not retried on every
        // frame: the failure stays noted until the folder is rescanned.
        if matches!(kind, Kind::Thumb | Kind::Preview)
            && self.failed_thumbs.contains(&(path.to_path_buf(), size))
        {
            return;
        }
        if self.pending.len() > 512 {
            return;
        }
        if back {
            loader.submit_prefetch(kind, path.to_path_buf(), size);
        } else {
            loader.submit(kind, path.to_path_buf(), size);
        }
        self.pending.insert(key);
    }

    /// Move a full-size entry to the back of the LRU so the image actually on
    /// screen is never the one evicted by `trim` — otherwise the viewer and its
    /// neighbour prefetch chase each other and the picture flickers.
    pub fn protect_full(&mut self, path: &Path) {
        if !self.fulls.contains_key(path) {
            return;
        }
        if let Some(at) = self.fulls_order.iter().position(|k| k == path) {
            let key = self.fulls_order.remove(at).expect("position found");
            self.fulls_order.push_back(key);
        }
    }

    /// Move finished decodes into the texture caches. Returns how many arrived.
    /// `protect` is the picture currently on screen: it is moved to the back of
    /// the LRU before eviction so a prefetch landing behind it cannot push the
    /// displayed image out (the classic viewer flicker).
    pub fn pump(&mut self, ctx: &Context, loader: &Loader, protect: Option<&Path>) -> usize {
        let replies = loader.drain(24);
        let arrived = replies.len();
        for reply in replies {
            let request = reply.request;
            self.pending
                .remove(&(request.kind, request.path.clone(), request.size));
            let built = match reply.result {
                Ok(decoded) => {
                    // Never hand the GPU a texture bigger than it advertises:
                    // an oversized upload fails, the cache entry never sticks and
                    // the viewer re-requests the same picture every frame.
                    let max_side = ctx.input(|i| i.max_texture_side).max(512) as u32;
                    // A full-size texture also has to fit the resident byte
                    // budget, or one panorama evicts everything and thrashes.
                    let cap = match request.kind {
                        Kind::Full => max_side.min(MAX_FULL_TEXTURE_SIDE),
                        _ => max_side,
                    };
                    let upload = decoded.scaled(cap, cap);
                    let image = if upload.premultiplied {
                        egui::ColorImage::from_rgba_premultiplied(
                            [upload.width as usize, upload.height as usize],
                            &upload.rgba,
                        )
                    } else {
                        egui::ColorImage::from_rgba_unmultiplied(
                            [upload.width as usize, upload.height as usize],
                            &upload.rgba,
                        )
                    };
                    let name = format!(
                        "{:?}:{}",
                        request.kind,
                        request
                            .path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("")
                    );
                    let options = match request.kind {
                        Kind::Thumb => TextureOptions::LINEAR,
                        Kind::Preview => TextureOptions::LINEAR,
                        Kind::Full => TextureOptions::LINEAR,
                    };
                    let texture = ctx.load_texture(name, image, options);
                    Some((
                        texture,
                        upload.width,
                        upload.height,
                        if request.kind == Kind::Full {
                            decoded.original_size.map_or(decoded.width, |size| size.0)
                        } else {
                            decoded.width
                        },
                        if request.kind == Kind::Full {
                            decoded.original_size.map_or(decoded.height, |size| size.1)
                        } else {
                            decoded.height
                        },
                    ))
                }
                Err(error) => {
                    log::warn!("{}: {error}", request.path.display());
                    match request.kind {
                        Kind::Full => {
                            self.failed_fulls.insert(request.path.clone());
                        }
                        Kind::Thumb | Kind::Preview => {
                            self.failed_thumbs
                                .insert((request.path.clone(), request.size));
                        }
                    }
                    None
                }
            };
            let Some((texture, tex_w, tex_h, width, height)) = built else {
                continue;
            };
            match request.kind {
                Kind::Thumb | Kind::Preview => {
                    let key = (request.path.clone(), request.size);
                    if let Some(old) = self.thumbs.remove(&key) {
                        self.thumb_bytes -= rgba_bytes(old.width, old.height);
                        if let Some(at) = self.thumbs_order.iter().position(|k| *k == key) {
                            self.thumbs_order.remove(at);
                        }
                    }
                    self.thumb_bytes += rgba_bytes(tex_w, tex_h);
                    self.thumbs_order.push_back(key.clone());
                    self.thumbs.insert(
                        key,
                        ThumbEntry {
                            texture,
                            width,
                            height,
                        },
                    );
                }
                Kind::Full => {
                    let key = request.path.clone();
                    self.failed_fulls.remove(&key);
                    if let Some(old) = self.fulls.remove(&key) {
                        self.full_bytes -= old.bytes;
                        if let Some(at) = self.fulls_order.iter().position(|k| *k == key) {
                            self.fulls_order.remove(at);
                        }
                    }
                    let bytes = rgba_bytes(tex_w, tex_h);
                    self.full_bytes += bytes;
                    self.fulls_order.push_back(key.clone());
                    self.fulls.insert(
                        key,
                        FullEntry {
                            texture,
                            width,
                            height,
                            bytes,
                        },
                    );
                }
            }
        }
        self.trim(protect);
        arrived
    }

    fn trim(&mut self, protect: Option<&Path>) {
        while self.thumbs_order.len() > MAX_THUMBS || self.thumb_bytes > MAX_THUMB_BYTES {
            let Some(key) = self.thumbs_order.pop_front() else {
                break;
            };
            if let Some(old) = self.thumbs.remove(&key) {
                self.thumb_bytes -= rgba_bytes(old.width, old.height);
            }
        }
        // The displayed picture goes to the back of the LRU *before* eviction:
        // arrivals queued behind it must be the ones that fall out.
        if let Some(path) = protect {
            if let Some(at) = self.fulls_order.iter().position(|k| k == path) {
                let key = self.fulls_order.remove(at).expect("position found");
                self.fulls_order.push_back(key);
            }
        }
        while self.fulls_order.len() > 1
            && (self.fulls_order.len() > MAX_FULLS || self.full_bytes > MAX_FULL_BYTES)
        {
            let Some(key) = self.fulls_order.pop_front() else {
                break;
            };
            if let Some(old) = self.fulls.remove(&key) {
                self.full_bytes -= old.bytes;
            }
        }
    }

    pub fn clear_thumbs(&mut self) {
        self.thumbs.clear();
        self.thumbs_order.clear();
        self.thumb_bytes = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumb_cache_evicts_to_stay_within_budget() {
        let ctx = egui::Context::default();
        let mut caches = Caches::default();
        let image = egui::ColorImage::filled([1000, 1000], egui::Color32::WHITE);

        for index in 0..60 {
            let texture =
                ctx.load_texture(format!("thumb{index}"), image.clone(), Default::default());
            let key = (
                PathBuf::from(format!("/tmp/lumiere_budget_{index}.png")),
                100,
            );
            caches.thumb_bytes += rgba_bytes(1000, 1000);
            caches.thumbs_order.push_back(key.clone());
            caches.thumbs.insert(
                key,
                ThumbEntry {
                    texture,
                    width: 1000,
                    height: 1000,
                },
            );
        }
        caches.trim(None);

        let expected = MAX_THUMB_BYTES / rgba_bytes(1000, 1000);
        assert_eq!(caches.thumbs.len(), expected);
        assert_eq!(caches.thumbs_order.len(), expected);
        assert!(caches.thumb_bytes <= MAX_THUMB_BYTES);
        assert!(caches
            .thumbs
            .contains_key(&(PathBuf::from("/tmp/lumiere_budget_59.png"), 100)));
    }
}
