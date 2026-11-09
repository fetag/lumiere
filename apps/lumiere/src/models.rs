//! 實驗性 3D 模型檢視器：`gltf` 解析 .glb、wgpu 離屏渲染。
//!
//! 架構照 [`crate::video`]：背景 thread 擁有 wgpu instance/device（不 Send
//! 到 UI thread），解析幾何與 baseColor 貼圖、上傳 GPU、建立管線，然後
//! 阻塞等待渲染指令；UI thread 持有 camera 狀態與 texture，orbit/zoom 時
//! 把最新 camera 送過去、只渲染最新一幀（不跑 60fps 迴圈）。
//!
//! 離屏渲染與 readback 路徑已於 Phase 0 spike（`experiments/glb-spike/`）
//! 在 Metal / D3D12 / Vulkan 三平台驗證。wgpu 27 關鍵限制照 spike 記錄：
//! - uniform 走 bind group（Metal 無 PUSH_CONSTANT capability）。
//! - readback：先 submit copy（staging 必須 Idle）、poll、再 map/read/unmap。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use eframe::egui::{self, ColorImage, Context, TextureHandle, TextureOptions, Ui, Vec2};
use glam::Vec3;
use wgpu::util::DeviceExt;
use wgpu::{Backends, PollType};

use crate::app::App;

/// 離屏渲染的內部解析度；texture 在 UI 端縮放填滿畫布。
const WIDTH: u32 = 1024;
const HEIGHT: u32 = 1024;
/// 渲染背景色（3D viewport 慣用深色）。
const BG: [f32; 4] = [24.0 / 255.0, 26.0 / 255.0, 32.0 / 255.0, 1.0];

/// 資源保護上限（spike 規劃）：防止超大模型拖垮記憶體 / GPU。
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024; // 64 MiB
const MAX_TOTAL_VERTICES: usize = 5_000_000;
const MAX_TEXTURE_DIM: u32 = 8192;

/// 相機軌道參數（球座標，围绕模型中心）。
#[derive(Clone, Copy, Default)]
struct Camera {
    yaw: f32,
    pitch: f32,
    /// 距離 = fit_dist * zoom；zoom 1.0 = 剛好框住模型。
    zoom: f32,
}

impl Camera {
    fn new() -> Self {
        // 與 spike 的 "front" 角度一致：微微仰視的正面。
        Self { yaw: 0.0, pitch: 0.18, zoom: 1.0 }
    }

    /// 球座標 → 由中心指向相機的單位方向。
    #[allow(dead_code)]
    fn direction(&self) -> Vec3 {
        Vec3::new(
            self.pitch.cos() * self.yaw.sin(),
            self.pitch.sin(),
            self.pitch.cos() * self.yaw.cos(),
        )
    }
}

enum Command {
    Render(Camera),
}

enum Event {
    Meta { primitives: usize, vertices: usize },
    Frame { width: usize, height: usize, pixels: Vec<u8> },
    Error(String),
}

#[derive(Default)]
pub struct ModelPlayer {
    path: Option<PathBuf>,
    cancel: Option<Arc<AtomicBool>>,
    commands: Option<mpsc::Sender<Command>>,
    events: Option<mpsc::Receiver<Event>>,
    texture: Option<TextureHandle>,
    size: [usize; 2],
    error: Option<String>,
    meta: Option<(usize, usize)>,
    camera: Camera,
}

impl Drop for ModelPlayer {
    fn drop(&mut self) {
        if let Some(cancel) = &self.cancel {
            cancel.store(true, Ordering::Relaxed);
        }
    }
}

impl ModelPlayer {
    pub fn clear(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        *self = Self::default();
    }

    pub fn is_ready(&self) -> bool {
        self.texture.is_some()
    }

    pub fn meta(&self) -> Option<(usize, usize)> {
        self.meta
    }

    fn open(&mut self, path: &Path, ctx: &Context) {
        if self.path.as_deref() == Some(path) && self.cancel.is_some() {
            return;
        }
        self.clear();
        self.path = Some(path.to_owned());
        self.camera = Camera::new();
        let (command_tx, command_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel = Some(Arc::clone(&cancel));
        self.commands = Some(command_tx);
        self.events = Some(event_rx);
        let path = path.to_owned();
        let ctx = ctx.clone();
        std::thread::Builder::new()
            .name("lumiere-model-gpu".into())
            .spawn(move || {
                let result = run(&path, &ctx, &cancel, &command_rx, &event_tx);
                if let Err(error) = result {
                    let _ = event_tx.send(Event::Error(error));
                    ctx.request_repaint();
                }
            })
            .ok();
        // 首幀由 UI 端送；thread 完成場景建立前指令會在 channel 緩衝。
        self.request_render(self.camera);
    }

    /// 收 background thread 的事件，更新 texture。
    fn pump(&mut self, ctx: &Context) {
        let Some(events) = &self.events else {
            return;
        };
        while let Ok(event) = events.try_recv() {
            match event {
                Event::Meta { primitives, vertices } => {
                    self.meta = Some((primitives, vertices));
                }
                Event::Frame { width, height, pixels } => {
                    self.size = [width, height];
                    let image = ColorImage::from_rgba_unmultiplied(self.size, &pixels);
                    if let Some(texture) = &mut self.texture {
                        texture.set(image, TextureOptions::LINEAR);
                    } else {
                        self.texture = Some(ctx.load_texture("model", image, TextureOptions::LINEAR));
                    }
                    ctx.request_repaint();
                }
                Event::Error(error) => {
                    self.error = Some(error);
                    ctx.request_repaint();
                }
            }
        }
    }

    /// 送出渲染指令；只渲染最新 camera（drain 舊指令）。
    fn request_render(&mut self, camera: Camera) {
        self.camera = camera;
        if let Some(commands) = &self.commands {
            let _ = commands.send(Command::Render(camera));
        }
    }
}

/// 背景 thread 擁有的 GPU 場景：device、幾何、管線、離屏 target。
struct Scene {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    uniform_buf: wgpu::Buffer,
    primitives: Vec<Primitive>,
    target: wgpu::Texture,
    target_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    staging: wgpu::Buffer,
    /// 離屏 target 的邊長（方型 size x size）。
    size: u32,
    center: Vec3,
    fov: f32,
    near: f32,
    far: f32,
}

struct Primitive {
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    index_count: u32,
    index_format: wgpu::IndexFormat,
    bind_group: wgpu::BindGroup,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Zeroable, bytemuck::Pod)]
struct Vertex {
    pos: [f32; 3],
    normal: [f32; 3],
    uv: [f32; 2],
}

struct GpuPrimitive {
    verts: Vec<Vertex>,
    index_data: Vec<u32>,
    index_format: wgpu::IndexFormat,
    albedo: Option<(Vec<u8>, [u32; 2])>,
    /// material 的 baseColorFactor（glTF linear 空間）；無則白。
    base_color: [f32; 4],
}

/// 解析 .glb bytes → GPU-ready primitives + 邊界球資訊。
///
/// 與 GPU / thread 無關，可 headless 測試。回傳 `(primitives, center,
/// radius, total_vertices)`。
fn parse_glb(bytes: &[u8]) -> Result<(Vec<GpuPrimitive>, Vec3, f32, usize), String> {
    let (document, buffers, images) =
        gltf::import_slice(bytes).map_err(|e| format!("gltf import failed: {e}"))?;

    let mut gpus: Vec<GpuPrimitive> = Vec::new();
    let mut total_vertices = 0usize;
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];

    'meshes: for mesh in document.meshes() {
        for primitive in mesh.primitives() {
            if primitive.mode() != gltf::mesh::Mode::Triangles {
                continue;
            }
            let get_buf = |b: gltf::Buffer| buffers.get(b.index()).map(|d| &d.0[..]);
            let reader = primitive.reader(get_buf);
            let positions = reader
                .read_positions()
                .ok_or("primitive has no positions")?
                .collect::<Vec<_>>();
            if positions.is_empty() {
                continue;
            }
            total_vertices += positions.len();
            if total_vertices > MAX_TOTAL_VERTICES {
                return Err(format!(
                    "model too complex ({} vertices, limit {})",
                    total_vertices, MAX_TOTAL_VERTICES
                ));
            }
            let indices = reader.read_indices().map(|ri| match ri {
                gltf::mesh::util::ReadIndices::U8(i) => i.map(|x| x as u32).collect::<Vec<u32>>(),
                gltf::mesh::util::ReadIndices::U16(i) => {
                    i.map(|x| x as u32).collect::<Vec<u32>>()
                }
                gltf::mesh::util::ReadIndices::U32(i) => i.collect::<Vec<u32>>(),
            });
            let normals = reader
                .read_normals()
                .map(|r| r.collect::<Vec<_>>())
                .unwrap_or_else(|| vec![[0.0, 0.0, 1.0]; positions.len()]);
            let uvs = match reader.read_tex_coords(0) {
                Some(gltf::mesh::util::ReadTexCoords::U8(i)) => i
                    .map(|[x, y]| [x as f32 / 255.0, y as f32 / 255.0])
                    .collect(),
                Some(gltf::mesh::util::ReadTexCoords::U16(i)) => i
                    .map(|[x, y]| [x as f32 / 65535.0, y as f32 / 65535.0])
                    .collect(),
                Some(gltf::mesh::util::ReadTexCoords::F32(i)) => i.collect(),
                None => vec![[0.5, 0.5]; positions.len()],
            };

            // 累加邊界盒（相機 fit 用）。
            for p in &positions {
                for k in 0..3 {
                    min[k] = min[k].min(p[k]);
                    max[k] = max[k].max(p[k]);
                }
            }

            // baseColor 貼圖。
            let mut albedo: Option<(Vec<u8>, [u32; 2])> = None;
            let material = primitive.material();
            let pbr = material.pbr_metallic_roughness();
            // baseColorFactor（linear）；無則預設白 [1,1,1,1]。
            let base_color = pbr.base_color_factor();
            if let Some(tex) = pbr.base_color_texture() {
                if let Some(gimg) = images.get(tex.texture().source().index()) {
                    albedo = Some(rgba_from_gltf_image(gimg));
                }
            }
            if let Some((_, [w, h])) = &albedo {
                if *w > MAX_TEXTURE_DIM || *h > MAX_TEXTURE_DIM {
                    return Err(format!(
                        "texture too large ({w}x{h}, limit {MAX_TEXTURE_DIM})",
                    ));
                }
            }

            let verts: Vec<Vertex> = positions
                .iter()
                .zip(normals.iter().zip(uvs.iter()))
                .map(|(p, (n, uv))| Vertex { pos: *p, normal: *n, uv: *uv })
                .collect();
            let (index_data, index_format) = match indices {
                Some(idx) => (idx, wgpu::IndexFormat::Uint32),
                None => ((0..positions.len() as u32).collect::<Vec<u32>>(), wgpu::IndexFormat::Uint32),
            };
            gpus.push(GpuPrimitive {
                verts,
                index_data,
                index_format,
                albedo,
                base_color,
            });
            if gpus.len() >= 64 {
                break 'meshes;
            }
        }
    }

    if gpus.is_empty() {
        return Err("no triangle primitives found".into());
    }

    // 邊界球（邊界盒中心 + 最遠頂點距離）。
    let center = Vec3::new(
        (min[0] + max[0]) / 2.0,
        (min[1] + max[1]) / 2.0,
        (min[2] + max[2]) / 2.0,
    );
    let mut radius = 0.0f32;
    for gp in &gpus {
        for v in &gp.verts {
            radius = radius.max((Vec3::from(v.pos) - center).length());
        }
    }
    if radius <= 0.0 {
        radius = 1.0;
    }

    Ok((gpus, center, radius, total_vertices))
}

fn run(
    path: &Path,
    ctx: &Context,
    cancel: &AtomicBool,
    commands: &mpsc::Receiver<Command>,
    events: &mpsc::Sender<Event>,
) -> Result<(), String> {
    // 資源保護：先查檔案大小（解析前）。
    let file_len = std::fs::metadata(path)
        .map_err(|e| format!("cannot read model: {e}"))?
        .len();
    if file_len > MAX_FILE_BYTES {
        return Err(format!(
            "model too large ({} bytes, limit {} bytes)",
            file_len, MAX_FILE_BYTES
        ));
    }

    // async 部分（instance/adapter/device）用 pollster 阻塞等待。instance
    // 保持存活到 run 結束（device 已獨立，但保留較安全）。
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: Backends::all(),
        ..Default::default()
    });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..Default::default()
    }))
    .map_err(|e| format!("no GPU adapter: {e}"))?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("lumiere-model"),
        ..Default::default()
    }))
    .map_err(|e| format!("device request failed: {e}"))?;

    if cancel.load(Ordering::Relaxed) {
        return Ok(());
    }

    let bytes = std::fs::read(path).map_err(|e| format!("cannot read model: {e}"))?;
    let (gpus, center, radius, total_vertices) = parse_glb(&bytes)?;

    let fov = 45.0f32.to_radians();
    let near = (radius * 0.01).max(0.001);
    let far = radius * 10.0;

    let scene = Scene::build(&device, &queue, &gpus, center, fov, near, far, WIDTH)
        .map_err(|e| format!("GPU setup failed: {e}"))?;

    let _ = events.send(Event::Meta {
        primitives: gpus.len(),
        vertices: total_vertices,
    });

    // ---- 渲染迴圈：阻塞等 camera，drain 到最新，只渲染最新一幀 -----------
    // （光源在 render 內固定於世界座標，不需由外部傳入。）
    let mut scene = scene;
    let fit_dist = radius / (fov * 0.5).tan() * 1.35;
    loop {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        // 阻塞等第一筆指令；channel 關閉（UI 端 drop）則結束。
        let first = match commands.recv() {
            Ok(c) => c,
            Err(_) => break,
        };
        let mut latest = first;
        while let Ok(c) = commands.try_recv() {
            latest = c;
        }
        let Command::Render(Camera { yaw, pitch, zoom }) = latest;
        let dist = fit_dist * zoom;
        match scene.render(yaw, pitch, dist) {
            Ok(pixels) => {
                let _ = events.send(Event::Frame {
                    width: WIDTH as usize,
                    height: HEIGHT as usize,
                    pixels,
                });
                ctx.request_repaint();
            }
            Err(e) => {
                let _ = events.send(Event::Error(e));
                break;
            }
        }
    }
    Ok(())
}

impl Scene {
    #[allow(clippy::too_many_arguments)]
    fn build(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        gpus: &[GpuPrimitive],
        center: Vec3,
        fov: f32,
        near: f32,
        far: f32,
        size: u32,
    ) -> Result<Self, String> {
        // Normalize the offscreen size to a multiple of 64 so that `size * 4`
        // is always a multiple of 256 (COPY_BYTES_PER_ROW_ALIGNMENT) for the
        // readback. Callers (viewer: 1024, thumbnail service: any size) pass a
        // logical size; the GPU target is rounded up to the nearest 64.
        let size = size.max(64).div_ceil(64) * 64;
        // ---- shared pipeline resources ------------------------------------
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let bg_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("bg-layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shader"),
            source: wgpu::ShaderSource::Wgsl(CODE.into()),
        });
        let pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[&bg_layout],
                push_constant_ranges: &[],
            });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x3,
                        1 => Float32x3,
                        2 => Float32x2
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: Some(wgpu::Face::Back),
                front_face: wgpu::FrontFace::Ccw,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // 單一 uniform buffer，所有 primitive 共用；每次 render 更新。
        // 布局：mvp(16) + key(3) + fill(3) = 22 float，16-byte 對齊到 24。
        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniforms"),
            size: std::mem::size_of::<[f32; 24]>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // ---- per-primitive buffers + bind groups --------------------------
        let mut primitives = Vec::with_capacity(gpus.len());
        for gp in gpus {
            let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("vertices"),
                contents: bytemuck::cast_slice(&gp.verts),
                usage: wgpu::BufferUsages::VERTEX,
            });
            let ibuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("indices"),
                contents: bytemuck::cast_slice(&gp.index_data),
                usage: wgpu::BufferUsages::INDEX,
            });
            let (albedo_view, _albedo_tex) = upload_albedo(device, queue, &gp.albedo);
            // 每 primitive 的 baseColorFactor uniform（16 bytes，build 時寫一次）。
            let base_color_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("base_color"),
                contents: bytemuck::cast_slice(&gp.base_color),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("bg"),
                layout: &bg_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&albedo_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &uniform_buf,
                            offset: 0,
                            size: None,
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &base_color_buf,
                            offset: 0,
                            size: None,
                        }),
                    },
                ],
            });
            primitives.push(Primitive {
                vbuf,
                ibuf,
                index_count: gp.index_data.len() as u32,
                index_format: gp.index_format,
                bind_group,
            });
        }

        // ---- offscreen target / depth / staging ---------------------------
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen"),
            size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
        let byte_len = (size * size * 4) as wgpu::BufferAddress;
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("staging"),
            size: byte_len,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        Ok(Self {
            device: device.clone(),
            queue: queue.clone(),
            pipeline,
            uniform_buf,
            primitives,
            target,
            target_view,
            depth_view,
            staging,
            size,
            center,
            fov,
            near,
            far,
        })
    }

    /// The normalized (multiple-of-64) offscreen size this scene renders at.
    fn size(&self) -> u32 {
        self.size
    }

    fn render(
        &mut self,
        yaw: f32,
        pitch: f32,
        dist: f32,
    ) -> Result<Vec<u8>, String> {
        let dir = Vec3::new(
            pitch.cos() * yaw.sin(),
            pitch.sin(),
            pitch.cos() * yaw.cos(),
        );
        let cam = self.center + dir * dist;
        let proj = glam::camera::rh::proj::directx::perspective(
            self.fov,
            WIDTH as f32 / HEIGHT as f32,
            self.near,
            self.far,
        );
        let view = glam::camera::rh::view::look_at_mat4(cam, self.center, Vec3::Y);
        let mvp = proj * view;
        // 固定世界座標光（不跟相機轉）：key 主光（右上前）+ fill 補光（左下）。
        let key = Vec3::new(0.4, 0.8, 0.6).normalize();
        let fill = Vec3::new(-0.6, 0.2, 0.5).normalize();
        let uniform: [f32; 24] = {
            let mut v: [f32; 24] = [0.0; 24];
            v[..16].copy_from_slice(&mvp.to_cols_array());
            v[16..19].copy_from_slice(&key.to_array());
            v[19..22].copy_from_slice(&fill.to_array());
            v
        };
        self.queue
            .write_buffer(&self.uniform_buf, 0, bytemuck::cast_slice(&uniform));

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("render"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.target_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: BG[0] as f64,
                            g: BG[1] as f64,
                            b: BG[2] as f64,
                            a: BG[3] as f64,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            for prim in &self.primitives {
                pass.set_bind_group(0, &prim.bind_group, &[]);
                pass.set_vertex_buffer(0, prim.vbuf.slice(..));
                pass.set_index_buffer(prim.ibuf.slice(..), prim.index_format);
                pass.draw_indexed(0..prim.index_count, 0, 0..1);
            }
        }
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.staging,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    // `self.size` is a multiple of 64 (normalized in `build`),
                    // so `size * 4` is a multiple of 256, satisfying
                    // COPY_BYTES_PER_ROW_ALIGNMENT for the texture -> buffer
                    // readback.
                    bytes_per_row: Some(self.size * 4),
                    rows_per_image: None,
                },
            },
            wgpu::Extent3d {
                width: self.size,
                height: self.size,
                depth_or_array_layers: 1,
            },
        );
        let commands = encoder.finish();
        // copy 命令先提交（staging 必須 Idle），poll 完成後再 map/read/unmap。
        self.queue.submit([commands]);
        self.device
            .poll(PollType::Wait { submission_index: None, timeout: None })
            .map_err(|e| format!("poll failed: {e}"))?;
        self.staging
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |_| {});
        self.device
            .poll(PollType::Wait { submission_index: None, timeout: None })
            .map_err(|e| format!("poll failed: {e}"))?;
        let pixels = {
            let view = self.staging.slice(..).get_mapped_range();
            view.to_vec()
        };
        self.staging.unmap();
        Ok(pixels)
    }
}

/// gltf crate 1.4 已把 image 解成像素；統一轉 RGBA8。
fn rgba_from_gltf_image(gimg: &gltf::image::Data) -> (Vec<u8>, [u32; 2]) {
    let w = gimg.width;
    let h = gimg.height;
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    match gimg.format {
        gltf::image::Format::R8G8B8A8 => out.extend_from_slice(&gimg.pixels),
        gltf::image::Format::R8G8B8 => {
            for p in gimg.pixels.chunks(3) {
                out.extend_from_slice(p);
                out.push(255);
            }
        }
        gltf::image::Format::R8 => {
            for &r in &gimg.pixels {
                out.extend_from_slice(&[r, r, r, 255]);
            }
        }
        gltf::image::Format::R8G8 => {
            for p in gimg.pixels.chunks(2) {
                out.extend_from_slice(&[p[0], p[1], 0, 255]);
            }
        }
        // 16/32-bit 貼圖退回佔位白（乘上 baseColorFactor 後即為該色）。
        _ => {
            out = vec![255u8, 255, 255, 255];
            return (out, [1, 1]);
        }
    }
    (out, [w, h])
}

fn upload_albedo(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    albedo: &Option<(Vec<u8>, [u32; 2])>,
) -> (wgpu::TextureView, wgpu::Texture) {
    let (rgba, [w, h]) = match albedo {
        Some(x) => x.clone(),
        // 無貼圖：白色 1x1，讓 shader 的 `* base_color` 結果 = baseColorFactor。
        None => (vec![255u8, 255, 255, 255], [1u32, 1u32]),
    };
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("albedo"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(w * 4),
            rows_per_image: None,
        },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    (tex.create_view(&wgpu::TextureViewDescriptor::default()), tex)
}

/// 離屏渲染一個 GLB 到 `size`×`size` 的 RGBA pixels（固定縮圖鏡頭）。
/// 供 GPU 縮圖服務（`model_thumbs`）用：device 由呼叫方建立一次並重用，
/// 每個 (model, size) 只渲染一次（其餘靠 disk thumbnail cache）。
pub(crate) fn render_offscreen_with_device(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    bytes: &[u8],
    size: u32,
) -> Result<(Vec<u8>, u32), String> {
    let (gpus, center, radius, count) = parse_glb(bytes)?;
    if count == 0 {
        return Err("model has no primitives".to_string());
    }
    let fov = 45.0f32.to_radians();
    let near = (radius * 0.1).max(1e-3);
    let far = radius * 10.0;
    let mut scene =
        Scene::build(device, queue, &gpus, center, fov, near, far, size)
            .map_err(|e| format!("GPU setup failed: {e}"))?;
    // Actual GPU target size (rounded up to a multiple of 64).
    let rendered = scene.size();
    let dist = (radius / (fov * 0.5).tan()) * 1.35;
    // 固定縮圖鏡頭：微微由上往正面看，顯示 3D 立體感。
    let pixels = scene.render(0.0, 0.35, dist).map_err(|e| format!("render failed: {e}"))?;
    Ok((pixels, rendered))
}

/// UI：畫 model texture + orbit/zoom 互動。
pub fn view(app: &mut App, ui: &mut Ui, path: &Path) {
    app.gif_player.clear();
    // 注意：ui.ctx() 回傳 &Context（borrow ui），不可跨 mutable ui 使用，
    // 故各處 inline 取用。
    app.model_player.open(path, ui.ctx());
    app.model_player.pump(ui.ctx());

    let canvas = ui.available_rect_before_wrap();
    let response = ui.allocate_rect(canvas, egui::Sense::click_and_drag());

    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_owned();

    // ---- 互動：orbit（拖曳）+ zoom（滾輪）---------------------------------
    let mut dirty = false;
    if response.dragged() {
        let delta = response.drag_delta();
        let cam = app.model_player.camera;
        let mut next = cam;
        next.yaw += -delta.x * 0.01;
        next.pitch += delta.y * 0.01;
        next.pitch = next.pitch.clamp(-1.5, 1.5);
        app.model_player.request_render(next);
        dirty = true;
    }
    let scroll = ui.input(|i| i.raw_scroll_delta);
    if scroll.y != 0.0 {
        let cam = app.model_player.camera;
        let mut next = cam;
        next.zoom *= 1.0 - scroll.y * 0.01;
        next.zoom = next.zoom.clamp(0.1, 10.0);
        app.model_player.request_render(next);
        dirty = true;
    }
    // 雙擊 = 重設視角。
    if response.double_clicked() {
        let fresh = Camera::new();
        app.model_player.request_render(fresh);
        dirty = true;
    }

    // ---- 畫 -----------------------------------------------------------------
    let center = canvas.center();
    let available = canvas.size();
    // 維持模型 aspect（內部 1:1），fit 進畫布。
    let scale = (available.x / WIDTH as f32).min(available.y / HEIGHT as f32);
    let size = Vec2::new(WIDTH as f32 * scale, HEIGHT as f32 * scale);

    match &app.model_player.texture {
        Some(texture) => {
            let rect = egui::Rect::from_center_size(center, size);
            ui.painter().image(
                texture.id(),
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
        None => {
            let status = match &app.model_player.error {
                Some(e) => format!("Failed to load {file_name}\n{e}"),
                None => format!("Loading {file_name}…"),
            };
            let color = if app.canvas_dark {
                egui::Color32::from_gray(200)
            } else {
                egui::Color32::WHITE
            };
            ui.painter().text(
                center,
                egui::Align2::CENTER_CENTER,
                status,
                egui::FontId::proportional(14.0),
                color,
            );
        }
    }

    // HUD：orbit 提示 + 元資訊。
    let hint = "Drag to orbit · Scroll to zoom · Double-click to reset";
    let label = if let Some((prims, verts)) = app.model_player.meta() {
        format!("{file_name}   {prims} primitive(s), {verts} vertices   ·   {hint}")
    } else {
        hint.to_string()
    };
    ui.painter().text(
        egui::pos2(canvas.left() + 8.0, canvas.top() + 8.0),
        egui::Align2::LEFT_TOP,
        label,
        egui::FontId::proportional(12.0),
        if app.canvas_dark { egui::Color32::from_gray(180) } else { egui::Color32::from_gray(90) },
    );

    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_role(egui::accesskit::Role::Image);
        node.set_label(file_name.as_str());
    });

    if dirty {
        ui.ctx().request_repaint();
    }
    // 模型是靜態的：只在有互動時重繪（上面 dirty 已 request_repaint），
    // 不跑 60fps 迴圈。首幀前需要定期檢查 background thread 的 frame。
    if !app.model_player.is_ready() {
        ui.ctx().request_repaint_after(Duration::from_millis(50));
    }
}

const CODE: &str = r#"
struct Uniforms {
    mvp: mat4x4<f32>,
    key: vec3<f32>,
    fill: vec3<f32>,
};
@group(0) @binding(2)
var<uniform> u: Uniforms;

// 每 primitive 的 baseColorFactor（glTF 語義：baseColor = texture * factor，
// 無貼圖時 texture 為白 1x1，故結果 = factor）。
struct PrimUniforms {
    base_color: vec4<f32>,
}
@group(0) @binding(3)
var<uniform> prim: PrimUniforms;

@group(0) @binding(0)
var smp: sampler;
@group(0) @binding(1)
var albedo: texture_2d<f32>;

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) n: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

@vertex
fn vs_main(
    @location(0) p: vec3<f32>,
    @location(1) n: vec3<f32>,
    @location(2) uv: vec2<f32>,
) -> VOut {
    var o: VOut;
    o.pos = u.mvp * vec4<f32>(p, 1.0);
    o.n = n;
    o.uv = uv;
    return o;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    let a = textureSample(albedo, smp, in.uv) * prim.base_color;
    let n = normalize(in.n);
    // 半球 ambient（天/地）：讓各面都有基礎亮度、不會全黑。
    let hemi = mix(vec3<f32>(0.24, 0.26, 0.30), vec3<f32>(0.46, 0.46, 0.50), n.y * 0.5 + 0.5);
    // key + fill 都是固定世界座標方向光（不跟相機轉）。
    let ndl = max(dot(n, normalize(u.key)), 0.0);
    let nfl = max(dot(n, normalize(u.fill)), 0.0);
    let lit = a.rgb * (hemi + vec3<f32>(0.90) * ndl + vec3<f32>(0.30) * nfl);
    // linear -> sRGB gamma（否則中間調大幅偏暗）。
    let clamped = clamp(lit, vec3<f32>(0.0), vec3<f32>(1.0));
    let out = pow(clamped, vec3<f32>(1.0 / 2.2));
    return vec4<f32>(out, 1.0);
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    // 兩個三角形（不同顏色 primitive）的最小 GLB fixture。
    static FIXTURE: &[u8] = include_bytes!("../fixtures/model-test.glb");

    #[test]
    fn parse_glb_reads_two_primitives() {
        let (gpus, center, radius, vertices) = parse_glb(FIXTURE).expect("parse fixture");
        assert_eq!(gpus.len(), 2, "two triangle primitives");
        assert_eq!(vertices, 6, "3 verts per primitive");
        for gp in &gpus {
            assert_eq!(gp.index_data.len(), 3);
        }
        // baseColorFactor 被讀取：prim0 紅、prim1 綠。
        let (r, g) = (gpus[0].base_color, gpus[1].base_color);
        assert!(r[0] > 0.9 && r[1] < 0.2 && r[2] < 0.2, "prim0 should be red: {r:?}");
        assert!(g[1] > 0.9 && g[0] < 0.2 && g[2] < 0.2, "prim1 should be green: {g:?}");
        // 兩個三角形 x∈[0,3]，中心 x=1.5。
        assert!((center.x - 1.5).abs() < 1e-3);
        assert!(radius > 0.0);
    }

    #[test]
    fn parse_glb_rejects_non_glb() {
        assert!(parse_glb(b"not a glb").is_err());
    }

    // 無 mesh / 無 primitive 的 GLB → 「no triangle primitives found」。
    static EMPTY: &[u8] = include_bytes!("../fixtures/model-empty.glb");

    #[test]
    fn parse_glb_rejects_empty_model() {
        match parse_glb(EMPTY) {
            Ok(_) => panic!("empty model must fail"),
            Err(e) => assert!(e.contains("no triangle primitives"), "got: {e}"),
        }
    }

    /// 完整 GPU 路徑：parse → Scene::build → 單 pass 多-primitive render →
    /// readback。無 GPU adapter 的環境（CI）直接 skip。
    #[test]
    fn render_multi_primitive_offscreen() {
        let (gpus, center, radius, _vertices) = parse_glb(FIXTURE).expect("parse fixture");

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: Backends::all(),
            ..Default::default()
        });
        let adapter = match pollster::block_on(instance.request_adapter(&
            wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                ..Default::default()
            }
        )) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("skip render test: no GPU adapter ({e})");
                return;
            }
        };
        let (device, queue) = match pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("lumiere-model-test"),
            ..Default::default()
        })) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("skip render test: device request failed ({e})");
                return;
            }
        };

        let fov = 45.0f32.to_radians();
        let near = (radius * 0.01).max(0.001);
        let far = radius * 10.0;
        let scene = Scene::build(&device, &queue, &gpus, center, fov, near, far, WIDTH)
            .expect("build scene");

        // 正前方 + 微仰視；距離 = fit。
        let fit_dist = radius / (fov * 0.5).tan() * 1.35;
        let mut scene = scene;
        let pixels = scene.render(0.0, 0.18, fit_dist).expect("render");

        assert_eq!(pixels.len(), (WIDTH * HEIGHT * 4) as usize);
        // 前景 = 背景色 (24,26,32) 之外的像素；兩個三角形必須被畫出。
        let fg = pixels
            .chunks(4)
            .filter(|px| !(px[0] < 40 && px[1] < 40 && px[2] < 48))
            .count();
        let ratio = fg as f64 / (WIDTH * HEIGHT) as f64;
        assert!(
            ratio > 0.005,
            "foreground too small: {ratio} (two triangles should be visible)"
        );
        // baseColorFactor 必須生效：prim0 紅 [1,0.1,0.1]、prim1 綠 [0.1,1,0.1]。
        // 若 base_color 未乘入（全白），兩三角形同色，以下兩項不會同時 > 0。
        let red = pixels
            .chunks(4)
            .filter(|px| px[0] as u32 > 50 && px[0] as u32 > 2 * px[1] as u32 && px[0] as u32 > 2 * px[2] as u32)
            .count();
        let green = pixels
            .chunks(4)
            .filter(|px| px[1] as u32 > 50 && px[1] as u32 > 2 * px[0] as u32 && px[1] as u32 > 2 * px[2] as u32)
            .count();
        assert!(red > 0 && green > 0, "baseColorFactor not applied: red={red} green={green}");
    }
}
