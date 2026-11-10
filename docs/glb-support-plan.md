# GLB 3D Model 支援規劃

更新：2026-11-13（基於 v0.1.6 代碼盤點；與 `media-format-support-plan.md` 同期架構）

## 結論

費工：**中等**。比任何純影像格式貴（GLB 是 3D 場景不是位圖，現有
RGBA → texture 管線無法直接處理，必須新增一條 3D 渲染管線），
但比影片 phase 便宜：關鍵依賴 `gltf`（解析）與 `wgpu`（渲染）
都是純 Rust 繫結，**不需要原生函式庫跨平台打包**——而那正是
`mp4/avi` phase 卡住的最大成本（libmpv dylib/DLL 三平台打包
至今未收口，見 `media-format-support-plan.md` Phase 3）。

| 新增格式 | 管線 | 原生依賴打包 | 狀態 |
|---|---|---|---|
| `.svg` | resvg 光柵化 → 現有 texture 路徑 | 無 | 已完成 |
| `.gif` 動畫 | zengif 背景逐幀解碼 → 單一 texture | 無 | 已完成 |
| `.mp4`/`.avi` | mpv-engine 背景播放 → texture | **libmpv（未收口）** | 實驗性 |
| `.glb`（本規劃） | wgpu offscreen 渲染 → texture | **無**（wgpu 為系統 GPU API 繫結） | 規劃中 |

## 架構要點（盤點證據，影響所有整合決策）

- 影像管線：`image` crate 解碼 → RGBA → egui texture；縮圖/預覽/全尺寸
  三級 cache 同路徑。`crates/lumiere-desktop/src/imaging.rs`
  （`SUPPORTED_EXTS`、`decode()`、`MAX_DECODE_PIXELS` 保護）、
  `crates/lumiere-desktop/src/loader.rs`（4 worker 背景解碼池）、
  `apps/lumiere/src/app/cache.rs`。
- **非靜態媒體先例（GLB 直接沿用此模式）**：`video` feature flag
  + 背景 thread → frame → `ctx.load_texture` 上傳 → `viewer.rs`
  開頭 `is_video` 分派到專屬 `view()`（`apps/lumiere/src/viewer.rs`
  的 `#[cfg(feature = "video")]` 分支、`apps/lumiere/src/video.rs`
  的 `VideoPlayer` 架構）。
- 格式 gate：`is_image()`（`imaging.rs:114`）是 browser/navigation/
  檔案對話框的唯一開關，`Entry.is_image`（`fsmodel.rs`）下游自動跟到；
  新增 `is_model()` 併入 `is_image()` 即可。
- 影片縮圖 = placeholder：browser 縮圖請求過濾掉 video
  （`apps/lumiere/src/browser.rs` 的 `filter(|e| e.is_image && !imaging::is_video(...))`）。
- `bundle.sh` 的 `CFBundleDocumentTypes` extensions 與 `SUPPORTED_EXTS`
  有測試綁定（`bundle_declares_every_supported_extension`），漏了 CI 會掛。
- eframe 渲染器選擇（`apps/lumiere/src/main.rs`）：Windows 用
  `Renderer::Wgpu`，macOS/Linux 用 `Glow`。對本規劃**無影響**：
  GLB 管線自建獨立 wgpu device，不碰 eframe 的後端。
- 發版驗證流程（KB：lumiere-release-workflow）：Windows .120、
  Linux .123、本機 macOS 三台原生建置後交使用者驗收。
- Spike 先例：`experiments/video-spike/`、`experiments/mpv-spike/`
  （三平台獨立 spike 先於正式串接）。

## 費工結構

| 項目 | 成本 | 說明 |
|---|---|---|
| 整合點（extension gate、viewer 分支、bundle.sh、menus/panels） | 小 | 照 video 先例逐一對照即可 |
| `gltf` 解析 + 幾何/貼圖上傳 | 中 | `gltf` crate 成熟；抽 positions/normals/UVs/indices + baseColor 貼圖 |
| wgpu offscreen 渲染 + WGSL shader | **中（主要風險）** | 背景 thread 自建 device → offscreen target → readback → egui texture。三 backend（Metal/Vulkan/D3D12）readback 路徑是唯一技術未知數，Phase 0 spike 當控制組先驗 |
| 互動（orbit / pan / zoom / fit view） | 中小 | 拖曳 orbit + 縮放 + 雙擊 fit；只在互動時 re-render |
| 資源保護 | 小 | 照 `MAX_DECODE_PIXELS` 精神：檔案大小、頂點數、貼圖尺寸上限 |
| 縮圖（GPU 渲染進 disk cache） | 中（v1 跳過） | 需獨立 GPU thumbnail 服務，不能塞進 CPU loader worker |
| 三平台驗證 | 中 | 照 v0.1.6 發版流程 |

## Phase 0：Spike（1–2 天）— 決策門 — **已通過（2026-11-09）**

`experiments/glb-spike/`。目標：驗證渲染管線在三個 backend 都通。

**驗證結果**（DamagedHelmet.glb，1024×1024，3 角度）：

| 平台 | backend | 驅動 | 結果 | 前景佔比 front/az45/elev30 |
|------|---------|------|------|---------------------------|
| macOS（本機） | Metal | Apple M5 硬体 | PASS | 6.0 / 3.8 / 3.0% |
| Windows .120 | D3D12 | WARP（Microsoft Basic Render Driver） | PASS | 6.0 / 3.8 / 3.0% |
| Linux .123 | Vulkan | llvmpipe（軟體） | PASS | 6.0 / 3.8 / 3.0% |

- 三平台前景佔比完全一致；pixel diff（vs macOS 基準）< 0.002%（8/12 個
  anti-aliasing 邊緣像素，meanΔ < 0.04/765），實質像素一致。
- 幾何正確性以控制組確認：Crate（方塊）、Chair_1（條紋椅）輪廓正確；
  DamagedHelmet 貼圖（2048² atlas）sampling 正確。
- 無貼圖模型（albedo 缺）走 1×1 gray placeholder，flat color 正常。
- 負例（非 GLB 檔）合理報錯：`gltf import failed: expected value ...`。
- 關鍵 API 坑（wgpu 27）：uniform 用 bind group（Metal 無 PUSH_CONSTANT
  capability）；readback 先 submit copy 再 map（同 submission 的 COPY_DST
  buffer 不可同時有 pending map）。詳見 spike `README.md`。

原始設計（以 wgpu 在背景 thread 建 instance/device，離屏渲染一個標準測試
  GLB（Khronos 官方 DamagedHelmet.glb），readback 成 RGBA，
  確認像素正確（非全黑/非錯位）。
- 成功標準（控制組 = 已知正常模型出圖）：
  - macOS（Metal）、Linux .123（Vulkan）、Windows .120（D3D12）
    三台各出一張正確截圖。
  - `gltf` crate 解析 DamagedHelmet：mesh/貼圖抽取正確。
- Fallback 梯隊：
  1. 若 `SurfaceTarget::Offscreen` 在某平台不可用/讀不回：
     改用 offscreen texture + `copy_external_to_buffer`。
  2. 仍失敗的平台：该平台 MVP 退回「載入時渲染單幀靜態預覽」
     （或該平台砍掉），不阻塞其他平台。
- **停手條件**：三平台全讀不回 → 停下與使用者重新決策
  （CPU rasterizer 或延期），不硬寫正式串接。

## Phase 1：MVP viewer（4–6 天，spike 通過後）— 已完成（2026-11-09 代碼完成；2026-11-10 三平台使用者實測通過）

架構：照 `video.rs` 模式，新增 `apps/lumiere/src/models.rs` +
`models` feature flag（預設關閉，同 `video` 先例；因無原生依賴，
三平台驗證通過後可再評估預設開啟）。

- **格式 gate**：`is_model(path)`（副檔名 `glb`）；`is_image()` 併入
  `is_model`；**不進 `SUPPORTED_EXTS`**（照 `is_video` 的 feature-gate
  先例——那些是 `image` crate / ImageIO 解得到的格式；feature 關時
  `.glb` 對 browser 不可見）；`bundle.sh` 加副檔名 `glb` + UTI
  `model.gltf-binary`。
- **viewer 分派**：`viewer.rs` 開頭照 video 分支，
  `is_model` → `crate::models::view(app, ui, &path)`。
- **渲染管線**：
  - 載入 thread：`gltf` 解析 → 幾何上傳 buffer（positions/normals/
    UVs/indices）→ baseColor 貼圖上傳 texture → 事件 channel 回報
    就绪/錯誤（照 `VideoPlayer::open` 的 thread + channel 架構）。
  - 相機：orbit（拖曳）/ zoom（滚輪與捏合）/ pan；雙擊或 Fit 鈕
    fit-to-view（模型 bounding box 計算）。
  - **只在互動與相機變化時 re-render**（`request_repaint` 驅動），
    不跑常駐 60fps loop。
  - Shader：WGSL 簡化照明（ambient + directional），貼圖採樣；
    無貼圖 fallback unlit。不追完整 PBR（Phase 2+ 或永不）。
- **資源保護**（超標顯示失敗訊息，語義照 `DecodeError`）：
  檔案大小 ≤ 64 MiB（同 GIF 上限）、頂點數上限（例如 5M）、
  貼圖最長邊上限（例如 8192）。
- **工具列**：模型顯示時隱藏影像專用的 zoom/rotate/flip，
  改顯示 Fit view / Reset camera（照 `draw_toolbar` 的 button 模式）。
- **Info panel**：三角形數、mesh 數、檔案大小。
- **縮圖**：placeholder（同影片現況），GPU 縮圖留 Phase 2。
- **測試**：`is_model` gate 測試；損壞 GLB、0 節點模型、超頂點數、
  超大檔案的 decode 失敗路徑；spike 截圖對照測試（如可行）。

### 實作進度（2026-11-09）

代碼完成，本機 macOS（Apple M5 / Metal）headless 驗證通過：

- 新增 `apps/lumiere/src/models.rs`（~1000 行）：`ModelPlayer`（UI thread
  狀態 + camera + texture + orbit/zoom/雙擊重設）+ `Scene`（背景 thread
  GPU：wgpu 27 offscreen target、單-pass 多-primitive render、readback）+
  `parse_glb`（gltf `import_slice`，可 headless 測）+ WGSL shader。
- **baseColorFactor 支援**（2026-11-09 修復）：原本只讀 baseColor 貼圖、
  無貼圖時一律 fallback 灰色，把模型的 `baseColorFactor` 平坦色丟掉
  （家具全灰）。改為 glTF 語義 `baseColor = texture(缺省白 1x1) * factor`：
  每 primitive 一個 base_color uniform buffer（binding 3，build 時寫一次），
  placeholder 改白色，fragment 乘 `prim.base_color`。有貼圖模型（factor
  預設白）行為不變。
- **光照改進**（2026-11-09 修復「太暗 + 光源跟著轉」）：原 shader 只有
  單一直射光 + 低 ambient (0.18) + **無 gamma 校正**→ 背光面幾乎全黑、
  中間調偏暗，orbit 時亮/暗面切換劇烈，看起來像光在掃。改為**三向固定
  世界座標光**（key 主光 + fill 補光 + hemisphere 天/地 ambient）+ **linear
  → sRGB gamma**（`pow(x, 1/2.2)`）。key/fill 皆固定世界座標（不跟相機
  轉），各面都有基礎亮度，orbit 時光讀起來是固定攝影棚光。uniform
  布局 mvp(16)+key(3)+fill(3)=24 float。
- `models` feature flag（預設關閉，同 `video`）；`is_model()` 加進
  `imaging.rs`（feature-gate，不進 `SUPPORTED_EXTS`）。
- viewer dispatch（`viewer.rs`）、browser（Model icon、不 prefetch 縮圖）、
  `app/view.rs`（非 Viewer 時 `model_player.clear()`）、`theme.rs`
  （`Icon::Model`）。
- `bundle.sh` 加 `glb` + UTI `model.gltf-binary`。
- 測試（4 個，全過）：`is_model` feature-gate、`parse_glb` 雙-primitive
  （含 baseColorFactor 紅/綠斷言）、損壞 GLB、空模型（0 primitive）、
  **多-primitive 單-pass offscreen GPU render**（`fixtures/model-test.glb`
  兩個三角形，headless 實測 readback + 紅/綠双色同時出現證明 factor 生效）。
- 測試模型：`experiments/glb-spike/test-models/`（DamagedHelmet 有貼圖 +
  Computer 4-primitives + 5 個無貼圖家具）。
- clippy 乾淨；default build（無 feature）編譯；全部 88 測試通過。

三平台實機驗證（2026-11-10 完成）：
- macOS 本機 + Windows .120 + Linux .123 均完成 `--features models` 原生建置
  （release），使用者確認 .glb viewer 通過（模型渲染 + 滑鼠 orbit/zoom）。
- 剩餘：README 格式清單更新（三平台）〔未做〕；超頂點數 / 超大檔案自動測試
  （可選、成本高，以 code review + headless 失敗路徑覆蓋）。

> 開發期踩坑：glTF `componentType` 常數 `5123`=U16、`5125`=U32（易與
> OpenGL 混淆）；fixture 用錯會使 indices 解錯。詳見 spike README。

## Phase 2：縮圖 + 打磨（3–5 天，視 MVP 驗收結果）— 已完成（2026-11-09~10：GPU 縮圖服務 / wireframe / auto-rotate / Draco-meshopt 偵測）

- **GPU thumbnail 服務（已實作 2026-11-09）**：獨立單 thread（`model_thumbs.rs`
  `ModelThumbService`）+ 自持單一 wgpu device，固定相機（yaw=0/pitch=0.35）
  離屏渲染 → 產出 `Decoded` → 餵進**同一條** `Caches::pump`（Decoded →
  texture → ThumbEntry）→ browser grid / preview pane 自動畫真實縮圖；渲染
  結果寫入現有 disk thumbnail cache（`store_thumbnail`），跨 session 免再渲染。
  - 路由：`Caches::queue_work` 偵測 `is_model` → 送 GPU 服務（非 image
    loader，它解不開 .glb）；browser prefetch/請求移除 `!is_model` 過濾。
  - `Scene` 參數化 size 且**正規化到 64 的倍數**（`size*4` 必為 256 倍數，
    滿足 wgpu `COPY_BYTES_PER_ROW_ALIGNMENT` 讀回）；縮圖服務渲染後
    `Decoded::scaled`（Lanczos）縮到請求尺寸。
  - 驗證：92 測試（含 `service_renders_and_persists` 全 path + disk cache
    round-trip）；DamagedHelmet（3.7MB 帶 texture）經本 path 渲染 11.9%
    coverage、max_pixel=255。
- wireframe 開關、auto-rotate 開關（已完成：`Command::Render(Camera, bool)` + 兩個 checkbox；adapter 不支援 `POLYGON_MODE_LINE` 時 wireframe 自動 no-op 不 crash；`render_wireframe_differs_from_solid` 測試）。
- Draco / meshopt 解壓縮（2026-11-09 完成偵測層 + 調研；完整解碼另行決策）：
  - **已實作：偵測 + 明確錯誤**。`parse_glb` 在讀取幾何前調 `check_compression_extensions`：
    - primitive 帶 `KHR_draco_mesh_compression` → 「Draco-compressed geometry … not supported」
    - accessor / bufferView 帶 `EXT_meshopt_compression` → 「meshopt-compressed geometry … not supported」
    - 啟用 gltf 的 `extensions` feature 才能讀 `extensions()`；測試 `parse_glb_rejects_draco` /
      `parse_glb_rejects_meshopt`（fixture `model-draco.glb` / `model-meshopt.glb`）。
    - 為什麼必要：gltf crate 不解碼壓縮 payload，壓縮 accessor 的 bufferView 比 declared
      size 小，直接讀會回 `None` / 吐錯位資料（之前會以「no positions」模糊錯誤失敗）。
  - **調研（為何未直接實作解碼）**：
    - **沒有純 Rust 解碼器**。只有 FFI binding：`draco-rs`（`autocxx+cmake+cxx`，build
      時從源碼編譯整個 C++ Draco，重）；`meshopt`（0.7.0，2026-10 活躍，`cc` 編譯小 MIT C 庫，輕）。
    - **gltf crate 限制**：`import_buffers` 用 `blob.take()` 只支援單一 BIN buffer；兩 buffer
      的 meshopt GLB 在 import 階段就失敗（`MissingBlob`），偵測邏輯沒機會跑。Draco GLB
      通常單 buffer → import 成功 → 偵測攔截。
    - 即使加解碼 crate 還需自行 wire glTF extension（Draco attribute remap 尤其複雜）。
  - **建議**：本專案「不要變臃腫」原則下，完整解碼另立決策；若要實作 meshopt 優先（輕）。
    現階段：壓縮模型以明確錯誤提示（不 crash、不吐錯位幾何）。
- （獨立可選項，單列不併入）GLB 動畫播放：animation sampling +
  播放控制，工作量不小，另立 phase。

## 明確不做（scope out）

- `.obj` / `.fbx` / `.usdz`（FBX 無可用 Rust parser；維持專案
  「不長 fat feature」定位）。
- 完整 PBR shader + IBL 環境光。
- 多場景/多 asset picker。

## 工作量估算

- Phase 0 spike → Phase 1 MVP 可用：**約 2 週**（含三平台驗證）。
- 含 Phase 2：**約 3 週**。
- 相對影片 phase 省掉的正是它卡住未收口的跨平台原生打包。

## 決策點（待使用者拍板）

1. **互動深度**：MVP 要互動式 orbit（本規劃預設，3D viewer 核心
   體驗）還是靜態單幀預覽即可？後者 Phase 1 可砍約一半。
2. **動畫**：MVP 是否包含含動畫的 GLB？（本規劃預設：不，另立 phase。）
3. **`.gltf` 副檔名**（JSON + 外部 .bin/貼圖）：本規劃只收 `.glb`；
   順手加需處理外部資源路徑解析。

## 驗收清單（各 phase 收尾前確認）

> Phase 1/2 代碼於 2026-11-09 完成；三平台（macOS 本機 / Win .120 / Linux .123）
> `--features models` 原生建置 + 使用者實測於 2026-11-10 完成。以下 `[x]` 已驗
> （代碼/本機/實測），`[ ]` 為剩餘項。

- [x] `bundle.sh` 同步（CI 測試強制）——`glb` 照 video 先例**不進**
      `SUPPORTED_EXTS`（feature-gate），已加進 `bundle.sh` 副檔名 +
      UTI `model.gltf-binary`；`bundle_declares_every_supported_extension`
      通過（只查 SUPPORTED_EXTS，多加 glb 不影響）
- [x] decode 回歸測試（代碼層）：feature-gate on/off、損壞 GLB、
      空模型（0 primitive）、多-primitive 單-pass offscreen render
      （headless GPU 實測）。超頂點數 / 超大檔案的失敗路徑尚無自動
      測試（需 5M 頂點 / >64MiB fixture，成本高，暫以 code review 覆蓋）
- [x] spike 三平台截圖與 DamagedHelmet 官方參考圖目視對照（2026-11-09，三平台像素一致，幾何結構吻合官方 screenshot）
- [x] 檢視器實機驗證：模型渲染 + orbit/zoom（滑鼠）三平台（macOS / Linux
      .123 / Windows .120，2026-11-10）確認通過；多-primitive / 有無貼圖差異
      以 headless GPU 測試覆蓋；縮圖為真實 GPU 渲染（Phase 2，已非 placeholder）
- [x] 資源保護：超標 / 壓縮檔案顯示明確失敗訊息且不 crash（code review +
      headless 覆蓋失敗路徑 + 壓縮偵測測試；實測正常模型未 crash）
- [x] 互動：orbit/zoom 在 macOS + Windows/Linux 滑鼠驗證過（2026-11-10）；
      觸控板捏合為 macOS 本機行為，未單列實測
- [x] macOS ImageIO fallback 路徑與既有影像管線不受影響（default build
      編譯 + 全部 88 測試通過，含影像管線測試）
- [x] 三平台（macOS/Linux/Windows）原生建置（2026-11-10，release build 皆成功 + 實測）
- [ ] README 格式清單更新（三平台）— 剩餘
- [x] `models` feature flag 的 default-off 建置（default 編譯通過，models
      測試僅 feature on 時編譯）；CI 待推
