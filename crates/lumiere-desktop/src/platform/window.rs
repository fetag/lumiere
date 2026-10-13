//! Where a window is allowed to appear.

/// Whether a window of this size at this position is reachable on any display.
///
/// A position remembered on a display that has since been unplugged puts the
/// title bar off screen, and then the window can only be moved by guessing.
/// macOS reports the displays; everywhere else the position is taken on trust.
#[cfg(target_os = "macos")]
pub fn position_is_on_screen(x: f32, y: f32, width: f32, height: f32) -> bool {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSScreen;

    let Some(mtm) = MainThreadMarker::new() else {
        return true;
    };
    let screens = NSScreen::screens(mtm);

    // egui measures from the top left of the primary display downwards, Cocoa
    // from its bottom left upwards, so the window has to be flipped before the
    // two can be compared.
    let primary = screens
        .iter()
        .find(|screen| screen.frame().origin.x == 0.0 && screen.frame().origin.y == 0.0)
        .or_else(|| screens.firstObject())
        .expect("NSScreen::screens is never empty");
    let primary_height = primary.frame().size.height;

    let left = f64::from(x);
    let right = left + f64::from(width);
    let bottom = primary_height - f64::from(y) - f64::from(height);
    let top = bottom + f64::from(height);

    screens.iter().any(|screen| {
        let frame = screen.frame();
        left < frame.origin.x + frame.size.width
            && right > frame.origin.x
            && bottom < frame.origin.y + frame.size.height
            && top > frame.origin.y
    })
}

#[cfg(not(target_os = "macos"))]
pub fn position_is_on_screen(_x: f32, _y: f32, _width: f32, _height: f32) -> bool {
    true
}
