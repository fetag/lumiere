# Lumiere TODO

規則：每完成一項就把 `[ ]` 改成 `[x]`，並在該項後面標註驗證方式（test / screenshot / manual）。
新發現的工作請加到對應區塊，不要只記在對話裡。

環境提醒：
- `export PATH="$HOME/.cargo/bin:$PATH"`（cargo 不在預設 PATH）
- 檢查指令：`cargo fmt --check && cargo clippy --all-targets && cargo test && cargo build --release`
- 視覺驗證：`./target/release/lumiere /tmp/lumiere_demo`，視窗定位 `osascript ... set position of window 1 to {60,60}`，截圖 `screencapture -x -R 60,60,1120,772 /tmp/shotN.png`
- 鍵盤驅動測試可用 `osascript -e 'tell application "System Events" to key code ...'`；`click at` 對 egui 視窗無效（沒有原生 AX 元素）
- headless 滑鼠/選單驅動測試：`classic::note_rect()` 會把有標籤的控制（選單項、工具列按鈕、Places、checkbox、Files 欄位）畫完後的 rect 註冊到 ctx temp data，測試用 `classic::rect_of(ctx, label)` 取座標再點；app.rs tests 有 `click_at / click_label / settle / hover / open_menu` 可沿用。三個坑：(1) egui 對「上一個 pass」的 widget 做 hit test，點擊前要先有一幀；(2) `RawInput.time` 要遞增，否則相鄰兩次點擊會被判成 double click；(3) `settle()` 會先 `clear_rects()`，否則已關閉的選單會留下過期座標

## 1. Bug / 已知問題
- [x] 全螢幕 / slideshow 背景是灰色而非黑色：`canvas_color()` 改為 `canvas_dark || fullscreen` 時用黑色 — test `fullscreen_canvas_is_black`
- [x] Slideshow 開頭卡在 "Loading..."：進入 viewer/slideshow 與換圖時呼叫 `Loader::purge_thumbs()`，清掉佇列中的縮圖工作 — 實測按 Space 開 viewer 後 1.5s 內顯示全圖（shot35）
- [x] 承上：被 purge 掉的佇列工作永遠不會回 reply，`Caches::pending` 的標記會永久卡住那些檔案（回到 browser 後縮圖再也不會載入）— `purge_thumbs()` / `bump_generation()` 現在回傳被丟掉的 `Request`，`Caches::abandon()` 清掉標記 — test `purged_thumbnail_work_clears_the_pending_marker` + `stale_generation_work_is_dropped_and_releasable`
- [x] `Action::Delete` 只處理圖片：改為以 `current()` 的 entry 作為目標，對話窗文案依 `is_dir()` 顯示 file/folder — test `delete_action_targets_folders_too`
- [ ] 多選 / Shift 範圍選取 / 右鍵選單只有 headless 層面驗證過，缺滑鼠實測（目前環境無法送滑鼠事件）
      註：headless 已可合成滑鼠事件（`PointerMoved` 一幀 + 下一幀 press/release，且 `RawInput.time` 要遞增，否則 egui 會把相鄰兩次點擊判成 double click）
- [x] Cmd+Click / Shift+Click 的多選依賴 `ui.input(|i| i.modifiers)`（frame 開始時的後端快照，注入事件時可能為 NONE）：改為讀 pointer release event 自帶的 modifiers（`browser::click_modifiers`，無 pointer 事件時才退回快照）— test `click_modifiers_prefers_the_pointer_event` + `cmd_and_shift_clicks_extend_the_selection`（合成滑鼠事件走完整 grid 路徑）
- [x] Information 面板在 browser 模式永遠沒有 Format / EXIF / 真實 Dimensions：面板只 request `Kind::Preview`，但那些欄位讀的是 `caches.full()`（只有 viewer 會填）。改為另用 `imaging::info()`（只讀 marker、不解碼像素，經 `Caches::info()` 快取避免每檔重複開檔）— test `information_panel_lists_exif_fields`
- [x] Information 的 ISO 欄位用錯 tag：kamadak 的 `Tag::ISOSpeed` 是 0x8833，相機實際寫的是 0x8827 → 改用 `Tag::PhotographicSensitivity`（另保留 0x8833 顯示為 "ISO speed"）— test `exif_summary_reads_a_real_jpeg_block`
- [x] 選單項目可用鍵盤直達：新增 Cmd+, (Options) 與 Cmd+I (Properties)，選單標籤已顯示快捷鍵 — test `menu_shortcuts_open_dialogs` + 截圖 shot36
- [ ] 選單列本身仍無法用鍵盤展開（無 Alt+字母 mnemonic）；目前靠直達快捷鍵繞過

## 2. 尚未視覺驗證的功能（用截圖或 headless test 補）
- [x] Options 對話窗版面（thumb size / show folders / black background / slideshow delay / clear cache / cache folder）— 截圖 shot36
- [x] Explore Folder 對話窗（標題 + 目前路徑有畫出來）— test `about_and_explore_dialogs_render_their_content`
- [x] About 對話窗（名稱 / 版本 / 快捷鍵說明）— test `about_and_explore_dialogs_render_their_content`
- [x] Information 面板的 EXIF 欄位：`imaging::write_exif_jpeg()` 會產生真正帶 APP1/EXIF 的 JPEG（Make/Model/Software/Artist/DateTimeOriginal/Exposure/FNumber/ISO/Focal/Orientation）— test `exif_summary_reads_a_real_jpeg_block` + `information_panel_lists_exif_fields`（面板只畫得出前幾列，後面的被 ScrollArea 裁掉，所以長欄位改查 `caches.info().exif`）
- [x] Files 萬用字元篩選（Address 列右側）實際效果：點欄位 → Cmd+A → 輸入 `*.jpg` → 按 Apply，只剩符合的檔 — test `files_filter_applies_from_the_address_row`
- [x] Places 列（Home/Desktop/Documents/...）實際效果：點 "Macintosh HD" 後 `dir == /` — test `places_row_navigates_to_the_selected_place`
- [x] 排序 Name/Date/Size/Type + Reverse 的實際效果：View ▸ Sort ▸ by Size 後順序變 a/c/b，再勾 Reverse order 變 b/c/a — test `sort_menu_reorders_the_grid`
- [x] 縮圖尺寸 Small/Medium/Large 切換（View 選單 → thumb_size 64/120/200）— test `view_menu_toggles_panels_and_thumb_size`
- [x] 隱藏/顯示 資料夾樹、Preview 面板、工具列、狀態列（View 選單）— test `view_menu_toggles_panels_and_thumb_size`
- [x] Viewer 的 flip horizontal/vertical（mesh UV 鏡射）、actual size（zoom=1）、fit to window、拖曳平移（pan 變化）— test `viewer_flip_actual_size_and_fit` + `viewer_drag_pans_the_image`
- [x] 上一個/下一個資料夾（Tab / Shift+Tab）— test `tab_moves_between_sibling_folders`

## 3. 功能缺口（經典 3.x 版有但這裡沒有）
範圍決定（2026-10）：只做看圖/瀏覽，不做圖片編輯。下面標「不做」的項目不再實作。
- [ ] 不做：對磁碟寫入 Rotate（lossless JPEG rotate）/ Flip 存檔 — 屬編輯功能；viewer 內的旋轉/鏡射為檢視用
- [x] Rename（F2）：`Action::Rename` + Rename 對話窗（父資料夾、文字欄、Rename/Cancel、Enter 送出、Esc 取消），`apply_rename()` 改名後 rescan 並選回新檔 — test `rename_updates_disk_and_selection` + 截圖 shot37
- [x] Move to / Copy to folder：`Transfer` 對話窗（Edit 選單）、`apply_transfer()` 批次搬移/複製（含多選）、`copy_path()` 遞迴複製 — test `move_and_copy_between_folders` + smoke test 開窗
- [ ] 不做：從資源回收桶還原 — 確認刪除後進系統回收桶即可，undo 由 Finder 負責
- [ ] 不做：圖片編輯最小集 Crop、Resize
- [ ] 不做：Rating / Category（stars + categories，存到 XMP sidecar 或 DB）
- [ ] 不做：Print / 匯出縮圖 contact sheet
- [x] 記住上次資料夾與視窗大小：`src/state.rs`（key=value，無新依賴），`set_dir` / 視窗 resize 時寫 `~/Library/Application Support/Lumiere/state.ini`，無參數啟動時還原 — test `state_round_trips` + 實測 state.ini 與重啟後標題 `/tmp/lumiere_test`
- [x] 視窗「位置」保存/還原：存 `outer_rect.min`，啟動後用兩段式 `OuterPosition` 修正（winit 與 AX 的 titlebar 偏移差 42px）— 實測移動到 (340,220) 後重啟，位置精確回到 (340,220)
- [ ] 不做：拖曳檔案到外部 app（drag out）
- [ ] 不做：巨集 / batch rename

## 4. 工程 / 交付
- [x] README.md：功能、格式、模組表、快捷鍵表、測試與截圖驗證方式 — 已建立 `README.md`
- [x] `.app` bundle 打包：`./bundle.sh`（Info.plist、CFBundleDocumentTypes、ad-hoc codesign）— 實測 `open build/Lumiere.app` 可啟動並還原上次資料夾
- [x] codesign / notarize 步驟文件：寫在 `bundle.sh` 註解（Developer ID + notarytool 步驟）
- [x] App icon（.icns）：`examples/make_icon.rs` 畫出 1024px PNG，`bundle.sh` 用 sips + iconutil 產生 `AppIcon.icns` 並寫入 `CFBundleIconFile` — 已確認 bundle 內有 icns 且 app 可啟動；Dock 外觀尚未截圖確認
- [ ] 用 LaunchServices 開檔（`open -a Lumiere.app foo.jpg`）不會帶 argv，需要處理 Apple Events openFile；目前只能靠 argv 或還原上次資料夾
      卡點：winit 會把開檔事件收進 `Event::Opened`，但 eframe 0.33 的 run loop 完全不處理這個 event（`grep Opened eframe/egui-winit` 無結果），所以 app 端拿不到；要嘛自己用 objc2 掛 NSApplicationDelegate（會和 winit 的 delegate 打架），要嘛等 eframe 支援
- [x] 啟動時帶圖片檔路徑會切到該檔所在資料夾並選取該檔 — test `opening_a_file_selects_it`
- [x] CI（fmt + clippy + test + release + bundle）— `.github/workflows/ci.yml`（macos-latest）；本地已重跑 `cargo clippy --all-targets -- -D warnings` 與 `./bundle.sh dist` 全綠
- [x] 建立 git repo：`git init -b main` + 首個 commit `566bbaf`（18 files，`dist/` 已加入 .gitignore）— manual `git log --oneline`
- [ ] 遠端 repo 未建立（`gh` 未登入：需 `gh auth login` 後 `gh repo create` + push，CI 才會真的跑）
- [x] 大資料夾效能：texture cache 加 byte 預算（thumbs 96MB / fulls 192MB，`trim()` 依 LRU 淘汰到數量與容量都上線以內；同 key 重複插入不重複計數）— test `thumb_cache_evicts_to_stay_within_budget` + 實測 `/tmp/lumiere_big`（1500 檔）RSS 203MB vs 空目錄 160MB、6s 後 CPU idle
- [x] 磁碟縮圖快取上限：`imaging::prune_cache()` 依 mtime 刪最舊直到 <512MB，啟動時呼叫 — test `prune_cache_removes_oldest_first`

## 5. 已完成（本輪）
- [x] 鍵盤選取後捲動到選取項（End/Home/方向鍵）— 截圖 shot15/shot17
- [x] Cmd 快捷鍵失效修正（改用事件自帶 modifiers，Shift 變體優先匹配）— test `command_shortcuts_read_event_modifiers` + 實測 Cmd+F `AXFullScreen=true`
- [x] 新增 Cmd+F 全螢幕快捷鍵 — 截圖 shot29
- [x] Escape 關閉 Confirm Delete / Options / About / Explore — test `escape_closes_open_dialogs` + 截圖 shot22
- [x] Slideshow / Open Viewer 選到資料夾時自動跳到第一張圖片 — test `slideshow_starts_from_first_image_when_folder_selected` + 截圖 shot31
- [x] Information 面板 headless 測試 — test `information_tab_lists_file_metadata`
- [x] 工具列直排修正、Rotate/Browser 圖示、CJK 字型 — 截圖 shot11~shot17
