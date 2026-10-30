#!/bin/sh
# Build Lumiere.app, install it to a stable per-user Applications folder, and
# optionally launch it through LaunchServices.
#
# LUMIERE_INSTALL_DIR overrides the destination folder (default ~/Applications),
# e.g. LUMIERE_INSTALL_DIR=/Applications ./install.sh
set -eu

# rustup from Homebrew is keg-only and not on PATH by default.
export PATH="$HOME/.cargo/bin:/opt/homebrew/opt/rustup/bin:$PATH"

ROOT="$(cd "$(dirname "$0")" && pwd)"
DEST_ROOT="${LUMIERE_INSTALL_DIR:-${HOME}/Applications}"
APP_NAME="Lumiere.app"
DEST_APP="${DEST_ROOT}/${APP_NAME}"

mkdir -p "$DEST_ROOT"

# Build a fresh signed bundle in dist/, where bundle.sh puts it.
(cd "$ROOT" && ./bundle.sh dist)

# Replacing the bundle under a running copy leaves a stale process behind.
if pgrep -xq Lumiere; then
    osascript -e 'quit app "Lumiere"' >/dev/null 2>&1 || true
    for _ in 1 2 3 4 5 6 7 8 9 10; do
        pgrep -xq Lumiere || break
        sleep 0.5
    done
fi

rm -rf "$DEST_APP"
cp -R "${ROOT}/dist/${APP_NAME}" "$DEST_APP"

# Register the installed location so Finder / LaunchServices see the final app.
LSR="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"
if [ -x "$LSR" ]; then
    "$LSR" -f "$DEST_APP" >/dev/null 2>&1 || true
fi

echo "installed ${DEST_APP}"

if [ "$#" -gt 0 ]; then
    open "$DEST_APP" --args "$@"
else
    open "$DEST_APP"
fi
