#!/bin/sh
# 驗證 Linux 發行套件的內容及 X11 視窗啟動；需要 xvfb、xauth 和 x11-utils。
set -eu

DIST=$(CDPATH= cd -- "${1:-dist}" && pwd)
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT HUP INT TERM

set -- "$DIST"/*.tar.gz
[ "$#" -eq 1 ] && [ -f "$1" ]
tar -xzf "$1" -C "$WORK"
set -- "$WORK"/lumiere-v*-linux-*
[ "$#" -eq 1 ] && [ -x "$1/bin/lumiere" ] && [ -f "$1/bin/AppIcon.png" ]
PORTABLE="$1/bin/lumiere"

set -- "$DIST"/*.deb
[ "$#" -eq 1 ] && [ -f "$1" ]
dpkg-deb -f "$1" Depends | grep -q 'libc6 (>= '
dpkg-deb -x "$1" "$WORK/deb"
[ -x "$WORK/deb/opt/lumiere/lumiere" ]
[ -f "$WORK/deb/opt/lumiere/AppIcon.png" ]
[ -f "$WORK/deb/usr/share/applications/lumiere.desktop" ]

set -- "$DIST"/*.AppImage
[ "$#" -eq 1 ] && [ -x "$1" ]
APPIMAGE="$1"
(cd "$WORK" && "$APPIMAGE" --appimage-extract >/dev/null)
[ -x "$WORK/squashfs-root/usr/bin/lumiere" ]
[ -f "$WORK/squashfs-root/usr/bin/AppIcon.png" ]
[ -f "$WORK/squashfs-root/lumiere.desktop" ]

# 檢查視窗真的出現，避免只以程序仍在執行判定啟動成功。
for executable in "$PORTABLE" "$WORK/deb/opt/lumiere/lumiere" "$WORK/squashfs-root/AppRun"; do
    mkdir -p "$WORK/config" "$WORK/cache" "$WORK/home"
    XDG_CONFIG_HOME="$WORK/config" XDG_CACHE_HOME="$WORK/cache" HOME="$WORK/home" \
        LIBGL_ALWAYS_SOFTWARE=1 WINIT_UNIX_BACKEND=x11 \
        xvfb-run -a -s '-screen 0 1280x800x24' sh -c '
            "$1" "$2" >"$3" 2>&1 &
            pid=$!
            trap '\''kill "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true'\'' EXIT
            for attempt in $(seq 1 30); do
                if xwininfo -root -tree | grep -q "Lumiere"; then
                    exit 0
                fi
                if ! kill -0 "$pid" 2>/dev/null; then
                    break
                fi
                sleep 1
            done
            cat "$3" >&2
            echo "Lumiere 視窗未啟動：$1" >&2
            exit 1
        ' sh "$executable" "$WORK/home" "$WORK/launch.log"
done

printf '%s\n' 'Linux 套件內容與視窗啟動檢查通過'
