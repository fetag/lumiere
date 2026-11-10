# Release build environment（三台原生機器的 mpv / 建置環境）

`docs/release.md` 是「怎麼發版」的流程；本檔記錄**三台原生建置機的實際環境**
（libmpv 位置、rootfs、工作路徑）。這些是機器本地的操作細節，发版前對照此檔，
不要再憑空猜路徑。

> 更新：2026-11-10（0.1.7）。確認狀態：`[已確認]` = 本次實地查過；`[待確認]` =
> 需再核對才能直接用。

## 工作路徑慣例（重要）

- **Windows .120**：所有建置/工作目錄一律放 `C:\workspace\`（**不是**
  `C:\Users\james\`）。
- **Linux .123**：一律放 `/home/james/workspace/`（**不是** `~/Downloads`、
  `~` 根目錄）。
- **macOS**：repo 在 `~/workspace/lumiere`，bundle 從 repo 根跑。

## 各機器 libmpv（video feature 的硬依賴）

video feature 經 `mpv-engine` crate link libmpv；三台各有一份本地 libmpv，
**不是**靠系統 apt/安裝，而是 workspace 下的既定副本。

### Windows（.120）
- `[已確認]` `MPV_LIBRARY_DIR = C:\workspace\lumiere-official-libmpv-20261101\lib`
  （含 `mpv.lib` + `libmpv-2.dll`；2026-11-01 官方 libmpv，0.1.6 那批）。
- 建置：`$env:MPV_LIBRARY_DIR="C:\workspace\lumiere-official-libmpv-20261101\lib"; .\bundle-windows.ps1`
- 舊副本（備援）：`C:\workspace\lumiere-mpv-spike-20261030\lib`。

### macOS（本機）
- `[已確認]` mpv dylibs（含 `libmpv.2.dylib` + 全套 mpv 相依 dylib）在
  `dist/Lumiere-video-test.app/Contents/Frameworks/`（舊 build 產物，repo 內 `dist/`）。
- `[待確認]` 兩個 env 的確切值：
  - `LUMIERE_MPV_FRAMEWORKS` = 上述 `Contents/Frameworks` 目錄（bundle 進 app 的來源）。
  - `MPV_LIBRARY_DIR` = link 用 libmpv 目錄（應同上 Frameworks 目錄；mpv-engine
    在 macOS 如何用此值需核對）。
- 建置：`MPV_LIBRARY_DIR=... LUMIERE_MPV_FRAMEWORKS=... ./bundle.sh`
- `third-party/iina-1.5.0/` 只有授權檔（LICENSE/Credits/versions.env），**不是**
  Frameworks 來源，別搞錯。

### Linux（.123）
- `[已確認]` **build 不直接用 host 系統 libmpv**。host 是 Ubuntu 24.04（系統
  `libmpv2` = 0.37 / `libmpv.so.2`），但 release 要用 **mpv 1.x**（bundle 出的
  runtime 是 `libmpv.so.1.109.0`）。
- `[已確認]` 用 **Ubuntu 22.04 rootfs** 建置：
  `lumiere-release-0.1.6/rootfs/`（含 `usr/lib/x86_64-linux-gnu/libmpv.so`
  dev symlink + `libmpv.so.1.109.0`、`cargo-bin/`、`usr/include/libmpv/`）。
- `[已確認]` 0.1.6 完整建置工作目錄：`/home/james/workspace/lumiere-release-0.1.6/`
  （`rootfs/` + `work/`：source checkout、`mpv-spike`、`rustup`、linuxdeploy
  AppImage、build logs/screenshots）。
- `[待確認]` **chroot 進 rootfs 的確切指令**未找到成文腳本（0.1.6 似為互動式
  probe 建置）。0.1.7 前需先取得/重建這條 chroot + build 指令。
- 建置（在 rootfs 環境內）：`./bundle-linux.sh`、`./bundle-linux-deb.sh`、
  `./bundle-linux-appimage.sh`；mpv runtime 由 `scripts/linux-video-appdir.sh`
  經 linuxdeploy 打包。

## 发版前檢查清單（對照本檔）

1. 工作路徑在 `C:\workspace\`（Win）/ `/home/james/workspace/`（Linux）下。
2. 各機 mpv 路徑存在（Windows `mpv.lib`+`libmpv-2.dll`；macOS Frameworks
   有 `libmpv.2.dylib`；Linux rootfs 有 `libmpv.so` dev symlink）。
3. Linux 的 chroot/build 指令已確認（0.1.7 目前 `[待確認]`）。
4. `bundle*.sh`/`.ps1` 都是 `--features video,models`、`--locked`。
