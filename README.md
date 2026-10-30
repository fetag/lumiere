# Lumiere

[![CI](https://github.com/fetag/lumiere/actions/workflows/ci.yml/badge.svg)](https://github.com/fetag/lumiere/actions/workflows/ci.yml)
[![License: GPL-3.0-or-later](https://img.shields.io/badge/license-GPL--3.0--or--later-blue.svg)](LICENSE)
[![Latest release](https://img.shields.io/github/v/release/fetag/lumiere?label=latest)](https://github.com/fetag/lumiere/releases/latest)

A native-feeling image browser and viewer for macOS, Linux, and Windows, built in Rust with [egui](https://github.com/emilk/egui)/[eframe](https://github.com/emilk/egui/tree/master/crates/eframe). Three-pane browsing, full-screen viewing, slideshow, and platform-adaptive UI.

---

## Why Lumiere?

Lumiere is a cross-platform image browser/viewer for macOS, Linux, and Windows that aims
to feel at home on each supported desktop while staying lightweight and fast. It
uses a three-pane layout for everyday browsing, a dedicated full-screen viewer
for focused viewing, and a slideshow mode for hands-off playback.

The UI follows the system light/dark appearance where available, uses installed
system fonts when available, and keeps the chrome visually restrained with
monochrome line icons. Windows runs on the same in-window menu bar and built-in
dialogs as Linux; the native macOS menu bar and AppKit panels are macOS-only for now.

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

The current headless UI suite contains macOS-native assertions, so CI runs the
full test command (with the default parallelism) on macOS:

```sh
cargo test --locked --workspace
```

Linux and Windows CI validate formatting, Clippy with warnings denied, a release
build and the portable package. Linux CI also builds and checks all three
packages on Ubuntu 22.04. Porting the remaining macOS-specific UI tests is ongoing.

### Package for macOS

Release archives target Apple Silicon (arm64); Intel Macs are not covered yet.
Build a macOS app bundle with icon, `Info.plist`, and ad-hoc signature, plus a
distributable zip named like the Linux and Windows bundles:

```sh
./bundle.sh          # -> dist/Lumiere.app
                     # -> dist/lumiere-v<version>-macos-<arch>.zip
```

### Install the macOS app to `~/Applications`

Install the bundle to a stable user-facing path. This is the recommended path
on macOS because it is more reliable for Dock icon cache behavior:

```sh
./install.sh                 # -> ~/Applications/Lumiere.app, then open it
./install.sh /path/to/folder # same, but launches that folder immediately
```

Set `LUMIERE_INSTALL_DIR` to install somewhere else, for example
`LUMIERE_INSTALL_DIR=/Applications ./install.sh`.

Session state is stored in:

- `~/Library/Application Support/Lumiere/state.ini`

Note: Finder / `NSWorkspace` already see the committed app icon correctly. If
the Dock still shows the generic placeholder after swapping `AppIcon.icns`, it
is usually a Dock / IconServices cache issue; installing to one stable path
(`~/Applications/Lumiere.app`) and launching that copy is more reliable than
repeatedly running bundles from `build/`, `dist/`, or `/tmp`.

### First launch on macOS: Gatekeeper

The release bundle is **ad-hoc signed**: it carries a signature, but no
developer identity, so macOS cannot verify who built it. The first time you
open `Lumiere.app` you will see "cannot be opened because the developer
cannot be verified". One of these unblocks it:

- right-click (or control-click) `Lumiere.app` in Finder → **Open** → confirm
  with **Open** in the dialog; or
- System Settings → Privacy & Security, open the "Lumiere.app was blocked"
  banner → **Open Anyway**.

Only the first launch asks; afterwards the app opens normally. (Advanced:
`xattr -d com.apple.quarantine Lumiere.app` removes the quarantine flag that
triggers the check.) A Developer ID signature plus notarization would remove
the step entirely — see [docs/release.md](docs/release.md) for the status.

### Package and install on Linux

ARM Linux is not covered yet; all three packages target x86_64 and are built
on Ubuntu 22.04, the oldest supported Ubuntu release. They require compatible
system graphics libraries and drivers; other distributions are not tested.
Releases ship three formats — pick whichever fits your distribution:

**`.deb`** (Debian, Ubuntu, Linux Mint, …) installs system-wide and adds a menu
launcher:

```sh
sudo dpkg -i lumiere-v<version>-linux-x86_64.deb
# or let apt resolve dependencies:
sudo apt install ./lumiere-v<version>-linux-x86_64.deb
```

**AppImage** needs no installation — make it executable and run it (it requires
FUSE, present on most desktops; use `--appimage-extract-and-run` if you lack it):

```sh
chmod +x lumiere-v<version>-linux-x86_64.AppImage
./lumiere-v<version>-linux-x86_64.AppImage /path/to/folder
```

**Portable archive** installs for the current user with no `sudo`:

```sh
tar -xzf lumiere-v<version>-linux-x86_64.tar.gz
cd lumiere-v<version>-linux-x86_64
./install.sh
```

The `.deb` and portable installer create a desktop launcher and hicolor icon
entries. The AppImage runs directly and does not install a menu entry; use an
AppImage integration tool or create a launcher yourself if desired.

To build these yourself on a Linux host, use `./bundle-linux-deb.sh`,
`./bundle-linux-appimage.sh`, or `./bundle-linux.sh` (portable).

### Package for Windows

Build a portable Windows archive on a Windows host (PowerShell):

```powershell
.\bundle-windows.ps1        # -> dist\lumiere-v<version>-windows-x86_64.zip
```

Extract the archive and run `lumiere.exe` directly, or pass it a folder to open:

```powershell
.\lumiere.exe C:\path\to\folder
```

No installation is required. The window icon is compiled into the executable; the
`AppIcon.png` kept beside it feeds the in-window About panel.

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

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for build and
test instructions per platform, commit conventions, and the pull request
checklist. Report bugs and request features through
[GitHub issues](https://github.com/fetag/lumiere/issues); security concerns go
through [SECURITY.md](SECURITY.md).

The project follows a [Code of Conduct](CODE_OF_CONDUCT.md), and notable
changes are tracked in the [changelog](CHANGELOG.md).

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
`lumiere`; use `cargo test --locked --workspace` for all packages.
Contributors: [docs/architecture.md](docs/architecture.md) maps the layers and
the decode pipeline; see also [the structure and validation record](docs/structure.md).
