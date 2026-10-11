# Lumiere macOS UI/UX 改善待辦

> 檢視範圍：本專案（Rust／eframe／egui）。
>
> 本文件是實作規格，不包含應用程式碼修改。專案目前刻意使用 Win95 風格（README 與 `src/classic.rs`），下列項目以 macOS 使用者的既有心智模型為目標，優先處理會造成操作錯誤、找不到功能或無法使用輔助技術的問題。

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
  - **待確認的偏離**：規格寫「重新命名改為 `Return`」，但 `Return` 目前保留給 `View Image`（與 Finder 的 Return＝開啟一致），重新命名用 `⌥⌘R`，且 `F2` 仍然可用。`⌘R` 已讓給 `Reload`，所以旋轉改用 `⇧⌘L`/`⇧⌘R`。若要照規格把 `Return` 給 Rename，只要改 `menu_spec()` 裡 `View Image`／`Rename…` 兩項的 `Shortcut`。
  - 另一個實作細節：AppKit 會先把 key equivalent 交給選單列，因此文字輸入框開啟時（重新命名等）會把 `⌘A`、`⌘⌫`、`Return` 從文字框搶走。`App::key_item()` 在有文字焦點時會暫時不掛這些 key equivalent（選單仍在，只是不顯示快捷鍵）。

- [ ] **停止以手繪 Win95 theme 作為預設介面，改採 macOS 系統色彩、字型與動態外觀。**
  - 現況／依據：`src/classic.rs:1-4` 自述 Win95 外觀；`src/classic.rs:12-27` 固定灰／海軍藍色盤；`src/classic.rs:541-601` 強制淺色、直角、零陰影、13 px 字與硬式邊框；`src/app.rs:371-374` 每次啟動套用此 theme。
  - 實作方向：將 `classic.rs` 拆分為可淘汰的 legacy renderer 與新的 macOS visual layer；跟隨系統 Light/Dark mode、提高對比度與減少透明度設定。以 macOS system font（SF Pro／系統 fallback）與語意色彩取代固定 RGB；除非使用者選擇「Classic」外觀，勿預設使用 bevel、深藍選取底、方角按鈕。
  - 驗收：切換 macOS 明暗模式時，不重啟即可正確重繪；文字、分隔線、selection、disabled 狀態在兩種模式皆符合對比；預設畫面不再有 Win95 雙像素 bevel、深藍 title strip 或方形彈窗。

- [ ] **以原生 Open/Save/選擇資料夾面板取代自製檔案瀏覽對話框。**
  - 現況／依據：`src/app.rs:1511-1551` 的 `Explore Folder` 是自繪 `egui::Window`；`src/app.rs:1657-1708` 的 Move/Copy 以路徑文字欄位輸入目的地。
  - 實作方向：使用 `NSOpenPanel` 選擇一或多個檔案／資料夾，使用原生 directory chooser 選擇移動或複製目的地；記住最近使用位置，並顯示 Finder 可理解的檔案名稱與位置。保留自繪 UI 僅作無原生能力平台的 fallback。
  - 驗收：選資料夾、移動、複製都可用 Finder 樣式的原生面板完成；使用者不必輸入 POSIX 路徑；取消不改變目前資料夾或選取項目。

- [ ] **把刪除流程改成 macOS「移到垃圾桶」語言與標準 destructive alert。**
  - 現況／依據：`src/app.rs:1736-1766` 使用標題 `Confirm Delete` 與 `Yes`/`No`；README 指出實際是用 `trash` crate 移至 Trash，功能意義與介面文案不一致。
  - 實作方向：命令命名為 `Move to Trash`，快捷鍵為 `⌘⌫`；使用原生 alert 或等效 macOS alert，明確列出檔名與數量，主要破壞性按鈕標為「Move to Trash」，次要按鈕為「Cancel」，預設焦點為取消。多選時一次處理並顯示結果。
  - 驗收：刪除一張或多張照片時，使用者會看到「Move to Trash」而非 Delete/Yes/No；按 `⌘⌫` 能完成、Esc/Cancel 不會變更檔案；檔案可從 Finder 垃圾桶復原。

- [ ] **建立最小可用的原生 Accessibility 基礎。**
  - 現況／依據：README 的 Testing 段落明載「egui windows 沒有 native accessibility elements」；`src/browser.rs:282-423` 的縮圖、`src/viewer.rs:188-233` 的浮動工具列與 `src/classic.rs:175-526` 的圖示皆由 canvas 繪製。
  - 實作方向：為所有 command 提供可被 macOS Accessibility API 揭露的名稱、role、value、enabled/selected state 與 keyboard focus；縮圖需有檔名、類型、尺寸與選取狀態；canvas 圖示不可成為唯一可操作入口。若框架無法支援，將核心導航、列表、按鈕和對話框改為原生控制項或補一層 NSAccessibility bridge。
  - 驗收：VoiceOver 可依序讀出工具列／縮圖／側欄／Inspector 的名稱與狀態，並可用鍵盤啟動；macOS Accessibility Inspector 可看到可辨識的 controls，而非單一 canvas；焦點環清楚可見。
  - 進度（尚未驗收完）：
    - 所有 command 現在都在原生選單列裡，因此名稱、快捷鍵、✓ 狀態、enabled 都由 AppKit 揭露，不需要自己 bridge；這是最主要的一塊。
    - egui 端（AccessKit 是 eframe 的預設 feature）已補上標籤：縮圖格子上報「檔名, selected/item」（`src/browser.rs`）、側欄樹節點上報資料夾名稱、視窗內選單項目的 accessible name 只唸命令名稱，快捷鍵改放 `AXKeyboardShortcut`（`src/app.rs` 的 `menu_items`）。
    - 測試 `thumbnails_are_readable_by_accessibility` 會檢查 AccessKit tree 是否包含各檔名。
    - **仍缺**：焦点環（keyboard focus ring）沒有視覺呈現，viewer 的浮動工具列圖示仍只有 tooltip 而無 accessible name，`role`/`value`/`selected` 大部分未設定。需要以 VoiceOver 與 Accessibility Inspector 手動驗收後才能標記完成。
  - 順帶修掉一個會讓人以為 App 沒啟動的問題：`state.ini` 記的視窗位置若在已拔掉的螢幕上，視窗會恢复到螢幕外。`src/window.rs` 現在用 `NSScreen` 檢查位置是否還在任何顯示器內，不在就交給系統決定位置。

## P1 — 讓瀏覽與看圖像一個現代 macOS App

- [ ] **重新設計工具列為 macOS toolbar，而不是兩排密集的像素圖示。**
  - 現況／依據：`src/app.rs:1193-1280` 同時放置資料夾、縮圖尺寸、導覽、播放、縮放、旋轉、刪除等大量操作；`src/classic.rs:66-93` 將其固定成 30×28 px 手繪按鈕；`src/viewer.rs:188-233` 再覆蓋一條 12 個按鈕的浮動工具列。
  - 實作方向：採原生 toolbar 置於標題列／標準位置，使用 SF Symbols、可存取 label 與 tooltip。只保留常用的 Back/Forward、Share、Info、Sidebar、縮放或播放；將旋轉／翻轉／排序等次要操作放入選單或 overflow。支援使用者自訂與工具列顯示偏好（若框架限制，至少做精簡、分組和標準間距）。
  - 驗收：視窗寬度 800 px 時工具列不會擠壓、截斷或覆蓋照片；每個圖示都具可辨識 tooltip／accessibility label；主瀏覽器與 viewer 不會各自顯示一整排重複導航按鈕。

- [ ] **以 sidebar + content + inspector 的 macOS 資訊架構取代固定三窗格與「Places」條。**
  - 現況／依據：`src/app.rs:1858-1895` 固定組合 menu、toolbar、places、status、folder tree、preview pane；`src/app.rs:1285-1369` 另有可編輯 path 和 wildcard filter；`src/browser.rs:15-104` 使用自繪 Windows 式展開符號與樹狀目錄。
  - 實作方向：左側用可收合的 sidebar（Favorites/Locations/最近位置），中間為內容網格，右側採可收合 Inspector；以標題列／路徑 control 呈現目前位置。將技術性的 wildcard filter 移入明確的搜尋或 Filter popover，不要常駐為類 DOS 檔案篩選欄。
  - 驗收：使用者第一次打開就能從側邊欄找到 Pictures、Home 與已加入的資料夾；sidebar/Inspector 可用工具列或 View 選單切換且狀態會保存；窄視窗時內容區仍優先可用。

- [ ] **改進看圖手勢、縮放與全螢幕行為以適配觸控板。**
  - 現況／依據：`src/viewer.rs:61-87` 把任何 raw scroll 當縮放；拖曳才平移；`src/app.rs:732-751` 開始 slideshow 時強制進全螢幕，且全螢幕可由 F11/`⌘F` 觸發。
  - 實作方向：兩指捲動為平移／切換（依模式），pinch 才縮放且以游標／手勢中心縮放；支援雙擊在 Fit/100% 間切換、Space 暫時平移、左右滑切換圖片並避免與系統返回手勢衝突。全螢幕只使用 `⌃⌘F`；幻燈片可由使用者選擇是否自動進入全螢幕，退出時恢復先前縮放與視窗狀態。
  - 驗收：在 MacBook trackpad 上可 pinch 縮放、兩指平移已放大的影像、左右瀏覽不誤觸縮放；全螢幕遵循 `⌃⌘F`；開始／停止幻燈片不會意外改變非全螢幕使用者的工作情境。

- [ ] **加入分享與 Finder/Quick Look 工作流。**
  - 現況／依據：`src/app.rs:603-610` 的 Open 僅以 `open <path>` 叫起預設 App；內容選單在 `src/browser.rs:375-404` 只有 View、Open with default app、Properties、Delete。
  - 實作方向：加入 Share（`NSSharingServicePicker`）、Reveal in Finder、Quick Look（Space）與 Open With…；右鍵選單依 macOS 用語排序，非破壞性動作先於 Move to Trash。支援拖入檔案／資料夾開啟，並可把選取的檔案拖出至 Finder／其他 App。
  - 驗收：選取圖片可用 Share 成功傳送到至少一個系統分享目標；`Space` 顯示 Quick Look、Reveal in Finder 開啟並選取該檔；拖放檔案和資料夾能正確開啟或瀏覽。

- [ ] **讓 Inspector/資訊面板與縮圖選取狀態更清楚且不依賴深藍色。**
  - 現況／依據：`src/browser.rs:282-374` 將選取縮圖整塊填為 `classic::NAVY`，檔名固定 11 px；`src/browser.rs:425-` 以兩個裸文字 tab 呈現 Preview / Information；`src/classic.rs:120-133` 使用深藍色 section label。
  - 實作方向：採 macOS selection tint 與 sidebar/list selection 規則；縮圖保留適當留白、支援多行檔名或省略號與完整名稱 tooltip；Inspector 使用標準 segmented control/tab 樣式，內容採 key/value 對齊、可複製的 metadata 與明確 loading/error state。
  - 驗收：明暗模式、Increase Contrast 下仍能一眼辨識選取項；長檔名不會只靠 11 px 文字難以識別，至少可懸停取得完整名稱；Inspector 的焦點、選擇與讀取狀態清楚。

- [ ] **建立狀態、錯誤與進度回饋的 macOS 語言。**
  - 現況／依據：`src/app.rs:1372-1412` 使用三格 Win95 status field；`src/app.rs:1710-1732`、`src/app.rs:1750-1755` 只產生短暫 toast／字串錯誤；`src/viewer.rs:39-47` 僅畫出 `Loading…`。
  - 實作方向：將常駐 status bar 改為需要時才出現的 inline progress／non-modal notification；批次操作提供可理解的進度、失敗檔案與「Show in Finder」/「Retry」等下一步。避免以固定 12 px 三格資訊承載核心狀態。
  - 驗收：載入大型圖片、移動／複製多檔與解碼失敗時，使用者能知道正在處理什麼、是否完成、哪些檔案失敗並能採取下一步；一般瀏覽時畫面沒有過時的常駐三格狀態列。

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
