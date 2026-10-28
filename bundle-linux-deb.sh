#!/bin/sh
# Build a Linux .deb package. Installs the app under /opt/lumiere with a
# /usr/bin symlink, a desktop launcher and hicolor icon-theme entries.
# Usage: ./bundle-linux-deb.sh [output dir]  (default: ./dist)
set -eu

export PATH="$HOME/.cargo/bin:$PATH"
[ "$(uname -s)" = "Linux" ] || {
    echo "bundle-linux-deb.sh must run on a Linux build host" >&2
    exit 1
}
OUT="${1:-dist}"
ARCH="$(uname -m)"
case "$ARCH" in
    x86_64|aarch64) ;;
    *) echo "Unsupported Linux architecture: $ARCH" >&2; exit 1 ;;
esac

# The package metadata. Change MAINTAINER if the contact address changes.
MAINTAINER="Lumiere Maintainers <james.cl.lin@gmail.com>"
HOME_URL="https://github.com/fetag/lumiere"

cargo build --release -p lumiere --locked
VERSION=$(cargo metadata --no-deps --format-version 1 | sed -n 's/.*"name":"lumiere","version":"\([^"]*\)".*/\1/p')
[ -n "$VERSION" ]
# The control file's Architecture uses dpkg's name (amd64/arm64); the output
# filename follows the release scheme shared with the portable archive, so all
# Linux assets on a release page share one prefix. dpkg reads the metadata
# inside, not the filename, so this is safe for `dpkg -i`.
DPKG_ARCH=$(dpkg --print-architecture)
DEB="$OUT/lumiere-v${VERSION}-linux-${ARCH}.deb"
mkdir -p "$OUT"

# Stage the package root in the layout dpkg-deb expects: a DEBIAN/ metadata
# directory plus the files under their final absolute paths.
PKGROOT="$(pwd)/.pkg.lumiere"
rm -rf "$PKGROOT" "$DEB"
mkdir -p \
    "$PKGROOT/DEBIAN" \
    "$PKGROOT/opt/lumiere" \
    "$PKGROOT/usr/bin" \
    "$PKGROOT/usr/share/applications" \
    "$PKGROOT/usr/share/icons/hicolor"

# The executable and its About-panel artwork live together in /opt, mirroring
# the portable archive's bin/ directory.
install -m 755 target/release/lumiere "$PKGROOT/opt/lumiere/lumiere"
install -m 644 assets/AppIcon.png "$PKGROOT/opt/lumiere/AppIcon.png"

# A /usr/bin symlink puts `lumiere` on PATH so it can be started from a
# terminal with a folder argument, like the portable install does.
ln -sf /opt/lumiere/lumiere "$PKGROOT/usr/bin/lumiere"

# Desktop launcher. Same fields as the portable archive's entry; only Exec is
# an absolute system path here.
cat > "$PKGROOT/usr/share/applications/lumiere.desktop" <<'DESKTOP'
[Desktop Entry]
Version=1.0
Type=Application
Name=Lumiere
Comment=Image browser and viewer
Exec=/opt/lumiere/lumiere %F
Icon=lumiere
Terminal=false
Categories=Graphics;Viewer;
MimeType=image/bmp;image/gif;image/jpeg;image/png;image/tiff;image/webp;
StartupWMClass=lumiere
StartupNotify=true
DESKTOP

# hicolor icon-theme entries at every common panel size.
cp -R assets/linux-icons/* "$PKGROOT/usr/share/icons/hicolor/"

# Refresh the desktop database and icon caches after install and removal. The
# guards keep them optional, so a minimal system without either tool still
# installs cleanly.
cat > "$PKGROOT/DEBIAN/postinst" <<'POSTINST'
#!/bin/sh
set -e
update-desktop-database /usr/share/applications 2>/dev/null || true
gtk-update-icon-cache -f /usr/share/icons/hicolor 2>/dev/null || true
exit 0
POSTINST
cat > "$PKGROOT/DEBIAN/postrm" <<'POSTRM'
#!/bin/sh
set -e
case "$1" in
    remove|deconfigure)
        update-desktop-database /usr/share/applications 2>/dev/null || true
        gtk-update-icon-cache -f /usr/share/icons/hicolor 2>/dev/null || true
        ;;
esac
exit 0
POSTRM
chmod 755 "$PKGROOT/DEBIAN/postinst" "$PKGROOT/DEBIAN/postrm"

# Installed-Size is the payload in KiB (the DEBIAN metadata dir is excluded).
INSTALLED_SIZE=$(du -sk \
    "$PKGROOT/opt" "$PKGROOT/usr" 2>/dev/null | awk '{s+=$1} END {print s}')

# 由 ELF 符號推導實際版本限制；動態載入的圖形函式庫另外列出。
mkdir -p "$PKGROOT/debian"
cat > "$PKGROOT/debian/control" <<'SOURCE_CONTROL'
Source: lumiere
Section: graphics
Priority: optional
Maintainer: Lumiere Maintainers <james.cl.lin@gmail.com>
Standards-Version: 4.6.0

Package: lumiere
Architecture: any
Description: Image browser and viewer
SOURCE_CONTROL
SHLIB_DEPENDS=$(cd "$PKGROOT" && dpkg-shlibdeps -O -eopt/lumiere/lumiere)
SHLIB_DEPENDS=${SHLIB_DEPENDS#shlibs:Depends=}
[ -n "$SHLIB_DEPENDS" ]
rm -rf "$PKGROOT/debian"

cat > "$PKGROOT/DEBIAN/control" <<CONTROL
Package: lumiere
Version: $VERSION
Architecture: $DPKG_ARCH
Maintainer: $MAINTAINER
Installed-Size: $INSTALLED_SIZE
Depends: $SHLIB_DEPENDS, libx11-6, libx11-xcb1, libxcb1, libxcursor1, libxi6, libxrandr2, libxkbcommon0, libxkbcommon-x11-0, libwayland-client0, libegl1, libgl1
Section: graphics
Priority: optional
Homepage: $HOME_URL
Triggers: desktop-file-parse, icon-theme
Description: Native-feeling image browser and viewer
 Lumiere is a cross-platform image browser and viewer for Linux built in
 Rust with egui/eframe. It provides three-pane browsing, a full-screen
 viewer with zoom, pan and rotation, and a slideshow mode, with a UI that
 follows the system light/dark appearance.
CONTROL

# --root-owner-group stamps files as root:root without needing fakeroot, so a
# dpkg-dev 提供 dpkg-deb 與 dpkg-shlibdeps。
dpkg-deb --build --root-owner-group "$PKGROOT" "$DEB" >/dev/null
rm -rf "$PKGROOT"
printf '%s\n' "built $DEB"
