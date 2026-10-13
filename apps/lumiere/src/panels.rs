pub use lumiere_desktop::platform::panels::*;
pub fn application_icon(path: &std::path::Path) -> Option<egui::ColorImage> {
    let image = lumiere_desktop::platform::panels::application_icon(path)?;
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [image.width as usize, image.height as usize],
        &image.rgba,
    ))
}
