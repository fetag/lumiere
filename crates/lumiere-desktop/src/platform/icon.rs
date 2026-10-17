//! Window manager (taskbar) icon plumbing.

/// X11 window managers read `_NET_WM_ICON`, which may carry several sizes so
/// the manager picks one close to what it actually draws. winit writes only
/// the single icon it was given, so a 512px source gets shrunk ~10x in one
/// pass when the panel draws it at 48px and smears. Rewriting the property
/// with a ladder of properly downsampled sizes fixes the blur.
#[cfg(target_os = "linux")]
pub fn set_taskbar_icon(window_id: u32, png: &[u8]) {
    use std::os::raw::{c_int, c_uchar, c_ulong};
    use std::ptr;
    use x11_dl::xlib::Xlib;

    let Ok(decoded) = image::load_from_memory(png).map(|img| img.to_rgba8()) else {
        return;
    };

    // XChangeProperty's format=32 is a 32-bit X protocol value, but the Xlib
    // API requires an array of C `unsigned long` values (8 bytes on x86_64).
    // A Vec<u32> corrupts the property by making Xlib read adjacent values as
    // one long, which leaves Cinnamon no valid icon to display.
    // Cinnamon's panel uses the first entry rather than selecting the best
    // EWMH size. Put its native 48px panel size first; keep the rest for
    // other window managers and HiDPI displays.
    let sizes = [48, 32, 64, 24, 16, 128, 256, 512];
    let mut data: Vec<c_ulong> = Vec::new();
    for size in sizes {
        if size as u32 > decoded.width() {
            break;
        }
        let shrunk = image::imageops::resize(
            &decoded,
            size as u32,
            size as u32,
            image::imageops::FilterType::Lanczos3,
        );
        data.push(size as c_ulong);
        data.push(size as c_ulong);
        // `_NET_WM_ICON` pixels are AARRGGBB cardinals.
        for pixel in shrunk.pixels() {
            data.push(
                (u32::from(pixel[2])
                    | u32::from(pixel[1]) << 8
                    | u32::from(pixel[0]) << 16
                    | u32::from(pixel[3]) << 24) as c_ulong,
            );
        }
    }
    if data.is_empty() {
        return;
    }

    let Ok(xlib) = Xlib::open() else {
        return;
    };
    unsafe {
        let display = (xlib.XOpenDisplay)(ptr::null());
        if display.is_null() {
            return;
        }
        let net_wm_icon = (xlib.XInternAtom)(display, c"_NET_WM_ICON".as_ptr(), 0);
        let cardinal = (xlib.XInternAtom)(display, c"CARDINAL".as_ptr(), 0);
        (xlib.XChangeProperty)(
            display,
            window_id as c_ulong,
            net_wm_icon,
            cardinal,
            32,
            0, // PropModeReplace
            data.as_ptr() as *const c_uchar,
            data.len() as c_int,
        );
        (xlib.XFlush)(display);
        (xlib.XCloseDisplay)(display);
    }
}

#[cfg(not(target_os = "linux"))]
pub fn set_taskbar_icon(_window_id: u32, _png: &[u8]) {}
