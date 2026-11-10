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
- `[已確認]` 0.1.6 release 產物在 `dist/release-v0.1.6/`（`lumiere-v0.1.6-macos-arm64.zip`）。
- `[已確認]` env 值（用上述 frameworks 目錄）：
  - `LUMIERE_MPV_FRAMEWORKS` = `dist/Lumiere-video-test.app/Contents/Frameworks`
    （bundle 進 app 的來源）。
  - `MPV_LIBRARY_DIR` = 同上（link 用 libmpv 目錄）。
- 建置：`MPV_LIBRARY_DIR=dist/Lumiere-video-test.app/Contents/Frameworks LUMIERE_MPV_FRAMEWORKS=dist/Lumiere-video-test.app/Contents/Frameworks ./bundle.sh`
- `third-party/iina-1.5.0/` 只有授權檔（LICENSE/Credits/versions.env），**不是**
  Frameworks 來源，別搞錯。

### Linux（.123）
- `[已確認]` **build 不直接用 host 系統 libmpv**。host 是 Ubuntu 24.04（noble，
  系統 `libmpv2` 0.37 / `libmpv.so.2`），但 release 用 **Ubuntu 22.04 rootfs**
  的 libmpv（soname `libmpv.so.1`）。
- `[已確認]` 0.1.6 實際環境（實地查證）：
  - **Ubuntu base**：`ubuntu-base-22.04.5-base-amd64.tar.gz`（jammy，29MB）。
  - **libmpv**：`libmpv1` / `libmpv-dev` = **0.34.1-1ubuntu3**（soname
    `libmpv.so.1`，檔 `libmpv.so.1.109.0`）。
  - **Rust**：`1.91.1-x86_64-unknown-linux-gnu`（rustup default）。
  - **已安裝套件**：365 個（清單存 `scripts/linux-release-packages.txt`）。
  - 0.1.6 完整工作目錄：`/home/james/workspace/lumiere-release-0.1.6/`（`rootfs/`
    + `work/`：source、`mpv-spike`、`rustup`/`cargo` CARGO/RUSTUP_HOME、linuxdeploy
    AppImage、validation logs）。0.1.6 是互動式 probe 建置，無單一成文 build 腳本。
- **可重複重建 asset（2026-11-10 新增）**：
  - `scripts/linux-release-rootfs.sh`：從 ubuntu-base tarball +
    `linux-release-packages.txt` 重建 rootfs（apt 裝 libmpv-dev + build 相依 +
    rustup Rust 1.91.1）。需 root。
  - `scripts/linux-release-packages.txt`：0.1.6 rootfs 的 365 套件清單（重建立參考）。
  - 注意：jammy apt 的 libmpv 目前是 0.35.0；0.1.6 rootfs 是 0.34.1。若要精確
    重現 0.34.1，需 pin 該 .deb（mpv-engine 0.2.1 兩版皆相容，release 可用 0.35）。
- 建置：chroot 進 rootfs（內有 cargo）後跑 `./bundle-linux.sh`、
  `./bundle-linux-deb.sh`、`./bundle-linux-appimage.sh`；mpv runtime 由
  `scripts/linux-video-appdir.sh` 經 linuxdeploy 打包。

## 发版前檢查清單（對照本檔）

1. 工作路徑在 `C:\workspace\`（Win）/ `/home/james/workspace/`（Linux）下。
2. 各機 mpv 路徑存在（Windows `mpv.lib`+`libmpv-2.dll`；macOS Frameworks
   有 `libmpv.2.dylib`；Linux rootfs 有 `libmpv.so` dev symlink）。
3. Linux rootfs：重用 `lumiere-release-0.1.6/rootfs`，或用
   `scripts/linux-release-rootfs.sh` 重建（jammy apt libmpv 目前 0.35.0，
   0.1.6 是 0.34.1，pin 該 .deb 可精確重現）。
4. `bundle*.sh`/`.ps1` 都是 `--features video,models`、`--locked`。
