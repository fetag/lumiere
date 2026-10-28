# Architecture

A contributor's map of the codebase: what lives in each layer, how an image
gets from disk to pixels on screen, and where new work belongs. For the
history of the workspace split (what moved where, and why), see
[structure.md](structure.md).

## The three layers

```
apps/lumiere ──> crates/lumiere-desktop ──> crates/lumiere-core
      └───────────────────────────────────────────> core DTOs
```

Dependencies only flow downward. The app may use core directly (it re-exports
`Action`/`Mode`); desktop never knows the UI exists.

| Crate | Owns | Must not contain |
| --- | --- | --- |
| `lumiere-core` | `command` (the `Action`/`Mode` enums), `state` (session fields + parse/serialize with clamping), `imaging` (the raw RGBA `Decoded` DTO, portable decode via the `image` crate, PCX decode, EXIF orientation, the 50 MP dimension limit) | Native APIs, UI, threads, filesystem caches. It is pure pixels and data — keep it that way so a future web build can reuse it. |
| `lumiere-desktop` | `loader` (decode worker pool), `imaging` (decode policy, ImageIO fallback, metadata, on-disk thumbnail cache), `fsmodel` (directory listing, sorting, wildcard filter, lazy folder tree), `file_ops` (pure filesystem operations), `menu_model` (platform-neutral menu/shortcut DTOs), `state` (load/save with atomic rename), `platform` (native panels, alerts, ImageIO, icons, window geometry) | egui types, texture handles, anything that needs a window. |
| `apps/lumiere` | The `App` state machine, frame composition (`app/view.rs`), command dispatch (`app/commands.rs`), the browser grid and context menu (`browser.rs`), viewer gestures (`viewer.rs`), drawing (`theme.rs`), texture caches (`app/cache.rs`), native window/menu glue (`platform_ui`) | Decode policy or filesystem semantics — call into desktop instead of re-deriving them. |

## How an image reaches the screen

1. **Listing.** `fsmodel` reads a directory on a worker thread and replies
   with a generation number, so results from a folder you have already left
   are harmless. `App::pump_listing` drains the reply each frame.
2. **Requesting.** The grid and viewer ask the `Loader` for work
   (`Kind::Thumb` / `Preview` / `Full`) with a target size. Full-size and
   preview requests cut the line; prefetches queue at the back. Bumping the
   generation drops queued work that belongs to an older visible set.
3. **Decoding.** A worker (four per loader) takes the request under the
   loader's pixel-work gate — at most one full-resolution buffer alive per
   loader — and runs `imaging::decode`:
   - dimensions are read from the file header first; anything over
     `MAX_DECODE_PIXELS` (50 MP) is rejected before allocating, except on
     macOS where ImageIO may still produce a reduced representation;
   - PCX decodes through the portable core path; HEIC/HEIF/HIF/AVIF go
     straight to ImageIO on macOS; everything else uses the `image` crate,
     with an ImageIO fallback on macOS when that fails;
   - `apply_orientation` bakes in the EXIF orientation so the pixels are
     upright before scaling or texturing.
   The reply travels over a bounded channel (capacity 2 — backpressure keeps
   decoded buffers from piling up while the UI is busy) and is drained once
   per frame by `App::pump`.
4. **Texturing.** Finished pixels are uploaded to egui textures in
   `app/cache.rs`. Full-size textures are clamped to a 6000 px side; the
   caches carry LRU order plus resident-byte budgets (96 MB thumbnails,
   192 MB full size) and remember failed decodes so one broken file cannot
   put the viewer into a re-request flicker loop.
5. **Thumbnail disk cache.** Rendered thumbnails are also stored on disk
   (`~/.cache/Lumiere/thumbs` or `~/Library/Caches/net.fetag.lumiere/thumbs`),
   keyed by an FNV-1a hash of the path and mtime, so revisiting a folder does
   not re-decode anything.

## Commands and menus

Menu items are described once, in platform-neutral terms: `menu_model`
carries the label, the `Action` and a `Shortcut` (explicit modifier booleans,
no key codes). Three consumers read that single spec, which is why a menu
label and its working shortcut cannot disagree:

- on macOS the native menu bar (`platform_ui/menu/macos.rs`) turns it into
  `NSMenuItem`s; AppKit consumes the `⌘` events before egui sees them;
- the in-window fallback menu (`app/menus.rs`) draws it for other platforms
  and for test builds;
- `App::handle_keys` maps raw key events through the same `Shortcut` values.

Everything ends in `App::act(action, ctx)`, one match arm per command.

## File operations and feedback

`file_ops` is pure filesystem logic (no `App`, no threads), which is what
makes its safety rules — never overwrite an existing destination, report each
failure separately — testable in isolation. Batch runs execute on a worker
thread (`RunningJob`) that reports progress ticks and one final outcome over
a channel; the result becomes a floating `Notice` card, the app's only
standing feedback. A failed run can be retried on just the failed items.

## State persistence

Window size/position, browsing preferences, pinned and recent folders live in
`core::state::State`. The codec is defensive (clamped sizes, escaped values,
unknown keys ignored) because the file is user-editable; `desktop::state`
adds the load/save and atomic rename. The app marks `state_dirty` when a
preference changes and persists on the way out or after idle frames.

## Platform services and test builds

Native work lives in `lumiere-desktop/src/platform/`, where each service keeps
a small local `cfg` branch rather than an empty per-OS file. The app's headless
test suite links desktop with the `test-support` feature, which preserves the
original headless behavior: native panels report `Unavailable` (the built-in
egui dialogs take over), the Open With catalog returns a fixed list, and
native lookups are skipped. **Rule: if you add a native call that a test build
would exercise, give it a `test-support` fallback** — real AppKit work from
many parallel test threads is how flaky timeouts are born.

## Adding an image format

1. Add the extension to `SUPPORTED_EXTS` in `crates/lumiere-desktop/src/imaging.rs`.
2. Pick the decode path:
   - the `image` crate already supports it → enable the feature in
     `crates/lumiere-core/Cargo.toml` and you are done; `decode_with_image`
     picks it up by extension;
   - macOS-only (e.g. camera RAW) → ImageIO handles it through the existing
     fallback; just list the extension (see `dng`, `nef`, …);
   - needs a portable custom decoder → add it to `core::imaging` next to
     `decode_pcx`, no native dependencies allowed.
3. Keep `bundle.sh`'s `CFBundleDocumentTypes` in sync — the test
   `bundle_declares_every_supported_extension` fails if you forget, so Finder
   can open the new type on macOS.
4. EXIF orientation and the 50 MP limit apply automatically; nothing extra to
   do unless the format stores orientation differently.

## Adding a menu command

1. Add the variant to `Action` in `crates/lumiere-core/src/command.rs`.
2. Handle it in `App::act` (`app/commands.rs`).
3. Add the item (label, shortcut) to the menu spec in `app/menus.rs`; on
   macOS the native menu picks it up from there.
4. Cover it: `menu_shortcuts_are_the_keyboard_map` asserts the spec against
   the expected bindings, and the headless suite drives commands through the
   same path a user would.

## Testing

The UI suite is headless: each test builds an `egui::Context::default()` and
drives frames with `ctx.run`, clicking controls by label through the
test-only rect registry in `theme.rs` (`note_rect` / `rect_of`). Fixtures are
small generated files under a per-test temp dir (`fixture_dir`); tests that
need a real photo return early when the local sample is absent, so they pass
on any machine. The suite runs with the default test parallelism — the decode
pipeline's pixel-work gate is per-loader, so parallel tests no longer queue
behind each other's decodes (see [structure.md](structure.md) for the history).
