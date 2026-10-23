#!/bin/sh
# Build a minimal .app bundle for macOS.
# Usage: ./bundle.sh [output dir]     (default: ./dist)
set -eu

export PATH="$HOME/.cargo/bin:$PATH"
OUT="${1:-dist}"
ARCH="$(uname -m)"
case "$ARCH" in
    arm64|x86_64) ;;
    *) echo "Unsupported macOS architecture: $ARCH" >&2; exit 1 ;;
esac
mkdir -p "$OUT"

cargo build --release -p lumiere --locked

# Cargo.toml is the release-version authority. Keep the bundle metadata in
# lockstep without asking a maintainer to edit a second value at release time.
VERSION=$(cargo metadata --no-deps --format-version 1 | sed -n 's/.*"name":"lumiere","version":"\([^"]*\)".*/\1/p')
[ -n "$VERSION" ]

APP="$OUT/Lumiere.app"

# Bump the build number: each rebuild is unambiguously newer, which is what
# LaunchServices uses to decide which copy wins an overwrite install.
BUILD=1
if OLD_BUILD=$(plutil -extract CFBundleVersion raw "$APP/Contents/Info.plist" 2>/dev/null); then
    case $OLD_BUILD in
        *[!0-9]* | '') BUILD=1 ;;
        *) BUILD=$((OLD_BUILD + 1)) ;;
    esac
fi

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/lumiere "$APP/Contents/MacOS/Lumiere"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>            <string>Lumiere</string>
    <key>CFBundleDisplayName</key>     <string>Lumiere</string>
    <key>CFBundleExecutable</key>      <string>Lumiere</string>
    <key>CFBundleIdentifier</key>      <string>net.fetag.lumiere</string>
    <key>CFBundlePackageType</key>     <string>APPL</string>
    <key>CFBundleShortVersionString</key> <string>$VERSION</string>
    <key>CFBundleVersion</key>         <string>$BUILD</string>
    <key>LSMinimumSystemVersion</key>  <string>11.0</string>
    <key>NSHighResolutionCapable</key> <true/>
    <key>CFBundleIconFile</key>       <string>AppIcon</string>
    <key>NSHumanReadableCopyright</key> <string>Copyright © 2026 fetag</string>
    <key>CFBundleDocumentTypes</key>
    <array>
        <dict>
            <key>CFBundleTypeName</key>     <string>Image</string>
            <key>CFBundleTypeRole</key>     <string>Viewer</string>
            <key>NSDocumentClass</key>      <string>LumiereDocument</string>
            <key>CFBundleTypeExtensions</key>
            <array>
                <string>jpg</string><string>jpeg</string><string>jpe</string><string>jfif</string>
                <string>png</string><string>gif</string><string>bmp</string><string>rle</string>
                <string>tif</string><string>tiff</string><string>webp</string>
                <string>ico</string><string>cur</string><string>tga</string><string>vda</string>
                <string>icb</string><string>vst</string><string>pcx</string>
                <string>pnm</string><string>ppm</string><string>pgm</string>
                <string>pbm</string><string>qoi</string><string>heic</string>
                <string>heif</string><string>hif</string><string>avif</string>
                <string>dng</string><string>cr2</string><string>cr3</string>
                <string>nef</string><string>nrw</string><string>arw</string>
                <string>srf</string><string>sr2</string><string>orf</string>
                <string>rw2</string><string>raf</string><string>pef</string><string>raw</string>
            </array>
            <key>LSItemContentTypes</key>
            <array>
                <string>public.image</string>
                <string>public.jpeg</string>
                <string>public.png</string>
                <string>com.compuserve.gif</string>
                <string>com.microsoft.bmp</string>
                <string>public.tiff</string>
                <string>org.webmproject.webp</string>
                <string>com.microsoft.ico</string>
                <string>public.pnm</string>
            </array>
        </dict>
    </array>
</dict>
</plist>
PLIST

# Install the app icon: the committed .icns (see assets/).
cp assets/Lumiere.icns "$APP/Contents/Resources/AppIcon.icns"

# The same artwork for the fallback (non-mac) about panel.
cp assets/AppIcon.png "$APP/Contents/Resources/AppIcon.png"

# The native macOS About panel uses a close-cropped variant in its fixed icon
# view; this intentionally does not affect the Dock/Finder .icns artwork.
cp assets/AboutIcon.png "$APP/Contents/Resources/AboutIcon.png"

# Ad-hoc signature so Gatekeeper lets a locally built app run.
# For distribution replace "-" with a Developer ID identity and notarize:
#   codesign --force --options runtime --sign "Developer ID Application: ..." "$APP"
#   xcrun notarytool submit --wait "$APP.zip"
codesign --force --deep --sign - "$APP"

# Package the .app into a distributable zip, named like the Linux and
# Windows bundles: lumiere-v<version>-macos-<arch>.zip
NAME="lumiere-v${VERSION}-macos-${ARCH}"
ARCHIVE="$OUT/${NAME}.zip"
rm -f "$ARCHIVE"
ditto -c -k --keepParent "$APP" "$ARCHIVE"

echo "built $APP"
echo "built $ARCHIVE"
