# Lumiere

Image browser/viewer for macOS, written in Rust with
[egui](https://github.com/emilk/egui)/eframe. Three-pane browser (folder tree,
thumbnail grid, preview/information), full-screen viewer, and slideshow, in a
macOS-adaptive light/dark UI that follows the system appearance, with the
system font and monochrome line icons.

## Build and run

```sh
export PATH="$HOME/.cargo/bin:$PATH"   # if cargo is not already on PATH
cargo build --release
./target/release/lumiere /path/to/folder
```

Checks:

```sh
cargo fmt --check
cargo clippy --all-targets
cargo test
```

Packaging a `.app` bundle (icon, Info.plist, ad-hoc signature):

```sh
./bundle.sh          # -> build/Lumiere.app
```

Installing the bundle to a stable user-facing path (recommended for Dock icon
cache stability on macOS):

```sh
./install.sh                 # -> ~/Applications/Lumiere.app, then open it
./install.sh /path/to/folder # same, but launches that folder immediately
```

Session state (last folder, window size and position) is stored in
`~/Library/Application Support/Lumiere/state.ini`.

Note: Finder / `NSWorkspace` already see the committed app icon correctly. If
the Dock still shows the generic placeholder after swapping `AppIcon.icns`, it
is usually a Dock / IconServices cache issue; installing to one stable path
(`~/Applications/Lumiere.app`) and launching that copy is more reliable than
repeatedly running bundles from `build/`, `dist/`, or `/tmp`.

## Decoding

- Built-in via the `image` crate: JPEG, PNG, GIF, BMP, TIFF, WebP, TGA, PCX, ICO, …
- HEIC / HEIF / AVIF via macOS ImageIO (`src/imaging/macos.rs`, `objc2-image-io`)
- EXIF via `kamadak-exif`
- "Move to Trash" (`⌘⌫`) confirms with a native alert and moves items to the
  macOS Trash (`trash` crate), so they stay recoverable from Finder
- Thumbnail cache: `~/Library/Caches/com.james.Lumiere/thumbs`

## Layout

| File | Purpose |
| --- | --- |
| `src/main.rs` | eframe entry point, CLI folder argument |
| `src/app.rs` | app state, menus, toolbar, status bar, keyboard map, dialogs, tests |
| `src/browser.rs` | folder tree, thumbnail grid, preview / information pane |
| `src/viewer.rs` | full image view: zoom, pan, rotate, flip, floating toolbar |
| `src/loader.rs` | worker pool, request queue (full/preview jump the queue), reply channel |
| `src/imaging.rs` | format detection, decode, EXIF, thumbnail cache |
| `src/imaging/macos.rs` | ImageIO bridge for HEIC/HEIF/AVIF |
| `src/fsmodel.rs` | directory scan, sorting, wildcard filter, tree model |
| `src/theme.rs` | macOS visual layer: adaptive light/dark chrome, system font, monochrome line icons |
| `src/menu.rs` | the menu bar as one platform-neutral description |
| `src/menu/macos.rs` | that description as `NSMenu`s in the real macOS menu bar |
| `src/window.rs` | whether a restored window position is still on a display |

## Keyboard shortcuts

The menu bar is the source of truth: `App::menu_spec()` declares each command
with its `Shortcut`, and the same value becomes the macOS key equivalent, the
text drawn next to the menu item, and the egui key binding. On macOS the menu
bar owns the `⌘` chords and AppKit consumes them before the window sees them.

| Key | Action |
| --- | --- |
| Arrows | Move selection in the grid |
| Home / End | First / last image |
| PageUp / PageDown | Previous / next image |
| Space, Return | Open viewer |
| Esc | Close dialog → stop slideshow → leave full screen → close viewer |
| `⌃⌘F` | Toggle full screen |
| `⇧⌘Y` | Start / stop slideshow |
| `⌘⌫` | Move to Trash (asks first) |
| `⌥⌘R`, F2 | Rename |
| `⌘I` | Get Info |
| `⌘,` | Settings… |
| `⌘O` | Open… (choose a folder) |
| `⌘R` | Reload |
| `⌘A` / `⇧⌘A` / `⌥⌘A` | Select all / none / invert |
| `⇧⌘L` / `⇧⌘R` | Rotate left / right |
| `⌃⌘S` / `⌥⌘T` / `⌥⌘I` | Show sidebar / toolbar / inspector |
| `⇧⌘U` / `⇧⌘H` / `⇧⌘P` | Up one level / Home / Pictures |
| `⌘+` / `⌘-` / `⌘0` / `⇧⌘F` | Zoom in / out / actual size / fit to window |
| `+` / `-` / `1` / `F` | Same, without modifiers, in the viewer |

Note: shortcuts match on the key event's own modifiers, not on
`InputState::modifiers` — winit/eframe does not always update the latter in the
same frame as the key event.

## Testing

Unit and UI tests live next to the code (`#[cfg(test)] mod tests`). The UI tests
drive `egui::Context::run` headlessly against a temp folder fixture
(`fixture_dir`), covering browser → viewer → slideshow, dialogs, keyboard
navigation, the information tab, and command-modifier shortcuts. They also
assert the menu description itself: its titles and `⌘` spellings, and that the
shortcut a menu advertises is the same one the keyboard map handles.

The native menu bar cannot be exercised headlessly. Check it against the real
thing:

```sh
./target/debug/lumiere &
osascript -e 'tell application "System Events" to tell process "lumiere" to get name of every menu bar item of menu bar 1'
osascript -e 'tell application "System Events" to tell process "lumiere" to perform action "AXPress" of menu item "Show Sidebar" of menu 1 of menu bar item "View" of menu bar 1'
```

`osascript ... click at {x, y}` does not work on egui windows, so interactive
checks of the canvas go through key codes; the menu bar, by contrast, is real
AppKit and fully scriptable.
Full screen uses a native macOS Space, which `screencapture` may not capture.

## Status

Work in progress. See [TODO.md](TODO.md) for the current task list and what has
already been verified.
