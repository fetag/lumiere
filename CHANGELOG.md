# Changelog

All notable changes to Lumiere are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Portable Windows x86_64 build via `bundle-windows.ps1`: the window icon is
  compiled into the executable, and the `AppIcon.png` beside it feeds the
  in-window About panel.

### Changed

- macOS bundles are packaged as a consistently named distributable zip
  (`lumiere-v<version>-macos-<arch>.zip`), matching the Linux and Windows
  naming scheme.
- Documented macOS and Linux support in the README.

### Fixed

- Atomic rename on Windows and Linux.

## [0.1.3] - 2026-10-17

### Added

- First Linux x86_64 release: portable archive with a no-sudo per-user
  installer, desktop launcher, and hicolor icon-theme entries.
- Dedicated, larger icon in the native macOS About panel; the Dock and Finder
  icons are unchanged.
- Screenshot of the browse view in the README.

### Changed

- Reorganized into a virtual Cargo workspace: `lumiere` (egui UI and native
  window/menu glue), `lumiere-desktop` (filesystem, workers, cache and native
  services), and `lumiere-core` (commands, session codec and portable pixels).
  Dependencies flow app → desktop → core; see [docs/structure.md](docs/structure.md).
- Improved browsing and batch trash behavior.

### Fixed

- Downsampled large images now preserve their original dimensions.
- Rust float literal warnings.

## [0.1.2] - 2026-10-13

### Added

- GPL-3.0-or-later licensing and third-party notices.

### Changed

- Replaced the Open With dialog with a preloaded application submenu.
- Refined macOS UI and viewer behavior.
- Split the application into focused modules (UI, commands, caches, tests).

### Fixed

- Rename, Move and Copy no longer overwrite existing destination files; native
  drag and drop restored.
- Hardened file operations and fixed browser context actions.

## [0.1.1] - 2026-10-11

### Fixed

- Window icon on macOS: eframe no longer overwrites the app icon with its
  default.

## [0.1.0] - 2026-10-11

### Added

- First release: three-pane image browser (folder tree, thumbnail grid,
  preview/information pane), full-screen viewer with zoom/pan/rotate/flip,
  slideshow mode, platform-adaptive UI, wide format support (HEIC/HEIF/AVIF via
  ImageIO on macOS), recoverable delete to the system Trash, and persistent
  session state.

[Unreleased]: https://github.com/fetag/lumiere/compare/v0.1.3...HEAD
[0.1.3]: https://github.com/fetag/lumiere/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/fetag/lumiere/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/fetag/lumiere/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/fetag/lumiere/releases/tag/v0.1.0
