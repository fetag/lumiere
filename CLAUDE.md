# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Lumiere is a cross-platform image viewer (macOS arm64, Windows x86_64, Linux x86_64) built with Rust + eframe/egui. Rust workspace, edition 2021, MSRV 1.91 (CI `msrv` job enforces it).

## Commands

CI gates (run all three before considering work done):

```sh
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace        # full headless UI suite; runs on macOS
```

- Single test: `cargo test --locked -p lumiere <name>` (or `-p lumiere-desktop` / `-p lumiere-core`).
- Release build: `cargo build --locked --release -p lumiere`.
- Bundling is always `--features video,models --locked`:
  - macOS: `./bundle.sh` (mpv dylibs are vendored in `dist/`; `MPV_LIBRARY_DIR` / `LUMIERE_MPV_FRAMEWORKS` override the defaults)
  - Linux: `./bundle-linux.sh` (+ `-deb`, `-appimage`); package smoke test: `./test-linux-packages.sh dist`
  - Windows: `$env:MPV_LIBRARY_DIR=<dir>; .\bundle-windows.ps1`
- The `video` feature links libmpv via `mpv-engine = "=0.2.1"`. `MPV_LIBRARY_DIR` must name a libmpv *without* the version suffix (`libmpv.dylib` / `libmpv.so` / `mpv.lib`); on macOS the real dylib is versioned (`libmpv.2.dylib`), so a symlink dir is needed. Per-machine mpv locations and the source-sync workflow (commit → James pushes to GitLab → pull on the .120/.123 build machines) are documented in `docs/release-env.md` — read it before release work instead of guessing paths.
- Running the `video`-feature tests on macOS additionally needs the mpv dylib resolvable at runtime (`DYLD_FALLBACK_LIBRARY_PATH` pointing at the dir holding `libmpv.2.dylib` and its dependencies).

## Architecture

Three crates, dependencies flow one way only: `apps/lumiere` → `crates/lumiere-desktop` → `crates/lumiere-core`.

- **core** is pure data and pixels: the `Action`/`Mode` command enums, the defensive session codec, and portable decode (the `image` crate, PCX, EXIF orientation, 50 MP dimension limit). No native APIs, no UI, no threads, no FS caches.
- **desktop** owns the filesystem and workers: `loader` (decode worker pool), `imaging` (decode policy, macOS ImageIO fallback, on-disk thumbnail cache), `fsmodel`, `file_ops`, `menu_model`, `state` (atomic-rename persistence), `platform/` (native panels, menus, ImageIO). No egui types, nothing that needs a window.
- **apps/lumiere** is the UI: the `App` state machine, command dispatch, browser grid, viewer, texture caches, and `platform_ui` native glue. It calls into desktop rather than re-deriving decode or filesystem semantics.

`docs/architecture.md` is the authoritative deep-dive — read it before nontrivial work. Load-bearing mechanics not obvious from a single file:

- **Decode pipeline**: directory listing → `Loader` requests (`Thumb`/`Preview`/`Full`) → workers gated by a per-loader pixel-work gate (at most one full-resolution buffer alive per loader; this is why the test suite is safe at default parallelism) → bounded reply channels (capacity 2) drained once per frame → egui texture caches with LRU + byte budgets. Dimension limits are checked from the file header *before* allocating.
- **Menus**: one platform-neutral spec in `menu_model` drives three consumers — the native macOS menu, the in-window fallback menu, and `App::handle_keys` — so labels, shortcuts, and key handling cannot disagree. Everything ends in `App::act`.
- **Headless UI tests**: tests build an `egui::Context::default()`, drive frames with `ctx.run`, and click controls by label through the rect registry in `theme.rs` (`note_rect`/`rect_of`). Fixtures are generated files under a per-test temp dir; tests needing a real photo return early when the sample is absent.
- **`test-support` feature (rule)**: headless tests link desktop with this feature so native panels report `Unavailable` and the built-in egui dialogs take over. If you add a native call a test build would exercise, give it a `test-support` fallback — real AppKit work from parallel test threads is how flaky timeouts are born.

## Adding things (checklists)

- **Image format**: add the extension to `SUPPORTED_EXTS` in `crates/lumiere-desktop/src/imaging.rs`, pick the decode path (image crate feature / ImageIO / portable decoder in core), and keep `bundle.sh`'s `CFBundleDocumentTypes` in sync — `bundle_declares_every_supported_extension` fails otherwise.
- **Menu command**: add the `Action` variant in `crates/lumiere-core/src/command.rs`, handle it in `App::act`, add it to the menu spec in `app/menus.rs`; `menu_shortcuts_are_the_keyboard_map` asserts the bindings.

## Conventions

- Commit messages: English, imperative mood, one logical change per commit. No `Co-Authored-By` attribution lines.
- Bump `version` in the root `Cargo.toml` only for releases (bundle scripts read it from there).
- `Cargo.lock` is committed; build commands use `--locked`. New dependencies must therefore come with a regenerated lockfile.
- Docs mix English (README, CONTRIBUTING, docs/architecture.md) and 繁體中文 (docs/release-env.md); user-facing docs updates follow the language of the file being edited.
