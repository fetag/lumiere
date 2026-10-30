# 媒體格式支援規劃（SVG / ICO / GIF 動畫 / 影片）

更新：2026-10-30（基於 v0.1.5 代碼與使用者測試）

## 現況盤點

| 格式 | 狀態 | 證據 |
|---|---|---|
| `.ico` / `.cur` | **已支援**（v0.1.4+，使用者實測確認） | `crates/lumiere-desktop/src/imaging.rs` 的 `SUPPORTED_EXTS` 含 `"ico","cur"`；`image` crate 的 `ico` feature 在 `crates/lumiere-core/Cargo.toml` 與 `apps/lumiere/Cargo.toml` 均已啟用；`bundle.sh` 已宣告 `ico` document extension |
| `.svg` | **已支援靜態顯示** | 使用 `resvg` / `usvg` 渲染；縮圖、預覽與檢視器依顯示尺寸產生 texture，放大時背景重渲染；使用者已確認清晰度與裁切修正 |
| `.gif` | **檢視器支援動畫（三平台使用者測試通過）** | `apps/lumiere/src/animation.rs` 使用 `zengif` 背景逐幀解碼，依影格延遲與循環次數播放；縮圖與右側預覽仍是靜態畫面 |
| `.mp4` / `.avi` | 不支援，屬架構級新增 | 現有管線假設「一個檔 = 一張 RGBA 圖」：decode worker（4 threads、`MAX_DECODE_PIXELS` 50MP）→ `Decoded{rgba}` → 單一 `TextureHandle`（Full/Thumb/Preview 三種 cache，見 `apps/lumiere/src/app/cache.rs`）→ 靜態 mesh（`apps/lumiere/src/viewer.rs`） |

架構要點（影響所有新增格式）：

- 縮圖 / 預覽 / 全尺寸三級 cache 都走同一條 RGBA → texture 路徑。
- macOS 有 ImageIO fallback（HEIC/AVIF/RAW），見 `crates/lumiere-desktop/src/platform/image_io.rs`。
- `bundle.sh` 的 document extensions 與 `SUPPORTED_EXTS` 有測試綁定
  （`bundle_declares_every_supported_extension`），加格式兩邊都要改，漏了 CI 會掛。
- 檔案對話框 filter 預設 `*.*`，新增格式不需要動 dialog。

## ~~Phase 0：.ico 排查~~（已排除）

使用者實測 `.ico` 可正常開啟，無需排查。若日後遇到特定 ico 打不開再啟用：
多尺寸 ICO 可能只解到其中一幀，改用 `into_frames()` 挑最大尺寸幀。

## Phase 1：SVG — **已完成，使用者已測試**

實作選擇：直接用 `resvg`/`usvg` 0.49 依賴（非 image crate 的 svg feature），
在 `lumiere-desktop/src/imaging.rs` 新增 `decode_svg()`，由 `decode()` 對
`.svg` 分派；超尺寸文件以縮小 transform 渲染（pixel budget 變渲染目標而非
拒絕條件），`original_size` 保留真實文件尺寸；premultiplied 輸出經
`take_demultiplied()` 轉 straight alpha 後餵現有管線；`pixel_size()`/`info()`
走 usvg 解析取尺寸。macOS 失敗時仍落 ImageIO fallback。

- `SUPPORTED_EXTS` 已加入 `svg`；`bundle.sh` 已加入副檔名及 `org.w3c.svg` UTI。
- 縮圖與預覽依顯示尺寸及螢幕像素密度渲染；檢視器放大時背景重渲染，完成前保留既有 texture，並避免較低解析度的結果覆蓋較清晰的結果。
- 保留 SVG 小數寬高計算畫布，避免截掉右側與底部；使用者已確認清晰度與裁切問題改善。
- 渲染受 `MAX_DECODE_PIXELS`、GPU texture 上限及全尺寸 texture 6000 像素最長邊限制；極端放大不保證維持清晰。
- 動畫 SVG（SMIL）僅渲染靜態畫面；README 已註明。
- 回歸測試涵蓋目標解析度、小數尺寸邊緣、損壞與超大 SVG。macOS 使用者測試已通過；Linux / Windows 實機驗證尚待執行。

## Phase 2：GIF 動畫 — 第一版（三平台使用者測試通過）

- 採用 `zengif` 0.6.0 的 `with_next_frame` 逐幀解碼及合成，使用最多兩幀的佇列、單一 GPU texture；檢視器支援暫停／繼續及循環。縮圖與右側預覽維持靜態。
- 上限為檔案 64 MiB、畫布 800 萬像素、10,000 幀及 decoder 記憶體 64 MiB；佇列與 GPU 等額外配置不計入 decoder 上限。超過限制時顯示警告。
- slideshow 仍依既有固定間隔切換，不等待動畫播放完畢。
- 回歸測試涵蓋影格延遲、透明與局部更新、disposal、循環次數、暫停及 101 幀串流；macOS GUI 已用使用者的 `test.gif` 確認畫面變化。Windows 與 Linux 原生建置和 GIF 測試通過；使用者回報 macOS、Windows、Linux 的 GIF 動畫功能測試均通過。

## Phase 3：影片 mp4 / avi（大，約 1–2 週；最大不確定項是打包）

- **解碼器**：ffmpeg-next（libav），mp4/h264/hevc/avi/xvid 一次覆蓋；
  AVI 只是容器，有 ffmpeg 即白送。
- **跨平台打包**（最大成本）：
  - macOS：`bundle.sh` 處理 libav dylib。
  - Linux：`bundle-linux-appimage.sh` / `bundle-linux-deb.sh`。
  - Windows：`bundle-windows.ps1` 處理 DLL。
  - 建議先做 **macOS-only spike** 驗證打包路線再全面開工。
- **架構新增**：
  - `Entry` 引入 media kind 概念（image / video），影片不走現有單 texture cache。
  - 播放管線：解碼 thread → frame ring buffer → texture update；
    v1 播放/暫停，seek bar 可放 v1.1。
  - **音訊是獨立子專案**（ffmpeg 音軌 + rodio/cpal）；建議 v1 靜音播放，
    音訊標 v1.1——否則工期約 +40%。
  - 縮圖：ffmpeg 抓首幀 → 餵進現有 thumbnail 管線。
  - info 欄位加 duration / 解析度；沿用 pixel cap。

## 順序與決策點

1. Phase 1（SVG）已完成；後續可補 Linux / Windows 實機驗證。
2. Phase 2（GIF 動畫）已改用 zengif 串流，使用者已完成三平台測試。
3. Phase 3 開工前拍板：
   - v1 要不要音訊（建議不要）。
   - v1 要不要 seek（建議要基本 seek bar，v1.1 再強化）。
   - ffmpeg 打包 spike 結果。

## 驗收清單（各 phase 收尾前確認）

- [ ] `SUPPORTED_EXTS` 與 `bundle.sh` 同步（CI 測試強制）
- [ ] 新格式有 decode 回歸測試（含邊界：超大尺寸、損壞檔、0 幀 GIF、無 viewBox SVG）
- [ ] 縮圖 / 預覽 / 全螢幕三級顯示都驗證過
- [ ] macOS ImageIO fallback 路徑不受影響
- [ ] 三平台（macOS/Linux/Windows）bundle 腳本與 README 格式清單更新
