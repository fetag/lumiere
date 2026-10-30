# Video 解碼 spike

此目錄是獨立於正式 workspace 的技術驗證，不會啟用 Lumiere 的影片功能，也不是最終套件選型。

## 目的與限制

使用 `ffmpeg-next` 9.0.0 測試本機 FFmpeg 的影片解碼和 RGBA 影格轉換。此程式不播放音訊、不依時間顯示影格、不 seek、不整合 GUI 或處理打包，也不驗證完整 FFmpeg 錯誤處理與不可信影片資源限制。MP4 和 AVI 是容器，不能由副檔名保證所有內部 codec 均可解碼。

## 重現（以 macOS 本機環境為例）

需先準備 FFmpeg 執行檔、開發 headers 與可透過 pkg-config 找到的原生 libraries。以下測試輸出皆寫入 `/tmp`：

```sh
mkdir -p /tmp/lumiere-video-spike
ffmpeg -v error -y -f lavfi -i testsrc2=size=320x240:rate=12 \
  -f lavfi -i sine=frequency=440:sample_rate=44100 -t 2 \
  -c:v libx264 -pix_fmt yuv420p -c:a aac /tmp/lumiere-video-spike/sample.mp4
ffmpeg -v error -y -f lavfi -i testsrc2=size=320x240:rate=12 \
  -t 2 -c:v mpeg4 /tmp/lumiere-video-spike/sample.avi
cargo run --manifest-path experiments/video-spike/Cargo.toml --locked -- /tmp/lumiere-video-spike/sample.mp4
cargo run --manifest-path experiments/video-spike/Cargo.toml --locked -- /tmp/lumiere-video-spike/sample.avi
```

兩個樣本預期各解出 24 個 320 × 240 的影片影格。第一個樣本包含 AAC 音軌，但此 spike **不解碼、不播放音訊**。

## 2026-10-30 實測紀錄

- macOS ARM64：Homebrew FFmpeg 9.0.2、Rust 1.91.1；MP4/H.264 和 AVI/MPEG-4 各成功解出 24 幀，並轉換成 RGBA。
- `otool -L` 顯示程式依賴 `/opt/homebrew/opt/ffmpeg/lib/` 的 `libavutil`、`libavformat`、`libswscale`、`libavcodec`；**不代表其他機器免安裝可執行**。
- Linux `.123`：有 FFmpeg 6.1.1 runtime，但檢查時沒有 FFmpeg 開發 headers 或 pkg-config metadata；尚未建置或測試此 spike。
- Windows `.120`：檢查時 `PATH` 未找到 `ffmpeg`、`ffprobe`、`mpv`；尚未建置或測試此 spike。
- 其他候選：`egui-video` 0.9.0 宣告 `egui` 0.29，與專案的 0.33 不同，且其 README 列出 FFmpeg 6 或 7；`egui-sharkplayer` 0.5.1 宣告 Rust 1.92，高於專案 MSRV 1.91。這些版本不可直接假定適用。

## 正式整合前待確認

- 決定音訊、播放／暫停、seek 和格式／codec 的第一版範圍。
- 比較現成播放器引擎及跨平台打包路線；不要假設 FFmpeg libraries 或 DLL 已安裝。
- 在 macOS、Linux、Windows 分別驗證編譯、音畫同步、暫停、seek、損壞檔案和清理資源；驗證乾淨機器上的 bundle 與依賴、授權和發行方式。
