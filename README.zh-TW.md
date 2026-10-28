# Lumiere

[English](README.md) | 繁體中文

[![CI](https://github.com/fetag/lumiere/actions/workflows/ci.yml/badge.svg)](https://github.com/fetag/lumiere/actions/workflows/ci.yml)
[![License: GPL-3.0-or-later](https://img.shields.io/badge/license-GPL--3.0--or--later-blue.svg)](LICENSE)
[![Latest release](https://img.shields.io/github/v/release/fetag/lumiere?label=latest)](https://github.com/fetag/lumiere/releases/latest)

為 macOS、Linux 與 Windows 打造的原生感圖片瀏覽器與檢視器，以 Rust 搭配 [egui](https://github.com/emilk/egui)/[eframe](https://github.com/emilk/egui/tree/master/crates/eframe) 建構。三欄式瀏覽、全螢幕檢視、投影片模式，以及隨平台適配的介面。

---

## 為什麼是 Lumiere？

Lumiere 是一款跨平台的圖片瀏覽器／檢視器，支援 macOS、Linux 與 Windows，目標是在每個桌面環境上都有「原生」的親切感，同時保持輕量與快速。日常瀏覽使用三欄式佈局，專注檢視有獨立的全螢幕檢視器，還提供免手動的投影片模式。

介面會隨系統的外觀（淺色／深色）切換、在可用時使用已安裝的系統字型，並以單色線稿圖示保持視覺上的節制。Windows 與 Linux 一樣使用視窗內選單列與內建對話框；原生的 macOS 選單列與 AppKit 面板目前僅限 macOS。

![Lumiere 瀏覽畫面](assets/lumiere-browse.webp)

## 特色

- **三欄式瀏覽器**：資料夾樹、縮圖格線、預覽／資訊欄
- **獨立檢視器**：縮放、平移、旋轉、翻轉、實際大小、配合視窗
- **投影片模式**：鍵盤或選單列快速開始／停止
- **隨平台適配的介面**：淺色／深色自適應樣式、可用的原生整合、系統字型
- **廣泛的格式支援**：常見格式透過 `image` crate；macOS 另以 ImageIO 支援 HEIC/HEIF/AVIF
- **可復原的刪除**：確認後將項目移至系統垃圾桶
- **持久化的工作階段狀態**：還原上次開啟的資料夾與視窗位置大小
- **真正的選單列整合**：鍵盤快捷鍵只定義一次，選單顯示與程式處理一致

## 快速開始

### 建置並執行

```sh
export PATH="$HOME/.cargo/bin:$PATH"   # 如果 cargo 還不在 PATH 上
cargo build --release
./target/release/lumiere /path/to/folder
```

### 檢查

```sh
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets
```

目前的 headless UI 測試套件包含 macOS 原生斷言，所以 CI 只在 macOS 上執行完整測試指令（使用默認並行度）：

```sh
cargo test --locked --workspace
```

Linux 與 Windows 的 CI 驗證格式、以 warning 即失敗標準跑的 Clippy、release 建置與 portable 套件。Linux CI 也會在 Ubuntu 22.04 建置並檢查三種套件。其餘 macOS 特有的 UI 測試移植進行中。

### 打包 macOS

release 套件僅支援 Apple Silicon（arm64）；Intel Mac 尚未涵蓋。建立含圖示、`Info.plist` 與 ad-hoc 簽章的 macOS app bundle，以及與 Linux／Windows 套件同命名規則的可分發 zip：

```sh
./bundle.sh          # -> dist/Lumiere.app
                      # -> dist/lumiere-v<version>-macos-<arch>.zip
```

### 安裝 macOS app 到 `~/Applications`

把 bundle 安裝到穩定的使用者路徑。這是 macOS 上建議的做法，因為對 Dock 圖示快取的行為較可靠：

```sh
./install.sh                 # -> ~/Applications/Lumiere.app，然後開啟它
./install.sh /path/to/folder # 一樣，但立即開啟該資料夾
```

工作階段狀態儲存在：

- `~/Library/Application Support/Lumiere/state.ini`

注意：Finder／`NSWorkspace` 已經能正確看到已提交的 app 圖示。如果更換 `AppIcon.icns` 後 Dock 仍顯示通用佔位圖示，通常是 Dock／IconServices 快取問題；安裝到一個穩定路徑（`~/Applications/Lumiere.app`）並啟動那份副本，比反覆執行 `build/`、`dist/` 或 `/tmp` 裡的 bundle 更可靠。

### 在 macOS 上首次開啟：Gatekeeper

release bundle 是 **ad-hoc 簽章**：有簽章，但沒有開發者身分，所以 macOS 無法驗證建置者。第一次開啟 `Lumiere.app` 時會看到「因為無法驗證開發者，所以無法開啟」。以下任一方式可以解除：

- 在 Finder 中對 `Lumiere.app` 右鍵（或 control-click）→ **開啟** → 在對話框中按 **開啟** 確認；或
- 系統設定 → 隱私與安全性，展開「Lumiere.app 已被擋下」的提示 → **仍要開啟**。

只有第一次會詢問，之後就能正常啟動。（進階：`xattr -d com.apple.quarantine Lumiere.app` 可移除觸發檢查的隔離標記。）Developer ID 簽章加上 notarization 可以完全省去這一步——狀態見 [docs/release.md](docs/release.md)。

### 在 Linux 上打包與安裝

ARM Linux 尚未涵蓋；三種套件都只支援 x86_64，並在最低支援版本 Ubuntu 22.04 上建置。執行時仍需要相容的系統圖形函式庫與驅動程式；其他發行版未經測試。release 提供三種格式，可依需求挑選：

**`.deb`**（Debian、Ubuntu、Linux Mint 等）會系統級安裝並加入選單啟動器：

```sh
sudo dpkg -i lumiere-v<version>-linux-x86_64.deb
# 或讓 apt 解析相依套件：
sudo apt install ./lumiere-v<version>-linux-x86_64.deb
```

**AppImage** 不需要安裝——加上執行權限後直接執行即可（需要 FUSE，多數桌面環境都有；若沒有可加 `--appimage-extract-and-run`）：

```sh
chmod +x lumiere-v<version>-linux-x86_64.AppImage
./lumiere-v<version>-linux-x86_64.AppImage /path/to/folder
```

**portable 套件**為目前使用者安裝，不需要 `sudo`：

```sh
tar -xzf lumiere-v<version>-linux-x86_64.tar.gz
cd lumiere-v<version>-linux-x86_64
./install.sh
```

`.deb` 與 portable 安裝程式會建立桌面啟動器與 hicolor 圖示主題項目。AppImage 可直接執行，不會自行加入應用程式選單；如有需要，可使用 AppImage 整合工具或自行建立啟動器。

若要自己在 Linux 主機上建置，可分別使用 `./bundle-linux-deb.sh`、`./bundle-linux-appimage.sh` 或 `./bundle-linux.sh`（portable）。

### 打包 Windows

在 Windows 主機上（PowerShell）建立 portable 套件：

```powershell
.\bundle-windows.ps1        # -> dist\lumiere-v<version>-windows-x86_64.zip
```

解開套件後直接執行 `lumiere.exe`，或傳入要開啟的資料夾：

```powershell
.\lumiere.exe C:\path\to\folder
```

不需要安裝。視窗圖示已編譯進執行檔；放在旁邊的 `AppIcon.png` 供視窗內的 About 面板使用。

## 格式支援與儲存

- 內建（透過 `image` crate）：JPEG、PNG、GIF、BMP、TIFF、WebP、TGA、PCX、ICO、…
- HEIC / HEIF / AVIF 透過 macOS ImageIO（`crates/lumiere-desktop/src/platform/image_io.rs`、`objc2-image-io`）
- EXIF 透過 `kamadak-exif`
- 「移到垃圾桶」會先確認，再使用平台垃圾桶（`trash` crate），項目仍可透過桌面環境復原
- 縮圖快取：平台快取目錄（macOS 為 `~/Library/Caches/net.fetag.lumiere/thumbs`）
- 重新命名、移動與複製遇到已存在的目標名稱時會拒絕，而不是覆寫檔案；複製也拒絕符號連結
- 全解析度解碼上限為 5000 萬像素。macOS 上更大的圖片改用 ImageIO 的下採樣表示（每邊最多 6000 像素），同時保留原始尺寸。背景像素處理會串列化，以限制同時的解碼配置
## 鍵盤快捷鍵

選單列是唯一來源：`App::menu_spec()` 為每個指令宣告其 `Shortcut`，同一個值會成為 macOS 的 key equivalent、畫在選單項目旁邊的文字、以及 egui 的鍵盤綁定。在 macOS 上，選單列擁有 `⌘` 組合鍵，AppKit 會在視窗看到之前先消費掉。

| 鍵 | 動作 |
| --- | --- |
| 方向鍵 | 在格線中移動選取 |
| Home / End | 第一張／最後一張圖片 |
| PageUp / PageDown | 上一張／下一張圖片 |
| Space、Return | 開啟檢視器 |
| Esc | 關閉對話框 → 停止投影片 → 離開全螢幕 → 關閉檢視器 |
| `⌃⌘F` | 切換全螢幕 |
| `⇧⌘Y` | 開始／停止投影片 |
| `⌘⌫` | 移到垃圾桶（先詢問） |
| `⌥⌘R`、F2 | 重新命名 |
| `⌘I` | 取得資訊 |
| `⌘,` | 設定… |
| `⌘O` | 開啟…（選擇資料夾） |
| `⌘R` | 重新載入 |
| `⌘A` / `⇧⌘A` / `⌥⌘A` | 全選／取消全選／反轉選取 |
| `⇧⌘L` / `⇧⌘R` | 向左／向右旋轉 |
| `⌃⌘S` / `⌥⌘T` / `⌥⌘I` | 顯示側欄／工具列／檢查器 |
| `⇧⌘U` / `⇧⌘H` / `⇧⌘P` | 上一層／家目錄／圖片 |
| `⌘+` / `⌘-` / `⌘0` / `⇧⌘F` | 放大／縮小／實際大小／配合視窗 |
| `+` / `-` / `1` / `F` | 同上，在檢視器中不需修飾鍵 |

注意：快捷鍵比對的是鍵盤事件自帶的修飾鍵，而不是 `InputState::modifiers`——winit/eframe 不一定在同一幀更新後者。

## 專案結構

| 檔案 | 用途 |
| --- | --- |
| `apps/lumiere/src/main.rs` | eframe 進入點、CLI 資料夾參數 |
| `apps/lumiere/src/app.rs` | app 狀態、導覽、選取、原生工作流與背景工作 |
| `apps/lumiere/src/app/cache.rs` | 紋理／metadata 快取、解碼排程與逐出預算 |
| `apps/lumiere/src/app/commands.rs` | 指令分派、鍵盤處理與投影片切換 |
| `apps/lumiere/src/app/menus.rs` | 共用的選單描述、工具列、位址列與狀態列 |
| `apps/lumiere/src/app/dialogs.rs` | 設定、重新命名、傳輸、確認與回饋卡 |
| `apps/lumiere/src/app/view.rs` | 幀組合成、面板與視窗幾何 |
| `apps/lumiere/src/app/tests.rs` | 既有的 headless 應用程式與互動測試 |
| `crates/lumiere-desktop/src/file_ops.rs` | 檔案系統操作的安全規則與可注入的垃圾桶後端 |
| `apps/lumiere/src/browser.rs` | 資料夾樹、縮圖格線、預覽／資訊欄 |
| `apps/lumiere/src/viewer.rs` | 全圖檢視：縮放、平移、旋轉、翻轉、浮動工具列 |
| `crates/lumiere-desktop/src/loader.rs` | worker pool、請求佇列（full/preview 插隊）、回覆 channel |
| `crates/lumiere-desktop/src/imaging.rs` | 格式偵測、解碼、EXIF、縮圖快取 |
| `crates/lumiere-desktop/src/platform/image_io.rs` | HEIC/HEIF/AVIF 的 ImageIO 橋接 |
| `crates/lumiere-desktop/src/fsmodel.rs` | 目錄掃描、排序、萬用字元篩選、樹模型 |
| `apps/lumiere/src/theme.rs` | 隨平台適配的視覺層：淺色／深色 chrome、系統字型、單色線稿圖示 |
| `crates/lumiere-desktop/src/menu_model.rs` | 以單一平台中立描述呈現的選單列 |
| `apps/lumiere/src/platform_ui/menu/macos.rs` | 把該描述變成真實 macOS 選單列中的 `NSMenu` |
| `crates/lumiere-desktop/src/platform/window.rs` | 還原的視窗位置是否仍在某個螢幕上 |

## 測試

單元與 UI 測試放在程式碼旁邊（`#[cfg(test)] mod tests`）。UI 測試以 headless 方式驅動 `egui::Context::run`，對照暫存資料夾 fixture（`fixture_dir`），涵蓋 browser → viewer → slideshow、對話框、鍵盤導覽、資訊分頁與 command 修飾鍵快捷鍵。也會斷言選單描述本身：標題與 `⌘` 拼寫，以及選單宣告的快捷鍵就是鍵盤地圖處理的那一個。

macOS 原生選單列無法以 headless 方式測試，請對照真的東西驗證：

```sh
./target/debug/lumiere &
osascript -e 'tell application "System Events" to tell process "lumiere" to get name of every menu bar item of menu bar 1'
osascript -e 'tell application "System Events" to tell process "lumiere" to perform action "AXPress" of menu item "Show Sidebar" of menu 1 of menu bar item "View" of menu bar 1'
```

`osascript ... click at {x, y}` 對 egui 視窗無效，所以畫布的互動檢查走 key code；選單列則是真正的 AppKit，完全可以用指令腳本操作。
全螢幕使用原生 macOS Space，`screencapture` 可能截不到。

## 貢獻

歡迎貢獻。各平台的建置與測試步驟、commit 規範與 PR checklist 見 [CONTRIBUTING.md](CONTRIBUTING.md)。Bug 回報與功能請求請透過 [GitHub issues](https://github.com/fetag/lumiere/issues)；安全相關問題走 [SECURITY.md](SECURITY.md)。

本專案遵守[行為準則](CODE_OF_CONDUCT.md)，重要變更記錄在[變更日誌](CHANGELOG.md)。

## 授權

Lumiere 採用 [GNU General Public License, version 3 or later](LICENSE)（GPL-3.0-or-later）授權。你可以依該條款使用、修改與再分發；分發修改版時必須以相同授權提供對應的原始碼。

專案包含依各自條款授權的第三方素材，適用聲明見 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。

虛擬 Cargo workspace 有三個 package：`lumiere`（egui UI 與原生視窗／選單黏合層）、`lumiere-desktop`（檔案系統、worker、快取與原生服務）、`lumiere-core`（指令、工作階段編解碼與可攜式像素）。相依方向從 app 到 desktop 再到 core。根目錄的建置／執行仍選擇 `lumiere`；所有 package 的測試用 `cargo test --locked --workspace`。
貢獻者：[docs/architecture.md](docs/architecture.md) 說明各層職責與解碼管線；另見[結構與驗證紀錄](docs/structure.md)。
