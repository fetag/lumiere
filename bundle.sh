#!/bin/sh
# Build a minimal .app bundle for macOS.
# Usage: ./bundle.sh [output dir]     (default: ./build)
set -eu

export PATH="$HOME/.cargo/bin:$PATH"
OUT="${1:-build}"
mkdir -p "$OUT"

cargo build --release

APP="$OUT/Lumiere.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/lumiere "$APP/Contents/MacOS/Lumiere"

cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>            <string>Lumiere</string>
    <key>CFBundleDisplayName</key>     <string>Lumiere</string>
    <key>CFBundleExecutable</key>      <string>Lumiere</string>
    <key>CFBundleIdentifier</key>      <string>com.lumiere.Lumiere</string>
    <key>CFBundlePackageType</key>     <string>APPL</string>
    <key>CFBundleShortVersionString</key> <string>0.1.0</string>
    <key>CFBundleVersion</key>         <string>1</string>
    <key>LSMinimumSystemVersion</key>  <string>11.0</string>
    <key>NSHighResolutionCapable</key> <true/>
    <key>CFBundleIconFile</key>       <string>AppIcon</string>
    <key>CFBundleDocumentTypes</key>
    <array>
        <dict>
            <key>CFBundleTypeName</key>     <string>Image</string>
            <key>CFBundleTypeRole</key>     <string>Viewer</string>
            <key>LSItemContentTypes</key>
            <array>
                <string>public.image</string>
                <string>public.folder</string>
            </array>
        </dict>
    </array>
</dict>
</plist>
PLIST

# Generate the app icon (drawn by examples/make_icon.rs).
cargo run --release --example make_icon -- "$OUT/icon-1024.png"
ICONSET="$OUT/AppIcon.iconset"
rm -rf "$ICONSET"
mkdir -p "$ICONSET"
for spec in "16 icon_16x16" "32 icon_16x16@2x" "32 icon_32x32" "64 icon_32x32@2x" \
            "128 icon_128x128" "256 icon_128x128@2x" "256 icon_256x256" \
            "512 icon_256x256@2x" "512 icon_512x512" "1024 icon_512x512@2x"; do
    set -- $spec
    sips -z "$1" "$1" "$OUT/icon-1024.png" --out "$ICONSET/$2.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/AppIcon.icns"

# Ad-hoc signature so Gatekeeper lets a locally built app run.
# For distribution replace "-" with a Developer ID identity and notarize:
#   codesign --force --options runtime --sign "Developer ID Application: ..." "$APP"
#   xcrun notarytool submit --wait "$APP.zip"
codesign --force --deep --sign - "$APP"

echo "built $APP"
