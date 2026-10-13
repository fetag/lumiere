//! Dragging files into and out of the window.
//!
//! In. The system writes a drag's payload to the app's general pasteboard
//! the moment the drag enters the app, and it leaves no drop event behind —
//! the press and the release both belong to the application the drag came
//! from, so neither winit's `DroppedFile` nor a pointer state change is
//! ever delivered here. The drop is therefore recognized by watching the
//! pasteboard: a change in its change count while the mouse button is
//! *physically* held (queried from CoreGraphics, which needs no permission)
//! marks a drag as in flight, and the drag dropping is the button coming
//! up with the cursor over the window. A pasteboard change while the
//! button is idle is a plain copy and must not be mistaken for a drag.
//!
//! Out. A real `NSDraggingSession`, started from a synthesized mouse event
//! on the window's content view, with one `NSDraggingItem` per file whose
//! key is `public.file-url` — the type Finder matches on a drop.

use std::path::PathBuf;

/// One sample of the drag environment.
pub struct DragObservation {
    /// The general pasteboard's change count.
    pub count: u32,
    /// The left mouse button is physically held down right now, no matter
    /// where it was pressed.
    pub button_down: bool,
    /// The cursor is over the app's window.
    pub cursor_in_window: bool,
}

/// The drag-in state machine: it decides, from a stream of
/// [`DragObservation`]s, when a file drag was dropped on the window.
///
/// Kept pure on purpose — the native side only samples the environment,
/// so the decision itself can be tested without a window.
pub struct DropWatch {
    /// The change count when the pasteboard was last seen settle.
    base: Option<u32>,
    /// A file payload arrived while the button was held: a drag is (or was,
    /// up to this tick) in flight.
    in_flight: bool,
}

impl DropWatch {
    pub fn new() -> Self {
        Self {
            base: None,
            in_flight: false,
        }
    }

    /// True while a drag is being tracked; the caller keeps repaints going
    /// so the release is actually observed.
    pub fn in_flight(&self) -> bool {
        self.in_flight
    }

    /// Feed one sample. Returns true when a file drag was dropped on the
    /// window during this tick.
    pub fn tick(&mut self, count: Option<u32>, button_down: bool, cursor_in_window: bool) -> bool {
        let Some(count) = count else {
            // No native pasteboard (test build, other OS): nothing to say.
            self.base = None;
            self.in_flight = false;
            return false;
        };
        if self.in_flight {
            if !button_down {
                // The button came up: a drop when it ended over the window,
                // a cancel otherwise. Either way the payload is spent.
                self.in_flight = false;
                self.base = Some(count);
                return cursor_in_window;
            }
            if self.base != Some(count) {
                // A fresh payload replaced the one being dragged while the
                // button is still held: that drag is over.
                self.in_flight = false;
                self.base = Some(count);
            }
            return false;
        }
        if self.base == Some(count) {
            return false;
        }
        // The payload changed. While the button is physically held, that is
        // a drag arriving; while idle, it is a copy or the like — and a
        // later click must not "drop" it.
        self.base = Some(count);
        self.in_flight = button_down;
        false
    }
}

impl Default for DropWatch {
    fn default() -> Self {
        Self::new()
    }
}

/// Sample the drag environment, when a native environment exists.
///
/// `None` on other platforms and in a test build, where there is no
/// `NSApplication` behind the window; the caller then simply does not
/// watch for drops.
#[cfg(target_os = "macos")]
pub fn observe_drag() -> Option<DragObservation> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSEvent, NSPasteboard};
    use objc2_core_graphics::{CGEventSource, CGEventSourceStateID, CGMouseButton};

    if cfg!(test) {
        return None;
    }
    // AppKit and CoreGraphics queries want the main thread.
    let _mtm = MainThreadMarker::new()?;
    // The button state comes from CoreGraphics, not from the app's events:
    // a drag started in another app never presses *our* window, so egui's
    // pointer state would stay down for the whole thing.
    let button_down = CGEventSource::button_state(
        CGEventSourceStateID::CombinedSessionState,
        CGMouseButton::Left,
    );
    let count = NSPasteboard::pasteboardWithName(unsafe { objc2_app_kit::NSPasteboardNameDrag })
        .changeCount() as u32;
    // Finder keeps focus during a foreign drag, so keyWindow may be absent.
    // NSEvent and NSWindow already share AppKit screen coordinates.
    let cursor = NSEvent::mouseLocation();
    let cursor_in_window = NSApplication::sharedApplication(_mtm)
        .windows()
        .iter()
        .any(|window| {
            if !window.isVisible() {
                return false;
            }
            let frame = window.frame();
            frame.origin.x <= cursor.x
                && cursor.x <= frame.origin.x + frame.size.width
                && frame.origin.y <= cursor.y
                && cursor.y <= frame.origin.y + frame.size.height
        });
    Some(DragObservation {
        count,
        button_down,
        cursor_in_window,
    })
}

#[cfg(not(target_os = "macos"))]
pub fn observe_drag() -> Option<DragObservation> {
    None
}

/// The file paths currently carried by the general pasteboard — what a
/// just-finished drag left behind. Empty when the pasteboard holds nothing
/// that looks like files.
#[cfg(target_os = "macos")]
pub fn pasteboard_file_paths() -> Vec<PathBuf> {
    use objc2::rc::Retained;
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSPasteboard;
    use objc2_foundation::{NSArray, NSString, NSURL};

    if cfg!(test) {
        return Vec::new();
    }
    let Some(_mtm) = MainThreadMarker::new() else {
        return Vec::new();
    };
    // A drag writes its file payload as an `NSFilenamesPboardType` property
    // list: an array of path strings. Reading it back as a property list is
    // exactly what winit does for its own `DroppedFile` events.
    let pb = NSPasteboard::pasteboardWithName(unsafe { objc2_app_kit::NSPasteboardNameDrag });
    // Finder uses per-item file URLs on modern macOS. Decode URLs through
    // NSURL so spaces, non-ASCII names and percent escapes remain correct.
    if let Some(items) = pb.pasteboardItems() {
        let paths: Vec<PathBuf> = items
            .iter()
            .filter_map(|item| {
                let text = item.stringForType(unsafe { objc2_app_kit::NSPasteboardTypeFileURL })?;
                let url = NSURL::URLWithString(&text)?;
                if !url.isFileURL() {
                    return None;
                }
                url.path().map(|path| PathBuf::from(path.to_string()))
            })
            .collect();
        if !paths.is_empty() {
            return paths;
        }
    }
    // `NSFilenamesPboardType` is the classic file-drag type winit reads the
    // same way; it is marked deprecated in favour of per-item URL types, but
    // a drag still writes exactly this payload, so it is the right thing to
    // ask for.
    #[allow(deprecated)]
    let Some(list) = pb.propertyListForType(unsafe { objc2_app_kit::NSFilenamesPboardType }) else {
        return Vec::new();
    };
    // SAFETY: the property list for the filenames type is an array of path
    // strings; winit makes the same re-interpretation for the same payload.
    // `downcast` cannot name generic container types, so `cast_unchecked` is
    // the sanctioned route.
    let list: Retained<NSArray<NSString>> = unsafe { Retained::cast_unchecked(list) };
    list.iter()
        .map(|name| PathBuf::from(name.to_string()))
        .collect()
}

#[cfg(not(target_os = "macos"))]
pub fn pasteboard_file_paths() -> Vec<PathBuf> {
    Vec::new()
}

/// Start a native drag session carrying `paths` (the caller is already
/// holding the mouse button and moving it). `preview` is the side, in
/// points, of the drag preview AppKit shows while the session runs. Returns
/// true when a session was started; the system takes the mouse from here, so
/// the app should stop treating the held button as its own.
#[cfg(target_os = "macos")]
pub fn begin_file_drag(paths: &[PathBuf], preview: f64) -> bool {
    use objc2::rc::Retained;
    use objc2::runtime::ProtocolObject;
    use objc2::AnyThread;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSEvent, NSEventModifierFlags, NSEventType};
    use objc2_foundation::{NSArray, NSPoint, NSRect, NSSize, NSString};

    if cfg!(test) || paths.is_empty() {
        return false;
    }
    let Some(_mtm) = MainThreadMarker::new() else {
        return false;
    };
    // The session must be hosted by a live view in the window the drag
    // leaves from. NSEvent::mouseLocation and NSWindow::frame already use
    // AppKit screen coordinates (origin bottom-left); do not flip the y axis.
    let cursor = NSEvent::mouseLocation();
    // The windows list is back to front, so the last visible window
    // containing the cursor is the one on top.
    let mut host: Option<Retained<objc2_app_kit::NSWindow>> = None;
    for window in NSApplication::sharedApplication(_mtm).windows().iter() {
        if !window.isVisible() {
            continue;
        }
        let frame = window.frame();
        if cursor.x >= frame.origin.x
            && cursor.x <= frame.origin.x + frame.size.width
            && cursor.y >= frame.origin.y
            && cursor.y <= frame.origin.y + frame.size.height
        {
            host = Some(window.clone());
        }
    }
    let Some(view) = host.and_then(|window| window.contentView()) else {
        return false;
    };

    // The drag payload: one item per file, initialized with the file's URL
    // as its pasteboard writer — NSURL writes itself to the drag pasteboard
    // as `public.file-url`, the type Finder matches on a drop.
    let urls: Vec<Retained<objc2_foundation::NSURL>> = paths
        .iter()
        .map(|path| {
            let name = path.to_string_lossy();
            objc2_foundation::NSURL::fileURLWithPath(&NSString::from_str(name.as_ref()))
        })
        .collect();
    let items: Vec<Retained<objc2_app_kit::NSDraggingItem>> = urls
        .iter()
        .map(|url| {
            // NSURL conforms to NSPasteboardWriting, so the session can ask
            // it to write its file-URL payload straight onto the drag
            // pasteboard.
            let writer: &ProtocolObject<dyn objc2_app_kit::NSPasteboardWriting> =
                ProtocolObject::from_ref(&**url);
            let item = objc2_app_kit::NSDraggingItem::initWithPasteboardWriter(
                objc2_app_kit::NSDraggingItem::alloc(),
                writer,
            );
            // The drag preview is laid out from the dragging frame; the
            // session places it under the cursor. A zero-size frame is
            // rejected with an NSRangeException, so the caller supplies the
            // size of the cell the drag leaves from.
            item.setDraggingFrame(NSRect::new(
                NSPoint::new(0.0, 0.0),
                NSSize::new(preview, preview),
            ));
            item
        })
        .collect();
    let items = NSArray::from_retained_slice(&items);

    // The session tracks best when it starts from a genuine member of the
    // window server's event stream: the event AppKit is dispatching right
    // now (winit runs the frame inside that dispatch while the user holds
    // and moves the mouse) carries a live event number the tracking loop
    // recognizes. When this frame was not triggered by a pointer event
    // (repaint backstop, …) there is no such event and a synthesized one is
    // the fallback.
    let real = NSApplication::sharedApplication(_mtm)
        .currentEvent()
        .filter(|event| {
            let kind = event.r#type();
            kind == NSEventType::LeftMouseDragged || kind == NSEventType::LeftMouseDown
        });
    let event = real.or_else(|| {
        // NSEvent mouse-event locations are window-local, not screen-local.
        let window_number = view.window().map_or(0, |window| window.windowNumber());
        let location = view.window().map(|window| window.convertPointFromScreen(cursor))?;
        NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
            NSEventType::LeftMouseDragged,
            location,
            NSEventModifierFlags::empty(),
            0.0,
            window_number,
            None,
            0,
            1,
            1.0,
        )
    });
    let Some(event) = event else {
        return false;
    };

    // The source is the view hosting the session. The bindings do not spell
    // out NSView's conformance to NSDraggingSource, but AppKit provides the
    // protocol's methods on every NSView, so the re-interpretation the
    // `ProtocolObject` conversion does internally is sound.
    //
    // SAFETY: see above; `cast_unchecked` is exactly what
    // `ProtocolObject::from_retained` does once the conformance is proven.
    let source: Retained<ProtocolObject<dyn objc2_app_kit::NSDraggingSource>> =
        unsafe { Retained::cast_unchecked(view.clone()) };
    view.beginDraggingSessionWithItems_event_source(&items, &event, &source);
    true
}

#[cfg(not(target_os = "macos"))]
pub fn begin_file_drag(paths: &[PathBuf], preview: f64) -> bool {
    let _ = (paths, preview);
    false
}

#[cfg(test)]
mod tests {
    use super::DropWatch;

    /// A change in the payload while the button is physically held marks a
    /// drag; the drop is recognized only when the button comes up over the
    /// window.
    #[test]
    fn drop_is_recognized_on_release_over_the_window() {
        let mut w = DropWatch::new();
        // Idle baseline.
        assert!(!w.tick(Some(1), false, false));
        // The drag arrives while the button is held: no drop yet.
        assert!(!w.tick(Some(2), true, true));
        assert!(w.in_flight());
        // Still held, over the window: still no drop.
        assert!(!w.tick(Some(2), true, true));
        // Released over the window: the drop.
        assert!(w.tick(Some(2), false, true));
        assert!(!w.in_flight());
        // The spent payload does not drop again.
        assert!(!w.tick(Some(2), false, true));
    }

    /// Releasing outside the window is a cancel, not a drop.
    #[test]
    fn release_outside_the_window_is_a_cancel() {
        let mut w = DropWatch::new();
        assert!(!w.tick(Some(1), false, false));
        assert!(!w.tick(Some(2), true, true));
        assert!(w.in_flight());
        assert!(!w.tick(Some(2), false, false));
        assert!(!w.in_flight());
    }

    /// A payload change while the button is idle is a copy, not a drag: a
    /// later click must not "drop" it.
    #[test]
    fn an_idle_change_is_not_a_drag() {
        let mut w = DropWatch::new();
        assert!(!w.tick(Some(1), false, false));
        // Copied while idle.
        assert!(!w.tick(Some(2), false, false));
        assert!(!w.in_flight());
        // A plain click over the window: down then up, no change in payload.
        assert!(!w.tick(Some(2), true, true));
        assert!(!w.tick(Some(2), false, true));
    }

    /// A fresh payload replacing the in-flight one ends that drag.
    #[test]
    fn a_replaced_payload_cancels_the_previous_drag() {
        let mut w = DropWatch::new();
        assert!(!w.tick(Some(1), false, false));
        assert!(!w.tick(Some(2), true, true));
        assert!(w.in_flight());
        // A different drag's payload arrives while still held.
        assert!(!w.tick(Some(3), true, true));
        assert!(!w.in_flight());
        // Releasing now drops nothing.
        assert!(!w.tick(Some(3), false, true));
    }

    /// Without a native pasteboard there is nothing to watch.
    #[test]
    fn no_pasteboard_means_no_drop() {
        let mut w = DropWatch::new();
        assert!(!w.tick(None, true, true));
        assert!(!w.in_flight());
    }
}
