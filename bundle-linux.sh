#!/bin/sh
# Build a portable Linux archive with a per-user desktop installer.
# Usage: ./bundle-linux.sh [output dir]  (default: ./dist)
set -eu

export PATH="$HOME/.cargo/bin:$PATH"
[ "$(uname -s)" = "Linux" ] || {
    echo "bundle-linux.sh must run on a Linux build host" >&2
    exit 1
}
OUT="${1:-dist}"
ARCH="$(uname -m)"
case "$ARCH" in
    x86_64|aarch64) ;;
    *) echo "Unsupported Linux architecture: $ARCH" >&2; exit 1 ;;
esac

cargo build --release -p lumiere --locked
VERSION=$(cargo metadata --no-deps --format-version 1 | sed -n 's/.*"name":"lumiere","version":"\([^"]*\)".*/\1/p')
[ -n "$VERSION" ]

NAME="lumiere-v${VERSION}-linux-${ARCH}"
STAGE="$OUT/$NAME"
ARCHIVE="$OUT/$NAME.tar.gz"
rm -rf "$STAGE" "$ARCHIVE"
mkdir -p "$STAGE/bin" "$STAGE/icons/hicolor"

# Keep the About panel artwork beside the executable. The executable itself
# embeds the same image for the winit window icon fallback.
install -m 755 target/release/lumiere "$STAGE/bin/lumiere"
install -m 644 assets/AppIcon.png "$STAGE/bin/AppIcon.png"
cp -R assets/linux-icons/* "$STAGE/icons/hicolor/"

cat > "$STAGE/install.sh" <<'INSTALL'
#!/bin/sh
# Install Lumiere for the current user. No root privileges are required.
set -eu

HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
PREFIX=${LUMIERE_PREFIX:-"$HOME/.local"}
APPDIR="$PREFIX/lib/lumiere"
BINDIR="$PREFIX/bin"
DATADIR="$PREFIX/share"
ICONDIR="$DATADIR/icons/hicolor"
DESKTOPDIR="$DATADIR/applications"

mkdir -p "$APPDIR" "$BINDIR" "$DESKTOPDIR"
install -m 755 "$HERE/bin/lumiere" "$APPDIR/lumiere"
install -m 644 "$HERE/bin/AppIcon.png" "$APPDIR/AppIcon.png"
ln -sfn "$APPDIR/lumiere" "$BINDIR/lumiere"

# Ship real pixels at every common panel size. Cinnamon associates the
# running X11 window (WM_CLASS=lumiere) with this entry through
# StartupWMClass, and chooses these icon-theme assets instead of scaling the
# window's raw _NET_WM_ICON texture.
for size_dir in "$HERE"/icons/hicolor/*x*; do
    [ -d "$size_dir/apps" ] || continue
    size=$(basename "$size_dir")
    mkdir -p "$ICONDIR/$size/apps"
    install -m 644 "$size_dir/apps/lumiere.png" "$ICONDIR/$size/apps/lumiere.png"
done

cat > "$DESKTOPDIR/lumiere.desktop" <<DESKTOP
[Desktop Entry]
Version=1.0
Type=Application
Name=Lumiere
Comment=Image browser and viewer
Exec=$APPDIR/lumiere %F
Icon=lumiere
Terminal=false
Categories=Graphics;Viewer;
MimeType=image/bmp;image/gif;image/jpeg;image/png;image/tiff;image/webp;
StartupWMClass=lumiere
StartupNotify=true
DESKTOP

command -v update-desktop-database >/dev/null 2>&1 && \
    update-desktop-database "$DESKTOPDIR" 2>/dev/null || true
command -v gtk-update-icon-cache >/dev/null 2>&1 && \
    gtk-update-icon-cache -f "$ICONDIR" 2>/dev/null || true

printf '%s\n' "Installed Lumiere to $APPDIR"
printf '%s\n' "Launch it from your application menu, or run: $BINDIR/lumiere"
INSTALL
chmod 755 "$STAGE/install.sh"

cat > "$STAGE/uninstall.sh" <<'UNINSTALL'
#!/bin/sh
# Remove the files installed by install.sh for the current user.
set -eu

PREFIX=${LUMIERE_PREFIX:-"$HOME/.local"}
rm -f "$PREFIX/bin/lumiere" "$PREFIX/share/applications/lumiere.desktop"
rm -rf "$PREFIX/lib/lumiere"
for size in 16 24 32 48 64 128 256 512; do
    rm -f "$PREFIX/share/icons/hicolor/${size}x${size}/apps/lumiere.png"
done
command -v update-desktop-database >/dev/null 2>&1 && \
    update-desktop-database "$PREFIX/share/applications" 2>/dev/null || true
command -v gtk-update-icon-cache >/dev/null 2>&1 && \
    gtk-update-icon-cache -f "$PREFIX/share/icons/hicolor" 2>/dev/null || true
printf '%s\n' "Uninstalled Lumiere from $PREFIX"
UNINSTALL
chmod 755 "$STAGE/uninstall.sh"

cat > "$STAGE/README.md" <<EOF
# Lumiere for Linux

Install for the current user (no sudo required):

\`\`\`sh
./install.sh
\`\`\`

Launch **Lumiere** from the desktop application menu. This is recommended over
starting the binary directly because the launcher provides the desktop shell
with correctly sized application icons.

The default install prefix is \`~/.local\`. Set \`LUMIERE_PREFIX\` before running
\`install.sh\` or \`uninstall.sh\` to use another prefix.
EOF

# Archive the containing directory, so extraction never scatters files into
# the caller's working directory.
(
    cd "$OUT"
    tar -czf "$(basename "$ARCHIVE")" "$NAME"
)
rm -rf "$STAGE"
printf '%s\n' "built $ARCHIVE"
