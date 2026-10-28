# Changelog

All notable changes to Lumiere are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.4] - 2026-10-28

### Added

- Portable Windows x86_64 build via `bundle-windows.ps1`: the window icon is
  compiled into the executable, and the `AppIcon.png` beside it feeds the
  in-window About panel.
- Automated GitHub Releases: pushing a version tag now publishes macOS (arm64),
  Linux (x86_64) and Windows (x86_64) archives with SHA-256 checksums, built on
  native runners without cross-compilation.
- Declared the minimum supported Rust version (1.91) in the workspace manifest
  and added a CI job that checks the whole workspace against it on every push.
- A contributor architecture guide (`docs/architecture.md`): what each crate
  owns, how an image flows from disk to screen, and step-by-step checklists for
  adding an image format or a menu command.
- A release maintainer's guide (`docs/release.md`): the tag-driven pipeline,
  the asset naming scheme, post-release verification, and the current signing
  status.
- A Traditional Chinese README (`README.zh-TW.md`).
- Linux `.deb` and AppImage packages: `bundle-linux-deb.sh` builds a system
  package (installs to `/opt/lumiere` with a `/usr/bin` symlink, a desktop
  launcher and hicolor icons) and `bundle-linux-appimage.sh` builds a portable
  AppImage. The release workflow now publishes both alongside the portable
  archive, all sharing the `lumiere-v<version>-linux-x86_64` filename prefix.
- First-launch Gatekeeper instructions for macOS in the README (the ad-hoc-signed
  bundle needs a one-time exception), with a Developer ID / notarization
  assessment in the release guide.

### Changed

- Windows executables include a multi-size application icon for File Explorer.
- Linux release packages are built and smoke-tested on Ubuntu 22.04, the
  minimum supported Ubuntu version; the `.deb` declares runtime dependencies.
- AppImage tooling and runtime downloads use pinned versions and verified
  SHA-256 hashes. The AppImage includes the About panel artwork.
- All workspace packages inherit the release version; release checks verify
  the app version and lockfile before building.
- The README's Linux section now documents all three distribution formats
  (`.deb`, AppImage, and the portable archive).
- macOS bundles are packaged as a consistently named distributable zip
  (`lumiere-v<version>-macos-<arch>.zip`), matching the Linux and Windows
  naming scheme.
- Documented macOS and Linux support in the README, including that macOS
  releases target Apple Silicon only and Linux archives are x86_64 only for
  now.
- CI now validates Windows builds and packaging alongside macOS and Linux.

### Fixed

- Atomic rename on Windows and Linux.
- EXIF orientation corrections 2, 4, 5 and 7 were applied incorrectly: the
  horizontal and vertical flips returned the untransformed image, and the two
  transpose orientations used the wrong flip axis. Photos stored with those
  orientations now display upright.
- The test suite no longer times out under the default parallel execution: the
  decode pipeline's pixel-work gate is now per-loader instead of process-wide,
  and test builds skip native Open With lookups. CI runs the macOS suite with
  the default parallelism again.

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

[Unreleased]: https://github.com/fetag/lumiere/compare/v0.1.4...HEAD
[0.1.4]: https://github.com/fetag/lumiere/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/fetag/lumiere/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/fetag/lumiere/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/fetag/lumiere/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/fetag/lumiere/releases/tag/v0.1.0
