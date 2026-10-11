#!/bin/sh
# Build Lumiere.app, install it to a stable per-user Applications folder, and
# optionally launch it through LaunchServices.
set -eu

export PATH="$HOME/.cargo/bin:$PATH"

DEST_ROOT="${HOME}/Applications"
APP_NAME="Lumiere.app"
DEST_APP="${DEST_ROOT}/${APP_NAME}"

mkdir -p "$DEST_ROOT"

# Build a fresh signed bundle in the default build/ location.
"$(dirname "$0")/bundle.sh"

rm -rf "$DEST_APP"
cp -R "$(dirname "$0")/build/${APP_NAME}" "$DEST_APP"

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
