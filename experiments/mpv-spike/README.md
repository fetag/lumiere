# libmpv 播放引擎 spike

此獨立 Rust 專案使用 `mpv-engine` 0.2.1／libmpv 評估影片軟體渲染、播放控制及音訊。它**不是 Lumiere 的影片功能**，不更改正式 workspace，也不提供 GUI、縮圖、發行套件或正式使用的 libmpv。

## 如何執行

依賴 Rust 1.91 以上、可連結的 libmpv 動態庫及其 runtime 依賴。必須使用實際有音軌的檔案測試 `--audio`；預設使用 `ao=null`，以便在無音效裝置的 SSH session 驗證畫面與 seek：

```sh
cargo run --locked --manifest-path experiments/mpv-spike/Cargo.toml -- /path/to/video.mp4
cargo run --locked --manifest-path experiments/mpv-spike/Cargo.toml -- /path/to/video.mp4 --audio
```

測試載入、非黑色 RGBA 畫面、暫停、seek 到中點以及繼續後的時間前進。`--audio` 另檢查音軌 codec、輸出設備及取樣率；設備初始化通過**不等於**人耳確認聽到聲音或音畫同步正確。播放輸出使用 libmpv，而非自己實作的音訊時鐘。

## 2026-10-30 技術驗證結果

樣本由 `experiments/video-spike/README.md` 的指令產生，均為 320 × 240、2 秒；MP4 包含 H.264 + AAC，AVI 包含 MPEG-4 影片、無音軌。

| 平台 | 結果 | 環境與限制 |
|---|---|---|
| macOS ARM64 | MP4 / AVI 的畫面、暫停、繼續、seek 通過；MP4 的 `--audio` 回報 `coreaudio`、44100 Hz | 實驗性借用官方 IINA 1.5.0 DMG 內的 libmpv 與相依 dylib（[IINA releases](https://github.com/iina/iina/releases)）；不代表可直接再發行。載入時出現重複 Swift class 警告；實測過一次音訊模式逾時，控制組 `ffplay` 當時也逾時，稍後重測均成功，根因未明。未確認人耳聽到或音畫同步。 |
| Linux `.123` x86_64 | MP4 / AVI 的畫面、暫停、繼續、seek 通過；MP4 的 `--audio` 回報 `pipewire`、44100 Hz | 使用系統 libmpv 0.37.0；SSH session 設 `XDG_RUNTIME_DIR` 指向使用者桌面 audio socket。未確認人耳聽到或音畫同步。 |
| Windows `.120` x86_64 | `ao=null` 下 MP4 / AVI 畫面、暫停、繼續、seek 通過 | 使用 [mpv 官方安裝頁](https://mpv.io/installation/)列出的第三方 `zhongfly/mpv-winbuild`，校對發行者提供的 SHA-256；此 build 的 `libmpv-2.dll` 約 117 MiB，依賴 `vulkan-1.dll`。測試時從該機已安裝的 Codex 複製 Vulkan loader 至隔離工作目錄；不是可發行的依賴方案。SSH `--audio` 未取得音訊輸出，尚需 Windows 互動式桌面測試。 |

上述 spike 的音訊結果僅確認設備初始化，不代表音畫同步驗證。後續已將影片播放整合至 Lumiere 的實驗性 `video` feature；使用者回報 Windows、Linux 影片及聲音播放正常，並確認目前 Video Player 功能可用。Windows `.120` 的 spike 在 `C:\\workspace\\lumiere-mpv-spike-20261030`；Linux `.123` 的 spike 在 `/tmp/lumiere-mpv-spike-20261030`，均非正式安裝。

## 正式採納前的阻擋事項

- 完成 macOS `.app`、Windows ZIP、Linux tar/deb/AppImage 的自帶 runtime、相依庫搜尋與乾淨環境驗證，不假設使用者安裝 VLC、mpv、FFmpeg 或 Vulkan SDK。
- 查核各平台打包依賴來源、FFmpeg／libmpv 編譯設定、授權與重新散布義務；不能直接複製 IINA 或 Codex 的 library 當作發行方案。
- 在三平台互動式桌面測試真正可聽的音訊、音畫同步、拖曳 seek、播放結束、切檔及安全資源釋放。
- 釐清 macOS 音訊模式先前的逾時與重複 Swift class 警告；確認不是預期發行方式的問題。
- 將實驗性影片功能納入正式發行；補上實際影片縮圖、回歸測試與完整三平台打包。任何新 UI 訊息應維持英文。
