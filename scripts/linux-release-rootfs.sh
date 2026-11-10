#!/bin/sh
# 可重複重建 Lumiere Linux release 建置 rootfs（Ubuntu 22.04 jammy）。
#
# 安裝 libmpv-dev（0.34.1，jammy 官方 apt 版 → libmpv.so.1）+ 全套 build
# 相依 + Rust 1.91.1（rustup）。產出的 rootfs 可 chroot 進去 build Lumiere
# 的 video feature（link jammy libmpv 0.34.1）。
#
# 這是「未來重建」asset：當既有 rootfs 工作目錄被清掉時，用它從
# ubuntu-base tarball + linux-release-packages.txt 重建同一套環境。
#
# 用法：
#   sudo ./scripts/linux-release-rootfs.sh <rootfs-dir> <ubuntu-base-tarball>
#
# 參數：
#   <rootfs-dir>           重建目標目錄（會被 rm -rf 後重建）
#   <ubuntu-base-tarball>  ubuntu-base-22.04.x-base-amd64.tar.gz
#
# 重建後 build：
#   chroot <rootfs-dir> /bin/sh -c \
#     'export CARGO_HOME=/usr/local/cargo RUSTUP_HOME=/usr/local/rustup \
#        PATH=/usr/local/cargo/bin:$PATH; cd <source>; ./bundle-linux.sh'
#
# 事實依據（2026-11-10 自 0.1.6 rootfs 實地查證）：
#   - Ubuntu 22.04.5 base（jammy）
#   - libmpv1 / libmpv-dev = 0.34.1-1ubuntu3（jammy stock，apt 可得）
#   - Rust 1.91.1-x86_64-unknown-linux-gnu（rustup default toolchain）
#   - 365 個已安裝套件（清單見 linux-release-packages.txt）
set -eu

ROOTFS="${1:?usage: linux-release-rootfs.sh <rootfs-dir> <ubuntu-base-tarball>}"
UBUNTU_BASE="${2:?usage: linux-release-rootfs.sh <rootfs-dir> <ubuntu-base-tarball>}"
RUST_VERSION="1.91.1"
HERE="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
PKG_LIST="$HERE/linux-release-packages.txt"

[ "$(id -u)" = 0 ] || { echo "需要 root（chroot）" >&2; exit 1; }
[ -f "$UBUNTU_BASE" ] || { echo "找不到 ubuntu-base tarball: $UBUNTU_BASE" >&2; exit 1; }
[ -f "$PKG_LIST" ] || { echo "找不到套件清單: $PKG_LIST" >&2; exit 1; }

echo "==> 1/3 展開 Ubuntu base 到 $ROOTFS"
rm -rf "$ROOTFS"
mkdir -p "$ROOTFS"
tar -xf "$UBUNTU_BASE" -C "$ROOTFS"

# chroot 內的 apt/rustup 需要網路解析。
cp /etc/resolv.conf "$ROOTFS/etc/resolv.conf"
cp "$PKG_LIST" "$ROOTFS/tmp/packages.txt"

echo "==> 2/3 chroot 內 apt 安裝套件清單（含 libmpv-dev 0.34.1 + build 相依）"
chroot "$ROOTFS" /bin/sh -c "
    set -eu
    export DEBIAN_FRONTEND=noninteractive
    apt-get update
    xargs -a /tmp/packages.txt apt-get install -y
    rm -f /tmp/packages.txt
    apt-get clean
    rm -rf /var/lib/apt/lists/*
"

echo "==> 3/3 chroot 內安裝 Rust $RUST_VERSION（rustup）"
chroot "$ROOTFS" /bin/sh -c "
    set -eu
    export RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --default-toolchain $RUST_VERSION --profile minimal
    ln -sf /usr/local/cargo/bin/cargo /usr/local/bin/cargo
    ln -sf /usr/local/cargo/bin/rustc /usr/local/bin/rustc
"

echo "==> rootfs 就緒: $ROOTFS"
echo "    Ubuntu 22.04 + libmpv 0.34.1 + Rust $RUST_VERSION"
echo "    build: chroot $ROOTFS /bin/sh -c 'export CARGO_HOME=/usr/local/cargo RUSTUP_HOME=/usr/local/rustup PATH=/usr/local/cargo/bin:\$PATH; cd <source>; ./bundle-linux.sh'"
