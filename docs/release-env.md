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

## 來源同步（build 前）

正常流程（2026-11-10 確認，James 手動 push）：
1. 本地 commit 完 → **push 到 GitLab（origin）**（AI 不 push，James 做）。
2. **Windows .120**：`C:\workspace\lumiere` → `git pull origin main`。
3. **Linux .123**：`~/workspace/lumiere` → `git pull origin main`。
4. **macOS**：本地，已在最新（不需 pull）。
5. 三台 pull 到最新後各自 build（對照下面「各機器 libmpv」）。

## 各機器 libmpv（video feature 的硬依賴）

video feature 經 `mpv-engine` crate link libmpv；Windows、macOS 使用本地副本，Linux 0.1.7 使用系統套件。

### Windows（.120）
- `[已確認]` `MPV_LIBRARY_DIR = C:\workspace\lumiere-official-libmpv-20261101\lib`
  （含 `mpv.lib` + `libmpv-2.dll`；2026-11-01 官方 libmpv，0.1.6 那批）。
- 建置：`$env:MPV_LIBRARY_DIR="C:\workspace\lumiere-official-libmpv-20261101\lib"; .\bundle-windows.ps1`
- 舊副本（備援）：`C:\workspace\lumiere-mpv-spike-20261030\lib`。

### macOS（本機）
- `[已確認]` mpv dylibs（含 `libmpv.2.dylib` + 全套 mpv 相依 dylib）在
  `dist/Lumiere-video-test.app/Contents/Frameworks/`（舊 build 產物，repo 內 `dist/`）。
- `[已確認]` 0.1.6 release 產物在 `dist/release-v0.1.6/`（`lumiere-v0.1.6-macos-arm64.zip`）。
- `[已實測]` 該目錄只有 `libmpv.2.dylib`，linker 需要無版本名稱：
  ```sh
  mkdir -p dist/mpv-link
  ln -sf ../Lumiere-video-test.app/Contents/Frameworks/libmpv.2.dylib dist/mpv-link/libmpv.dylib
  MPV_LIBRARY_DIR=dist/mpv-link LUMIERE_MPV_FRAMEWORKS=dist/Lumiere-video-test.app/Contents/Frameworks ./bundle.sh
  ```
- `third-party/iina-1.5.0/` 只有授權檔（LICENSE/Credits/versions.env），**不是**
  Frameworks 來源，別搞錯。

### Linux（.123）
- `[0.1.7 已實測]` 直接在 Linux Mint 22.3 host 建置，使用系統 `libmpv.so.2`；
  binary 需要 GLIBC_2.39（`objdump -T`），不再宣稱支援 Ubuntu 22.04。
  ```sh
  mkdir -p ~/workspace/lumiere-mpv-link
  ln -sfn /usr/lib/x86_64-linux-gnu/libmpv.so.2 ~/workspace/lumiere-mpv-link/libmpv.so
  cd ~/workspace/lumiere
  . "$HOME/.cargo/env"
  export RUSTFLAGS="-L native=$HOME/workspace/lumiere-mpv-link"
  ./bundle-linux.sh
  ./bundle-linux-deb.sh
  ./bundle-linux-appimage.sh
  ```
  三種包裝皆已產出；GUI、影片與模型仍需使用者驗收。
- `[已確認]` 0.1.6 實際環境（實地查證）：
  - **Ubuntu base**：`ubuntu-base-22.04.5-base-amd64.tar.gz`（jammy，29MB）。
  - **libmpv**：`libmpv1` / `libmpv-dev` = **0.34.1-1ubuntu3**（soname
    `libmpv.so.1`，檔 `libmpv.so.1.109.0`）。
  - **Rust**：`1.91.1-x86_64-unknown-linux-gnu`（rustup default）。
  - **已安裝套件**：365 個（清單存 `scripts/linux-release-packages.txt`）。
  - 0.1.6 完整工作目錄：`/home/james/workspace/lumiere-release-0.1.6/`（`rootfs/`
    + `work/`：source、`mpv-spike`、`rustup`/`cargo` CARGO/RUSTUP_HOME、linuxdeploy
    AppImage、validation logs）。0.1.6 是互動式 probe 建置，無單一成文 build 腳本。
- **舊 rootfs 重建嘗試（未驗證，不可當作可重現的 release 流程）**：
  - `scripts/linux-release-rootfs.sh`：從 ubuntu-base tarball +
    `linux-release-packages.txt` 重建 rootfs（apt 裝 libmpv-dev + build 相依 +
    rustup Rust 1.91.1）。需 root。
  - `scripts/linux-release-packages.txt`：0.1.6 rootfs 的 365 套件清單（重建立參考）。
  - 套件版本未固定；腳本會刪除目標目錄，執行前須先審查，勿指向既有 rootfs。
  - rootless chroot 嘗試未成功；不能據此聲稱已驗證 0.1.6 的建置流程。

## 发版前檢查清單（對照本檔）

1. 工作路徑在 `C:\workspace\`（Win）/ `/home/james/workspace/`（Linux）下。
2. 各機 mpv 路徑存在（Windows `mpv.lib`+`libmpv-2.dll`；macOS Frameworks
   有 `libmpv.2.dylib`；Linux host 有 `libmpv.so.2` 與 link symlink）。
3. 確認 Linux 產物實際 glibc 需求及 README 說明一致。
4. `bundle*.sh`/`.ps1` 都是 `--features video,models`、`--locked`。
