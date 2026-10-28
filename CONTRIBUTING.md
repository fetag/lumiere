# Contributing to Lumiere

Thanks for your interest in Lumiere! This document explains how to build,
test, and submit changes.

## Prerequisites

- Rust stable toolchain (`rustc`, `cargo`) with the `rustfmt` and `clippy`
  components: `rustup component add rustfmt clippy`
- The platform notes below explain which checks run where.

## Repository layout

The workspace has three packages; dependencies flow app → desktop → core:

| Package | Responsibility |
| --- | --- |
| `apps/lumiere` | egui UI, window and menu glue, headless UI tests |
| `crates/lumiere-desktop` | filesystem operations, worker pool, imaging, platform services |
| `crates/lumiere-core` | commands, session codec, portable pixels |

See [README.md](README.md#project-layout) for the file-level layout and
[docs/structure.md](docs/structure.md) for the rationale behind the split.

## Build and test per platform

### macOS (full suite)

The headless UI suite contains macOS-native assertions (AppKit menus, Preview,
gestures), so the full test command runs on macOS:

```sh
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cargo build --locked --release -p lumiere
./bundle.sh              # optional: dist/Lumiere.app + distributable zip
```

The suite is stable under the default test parallelism: the decode pipeline's
pixel-work gate is per-loader, so parallel tests no longer queue behind each
other's background decodes.

### Linux (x86_64)

Build and validate on a Linux machine:

```sh
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo build --locked --release -p lumiere
./bundle-linux.sh        # optional: portable x86_64 archive + per-user installer
./bundle-linux-deb.sh
./bundle-linux-appimage.sh
./test-linux-packages.sh dist  # requires Xvfb, xauth, x11-utils and graphics libraries
```

Linux CI also builds all three packages on Ubuntu 22.04, installs the `.deb`
dependencies and checks their contents and X11 startup. Release builds must use
Ubuntu 22.04 to preserve the minimum supported glibc baseline. Building the
`.deb` requires `dpkg-dev`; building the AppImage requires `curl`, `file` and
`sha256sum`. The full test suite runs on macOS until the remaining
macOS-specific UI assertions are made portable.

### Windows (x86_64)

Build and validate on a Windows machine (PowerShell):

```powershell
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo build --locked --release -p lumiere
.\bundle-windows.ps1     # optional: portable x86_64 zip
```

## Commit and pull request conventions

- Write commit messages in English, imperative mood ("Fix atomic rename on
  Windows", not "Fixed ...").
- Keep changes focused; one logical change per commit.
- Bump `version` in the root `Cargo.toml` only for releases; the bundle
  scripts read the version from there.

Before opening a PR, make sure:

- [ ] `cargo fmt --all --check` passes
- [ ] `cargo clippy --locked --workspace --all-targets -- -D warnings` passes
- [ ] `cargo test --locked --workspace` passes on macOS
      (or the platform-specific checks above where the full suite cannot run)
- [ ] The affected platform builds its release bundle
- [ ] README / docs are updated for user-visible changes

## Where to start

Read [docs/architecture.md](docs/architecture.md) first — it maps the three
crates, how an image gets from disk to screen, and where new work belongs
(adding a format or a menu command is a short checklist).

Good first areas: `lumiere-core` unit tests, documentation, and making the
macOS-specific UI assertions portable so Linux CI can run the full suite.

## License

By contributing, you agree that your contributions will be licensed under the
project's [GPL-3.0-or-later](LICENSE) license.
