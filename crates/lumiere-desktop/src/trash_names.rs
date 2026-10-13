use std::path::PathBuf;
pub fn trash_names(paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string())
        })
        .collect()
}
