//! Preload LaunchServices results and app artwork away from menu rendering.

use super::*;

#[derive(Clone)]
pub(crate) struct Application {
    pub name: String,
    pub path: PathBuf,
    pub default: bool,
    pub icon: Option<TextureHandle>,
}

struct LoadedApplication {
    name: String,
    path: PathBuf,
    default: bool,
    icon: Option<egui::ColorImage>,
}

pub(crate) struct OpenWithCache {
    tx: mpsc::Sender<(String, PathBuf)>,
    rx: mpsc::Receiver<(String, Vec<LoadedApplication>)>,
    entries: HashMap<String, Option<Vec<Application>>>,
    warmed_generation: Option<u64>,
}

impl Default for OpenWithCache {
    fn default() -> Self {
        let (tx, requests) = mpsc::channel::<(String, PathBuf)>();
        let (results, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut icons = HashMap::<PathBuf, Option<egui::ColorImage>>::new();
            while let Ok((key, path)) = requests.recv() {
                let mut load = || {
                    let mut apps = crate::panels::apps_to_open(&path);
                    let default = crate::panels::default_application(&path);
                    if let Some(application) = &default {
                        if !apps.iter().any(|(_, path)| path == application) {
                            apps.push((
                                application
                                    .file_stem()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .into_owned(),
                                application.clone(),
                            ));
                        }
                    }
                    apps.sort_by_key(|(name, path)| {
                        (default.as_ref() != Some(path), name.to_lowercase())
                    });
                    apps.dedup_by(|a, b| a.1 == b.1);
                    apps.into_iter()
                        .map(|(name, path)| {
                            let icon = icons
                                .entry(path.clone())
                                .or_insert_with(|| crate::panels::application_icon(&path))
                                .clone();
                            LoadedApplication {
                                name,
                                default: default.as_ref() == Some(&path),
                                path,
                                icon,
                            }
                        })
                        .collect()
                };
                #[cfg(target_os = "macos")]
                let apps = objc2::rc::autoreleasepool(|_| load());
                #[cfg(not(target_os = "macos"))]
                let apps = load();
                if results.send((key, apps)).is_err() {
                    break;
                }
            }
        });
        Self {
            tx,
            rx,
            entries: HashMap::new(),
            warmed_generation: None,
        }
    }
}

impl OpenWithCache {
    pub fn warm_folder(&mut self, entries: &[Entry], generation: u64) {
        if self.warmed_generation == Some(generation) {
            return;
        }
        self.warmed_generation = Some(generation);
        for entry in entries.iter().filter(|entry| entry.is_image) {
            self.prefetch(&entry.path);
        }
    }
    pub fn prefetch(&mut self, path: &Path) {
        let key = imaging::extension(path);
        if self.entries.contains_key(&key) || self.entries.len() >= 128 {
            return;
        }
        if self.tx.send((key.clone(), path.to_path_buf())).is_ok() {
            self.entries.insert(key, None);
        }
    }

    pub fn pump(&mut self, ctx: &Context) {
        while let Ok((key, loaded)) = self.rx.try_recv() {
            let applications = loaded
                .into_iter()
                .map(|application| Application {
                    icon: application.icon.map(|image| {
                        ctx.load_texture(
                            format!("app:{}", application.path.display()),
                            image,
                            TextureOptions::LINEAR,
                        )
                    }),
                    name: application.name,
                    path: application.path,
                    default: application.default,
                })
                .collect();
            self.entries.insert(key, Some(applications));
        }
        if self.entries.values().any(Option::is_none) {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    pub fn applications(&self, path: &Path) -> Option<Vec<Application>> {
        self.entries
            .get(&imaging::extension(path))
            .and_then(Clone::clone)
    }
}
