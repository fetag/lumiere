# glb-spike

Phase 0 決策門 spike：驗證「`gltf` 解析 + wgpu offscreen 渲染 + readback」
管線在三平台三個 GPU backend 都通。詳見 `docs/glb-support-plan.md` Phase 0。

## 結果（2026-11-09，已通過）

測試模型：Khronos 官方 `DamagedHelmet.glb`（14556 頂點 / 15452 三角形 /
2048² albedo atlas）。離屏 1024×1024 渲染 3 角度（front / az45 / elev30），
readback RGBA，存 PNG，檢查前景佔比（0.5%–60% 視為 PASS）。

| 平台 | backend | 驅動 | 結果 | 前景佔比 front/az45/elev30 |
|------|---------|------|------|---------------------------|
| macOS | Metal | Apple M5 硬体 | PASS | 6.0 / 3.8 / 3.0% |
| Windows .120 | D3D12 | WARP（Microsoft Basic Render Driver） | PASS | 6.0 / 3.8 / 3.0% |
| Linux .123 | Vulkan | llvmpipe（軟體） | PASS | 6.0 / 3.8 / 3.0% |

- 三平台前景佔比完全一致；pixel diff（vs macOS 基準）< 0.002%（8/12 個
  anti-aliasing 邊緣像素，meanΔ < 0.04/765），實質像素一致。
- 幾何控制組：`Crate.glb`（方塊）、`Chair_1.glb`（條紋椅）輪廓正確，
  確認 vertex/index/camera/depth 管線正確。
- 貼圖：DamagedHelmet 2048² atlas sampling 正確；與官方
  `screenshot/screenshot.png` 目視對照，幾何結構（visor/面板/電纜/accent）吻合。
- 無貼圖模型走 1×1 gray placeholder，flat color 正常。
- 負例（非 GLB 檔）合理報錯：`gltf import failed: expected value ...`。

> 簡化 shader（單光源、無 PBR/IBL/normal map）使材質較官方參考圖平/暗，
> 屬 Phase 0 預期，非 bug。正式 Phase 1 可再補 PBR 或維持 flat。

## 用法

```
lumiere-glb-spike <model.glb> <output_dir>
```

在背景 thread 建 wgpu instance/device（`Backends::all()`），離屏渲染，
readback，存 PNG，輸出各角度前景佔比與 PASS/FAIL。

## wgpu 27 API 坑（實測記錄）

這些是寫 spike 時踩到的 wgpu 27.0.1 / gltf 1.4.1 / glam 0.34.3 版本細節，
正式串接時照搬：

1. **uniform 用 bind group，不能用 push constant**：
   Apple M5（Metal）不 expose `PUSH_CONSTANT` capability，pipeline layout
   帶 `push_constant_ranges` 會在建 shader module 時 fail
   （`Capability PUSH_CONSTANT is not supported`）。改用
   `@group(0) @binding(2) var<uniform> u: Uniforms;` + 每幀
   `create_buffer_init` 寫 uniform buffer。
2. **readback 順序**：同 submission 的 `COPY_DST` buffer 不可同時有 pending
   `map_async`——submit 時該 buffer 必須是 `Idle`，否則
   `BufferStillMapped`。正確順序：`submit(copy 命令)` → `poll(Wait)` →
   `map_async` → `poll(Wait)` → `get_mapped_range().to_vec()` → `unmap()`。
   `get_mapped_range` 的 `BufferView` 要先 drop（scope）再 `unmap()`。
3. **gltf 1.4.1**：`gltf::import()`（非 `load()`）；reader 回 `Iter`；
   `ReadIndices` 是 enum（U8/U16/U32）；`ReadTexCoords` 是 enum（U8/U16/F32）；
   material 用 `pbr_metallic_roughness()`（非 `pbr_metadata()`）；
   `texture().source()` 直接回 `image::Image`（無 `allow_empty_texture` feature）。
4. **glam 0.34.3**：`glam::camera::rh::proj::directx::perspective()` 與
   `glam::camera::rh::view::look_at_mat4()`（非 `Mat4::perspective_rh` /
   `look_at_rh`）。
5. **image 0.25.10**：`RgbaImage::from_raw().save()`（`save_buffer` 需
   `ExtendedColorType`）。

## 跨平台部署

- macOS：本機 `cargo run`（Metal）。
- Windows .120：`cmd.exe` shell（**非 PowerShell/bash**），Rust 1.99.1 在
  `C:\Users\james\.cargo\bin\`。scp 拷貝 Cargo.toml/Cargo.lock/src/main.rs +
  測試 glb → `cargo build`（~1 min）→ `target\debug\lumiere-glb-spike.exe`。
- Linux .123：bash + Vulkan ICD（含 llvmpipe 軟體 fallback）。Rust 經 rustup
  裝在 `~/.cargo`（**非登入 shell 不在 PATH**，需 `source ~/.cargo/env`）。
  `cargo build`（~43 s）→ `./target/debug/lumiere-glb-spike`。
