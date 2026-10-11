# Lumiere macOS UI/UX 改善待辦

> 檢視範圍：本專案（Rust／eframe／egui）。
>
> 本文件是實作規格，不包含應用程式碼修改。原本的 Win95 風格視覺層（`src/classic.rs`）已移除，改採 macOS 系統色彩／字型／動態外觀的 visual layer（`src/theme.rs`）；下列項目以 macOS 使用者的既有心智模型為目標，優先處理會造成操作錯誤、找不到功能或無法使用輔助技術的問題。

## P0 — 先使核心操作符合 macOS

- [x] **改用真正的 macOS 選單列與標準 App 選單結構。**
  - 現況／依據：`src/app.rs:978-1186` 以 app 內容區的 `egui::MenuBar` 實作 File/Edit/View/Go/Tools/Help；`src/app.rs:1853-1858` 又固定把它畫在視窗內 22 px 高的面板。這會與螢幕頂端選單列、焦點切換與標準「Lumiere」選單脫節。
  - 實作方向：以 macOS 原生 menu API（或 eframe/winit 可用的原生橋接）建立頂端選單列；將 `About Lumiere`、`Settings…`、`Hide Lumiere`、`Hide Others`、`Show All`、`Quit Lumiere` 放入 App 選單。保留 `Action` 作為單一 command dispatcher，讓原生選單、工具列與快捷鍵呼叫同一 command。
  - 驗收：App 前景時，所有選單出現在 macOS 螢幕頂端選單列；切換到別的 App 後會自動切換；`⌘Q`、`⌘,`、About、Hide/Hide Others 的位置和行為符合一般 macOS App。
  - 已完成（交接）：
    - 選單只有一份描述：`src/menu.rs` 的 `MenuBar`／`Item`／`Shortcut`，由 `App::menu_spec()` 產生。macOS 由 `src/menu/macos.rs` 直接以 `objc2-app-kit` 建 `NSMenu` 並 `setMainMenu`；其他平台才畫 egui 視窗內選單（`App::menu_bar_panel` 在 macOS 是 no-op）。
    - 選單結構：`Lumiere`（About／Settings… `⌘,`／Services／Hide `⌘H`／Hide Others／Show All／Quit `⌘Q`）、`File`、`Edit`、`View`、`Go`、`Window`。Services、Hide、Quit 等由 `NSApplication` 自己實作；`Quit` 走自己的 dispatcher 讓 eframe 能收尾。
    - 點擊原生選單不會產生視窗事件，因此 dispatcher 會 `request_repaint()`，否則選單動作要等到下次滑鼠輸入才生效（這是實作時踩到的坑）。
    - 選單內容（含勾選狀態、enabled、標題）改變時會重建 `NSMenu`；`View ▸ Show Sidebar` 的 ✓ 已用 Accessibility API 驗證會即時更新。
    - 驗證方式：`System Events` 讀取 menu bar 結構與 `AXMenuItemMarkChar`，並實際以 `AXPress` 觸發 `Show Sidebar` 確認側欄消失。

- [x] **將 Windows 命名、選單分組與快捷鍵改為 macOS 慣例。**
  - 現況／依據：`src/app.rs:991-1184` 使用 `Exit`、`Properties…`、`Explore Folder…`、`Tools`、`Refresh (F5)`、`Delete (Del)`；`src/app.rs:855-927` 綁定 F2、F5、F11、Home/End、Tab 換資料夾等 Windows 習慣。
  - 實作方向：`Exit` 改為 `Quit Lumiere`；`Properties…` 改為 `Get Info`（`⌘I`）；`Explore Folder…` 改為 `Open…`／`Choose Folder…`；移除 `Tools`，將設定移到 App > `Settings…`；將重新讀取改為 `⌘R`，全螢幕僅用 `⌃⌘F`，重新命名改為 `Return`（選取檔案時）。不要顯示或主打 F-key 作為主要發現途徑。
  - 驗收：每一項選單顯示與實際可觸發的快捷鍵一致；`⌃⌘F` 切換全螢幕、`⌘R` 重新讀取、`Return` 重新命名、`⌘⌫` 移到垃圾桶；沒有 `Exit`、`Tools`、`F5`、`F11` 或 `Del` 的 Windows 文案殘留在主 UI。
  - 已完成（交接）：
    - `Action` 已改名為 macOS 語彙：`OpenFolder`、`GetInfo`、`Settings`、`MoveToTrash`、`Reload`、`ToggleSidebar`、`ToggleInspector`，新增 `ToggleReverse`，移除 `NextFolder`／`PrevFolder`（`fsmodel::sibling_folder` 一併刪除）。`Tools` 選單、`Exit`、`F5`、`F11`、裸 `Del`、`Tab`/`⇧Tab` 換資料夾都已移除。
    - 快捷鍵只有一個來源：`menu_spec()` 裡的 `Shortcut` 同時產生原生 key equivalent、選單顯示文字與 egui 鍵盤對應（`App::shortcuts()` 走訪選單描述）。測試 `menu_shortcuts_are_the_keyboard_map` 會驗證「選單顯示的按法」與「實際觸發的按法」相同。
    - 對照表：`⌘O` Open…、`⌘I` Get Info、`⌥⌘R` Rename…、`⌘⌫` Move to Trash、`⌘R` Reload、`⌘0` Actual Size、`⌘+`/`⌘-` Zoom、`⇧⌘F` Fit to Window、`⌃⌘S` Show Sidebar、`⌥⌘T` Show Toolbar、`⌥⌘I` Show Inspector、`⇧⌘U` Up One Level、`⇧⌘H` Home、`⇧⌘P` Pictures、`⇧⌘Y` Slideshow、`⌃⌘F` Full Screen、`⌘A`/`⇧⌘A`/`⌥⌘A` 選取、`⇧⌘L`/`⇧⌘R` 旋轉。
  - **決定（已與使用者確認）**：`Return` 保留給 `View Image`（與 Finder 的 Return＝開啟一致），重新命名用 `⌥⌘R`，`F2` 亦可用。這與規格「重新命名改為 `Return`」不同，但使用者確認保留目前行為，故不再視為偏離。`⌘R` 已讓給 `Reload`，所以旋轉用 `⇧⌘L`/`⇧⌘R`。
  - 另一個實作細節：AppKit 會先把 key equivalent 交給選單列，因此文字輸入框開啟時（重新命名等）會把 `⌘A`、`⌘⌫`、`Return` 從文字框搶走。`App::key_item()` 在有文字焦點時會暫時不掛這些 key equivalent（選單仍在，只是不顯示快捷鍵）。

- [x] **停止以手繪 Win95 theme 作為預設介面，改採 macOS 系統色彩、字型與動態外觀。**
  - 現況／依據：`src/classic.rs:1-4` 自述 Win95 外觀；`src/classic.rs:12-27` 固定灰／海軍藍色盤；`src/classic.rs:541-601` 強制淺色、直角、零陰影、13 px 字與硬式邊框；`src/app.rs:371-374` 每次啟動套用此 theme。
  - 實作方向：將 `classic.rs` 拆分為可淘汰的 legacy renderer 與新的 macOS visual layer；跟隨系統 Light/Dark mode、提高對比度與減少透明度設定。以 macOS system font（SF Pro／系統 fallback）與語意色彩取代固定 RGB；除非使用者選擇「Classic」外觀，勿預設使用 bevel、深藍選取底、方角按鈕。
  - 驗收：切換 macOS 明暗模式時，不重啟即可正確重繪；文字、分隔線、selection、disabled 狀態在兩種模式皆符合對比；預設畫面不再有 Win95 雙像素 bevel、深藍 title strip 或方形彈窗。
  - 已完成（交接）：
    - `src/classic.rs` 已整支刪除，改由 `src/theme.rs` 提供 macOS visual layer。使用者明確選擇「完整移除 Win95 層」，因此沒有保留可選的「Classic」外觀。
    - 顏色全部問目前這一幀的 `egui::Visuals`，不自訂色盤：`apply_theme` 只用 `ctx.all_styles_mut` 收緊間距與字型，顏色、圓角、陰影、selection tint 全留在 egui 預設值——它們是動態的，每幀依 OS 明暗挑選。移除舊 code 強制 `dark_mode = false` 的覆寫後，明暗切換就能即時生效（winit `ThemeChanged` → egui-winit 每幀重選 style），不需重啟。
    - 字型：`theme::install_fonts` 開機時由磁碟載入 `/System/Library/Fonts/SFNS.ttf`（比例）與 `SFNSMono.ttf`（等寬）並放最前，CJK fallback 用 `Arial Unicode.ttf` 放最後，讓中文檔名不會變方框。
    - 圖示：Unicode 字元覆蓋不足（已用 `ab_glyph`（egui 的解析器）驗證 SFNS 缺 ⌂ ▪ ⏮ ⏭ ⤢ ⛶ ⇄ ℹ 與 emoji），改由 `theme::draw_icon` 以單色線條繪製現代圖示，顏色由呼叫端傳入（通常 `ui.visuals().text_color()`），會跟隨明暗自適應；`Icon` 列舉與各呼叫端點不變。
    - 選取與文字：縮圖選取改採 `ui.visuals().selection.bg_fill/stroke`（不再有海軍藍），白底格子改透明讓 selection tint 透出；所有文字一律 `visuals.text_color()`（淺藍／深藍選取底都對比正確）；預覽區改成 6 px 圓角 `extreme_bg_color` 凹槽；browser 中央填色由硬編碼白改成 `panel_fill`。
    - viewer 浮動工具列改成半透明（`window_fill` alpha 235）+ `window_stroke` + `window_corner_radius` 的圓角面板，像原生媒體 HUD；寬度公式隨按鈕改成 `len*34+12`。
    - `canvas_color(app, ctx)` 改吃 ctx：`canvas_dark` 或全螢幕時黑，否則深色模式灰 `0x26`、淺色模式灰 `0x80`。
    - 文案：工具列「Full screen (F11)」→「Full screen」、「Delete」→「Move to Trash」、「Properties」→「Get Info」；About 視窗去掉「Classic」副標、F11 改成 `⌃⌘F`。
    - `theme::tool_button` 加上 `accesskit_node_builder` label，viewer／主工具列的圖示按鈕現在都有 accessible name（順帶推進 P0-6）。
    - headless 測試用的 control registry（`note_rect`／`rect_of`／`clear_rects`）移到 `theme.rs`，以 `#[cfg(test)]` 閹割，非測試建置 `note_rect` 是 no-op；點擊用的 label（「Macintosh HD」、`tree:{label}`、「Files filter」、「Apply」）全部保留。
    - egui 0.33 API 對接（本機只有 macOS target，跨平台路徑需人工檢查）：`CornerRadius.nw`（無 `.min`）、`Painter::rect` 多一個 `StrokeKind` 參數、`Align::LEFT`（無 `Align::Left`）、`Shape::Path(PathShape::closed_line)`（取代 `closed_polygon`）、`LayoutJob::single_section` 要 `String`、`Visuals` 非 `Copy`（`menu_row` 改用方法呼叫讀色，避免從 `Arc<Style>` move）。
    - 驗證：同一個 process 切 light→dark→light 皆即時重繪不重啟；browser（預設畫面）、viewer HUD、Options 對話框、About 視窗在明暗兩模式皆正常；`cargo fmt --check`、`cargo clippy --all-targets`、`cargo test`（47 passed）皆綠。

- [x] **以原生 Open/Save/選擇資料夾面板取代自製檔案瀏覽對話框。**
  - 現況／依據：`src/app.rs:1511-1551` 的 `Explore Folder` 是自繪 `egui::Window`；`src/app.rs:1657-1708` 的 Move/Copy 以路徑文字欄位輸入目的地。
  - 實作方向：使用 `NSOpenPanel` 選擇一或多個檔案／資料夾，使用原生 directory chooser 選擇移動或複製目的地；記住最近使用位置，並顯示 Finder 可理解的檔案名稱與位置。保留自繪 UI 僅作無原生能力平台的 fallback。
  - 驗收：選資料夾、移動、複製都可用 Finder 樣式的原生面板完成；使用者不必輸入 POSIX 路徑；取消不改變目前資料夾或選取項目。
  - 已完成（交接）：
    - 新增 `src/panels.rs`：`PanelResult`（`Chose(PathBuf)`／`Cancelled`／`Unavailable`）與 `choose_directory(title, prompt, start)`。macOS 走 `NSOpenPanel::openPanel`＋`runModal`（modal、blocking；`runModal` 自己跑 AppKit event loop，egui canvas 在面板期間只是凍結在後面，等同原生 modal 的行為，egui 端不需要特別處理）。
    - 依賴：`objc2-app-kit` 加 features `NSOpenPanel`／`NSPanel`／`NSSavePanel`／`NSWindow`（`NSResponder` 已有）；`objc2-foundation` 加 `NSURL`。
    - `title`／`prompt`／`directoryURL`／`runModal` 在 `NSSavePanel` superclass 上（`trait_upcasting` 是 nightly-only），用指標重新解释 upcast（`Retained::as_ptr(&panel).cast::<NSSavePanel>()`，同 `src/menu/macos.rs` 的模式）。
    - 測試安全門：`cfg!(test)` 直接回 `Unavailable`（test build 沒有 `NSApplication` run loop 可託 modal），加上 `MainThreadMarker::new()` 檢查。因此 headless 測試與非 macOS 都走 fallback：`explore_window`／`transfer_window` 完整保留，未改行為（現有測試 `app.transfer.is_some()`、「Explore」繪製檢查皆維持通過）。
    - `App::act`：`OpenFolder` → 原生面板選完即 `set_dir`；`MoveToFolder`／`CopyToFolder` → 新的 `begin_transfer(copy)`：`Chose(dest)` 立刻 `apply_transfer`（會 `rescan`＋通知）、`Cancelled` 什麼都不做、`Unavailable` 回落到舊的 `start_transfer` 對話框。選取路徑邏輯抽成 `transfer_paths()`（multi 選取，否則目前項目）供兩條路共用。
    - 每個 action 的面板文案：Open → title "Choose a Folder"、按鈕 "Choose"；Move → "Move to Folder"／"Move"；Copy → "Copy to Folder"／"Copy"。起始位置傳 `self.dir`（目前瀏覽的資料夾），符合 Finder 的 Open 慣例；「記住最近位置」以此實現（面板每次從目前資料夾開始）。
    - 驗證（live，深色模式）：Open 面板為真 NSOpenPanel（側欄／檔案清單／路徑 popup／New Folder／Cancel／Choose，跟隨系統深色）；**複製 end-to-end**：Copy to Folder 面板經路徑 popup 導航到 Pictures、按 Copy → `1.jpg` 複製成功、原檔未動、browser `rescan` 後格中出现新檔；**Open end-to-end**：Choose a Folder 導航到 Pictures、按 Choose → browser 切到 `/Users/james/Pictures`（title、address bar、格、樹皆更新）；**取消**：面板關閉後 app 正常 idle、目前資料夾與選取不變、無 hang（`sample` 確認阻塞在 `-[NSSavePanel runModal]`，取消後正常返回）。
    - 自動化操作注意：本環境下 `System Events` 的 `keystroke`／`key code` 送不到 modal 面板（Cmd+Shift+G、Cmd+↑ 皆無效，原因未明），但 **AXPress 可用**：面板在 AX 中是 Lumiere process 的 `AXDialog` 視窗，路徑 popup（`Where:` pop up button）、其 menu items、以及 New Folder/Cancel/Choose/Move/Copy 按鈕都可 `perform action "AXPress"`。側欄 row 的 AXPress 則不會觸發導航（要導航請用路徑 popup）。
    - 跨平台 fallback 路徑（非 macOS 的 `Unavailable` stub）只能人工檢查（本機只有 `aarch64-apple-darwin` target）。

- [x] **把刪除流程改成 macOS「移到垃圾桶」語言與標準 destructive alert。**
  - 現況／依據：`src/app.rs:1736-1766` 使用標題 `Confirm Delete` 與 `Yes`/`No`；README 指出實際是用 `trash` crate 移至 Trash，功能意義與介面文案不一致。
  - 實作方向：命令命名為 `Move to Trash`，快捷鍵為 `⌘⌫`；使用原生 alert 或等效 macOS alert，明確列出檔名與數量，主要破壞性按鈕標為「Move to Trash」，次要按鈕為「Cancel」，預設焦點為取消。多選時一次處理並顯示結果。
  - 驗收：刪除一張或多張照片時，使用者會看到「Move to Trash」而非 Delete/Yes/No；按 `⌘⌫` 能完成、Esc/Cancel 不會變更檔案；檔案可從 Finder 垃圾桶復原。
  - 已完成（交接）：
    - 新增 `src/alerts.rs`：`Confirmation`（`Confirmed`／`Cancelled`／`Unavailable`）與 `confirm_trash(paths)`。macOS 用真 `NSAlert`（`setAlertStyle`、`runModal`；modal、blocking，模式與 `panels.rs` 相同，egui canvas 在 alert 期間凍結在後面）。安全門與 `panels.rs` 一致：test build 直接回 `Unavailable`（沒有 `NSApplication` run loop 可託 modal），非主執行緒亦回 `Unavailable`；兩條 fallback 路徑都走內建確認對話框 `confirm_trash_window`。
    - 文案兩端共用：`trash_alert_text(paths)` 產生 messageText `Move to Trash`＋informative text——單項 `Move "name" to the Trash?`；多項 `Move N items to the Trash?` 加編碼列出檔名，超過 5 項截斷為 `…and N more`。`trash_names()` 取檔名（無檔名時回退完整路徑）。內建對話框（標題 `Move to Trash`、按鈕 `Move to Trash`／`Cancel`）复用同一份文字，原生與 fallback 不可能走字。
    - **預設焦點＝Cancel**：NSAlert 第一個加入的按鈕就是 default（Return），標題為 `Cancel` 的按鈕另外自動拿 Esc，所以「Cancel」先加、「Move to Trash」後加——鍵盤兩條路都只能取消，無法直接移到垃圾桶。AX 實測 Cancel 按鈕 `AXFocused=1`。**注意副作用**：視覺按鈕順序變成 [Move to Trash][Cancel]（窄 alert 時上下排列），這是「預設焦點為取消」的直接後果；若要慣例的 [Cancel][Move to Trash] 順序，就得把 default 換回移動鈕，與規格衝突。
    - alert 等級用 `NSAlertStyleWarning` 而非 `Critical`：AppKit 文件明載 caution 徽章「不應使用於目的只是移除資料的動作，例如存檔或清空垃圾桶」——垃圾桶可復原，屬可預期移除。
    - 多選：`Action::MoveToTrash` → `begin_trash(selection_paths())`（原 `transfer_paths` 改名，P0-4 的兩條 transfer 路徑一併改用）；multi 索引先排序，讓 alert 列出的順序＝grid 順序。右鍵選單是 item 層級（只傳被點的那一項），與該選單其他項（View／Get Info 皆單項語意）一致。右鍵選單 `Delete` 按鈕改名 `Move to Trash`，順帶把 `Properties` 對齊 `Get Info`（P1-4 只剩排序與 Share 等）。
    - `apply_trash(paths)`：一次批次以 `trash` crate 移動，失敗逐項記（檔名、error），成功才清選取＋`rescan`。結果 toast：單項成功 `Moved name to the Trash`、多項成功 `Moved N items to the Trash`、部分失敗 `Moved x of N items to the Trash: name — error`、全失敗 `Couldn't move N items to the Trash: name`。
    - 狀態欄位：`confirm_delete: Option<PathBuf>` → `confirm_trash: Option<Vec<PathBuf>>`（只由內建對話框使用；macOS 走原生 alert 時恆為 `None`）。Esc 仍關閉內建對話框（`escape_closes_open_dialogs` 已更新）。
    - 依賴：`objc2-app-kit` 加 features `NSAlert`、`NSButton`、`NSView`（`addButtonWithTitle` 需要後兩者）。
    - 測試：`trash_confirm_covers_the_whole_selection`（`⌘⌫` 納入整組 multi 選取、grid 順序）、`apply_trash_reports_the_result`（真的把 fixture 檔移進垃圾桶並驗成功／失敗文案；順帶證實 `trash` crate 在 headless test build 可用）、`alerts.rs` 三個純字串測試。52 passed，`cargo fmt --check`、`cargo clippy --all-targets` 全綠。
    - live 驗證（AX）：AXPress 觸發 File ▸ Move to Trash → NSAlert 顯示 `Move to Trash`／`Move "sub" to the Trash?`，按鈕 Move to Trash／Cancel，Cancel `AXFocused=1`；按 Cancel 後檔案原封不動、無殘留視窗；Select All＋Move to Trash → `Move 3 items to the Trash?`＋1./2./3. 列出三張圖；按 Move to Trash 後三檔離開目錄（進垃圾桶）、未選取的資料夾保留、grid 自動 rescan；主視窗 `⌘⌫`（key code 51＋Command）直接觸發。本環境 keystroke 送不到 modal alert（同 P0-4 的 NSOpenPanel 限制），default 鍵語義改以 `AXFocused` 屬性驗證。
    - 留給後面的（未動）：右鍵選單的 `Open with default app` 文案與整體排序（P1-4）；批次結果仍是單一 toast，失敗檔案清單＋Show in Finder／Retry 留給狀態回饋項（P1-6）。

- [x] **建立最小可用的原生 Accessibility 基礎。**
  - 現況／依據：README 的 Testing 段落明載「egui windows 沒有 native accessibility elements」；`src/browser.rs` 的縮圖、`src/viewer.rs` 的浮動工具列與 `src/theme.rs` 的 `draw_icon` 圖示皆由 canvas 繪製。
  - 實作方向：為所有 command 提供可被 macOS Accessibility API 揭露的名稱、role、value、enabled/selected state 與 keyboard focus；縮圖需有檔名、類型、尺寸與選取狀態；canvas 圖示不可成為唯一可操作入口。若框架無法支援，將核心導航、列表、按鈕和對話框改為原生控制項或補一層 NSAccessibility bridge。
  - 驗收：VoiceOver 可依序讀出工具列／縮圖／側欄／Inspector 的名稱與狀態，並可用鍵盤啟動；macOS Accessibility Inspector 可看到可辨識的 controls，而非單一 canvas；焦點環清楚可見。
  - 已完成（交接）：
    - 所有 command 都在原生選單列裡（P0-1／P0-2），名稱、快捷鍵、✓、enabled 全由 AppKit 揭露，不需要自己 bridge。egui 端（AccessKit 是 eframe 預設 feature）補齊 role／label／value／selected／expanded：
      - 縮圖格子（`src/browser.rs`）→ `Role::ListItem`（macOS 上呈為 `AXGroup`，item-like 所以暴露 `AXSelected`）＋`set_role_description("image"/"folder")`＋`set_label(檔名)`＋`set_value("PNG, 160 B")`／`"Folder"`＋`set_selected`。
      - 側欄樹列 → `Role::TreeItem`（macOS 上呈為 `AXRow`）＋`set_label(資料夾名)`＋`set_selected`＋有子資料夾時 `set_expanded`。
      - 工具列按鈕（`theme::tool_button`）→ `Role::Button`＋`set_label`（P0-3 已加 label，本次補 role）。
      - viewer canvas → `Role::Image`＋`set_label(檔名)`＋`set_value("64 x 48 px, 1804%")`；載入中／失敗時 value 為 `loading`／`failed to load`。
    - **焦點環**：`theme::focus_ring(ui, rect, radius)` — 2 px 描邊畫在 `rect.expand(2.0)`，顏色用 `visuals.hyperlink_color`（系統強調色，明暗模式皆適配，與 Finder 一致）。grid cell、tree row、tool_button 在 `has_focus()` 時畫。
    - **鍵盤模型**：格子與樹列改用 `ui.interact(rect, 穩定 id, Sense::click())` 取得焦點（點擊不聚焦，keyboard-only focus）。穩定 id：`grid_cell_id(index)`／`tree_row_id(index)`（`Id::new("lumiere grid cell"/"lumiere tree row").with(index)`），rescan／重排後焦點追蹤不失聯。
      - grid：有 cell 焦點（或完全沒焦點）時箭頭鍵移動 selected＋anchor＋multi 並同步 `request_focus` 到新格子；`Enter`（僅 cell 有焦點時）開啟選取項。
      - tree：僅有 row 焦點時活躍；Up/Down 移列（選取與焦點一起走）、Right/Left 展開／收合（先 `ensure_loaded` 再 `set_expanded`，與 expander 點擊行為一致）、`Enter` 開啟資料夾。
      - **工具列按鈕刻意不鎖箭頭鍵**：讓箭頭在相鄰按鈕間移動，這是原生 toolbar 的行為。
    - **egui 箭頭鍵劫持的解法（重要）**：egui `begin_pass` 會從箭頭鍵設定 `focus_direction`，`end_pass` 的方向搜尋（±45°）會**覆蓋**我們 mid-frame 的 `request_focus`。解法：鍵盤區塊 consume 箭頭後呼叫 `m.move_focus(egui::FocusDirection::None)`＋`m.request_focus(target)` 取消該幀劫持；同時對焦點 cell/row 每幀 `set_focus_lock_filter`（`EventFilter { horizontal_arrows, vertical_arrows: true }`）作雙保險。Tab 與 Escape 保持自由，可離開 pane 或丟掉焦點。
    - **`text_focused` 修正**（`src/app.rs`）：4 個 TextEdit（path bar、filter、rename、transfer）的 `response.id` 收錄到 `App.text_field_ids`（draw 開始時 clear），draw 結束時才算 `text_focused`＝焦點是否落在這些 id 上。舊的 `m.focused().is_some()` 會讓「焦點在 grid cell」時把選單的 key equivalents 撤掉；現在焦點在按鈕／cell／row 上時 `⌘A`、`Return` 等仍交給選單列。
    - viewer 的 `No image selected`／`Loading...`／`Failed to load image` 改為真實 `Label`（`status_label` helper：`ui.new_child`＋`Layout::centered_and_justified(TopDown)`＋`id_salt("viewer_status")`），狀態對輔助技術可見，不再是畫在 canvas 上的像素。
    - **測試**（headless，54 passed，`cargo fmt --check`、`cargo clippy --all-targets` 全綠）：
      - `thumbnails_are_readable_by_accessibility` 擴充：斷言 role（ListItem/TreeItem/Button）、role_description（image/folder）、value（"Folder"、`PNG, 160 B` 格式）、is_selected；Refresh 按鈕有 Click＋Focus actions。
      - 新增 `arrow_keys_and_enter_operate_the_focused_grid`：焦點環有／無斷言（掃 shapes 找環色描邊）、ArrowRight 後 selected 與 focus 同移到 cell 1、ArrowDown clamp 到最後一格、Enter 進 Viewer。
      - 新增 `arrow_keys_operate_the_folder_tree`：選一個有子資料夾的 root，ArrowRight 展開並載入 children、ArrowLeft 收合、ArrowDown 移列且 focus 跟隨。
    - **實機 AX 驗證**（遵循先前條目慣例：headless 不足夠，用 Swift AX C API 快照 walker 對 `target/release/lumiere` 實跑——AppleScript `entire contents` 會與 accesskit 每幀重建競態）：
      - 縮圖格 = `AXGroup`＋value（"PNG, 160 B"/"Folder"）＋`AXSelected`；側欄 9 個 `AXRow` 帶資料夾名；35 個具名 `AXButton`；2 個 `AXTextField`。
      - accesskit 的 Focus action 可設 `AXFocused`；`AXPress` 格子 → `AXSelected=true`。
      - grid 鍵盤：ArrowRight 選取＋焦點同步移動；ArrowDown clamp 到最後一格；Enter 開 Viewer（canvas 呈為 `AXImage`，label="two.png"、value="64 x 48 px, 1804%"，12 個具名 HUD 按鈕）；Escape 回瀏覽器。
      - tree 鍵盤：Focus "Documents" `AXRow` → `AXFocused` 落在 row 上；ArrowDown 焦點＋選取移到 "Downloads"（`AXSelected=true`）；ArrowRight 展開並載入子資料夾（7 個新 row）；ArrowLeft 收合。
      - 環境筆記：process 名小寫 `lumiere`；`System Events key code` 對主視窗有效（對 modal panel 無效，同 P0-4）；`CGEvent` 直接 post key 在本環境送不進去，用 `osascript -e 'tell application "System Events" to key code N'`。
    - **已知邊角案例**：
      - `set_focus_lock_filter` 要求 `had_focus_last_frame`，所以鎖鍵從焦點的第二幀起才生效；首幀的箭頭已由 mid-frame 的 `move_focus(None)` 補平，實測無漂移。
      - 未鎖箭頭的 non-pane widget（如樹列 expander 小按鈕）若拿到焦點，按箭頭會走 egui 的方向搜尋；Tab／Escape 可恢復。屬可接受的最小基礎範圍，若要更嚴密可在每個 widget 統一鎖鍵。
    - **留給使用者的手動走查**（工具無法代勞的部分）：開 VoiceOver 完整依序讀一遍工具列→側欄→縮圖→Inspector，確認讀法與跳動順手；用 macOS Accessibility Inspector 視覺確認控件樹與焦點環外觀。
  - 順帶修掉一個會讓人以為 App 沒啟動的問題：`state.ini` 記的視窗位置若在已拔掉的螢幕上，視窗會恢复到螢幕外。`src/window.rs` 現在用 `NSScreen` 檢查位置是否還在任何顯示器內，不在就交給系統決定位置。

## P1 — 讓瀏覽與看圖像一個現代 macOS App

- [ ] **重新設計工具列為 macOS toolbar，而不是兩排密集的像素圖示。**
  - 現況／依據：`src/app.rs:1193-1280` 同時放置資料夾、縮圖尺寸、導覽、播放、縮放、旋轉、刪除等大量操作；`src/classic.rs:66-93` 將其固定成 30×28 px 手繪按鈕；`src/viewer.rs:188-233` 再覆蓋一條 12 個按鈕的浮動工具列。
  - 實作方向：採原生 toolbar 置於標題列／標準位置，使用 SF Symbols、可存取 label 與 tooltip。只保留常用的 Back/Forward、Share、Info、Sidebar、縮放或播放；將旋轉／翻轉／排序等次要操作放入選單或 overflow。支援使用者自訂與工具列顯示偏好（若框架限制，至少做精簡、分組和標準間距）。
  - 驗收：視窗寬度 800 px 時工具列不會擠壓、截斷或覆蓋照片；每個圖示都具可辨識 tooltip／accessibility label；主瀏覽器與 viewer 不會各自顯示一整排重複導航按鈕。
  - 進度（2026-10-11，經使用者指示採「精簡 in-window」方向，未做真 native NSToolbar）：browser 工具列（`tool_row`）精簡為 11 個瀏覽按鈕，依功能分組（導覽 Up/Home/Refresh｜縮圖尺寸｜圖片導覽 First/Prev/Next/Last｜Slideshow｜Move to Trash＋Get Info）；移除 9 個 viewer 專用鈕（zoom in/out、fit、actual size、rotate 左右、flip H/V、Stop slideshow、Full screen）——全部仍經 Edit/View/Go 選單與快捷鍵可達，viewer 浮動 HUD 保留完整看圖鈕（兩者不會同框顯示，重複導航問題解決）。`Slideshow`（需有圖片）與 `Move to Trash`（需有選取）的 enabled 狀態改與選單項一致。headless 測試 `browser_toolbar_keeps_only_browser_controls` 驗證 11 鈕存在、9 鈕消失、整列 800 px 窗口容得下、選單路徑仍在。剩餘（未做）：真 native NSToolbar／SF Symbols、工具列使用者自訂與顯示偏好。

- [ ] **以 sidebar + content + inspector 的 macOS 資訊架構取代固定三窗格與「Places」條。**
  - 現況／依據：`src/app.rs:1858-1895` 固定組合 menu、toolbar、places、status、folder tree、preview pane；`src/app.rs:1285-1369` 另有可編輯 path 和 wildcard filter；`src/browser.rs:15-104` 使用自繪 Windows 式展開符號與樹狀目錄。
  - 實作方向：左側用可收合的 sidebar（Favorites/Locations/最近位置），中間為內容網格，右側採可收合 Inspector；以標題列／路徑 control 呈現目前位置。將技術性的 wildcard filter 移入明確的搜尋或 Filter popover，不要常駐為類 DOS 檔案篩選欄。
  - 驗收：使用者第一次打開就能從側邊欄找到 Pictures、Home 與已加入的資料夾；sidebar/Inspector 可用工具列或 View 選單切換且狀態會保存；窄視窗時內容區仍優先可用。
  - 進度（2026-10-11）：完成。Windows 式 folder tree（`TreeModel`，`fsmodel.rs` 整段移除）與「Places」條皆移除，改為 flat 三段式 sidebar（`browser.rs::sidebar_pane`）：
    1. **Favorites**：Home/Desktop/Documents/Downloads/Pictures/Movies（僅列出存在的目錄）。
    2. **Locations**：Macintosh HD（`/`）＋使用者 pin 的資料夾（`sidebar_dirs`）。
    3. **Recents**：`recent_dirs`，`set_dir` 自動記錄、newest first、上限 5、不重複、渲染時過濾已不存在者。
    - 高亮列以 `visuals.selection.bg_fill` 底紋畫出（整列 hit target，同 grid cell 的 `allocate_exact_size`＋`ui.interact` 模式）。**同一時間最多一列點亮**：`sidebar_pane` 以讀取順序（Favorites→Locations→Recents）找第一個路徑**精確等於**目前目錄的列（`active_row`）——不用祖先匹配（所有路徑都 `starts_with("/")`，Macintosh HD 會永遠反白；Finder 也是只亮精確位置），同一個資料夾重複出現在多個 section（如 Home 同時在 Favorites 與 Recents）時只有較前 section 的列亮。
    - **工具列**：最前新增 `Show Sidebar` toggle（`theme::tool_button_toggle`，`Icon::Sidebar`）；View 選單原 Show Sidebar/Show Inspector 不變。
    - **Filter popover**：address 列常駐 wildcard 欄改為 `Filter…` 按鈕（顯示目前 pattern）＋`egui::Area`（id `lumiere filter popover`，`Order::Foreground`）；欄位 lost_focus／Apply／Clear 皆 commit pattern，Esc／Apply／Clear 關閉。
    - **Add/Remove from Sidebar**：grid 資料夾 cell 右鍵選單（Reveal in Finder 之後）；pinned 的 Locations 列自身右鍵選單可移除。
    - **窄視窗**：viewport 寬 < 700 px 時兩側面板都不繪製（內容優先，`ctx.input(|i| i.viewport_rect().width())`），on/off 狀態保留、變寬自動回來；未知寬度視為寬。
    - **狀態保存**：`state.ini` 新增 `show_sidebar`/`show_inspector`（-1=未存）、`sidebar_dir_N`/`recent_dir_N`（上限 64）；toggle 與 pin/unpin 皆標 `state_dirty`。
  - 修復（2026-10-11，使用者回報）：高亮原本用 `starts_with` 祖先匹配，導致 Macintosh HD 常亮、多列同時反白；改為單一 `active_row` 精確匹配（見上）。新增 `sidebar_lights_at_most_one_row`：fixture 目錄 0 列亮、Home 在 Favorites＋Recents 重複時只亮 1 列、進入 Home 子資料夾時祖先不亮（headless 數左面板 clip 內的 5px 圓角 selection 底紋 shape）。
  - 測試：`arrow_keys_operate_the_sidebar`（平鋪列鍵盤導覽）、`sidebar_navigates_to_the_selected_place`、`clicking_a_sidebar_folder_navigates_to_it`（含 Recents 列導覽）、`files_filter_applies_from_the_filter_popover`、`narrow_window_keeps_the_content_pane_usable`（600 px 兩面板消失、toggle 狀態保留、變寬回來）、`context_menu_pins_a_folder_to_the_sidebar`（grid pin → sidebar 列右鍵 unpin → 再 pin 後列可導覽）、`recents_track_visits_newest_first`、`sidebar_and_inspector_state_survive_a_restart`；`browser_toolbar_keeps_only_browser_controls` 改為 14 鈕；`preview_pane_holds_the_width_the_user_set` 改測 `sidebar` 面板（預設 200 px）。
  - 實機驗證（AX＋合成事件）：sidebar 三段渲染、左鍵列導覽、工具列 toggle、Filter popover（欄位＋Apply/Clear）、grid 右鍵 Add to Sidebar、pinned 列右鍵 Remove from Sidebar、pin/recents/可見性跨重啟恢復、600 px 窄窗兩面板自動讓位且放大回來。
  - 踩坑記錄：egui 0.33.3 的 `Frame::show` 包層會讓行內 `ui.interact` 在**實體視窗**收不到 pointer 事件（headless 卻正常，故以 grid cell 的 flat 模式取代 Frame 底紋）；popup 內容的 `ui.close()` 若無條件呼叫會在第一幀關掉剛打開的選單。
  - 剩餘（P2）：面板寬度跨重啟保存（目前只存 on/off）。

- [ ] **改進看圖手勢、縮放與全螢幕行為以適配觸控板。**
  - 現況／依據：`src/viewer.rs:61-87` 把任何 raw scroll 當縮放；拖曳才平移；`src/app.rs:732-751` 開始 slideshow 時強制進全螢幕，且全螢幕可由 F11/`⌘F` 觸發。
  - 實作方向：兩指捲動為平移／切換（依模式），pinch 才縮放且以游標／手勢中心縮放；支援雙擊在 Fit/100% 間切換、Space 暫時平移、左右滑切換圖片並避免與系統返回手勢衝突。全螢幕只使用 `⌃⌘F`；幻燈片可由使用者選擇是否自動進入全螢幕，退出時恢復先前縮放與視窗狀態。
  - 驗收：在 MacBook trackpad 上可 pinch 縮放、兩指平移已放大的影像、左右瀏覽不誤觸縮放；全螢幕遵循 `⌃⌘F`；開始／停止幻燈片不會意外改變非全螢幕使用者的工作情境。
  - 進度（2026-10-11）：完成。
    - **手勢**（`viewer.rs` 互動區重寫）：pinch 經 `egui::Event::Zoom`（egui-winit 從 winit `PinchGesture` 轉來，`delta.exp()` 為倍率）縮放，以游標（＝手勢中心，winit 在 magnify 事件同步更新 pointer）為錨點、保持指下點不動；兩指捲動（`raw_scroll_delta`）**不再縮放**——圖 overflow 時 1:1 平移，fit 時水平累積 ≥120 pt 才換圖（**gesture lock**：換完整段手勢鎖定，同向輸入（含慣性）一律丟棄，不管滑多遠都只換一張；只有 >150 ms 間隔（trackpad 事件間距 8–16 ms 的 10 倍，同一次手勢不會被拆開；又短到快速連滑的 100–300 ms 間隔夠得到）或反向滿 120 pt 的滑動才重新起算）；雙擊改為 **Fit ⇄ 100%**（不再是「最後縮放」）；Space 按住顯示 grab/grabbing 游標（手形提示；拖曳平移本就常開，Space 鍵本身仍是下一張）。縮放上下限收攏為 `viewer::ZOOM_MIN/MAX`（0.02–64×），HUD 按鈕與手勢共用。
    - **全螢幕**：只有 `⌃⌘F`（View ▸ Enter/Exit Full Screen）與幻燈片選項會改視窗狀態。`Action::Slideshow` 開始前快照 `SlideshowView{fullscreen,fit,zoom,pan}`；Options ▸ Slideshow 新增「Enter full screen automatically」（預設 on，存 `state.ini` 的 `slideshow_fullscreen`）；`StopSlideshow`／幻燈片中 `CloseViewer` 用 `restore_slideshow_view` 還原快照；無幻燈片時 `CloseViewer` 不再強退全螢幕（⌃⌘F 的視窗狀態是使用者自己的）。
    - 測試：`pinch_gesture_zooms_about_the_pointer`（中心 pinch 不移 pan、偏心 pinch 的錨點數學、fit 起縮）、`scroll_pans_an_overflowing_image`（1:1 平移、不縮放不換圖）、`horizontal_swipe_browses_when_the_picture_fits`（半程不換、跨門檻換一張並鎖定、同向再遠（600 px）不換第二張、反向滿程換回、huge 起手也只換一張、垂直捲無效）、`double_click_toggles_fit_and_actual_size`、`slideshow_preserves_the_window_and_zoom_state`（opt-out 不進全螢幕、停止還原 zoom/pan/window、全螢幕起全螢幕止）、`closing_the_viewer_during_a_slideshow_restores_the_window`（幻燈片全螢幕隨幻燈片還原、⌃⌘F 的往返保留）；`saved_preferences_are_restored` 加 `slideshow_fullscreen`。新增測試 helper `double_click_at`/`scroll_frame`/`wait_for_full_image`。
    - 實機驗證（合成事件）：雙擊 37%⇄100%、⌘+/⌘- 46%/37%、fit 下垂直捲不縮放不換圖、100% overflow 下水平捲只平移、**fit 下一次 20×150 px（3000 px）連續長滑只換一張**（one→three；舊 350 ms cooldown 會換 2–3 張，已改 gesture lock）、快速連滑（間隔 250 ms）兩筆都換（one→three→two）、反向滑回（two→three）、⇧⌘Y 進 1470×923 全螢幕、Esc 停止後視窗 1200×620＋zoom 還原 37%、⌃⌘F 全螢幕往返、Options 勾掉後 ⇧⌘Y 留在視窗模式。
    - 踩坑：pinch 無法合成（macOS `NSEventTypeMagnify` 沒有公開 CGEvent 建法），pinch 路徑以 headless `Event::Zoom` 測試＋egui-winit 來源碼（`PinchGesture → Event::Zoom(delta.exp())`、magnify 事件同步 `mouse_motion`）為準；合成 pixel-unit scroll（`CGEvent(scrollWheelEvent2Source:units:.pixel,…)`）可當兩指捲動；非 key window 時第一下點擊會被 macOS 拿去 focus（雙擊驗證前要先 `set frontmost`）。
    - Space 暫時平移以游標提示實作：拖曳平移本來就常開，Space 鍵在本 app 已有「下一張」綁定（About 說明＋測試），未改綁定。

- [ ] **加入分享與 Finder/Quick Look 工作流。**
  - 現況／依據：`src/app.rs:603-610` 的 Open 僅以 `open <path>` 叫起預設 App；內容選單在 `src/browser.rs:375-404` 只有 View、Open with default app、Properties、Delete。
  - 實作方向：加入 Share（`NSSharingServicePicker`）、Reveal in Finder、Quick Look（Space）與 Open With…；右鍵選單依 macOS 用語排序，非破壞性動作先於 Move to Trash。支援拖入檔案／資料夾開啟，並可把選取的檔案拖出至 Finder／其他 App。
  - 驗收：選取圖片可用 Share 成功傳送到至少一個系統分享目標；`Space` 顯示 Quick Look、Reveal in Finder 開啟並選取該檔；拖放檔案和資料夾能正確開啟或瀏覽。
  - 進度（2026-10-11）：五項工作流完成；**Quick Look（Space）經使用者指示裁掉**，驗收項「Space 顯示 Quick Look」不適用。
    1. **Share**：`NSSharingServicePicker`（`panels::share_paths`，items 為 NSURL），右鍵選單 Share（圖片與資料夾皆可）。
    2. **Reveal in Finder**：`NSWorkspace.activateFileViewerSelectingURLs`（`panels::reveal_in_finder`，notice 的 Show in Finder 同路徑），右鍵選單 Reveal in Finder。
    3. **Open With…**：`panels::apps_to_open`（LaunchServices 註冊查詢）＋list dialog＋`panels::open_with`，右鍵選單（圖片專屬，「Open with default app」之下）。
    4. **拖入**：eframe 端 `DroppedFile` 取不到可用路徑，改由 `dnd::DropWatch` 狀態機每幀取樣（pasteboard changeCount 上升＋`CGEventSource` 實體 button state＋游標在視窗內），release 時以 `pasteboard_file_paths()` 開啟檔案／資料夾。
    5. **拖出**：input-based gesture（cell 上 press＋6 px 門檻）→`dnd::begin_file_drag`（`NSDraggingSession`＋`public.file-url` payload，preview 傳 cell 尺寸以避免零尺寸 dragging frame 的 NSRangeException）；layout 期間 `entries` 被 take，payload 以 `pending_drag` 延至 `pump_dnd` 發射；session 事件優先用 `NSApplication.currentEvent()`（拖曳中為真實事件），無則合成；`drag_out_active` 期間 DropWatch 停止 tick，防把自家 session 的 drop 再 ingest 一次。
  - 右鍵選單重排為 macOS 順序：View/Explore → Share → Reveal in Finder → Open with default app → Open With… → Get Info →（separator）→ Move to Trash（非破壞性在前、trash 以分隔線獨立）。
  - 依賴：objc2 0.6.5、objc2-app-kit 0.3.2、objc2-core-graphics 0.3.2。
  - 測試：`dropped_paths_browse_folders_and_open_images`（拖入）、`drag_out_gesture_carries_the_cell_file`（拖出 payload）、`open_with_dialog_lists_apps_and_opens_in_one`、`right_click_menu_lists_the_macos_workflow_in_order`、`dnd::tests`（DropWatch：release 認 drop、窗外 release 取消、idle change 非拖曳、payload 替換取消舊 drag、無 pasteboard 不 drop）。
  - 實機驗證：右鍵選單順序、Open With 對話框（選 Preview 開啟）、Share picker（含郵件目標）、Reveal in Finder 開啟並選取該檔、拖出 one.png／three.png 均正確 copy 進 Finder 視窗。

- [ ] **讓 Inspector/資訊面板與縮圖選取狀態更清楚且不依賴深藍色。**
  - 現況／依據：`src/browser.rs:282-374` 將選取縮圖整塊填為 `classic::NAVY`，檔名固定 11 px；`src/browser.rs:425-` 以兩個裸文字 tab 呈現 Preview / Information；`src/classic.rs:120-133` 使用深藍色 section label。
  - 實作方向：採 macOS selection tint 與 sidebar/list selection 規則；縮圖保留適當留白、支援多行檔名或省略號與完整名稱 tooltip；Inspector 使用標準 segmented control/tab 樣式，內容採 key/value 對齊、可複製的 metadata 與明確 loading/error state。
  - 驗收：明暗模式、Increase Contrast 下仍能一眼辨識選取項；長檔名不會只靠 11 px 文字難以識別，至少可懸停取得完整名稱；Inspector 的焦點、選擇與讀取狀態清楚。
  - 進度（部分完成，經使用者指示）：Preview／Information 兩個 tab 已合併——**Information 整個移除**，其多出來的欄位（Type、Path、Dimensions、Format、EXIF）直接併入 Preview 面板下方（`src/browser.rs` 的 `preview_pane`，去掉了 tab row 與 `inspector_tab` 狀態欄位；`Get Info`／右鍵選單只負責打開面板）。因此實作方向裡的「segmented control/tab 樣式」已無對象；剩餘：selection tint、長檔名省略號／tooltip、metadata 可複製、loading/error state（現況 `Dimensions: loading...` 在完整解碼前恆顯示，路徑等長值無省略號，皆沿用舊樣式未改）。
  - 進度（2026-10-11，經使用者指示）：工具列 `Get Info` 按鈕改為 Inspector（Preview）面板的顯示／隱藏 toggle——推 `Action::ToggleInspector`（取代原「只打開」的 `GetInfo`，並移除殘留的死變數 `open_dialog`）；`theme::tool_button` 拆出 `tool_button_toggle`，active 時以 `visuals.selection.bg_fill` 常亮底色，面板開關狀態一眼可見（明暗自適應）。選單 `⌘I` Get Info（打開）、`⌥⌘I` Show Inspector（toggle）與右鍵選單 `Get Info`（打開）行為不變。headless 測試 `get_info_button_toggles_the_preview_pane` 驗證隱藏後 metadata 不再繪製、再點回來。

- [ ] **建立狀態、錯誤與進度回饋的 macOS 語言。**
  - 現況／依據：`src/app.rs:1372-1412` 使用三格 Win95 status field；`src/app.rs:1710-1732`、`src/app.rs:1750-1755` 只產生短暫 toast／字串錯誤；`src/viewer.rs:39-47` 僅畫出 `Loading…`。
  - 實作方向：將常駐 status bar 改為需要時才出現的 inline progress／non-modal notification；批次操作提供可理解的進度、失敗檔案與「Show in Finder」/「Retry」等下一步。避免以固定 12 px 三格資訊承載核心狀態。
  - 驗收：載入大型圖片、移動／複製多檔與解碼失敗時，使用者能知道正在處理什麼、是否完成、哪些檔案失敗並能採取下一步；一般瀏覽時畫面沒有過時的常駐三格狀態列。
  - 進度（2026-10-11）：常駐三格狀態列改為**預設關閉**（View ▸ Show Status Bar 仍可開啟，開啟時只顯示資料夾事實）；所有 `notify` 訊息改為**浮動 notice card**（`app.rs::notice_card`，底部置中、non-modal、普通訊息 4 秒自動消失、可 Dismiss）。move/copy/trash 批次操作改在 **worker thread** 執行（`start_job`/`pump_job`，沿用 loader 的 channel 模式）：執行中卡片顯示「Moving 2 of 4 — sunset.jpg」進度，完成後顯示結果。失敗**逐檔列名與原因**（`failure_detail`，超過 3 項顯示 and N more），並提供「**Show in Finder**」（`panels::reveal_in_finder`，macOS `NSWorkspace.activateFileViewerSelectingURLs`）與「**Retry**」（只重跑失敗項，`NoticeAction::Retry(FileOp)`）兩個下一步。viewer 的 `Loading…`／解碼失敗改為含檔名（`Loading one.png…`）。headless 測試：`failed_transfer_offers_retry_and_reveal`（失敗→點 Retry→worker 完成→成功卡）、`batch_transfer_reports_each_failure`（半成功逐檔報備、retry 只帶失敗項）、`notice_card_is_painted_and_dismissible`（卡片繪出＋Dismiss）；`apply_transfer`/`apply_trash` 保留為 `#[cfg(test)]` 同步執行器供測試直調。剩餘（未做）：長操作的 cancel、notice 的選單/鍵盤可達性、跨重啟保留。

## P2 — 一致性、偏好與品質收尾

- [ ] **將偏好改為 macOS Settings，並保存所有使用者可見的視圖狀態。**
  - 現況／依據：`src/app.rs:1415-1473` 是自繪 `Options`；`src/state.rs:8-113` 以 `state.ini` 保存少數欄位，未保存 sidebar、preview、toolbar、status、places、zoom、canvas appearance 等；`src/app.rs:1981-1996` 也只還原部分設定。
  - 實作方向：名稱改為 `Settings…`，以分頁/側邊欄分組（General、Viewing、Slideshow、Advanced），將可恢復的 UI 狀態集中在版本化設定模型；遵從系統偏好，並提供「Restore Defaults」。`Clear thumbnail cache` 應置於 Advanced 並有容量與確認提示。
  - 驗收：調整縮圖大小、排序、側欄／Inspector 可見性、檢視模式後重啟仍一致；Settings 可由 `⌘,` 開啟；清除快取前能看到影響並可取消。

- [ ] **釐清非破壞性檢視操作與真正檔案編輯的界線。**
  - 現況／依據：`src/app.rs:714-727` 的旋轉與翻轉只修改記憶體中的 viewer state；但 `src/app.rs:1054-1063` 把它們放在 Edit，沒有說明不會寫入照片。
  - 實作方向：若僅為檢視，命名為 `Rotate View`／`Flip View`，在 Inspector 或 toast 表明「未修改原始檔」；若要支援儲存，另設 `Save`／`Export…`、格式／覆寫處理與明確 undo 策略，不能暗中覆蓋。
  - 驗收：使用者旋轉後能明確知道是否修改原檔；重新開啟檔案的結果與文案一致；任何可能覆寫都會先確認並提供安全的輸出路徑。

- [ ] **補齊本地化、Dynamic Type/縮放與鍵盤焦點測試。**
  - 現況／依據：所有主要 UI 文案硬編碼為英文，例如 `src/app.rs:991-1184`、`src/browser.rs:380-403`、`src/viewer.rs:17-47`；`src/classic.rs:591-601` 大量固定字級；README 只提 CJK fallback font，未處理 macOS 系統字型與 accessibility text size。
  - 實作方向：建立 strings catalog/集中式本地化資源，至少提供英文與繁體中文；採用系統字體與語意字級，避免把中文字型直接塞進 monospace family。測試鍵盤 Tab order、VoiceOver、不同 display scale、Reduce Motion／Increase Contrast、長字串與 CJK 檔名。
  - 驗收：切換支援語言後選單、對話框、錯誤訊息和 tooltip 都翻譯一致且無截斷；在放大字級／高對比／深色模式下核心流程仍可完成；全程可不使用滑鼠瀏覽、開啟、取得資訊、重新命名與移到垃圾桶。

## 建議實作順序與驗收門檻

1. 先完成 P0 的原生選單、原生檔案／刪除對話框、快捷鍵與 accessibility 基礎；這些是 macOS 使用者最先碰到的行為契約。
2. 再移除預設 Win95 presentation，改造 toolbar、sidebar/Inspector 與手勢（P1）。不要只替換顏色而保留舊的密集控制結構。
3. 最後整理 Settings、持久化、本地化與非破壞性編輯語意（P2）。

合併前至少以乾淨帳號手動驗收：Light/Dark mode、trackpad、純鍵盤、VoiceOver、繁中檔名、Finder 拖放、原生 Open panel、`⌘⌫` 移到垃圾桶、`⌃⌘F` 全螢幕與 `⌘Q` 離開。現有的 headless egui 測試可保留作 command regression，但不能作為 macOS accessibility 或原生 menu/dialog 的唯一驗證。
