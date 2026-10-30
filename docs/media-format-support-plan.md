# 媒體格式支援規劃（SVG / ICO / GIF 動畫 / 影片）

建立：2026-12-15（基於 v0.1.5 代碼調查）

## 現況盤點

| 格式 | 狀態 | 證據 |
|---|---|---|
| `.ico` / `.cur` | **已支援**（v0.1.4+，2026-12-15 使用者實測確認） | `crates/lumiere-desktop/src/imaging.rs` 的 `SUPPORTED_EXTS` 含 `"ico","cur"`；`image` crate 的 `ico` feature 在 `crates/lumiere-core/Cargo.toml` 與 `apps/lumiere/Cargo.toml` 均已啟用；`bundle.sh` 已宣告 `ico` document extension |
| `.svg` | 不支援 | 不在 `SUPPORTED_EXTS`；`image` crate 的 `svg` feature（resvg）未啟用 |
| `.gif` | **僅第一幀，無動畫** | 解碼管線為 `decode_with_image()`（`crates/lumiere-core/src/imaging.rs`）：`into_decoder().to_rgba8()` 產出單張 RGBA buffer；全專案無任何 frame-delay / GifFrame 處理；egui texture 靜態 |
| `.mp4` / `.avi` | 不支援，屬架構級新增 | 現有管線假設「一個檔 = 一張 RGBA 圖」：decode worker（4 threads、`MAX_DECODE_PIXELS` 50MP）→ `Decoded{rgba}` → 單一 `TextureHandle`（Full/Thumb/Preview 三種 cache，見 `apps/lumiere/src/app/cache.rs`）→ 靜態 mesh（`apps/lumiere/src/viewer.rs`） |

架構要點（影響所有新增格式）：

- 縮圖 / 預覽 / 全尺寸三級 cache 都走同一條 RGBA → texture 路徑。
- macOS 有 ImageIO fallback（HEIC/AVIF/RAW），見 `crates/lumiere-desktop/src/platform/image_io.rs`。
- `bundle.sh` 的 document extensions 與 `SUPPORTED_EXTS` 有測試綁定
  （`bundle_declares_every_supported_extension`），加格式兩邊都要改，漏了 CI 會掛。
- 檔案對話框 filter 預設 `*.*`，新增格式不需要動 dialog。

## ~~Phase 0：.ico 排查~~（2026-12-15 已排除）

使用者實測 `.ico` 可正常開啟，無需排查。若日後遇到特定 ico 打不開再啟用：
多尺寸 ICO 可能只解到其中一幀，改用 `into_frames()` 挑最大尺寸幀。

## Phase 1：SVG（中等，約 1–2 天）— **2026-12-15 已完成**

實作選擇：直接用 `resvg`/`usvg` 0.49 依賴（非 image crate 的 svg feature），
在 `lumiere-desktop/src/imaging.rs` 新增 `decode_svg()`，由 `decode()` 對
`.svg` 分派；超尺寸文件以縮小 transform 渲染（pixel budget 變渲染目標而非
拒絕條件），`original_size` 保留真實文件尺寸；premultiplied 輸出經
`take_demultiplied()` 轉 straight alpha 後餵現有管線；`pixel_size()`/`info()`
走 usvg 解析取尺寸。macOS 失敗時仍落 ImageIO fallback。

1. ~~啟用 `image` crate `svg` feature…~~（改用直接依賴 resvg）
2. ✅ `SUPPORTED_EXTS` 加 `"svg"`；`bundle.sh` 同步加 `<string>svg</string>` + `org.w3c.svg` UTI。
3. 向量格式特殊處理：
   - **按目標尺寸渲染**（縮圖渲 ~256px、全尺寸按 texture 上限渲），
     不能先解成固定尺寸再縮放，否則放大糊。
   - 無 `viewBox` / 無限尺寸的 SVG 設預設邊界；沿用 `MAX_DECODE_PIXELS` 防 bomb。
   - 動畫 SVG（SMIL）v1 只取第一幀，README/文件寫明。
4. macOS ImageIO fallback 不動（ImageIO 對 SVG 支援差，一律走 resvg）。

## Phase 2：GIF 動畫（中偏大，約 3–5 天）

- **解碼**：`gif` crate（或 image 的 `GifDecoder::into_frames()`）取全部幀
  + delay。重點驗證 disposal method 合成（透明/局部更新 GIF 最易出錯）。
- **播放時鐘**：egui 以 `request_repaint_after(frame_delay)` 驅動逐幀更新；
  小 GIF 預上傳全部 texture，大 GIF 按需解碼。
  - 播放/暫停、loop count（`0` = 無限）。
  - 記憶體上限：幀數 × 尺寸 cap，超額走按需解碼。
- **縮圖**：維持第一幀靜態（動畫縮圖當 v2）。
- **slideshow 整合**：決定 GIF 播完才切下一張，或固定間隔（建議播完再切）。

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

1. **Phase 1（SVG）先做**（約 1–2 天，一週內可上），風險最低、立即有感。
2. Phase 2 獨立可上，不擋 Phase 3。
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
