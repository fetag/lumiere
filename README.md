# Lumiere

Classic late-90s style image browser/viewer for macOS, written in Rust with
[egui](https://github.com/emilk/egui)/eframe. Three-pane browser (folder tree,
thumbnail grid, preview/information), full-screen viewer, slideshow, and a
Windows-95-ish bevel theme drawn by hand.

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

Session state (last folder, window size and position) is stored in
`~/Library/Application Support/Lumiere/state.ini`.

## Decoding

- Built-in via the `image` crate: JPEG, PNG, GIF, BMP, TIFF, WebP, TGA, PCX, ICO, …
- HEIC / HEIF / AVIF via macOS ImageIO (`src/imaging/macos.rs`, `objc2-image-io`)
- EXIF via `kamadak-exif`
- Delete moves to the macOS Trash (`trash` crate)
- Thumbnail cache: `~/Library/Caches/Lumiere/thumbs`

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
| `src/classic.rs` | Win95 era theme: bevels, hand-drawn icons |

## Keyboard shortcuts

| Key | Action |
| --- | --- |
| Arrows | Move selection in the grid |
| Home / End | First / last image |
| PageUp / PageDown | Previous / next image |
| Space, Enter | Open viewer |
| Tab / Shift+Tab | Next / previous folder |
| Esc | Close dialog → stop slideshow → leave full screen → close viewer |
| F11 or Cmd+F | Toggle full screen |
| Cmd+Shift+Y | Start / stop slideshow |
| Del | Move selected item to the Trash (asks first) |
| F2 | Rename |
| Cmd+I | Properties (information tab) |
| Cmd+, | Options |
| Cmd+A / Cmd+Shift+A | Select all / select none |
| Cmd+R / Cmd+Shift+R | Rotate right / left |
| `+` / `-` / `1` / `F` | Zoom in / out / actual size / fit to window |

Note: shortcuts match on the key event's own modifiers, not on
`InputState::modifiers` — winit/eframe does not always update the latter in the
same frame as the key event.

## Testing

Unit and UI tests live next to the code (`#[cfg(test)] mod tests`). The UI tests
drive `egui::Context::run` headlessly against a temp folder fixture
(`fixture_dir`), covering browser → viewer → slideshow, dialogs, keyboard
navigation, the information tab, and command-modifier shortcuts.

Visual verification is done by launching the release build and taking region
screenshots:

```sh
./target/release/lumiere /tmp/lumiere_demo &
osascript -e 'tell application "System Events" to tell process "lumiere" to set position of window 1 to {60, 60}'
screencapture -x -R 60,60,1120,772 /tmp/shot.png
```

`osascript ... click at {x, y}` does not work on egui windows (no native
accessibility elements), so interactive checks are driven by key codes.
Full screen uses a native macOS Space, which `screencapture` may not capture.

## Status

Work in progress. See [TODO.md](TODO.md) for the current task list and what has
already been verified.
