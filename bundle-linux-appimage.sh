#!/bin/sh
# Build a Linux AppImage. Downloads appimagetool and the type 2 runtime,
# stages an AppDir from the release binary, and produces a portable .AppImage
# that users can run directly (no install).
# Usage: ./bundle-linux-appimage.sh [output dir]  (default: ./dist)
set -eu

export PATH="$HOME/.cargo/bin:$PATH"
[ "$(uname -s)" = "Linux" ] || {
    echo "bundle-linux-appimage.sh must run on a Linux build host" >&2
    exit 1
}
OUT="${1:-dist}"
ARCH="$(uname -m)"
case "$ARCH" in
    x86_64)
        TOOL_SHA256="ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0"
        RUNTIME_SHA256="2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d"
        ;;
    aarch64)
        TOOL_SHA256="f0837e7448a0c1e4e650a93bb3e85802546e60654ef287576f46c71c126a9158"
        RUNTIME_SHA256="00cbdfcf917cc6c0ff6d3347d59e0ca1f7f45a6df1a428a0d6d8a78664d87444"
        ;;
    *) echo "Unsupported Linux architecture: $ARCH" >&2; exit 1 ;;
esac

# 固定上游版本和 SHA-256，更新版本時須一併核對官方發行資產的雜湊。
TOOL_VERSION="1.9.1"
RUNTIME_TAG="20251108"
APPIMAGE_TOOL_URL="https://github.com/AppImage/appimagetool/releases/download/${TOOL_VERSION}/appimagetool-${ARCH}.AppImage"
RUNTIME_URL="https://github.com/AppImage/type2-runtime/releases/download/${RUNTIME_TAG}/runtime-${ARCH}"

cargo build --release -p lumiere --locked
VERSION=$(cargo metadata --no-deps --format-version 1 | sed -n 's/.*"name":"lumiere","version":"\([^"]*\)".*/\1/p')
[ -n "$VERSION" ]

# The filename follows the release scheme shared with the portable archive and
# the .deb, so all Linux assets on a release page share one prefix. The
# AppImage's internal name (shown by the file manager) still comes from the
# desktop entry's Name= field.
NAME="lumiere-v${VERSION}-linux-${ARCH}"
OUT_IMG="$OUT/${NAME}.AppImage"
mkdir -p "$OUT"

WORK="$(pwd)/.appimage-build"
rm -rf "$WORK" "$OUT_IMG"
mkdir -p "$WORK"

# Fetch the tooling. Both are AppImages, so run appimagetool with
# --appimage-extract-and-run: it works whether or not FUSE is available, which
# keeps the build working on minimal CI runners and headless hosts alike.
echo "downloading appimagetool ${TOOL_VERSION}..."
curl -fsSL "$APPIMAGE_TOOL_URL" -o "$WORK/appimagetool.AppImage"
echo "downloading type 2 runtime (${RUNTIME_TAG})..."
curl -fsSL "$RUNTIME_URL" -o "$WORK/runtime"
printf '%s  %s\n' "$TOOL_SHA256" "$WORK/appimagetool.AppImage" "$RUNTIME_SHA256" "$WORK/runtime" | sha256sum -c -
chmod +x "$WORK/appimagetool.AppImage"

# Stage the AppDir: an AppRun, the desktop entry, the binary under usr/bin and
# hicolor icon entries — the layout appimagetool expects.
APPDIR="$WORK/AppDir"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/icons/hicolor"
install -m 755 target/release/lumiere "$APPDIR/usr/bin/lumiere"
install -m 644 assets/AppIcon.png "$APPDIR/usr/bin/AppIcon.png"
cp -R assets/linux-icons/* "$APPDIR/usr/share/icons/hicolor/"

# appimagetool embeds a top-level icon named after the desktop entry's Icon=
# field; without it the build aborts. Use a large source for crisp scaling.
install -m 644 assets/linux-icons/256x256/apps/lumiere.png "$APPDIR/lumiere.png"

cat > "$APPDIR/AppRun" <<'APPRUN'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
export PATH="$HERE/usr/bin:$PATH"
exec "$HERE/usr/bin/lumiere" "$@"
APPRUN
chmod 755 "$APPDIR/AppRun"

cat > "$APPDIR/lumiere.desktop" <<'DESKTOP'
[Desktop Entry]
Version=1.0
Type=Application
Name=Lumiere
Comment=Image browser and viewer
Exec=lumiere %F
Icon=lumiere
Terminal=false
Categories=Graphics;Viewer;
MimeType=image/bmp;image/gif;image/jpeg;image/png;image/tiff;image/webp;
StartupWMClass=lumiere
DESKTOP

# Assemble the AppImage. The desktop entry (lumiere) is what appimagetool binds
# the launcher to; the output file is named for the release.
"$WORK/appimagetool.AppImage" --appimage-extract-and-run \
    --runtime-file="$WORK/runtime" \
    "$APPDIR" "$OUT_IMG"

rm -rf "$WORK"
chmod +x "$OUT_IMG"
printf '%s\n' "built $OUT_IMG"
