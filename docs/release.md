# Releasing

Maintainer's guide to cutting a release. The pipeline is tag-driven: pushing
a version tag builds all three platform bundles on native runners (no
cross-compilation) and publishes them as a GitHub release with SHA-256
checksums. Nothing here runs automatically until the tag is pushed.

## Before you tag

1. **The tree is ready.** The release contains exactly what is on `main`;
   merge anything else first.
2. **Bump the version** in the root `Cargo.toml` (`[workspace.package]
   version`). The bundle scripts read it via `cargo metadata`, so this one
   field drives every artifact name and the Info.plist version. Do not bump
   it for anything but a release (see CONTRIBUTING.md). Regenerate and commit
   `Cargo.lock` after the bump; all bundle builds use `--locked`.
3. **Finalize the CHANGELOG.** Move the `[Unreleased]` entries into a dated
   `[X.Y.Z]` section, add a fresh empty `[Unreleased]`, and update the
   compare links at the bottom of the file. The workflow's auto-generated
   release notes list commits; the CHANGELOG is the curated story.
4. **Commit** the version bump and changelog (e.g. `Bump version to 0.1.4`).

## Tag and push

```sh
git tag v0.1.4
git push origin main --tags   # push to the remote that runs GitHub Actions
```

The `check-version` job fails fast if the tag does not equal both the workspace
and app versions, or if the lockfile is stale. If a tag
must be redone: fix the tree, push, then delete and recreate the tag locally
and on the remote — the previous release's assets are replaced by the new
run.

## What the pipeline does

`release.yml`, in order:

1. `check-version`: tag == workspace and app versions, with a current lockfile.
2. `build-macos` (macos-latest, arm64), `build-linux` (Ubuntu 22.04 container,
   x86_64), `build-windows` (windows-latest, x86_64): each runs its bundle
   script(s) on a native runner and uploads the result. `build-linux` runs
   three — `bundle-linux.sh` (portable archive), `bundle-linux-deb.sh`
   (`.deb`) and `bundle-linux-appimage.sh` (AppImage). The AppImage script
   downloads pinned appimagetool and type 2 runtime releases, verifies their
   SHA-256 hashes, and assembles via `--appimage-extract-and-run`. CI installs
   the `.deb` dependencies and checks all three packages under Xvfb.
3. `publish`: writes `SHA256SUMS.txt` (bare filenames) and uploads everything
   via `softprops/action-gh-release`.

## Assets and platform coverage

| Asset | Contents | Coverage |
| --- | --- | --- |
| `lumiere-v<version>-macos-arm64.zip` | `Lumiere.app`, ad-hoc signed; first launch needs the Gatekeeper exception (see README) | Apple Silicon only — Intel Macs are not targeted |
| `lumiere-v<version>-linux-x86_64.tar.gz` | portable directory: binary + `AppIcon.png`, per-user installer, desktop launcher, hicolor icon-theme entries | x86_64 Linux only |
| `lumiere-v<version>-linux-x86_64.deb` | system package: `/opt/lumiere` + a `/usr/bin` symlink, desktop launcher and hicolor icons; install with `dpkg -i` or `apt install` | x86_64 Linux (Debian family) |
| `lumiere-v<version>-linux-x86_64.AppImage` | portable executable: run directly, no install; requires system graphics libraries and FUSE (or extract-and-run) | x86_64 Linux only |
| `lumiere-v<version>-windows-x86_64.zip` | portable directory: executable (window icon compiled in) + `AppIcon.png` for the in-window About panel | x86_64 Windows only |

Every asset shares the prefix `lumiere-v<version>-<os>-<arch>`; only the
suffix is platform-specific (`.zip`, `.tar.gz`, `.deb`, `.AppImage`). The
`.deb` and AppImage keep that shared filename even though a native apt
repository would name the package `lumiere_<version>_<dpkg-arch>.deb` — dpkg
reads the metadata inside, not the filename. Keep the shared prefix if a new
platform or architecture is added.

Linux assets are built on Ubuntu 22.04, the oldest supported Ubuntu release;
other distributions are not tested. The `.deb` declares symbol-versioned ELF
dependencies and dynamically loaded graphics libraries. The AppImage and
portable archive still depend on the system's graphics libraries and drivers.

## After the release

- **Verify on at least one real machine** per platform you can reach: unzip,
  run, open a folder. The CI builds prove packaging, not pixels.
- Checksums: users verify with `sha256sum -c SHA256SUMS.txt` (or the
  equivalent on their platform) against the file from the release page.
- **Start the next cycle** by adding entries to `[Unreleased]` as work lands;
  do not let it sit empty for long — that is the release buffer.

## Signing status

- **macOS**: ad-hoc signed today, so Gatekeeper asks for an exception on
  first launch (documented in the README). `bundle.sh` notes where a
  Developer ID identity and notarization slot in when those become available.
- **Linux**: the `.deb`, AppImage and portable archive are all unsigned (Linux
  has no widely used equivalent of code-signing notarization); users install or
  run them directly. **Windows**: unsigned portable archive; users run it
  directly.

### Developer ID + notarization: current assessment

Not done, deliberately. What it would take:

1. A paid Apple Developer account and a "Developer ID Application"
   certificate installed where the bundle is built (or exported into CI
   secrets).
2. `bundle.sh` signing with that identity instead of `-` (ad-hoc), plus
   stapling or on-demand notarization via `xcrun notarytool`.
3. A CI secret for `notarytool` (Apple ID + app-specific password or keychain
   profile) and a wait-for-finish step in the release workflow.

What it would buy: first launch without any Gatekeeper exception, and no
"app is from an unidentified developer" friction for download users.

Revisit when either happens: distribution reaches people who are not you
(Gatekeeper exceptions become support traffic), or a release needs to be
installed by machines that cannot right-click (MDM-managed fleets). Until
then the ad-hoc bundle plus the documented first-launch step is the
cheapest correct behavior.

### Windows code signing: current assessment

Deferred. Today the Windows archive is an unsigned portable zip, so
SmartScreen shows an "Unknown publisher" prompt on first run until enough
users have opened it (reputation) or it is signed. To remove that we would
need:

1. A code-signing certificate (OV or EV) installed where the bundle is built,
   or exported into CI secrets.
2. `bundle-windows.ps1` running `signtool sign` on the executable, plus a CI
   secret for the certificate and its password/thumbprint.

Revisit when distribution reaches people who are not you (SmartScreen prompts
become support traffic) — the same trigger as the macOS notarization above.
Until then the unsigned portable archive is the cheapest correct behavior.
