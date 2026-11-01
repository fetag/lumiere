#!/bin/sh
# 建立自帶 libmpv 與相依套件的 Linux AppDir。需先以 video feature 建置。
set -eu

[ "$#" -eq 1 ] || { echo "usage: $0 APPDIR" >&2; exit 1; }
[ "$(uname -s)" = Linux ] || { echo 'Linux is required' >&2; exit 1; }
command -v dpkg-query >/dev/null || { echo 'Debian/Ubuntu build environment is required' >&2; exit 1; }
case "$(uname -m)" in
    x86_64) sha=c20cd71e3a4e3b80c3483cef793cda3f4e990aca14014d23c544ca3ce1270b4d ;;
    aarch64) sha=620095110d693282b8ebeb244a95b5e911cf8f65f76c88b4b47d16ae6346fcff ;;
    *) echo 'unsupported architecture' >&2; exit 1 ;;
esac
[ -x target/release/lumiere ] || { echo 'release binary not found' >&2; exit 1; }
mkdir -p "$1"
appdir=$(CDPATH= cd -- "$1" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT HUP INT TERM
url="https://github.com/linuxdeploy/linuxdeploy/releases/download/1-alpha-20251107-1/linuxdeploy-$(uname -m).AppImage"
curl -fsSL "$url" -o "$work/linuxdeploy.AppImage"
printf '%s  %s\n' "$sha" "$work/linuxdeploy.AppImage" | sha256sum -c -
chmod +x "$work/linuxdeploy.AppImage"
"$work/linuxdeploy.AppImage" --appimage-extract-and-run --appdir "$appdir" --executable target/release/lumiere
[ -f "$appdir/usr/lib/libmpv.so.1" ] || [ -f "$appdir/usr/lib/libmpv.so.2" ] || {
    echo 'linuxdeploy did not bundle libmpv' >&2; exit 1;
}
# linuxdeploy 無法由部分 /lib 合併路徑找到對應版權；包含建置環境的
# Debian copyright 資料與精確套件版本，避免遺漏已打包的相依項目。
for doc in /usr/share/doc/*/copyright; do
    [ -f "$doc" ] || continue
    package=$(basename "$(dirname "$doc")")
    mkdir -p "$appdir/usr/share/doc/$package"
    cp -L "$doc" "$appdir/usr/share/doc/$package/copyright"
done
mkdir -p "$appdir/usr/share/doc/lumiere"
cp LICENSE THIRD_PARTY_NOTICES.md "$appdir/usr/share/doc/lumiere/"
dpkg-query -W -f='${binary:Package}\t${Version}\t${source:Package}\t${source:Version}\n' > "$appdir/usr/share/doc/lumiere/build-packages.tsv"
