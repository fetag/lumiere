<p align="center">
  <img src="assets/AppIcon.png" alt="Lumiere icon" width="128" height="128">
</p>

<h1 align="center">Lumiere</h1>

<p align="center">
  A native-feeling image browser and viewer for macOS and Linux, built in Rust with
  <a href="https://github.com/emilk/egui">egui</a>/<a href="https://github.com/emilk/egui/tree/master/crates/eframe">eframe</a>.
</p>

<p align="center">
  Three-pane browsing, full-screen viewing, slideshow, and platform-adaptive UI.
</p>

---

## Why Lumiere?

Lumiere is a cross-platform image browser/viewer for macOS and Linux that aims
to feel at home on each supported desktop while staying lightweight and fast. It
uses a three-pane layout for everyday browsing, a dedicated full-screen viewer
for focused viewing, and a slideshow mode for hands-off playback.

The UI follows the system light/dark appearance where available, uses installed
system fonts when available, and keeps the chrome visually restrained with
monochrome line icons. Windows support is planned.

![Lumiere browse view](assets/lumiere-browse.webp)

## Highlights

- **Three-pane browser**: folder tree, thumbnail grid, and preview/information pane
- **Dedicated viewer**: zoom, pan, rotate, flip, actual size, and fit-to-window
- **Slideshow mode**: quick start/stop from the keyboard or menu bar
- **Platform-adaptive UI**: adaptive light/dark styling, native integrations where available, system fonts
- **Wide format support**: common formats via `image`; macOS additionally uses ImageIO for HEIC/HEIF/AVIF
- **Recoverable delete**: moves items to the system Trash after confirmation
- **Persistent session state**: restores the last folder and window geometry
- **Real menu bar integration**: keyboard shortcuts are defined once and reflected consistently in menus and app handling

## Quick start

### Build and run

```sh
export PATH="$HOME/.cargo/bin:$PATH"   # if cargo is not already on PATH
cargo build --release
./target/release/lumiere /path/to/folder
```

### Checks

```sh
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets
```

The current headless UI suite contains macOS-native assertions, so CI runs its
full serial test command on macOS:

```sh
cargo test --locked --workspace -- --test-threads=1
```

Linux CI currently validates formatting, Clippy with warnings denied, a release
build and the portable package. Porting the remaining macOS-specific UI tests is
ongoing.

### Package for macOS

Build a macOS app bundle with icon, `Info.plist`, and ad-hoc signature:

```sh
./bundle.sh          # -> build/Lumiere.app
```

### Install the macOS app to `~/Applications`

Install the bundle to a stable user-facing path. This is the recommended path
on macOS because it is more reliable for Dock icon cache behavior:

```sh
./install.sh                 # -> ~/Applications/Lumiere.app, then open it
./install.sh /path/to/folder # same, but launches that folder immediately
```

Session state is stored in:

- `~/Library/Application Support/Lumiere/state.ini`

Note: Finder / `NSWorkspace` already see the committed app icon correctly. If
the Dock still shows the generic placeholder after swapping `AppIcon.icns`, it
is usually a Dock / IconServices cache issue; installing to one stable path
(`~/Applications/Lumiere.app`) and launching that copy is more reliable than
repeatedly running bundles from `build/`, `dist/`, or `/tmp`.

### Package and install on Linux

Build the portable x86_64 Linux archive on a Linux build host:

```sh
./bundle-linux.sh       # -> dist/lumiere-v<version>-linux-x86_64.tar.gz
```

Extract the archive and run its per-user installer; no `sudo` is required:

```sh
tar -xzf lumiere-v<version>-linux-x86_64.tar.gz
cd lumiere-v<version>-linux-x86_64
./install.sh
```

The installer creates a desktop launcher and hicolor icon-theme entries. Launch
Lumiere from the application menu so the desktop shell can associate the running
window with its correctly sized application icon.

## Format support and storage

- Built-in via the `image` crate: JPEG, PNG, GIF, BMP, TIFF, WebP, TGA, PCX, ICO, …
- HEIC / HEIF / AVIF via macOS ImageIO (`crates/lumiere-desktop/src/platform/image_io.rs`, `objc2-image-io`)
- EXIF via `kamadak-exif`
- "Move to Trash" confirms before using the platform Trash (`trash` crate), so
  items stay recoverable through the desktop environment
- Thumbnail cache: the platform cache directory (`~/Library/Caches/net.fetag.lumiere/thumbs` on macOS)
- Rename, Move and Copy reject existing destination names rather than overwrite files. Copy also rejects symbolic links.
- Full-resolution decoding is limited to 50 million pixels. On macOS, larger images use an ImageIO downsampled representation (up to 6000 pixels per side) while retaining original dimensions. Background pixel processing is serialized to limit simultaneous decode allocations.

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

## Project layout

| File | Purpose |
| --- | --- |
| `apps/lumiere/src/main.rs` | eframe entry point, CLI folder argument |
| `apps/lumiere/src/app.rs` | app state, navigation, selection, native workflows and background jobs |
| `apps/lumiere/src/app/cache.rs` | texture / metadata caches, decode scheduling and eviction budgets |
| `apps/lumiere/src/app/commands.rs` | command dispatch, keyboard handling and slideshow transitions |
| `apps/lumiere/src/app/menus.rs` | shared menu descriptions, toolbar, address row and status bar |
| `apps/lumiere/src/app/dialogs.rs` | settings, rename, transfer, confirmations and feedback cards |
| `apps/lumiere/src/app/view.rs` | frame composition, panels and window geometry |
| `apps/lumiere/src/app/tests.rs` | existing headless application and interaction tests |
| `crates/lumiere-desktop/src/file_ops.rs` | filesystem operation safety rules and injectable Trash backend |
| `apps/lumiere/src/browser.rs` | folder tree, thumbnail grid, preview / information pane |
| `apps/lumiere/src/viewer.rs` | full image view: zoom, pan, rotate, flip, floating toolbar |
| `crates/lumiere-desktop/src/loader.rs` | worker pool, request queue (full/preview jump the queue), reply channel |
| `crates/lumiere-desktop/src/imaging.rs` | format detection, decode, EXIF, thumbnail cache |
| `crates/lumiere-desktop/src/platform/image_io.rs` | ImageIO bridge for HEIC/HEIF/AVIF |
| `crates/lumiere-desktop/src/fsmodel.rs` | directory scan, sorting, wildcard filter, tree model |
| `apps/lumiere/src/theme.rs` | platform-adaptive visual layer: light/dark chrome, system fonts, monochrome line icons |
| `crates/lumiere-desktop/src/menu_model.rs` | the menu bar as one platform-neutral description |
| `apps/lumiere/src/platform_ui/menu/macos.rs` | that description as `NSMenu`s in the real macOS menu bar |
| `crates/lumiere-desktop/src/platform/window.rs` | whether a restored window position is still on a display |

## Testing

Unit and UI tests live next to the code (`#[cfg(test)] mod tests`). The UI tests
drive `egui::Context::run` headlessly against a temp folder fixture
(`fixture_dir`), covering browser → viewer → slideshow, dialogs, keyboard
navigation, the information tab, and command-modifier shortcuts. They also
assert the menu description itself: its titles and `⌘` spellings, and that the
shortcut a menu advertises is the same one the keyboard map handles.

The macOS native menu bar cannot be exercised headlessly. Check it against the
real thing:

```sh
./target/debug/lumiere &
osascript -e 'tell application "System Events" to tell process "lumiere" to get name of every menu bar item of menu bar 1'
osascript -e 'tell application "System Events" to tell process "lumiere" to perform action "AXPress" of menu item "Show Sidebar" of menu 1 of menu bar item "View" of menu bar 1'
```

`osascript ... click at {x, y}` does not work on egui windows, so interactive
checks of the canvas go through key codes; the menu bar, by contrast, is real
AppKit and fully scriptable.
Full screen uses a native macOS Space, which `screencapture` may not capture.

## License

Lumiere is licensed under the [GNU General Public License, version 3 or later](LICENSE)
(GPL-3.0-or-later). You may use, modify, and redistribute it under those terms;
distributed modified versions must provide their corresponding source code under
the same license.

The project includes third-party material under its own terms. See
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for the applicable notices.

The virtual Cargo workspace has three packages: `lumiere` (egui UI and native
window/menu glue), `lumiere-desktop` (filesystem, workers, cache and native
services), and `lumiere-core` (commands, session codec and portable pixels).
Dependencies flow from app to desktop to core. Root build/run still select
`lumiere`; use `cargo test --locked --workspace -- --test-threads=1` for all packages.
See [the structure and validation record](docs/structure.md).
