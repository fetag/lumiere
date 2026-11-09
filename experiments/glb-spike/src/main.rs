//! Phase 0 spike：驗證 wgpu offscreen 渲染 GLB → readback → PNG 的路徑。
//!
//! 控制組 = Khronos 官方 DamagedHelmet.glb。目標：
//! 1. 背景 thread 建立 wgpu instance/device（與正式架構同構）。
//! 2. `gltf` crate 解析幾何與 baseColor 貼圖。
//! 3. 離屏渲染 3 個角度，readback 成 RGBA 存 PNG。
//! 4. 前景像素佔比統計；0.5%–60% 之外視為 FAIL（全黑/爆白偵測）。
//!
//! 用法：cargo run -- <model.glb> <out_dir>

use std::path::{Path, PathBuf};
use std::sync::mpsc;

use glam::Vec3;
use wgpu::util::DeviceExt;
use wgpu::{Backends, PollType};

const WIDTH: u32 = 1024;
const HEIGHT: u32 = 1024;
const BG: [f32; 4] = [24.0 / 255.0, 26.0 / 255.0, 32.0 / 255.0, 1.0];

#[derive(Debug)]
struct Frame {
    name: String,
    rgba: Vec<u8>,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let model = args
        .get(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("DamagedHelmet.glb"));
    let out_dir = args
        .get(2)
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap());
    std::fs::create_dir_all(&out_dir).unwrap();

    let (tx, rx) = mpsc::channel();
    let model = model.clone();
    std::thread::Builder::new()
        .name("glb-spike-gpu".into())
        .spawn(move || {
            let frames = pollster::block_on(render_all(&model));
            let _ = tx.send(frames);
        })
        .unwrap();
    let frames = rx.recv().unwrap();
    let frames = match frames {
        Ok(f) => f,
        Err(e) => {
            eprintln!("FAIL: {e}");
            std::process::exit(1);
        }
    };

    let mut ok = true;
    for frame in &frames {
        let file = out_dir.join(format!("glb-spike-{}.png", frame.name));
        image::RgbaImage::from_raw(WIDTH, HEIGHT, frame.rgba.clone())
            .unwrap()
            .save(&file)
            .unwrap();
        // 前景 = 背景色之外的像素（背景 (24,26,32)）。
        let fg = frame
            .rgba
            .chunks(4)
            .filter(|px| !(px[0] < 40 && px[1] < 40 && px[2] < 48))
            .count();
        let ratio = fg as f64 / (WIDTH * HEIGHT) as f64;
        let good = (0.005..=0.60).contains(&ratio);
        println!(
            "{} {:?} 前景佔比 {:.1}%（期望 0.5%–60%）",
            if good { "PASS" } else { "FAIL" },
            file,
            ratio * 100.0
        );
        if !good {
            ok = false;
        }
    }
    if ok {
        println!("ALL PASS");
    } else {
        std::process::exit(1);
    }
}

/// 背景 thread：建 device、解析模型、渲染 3 個角度、readback 回傳。
async fn render_all(model: &Path) -> Result<Vec<Frame>, String> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: Backends::all(),
        ..Default::default()
    });
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        })
        .await
        .map_err(|e| format!("no GPU adapter: {e}"))?;
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("glb-spike"),
            ..Default::default()
        })
        .await
        .map_err(|e| format!("device request failed: {e}"))?;
    let info = adapter.get_info();
    println!("adapter: {:?} (backend: {:?})", info.name, info.backend);

    let (document, buffers, images) =
        gltf::import(model).map_err(|e| format!("gltf import failed: {e}"))?;
    let mut frames = Vec::new();
    let mut prim_idx = 0usize;

    for mesh in document.meshes() {
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
            let indices = reader.read_indices().map(|ri| match ri {
                gltf::mesh::util::ReadIndices::U8(i) => i.map(|x| x as u32).collect::<Vec<u32>>(),
                gltf::mesh::util::ReadIndices::U16(i) => i.map(|x| x as u32).collect::<Vec<u32>>(),
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
            if positions.is_empty() {
                continue;
            }
            let has_normals = reader.read_normals().is_some();
            let has_uvs = reader.read_tex_coords(0).is_some();
            println!(
                "primitive {prim_idx}: {} 頂點, {} 三角形, normals={}, uv={}",
                positions.len(),
                indices.as_ref().map(|i| i.len() / 3).unwrap_or(positions.len() / 3),
                has_normals,
                has_uvs
            );

            // ---- 貼圖：material.baseColorTexture → gltf image（已解碼）----
            let mut albedo: Option<(Vec<u8>, [u32; 2])> = None;
            let material = primitive.material();
            if let Some(tex) = material.pbr_metallic_roughness().base_color_texture() {
                if let Some(gimg) = images.get(tex.texture().source().index()) {
                    albedo = Some(rgba_from_gltf_image(gimg));
                }
            }

            // ---- 邊界球（相機用）---------------------------------------------
            let mut center = [0.0f32; 3];
            for p in &positions {
                for (k, c) in center.iter_mut().enumerate() {
                    *c += p[k];
                }
            }
            for k in 0..3 {
                center[k] /= positions.len() as f32;
            }
            let center_v = Vec3::new(center[0], center[1], center[2]);
            let mut radius = 0.0f32;
            for p in &positions {
                radius = radius.max((Vec3::from(*p) - center_v).length());
            }
            if radius <= 0.0 {
                radius = 1.0;
            }

            // ---- GPU 資源 -----------------------------------------------------
            #[repr(C)]
            #[derive(Clone, Copy, bytemuck::Zeroable, bytemuck::Pod)]
            struct Vertex {
                pos: [f32; 3],
                normal: [f32; 3],
                uv: [f32; 2],
            }
            let verts: Vec<Vertex> = positions
                .iter()
                .zip(normals.iter().zip(uvs.iter()))
                .map(|(p, (n, uv))| Vertex {
                    pos: *p,
                    normal: *n,
                    uv: *uv,
                })
                .collect();
            let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("vertices"),
                contents: bytemuck::cast_slice(&verts),
                usage: wgpu::BufferUsages::VERTEX,
            });
            let index_data: Vec<u32> = match indices {
                Some(idx) => idx,
                None => (0..positions.len() as u32).collect(),
            };
            let ibuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("indices"),
                contents: bytemuck::cast_slice(&index_data),
                usage: wgpu::BufferUsages::INDEX,
            });
            let index_count = index_data.len() as u32;
            let index_format = wgpu::IndexFormat::Uint32;

            let (albedo_view, _albedo_tex) = upload_albedo(&device, &queue, albedo);

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
                                sample_type: wgpu::TextureSampleType::Float {
                                    filterable: true,
                                },
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
                    cull_mode: None, // spike 先不 cull；Phase 1 再定 front face。
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

            // ---- 渲染 3 個角度 ---------------------------------------------------
            let fov = 45.0f32.to_radians();
            let dist = radius / (fov * 0.5).tan() * 1.35;
            let near = (radius * 0.01).max(0.001);
            let far = radius * 10.0;
            let light = Vec3::new(0.4, 0.8, 0.6).normalize();
            let views: [(&str, Vec3); 3] = [
                ("front", Vec3::new(0.0, 0.18, 1.0).normalize()),
                ("az45", Vec3::new(0.75, 0.18, 0.75).normalize()),
                ("elev30", Vec3::new(0.45, 0.6, 0.65).normalize()),
            ];

            for (name, dir) in views {
                let cam = center_v + dir * dist;
                let proj = glam::camera::rh::proj::directx::perspective(fov, WIDTH as f32 / HEIGHT as f32, near, far);
                let view = glam::camera::rh::view::look_at_mat4(cam, center_v, Vec3::Y);
                let mvp = proj * view;
                let uniform: [f32; 20] = {
                    let mut v: [f32; 20] = [0.0; 20];
                    v[..16].copy_from_slice(&mvp.to_cols_array());
                    v[16..19].copy_from_slice(&light.to_array());
                    v
                };
                let ubuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("uniforms"),
                    contents: bytemuck::cast_slice(&uniform),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
                let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
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
                            resource: wgpu::BindingResource::Buffer(
                                wgpu::BufferBinding {
                                    buffer: &ubuf,
                                    offset: 0,
                                    size: None, // 整個 buffer
                                },
                            ),
                        },
                    ],
                });

                let target = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("offscreen"),
                    size: wgpu::Extent3d {
                        width: WIDTH,
                        height: HEIGHT,
                        depth_or_array_layers: 1,
                    },
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
                    size: wgpu::Extent3d {
                        width: WIDTH,
                        height: HEIGHT,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Depth32Float,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                });
                let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());

                let mut encoder =
                    device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
                {
                    let mut pass =
                        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("render"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &target_view,
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
                                view: &depth_view,
                                depth_ops: Some(wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(1.0),
                                    store: wgpu::StoreOp::Store,
                                }),
                                stencil_ops: None,
                            }),
                            timestamp_writes: None,
                            occlusion_query_set: None,
                        });
                    pass.set_pipeline(&pipeline);
                    pass.set_bind_group(0, &bg, &[]);
                    pass.set_vertex_buffer(0, vbuf.slice(..));
                    pass.set_index_buffer(ibuf.slice(..), index_format);
                    pass.draw_indexed(0..index_count, 0, 0..1);
                }

                // ---- readback ----------------------------------------------------
                let byte_len = (WIDTH * HEIGHT * 4) as wgpu::BufferAddress;
                let staging = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("staging"),
                    size: byte_len,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                });
                encoder.copy_texture_to_buffer(
                    wgpu::TexelCopyTextureInfo {
                        texture: &target,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyBufferInfo {
                        buffer: &staging,
                        layout: wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(WIDTH * 4),
                            rows_per_image: None,
                        },
                    },
                    wgpu::Extent3d {
                        width: WIDTH,
                        height: HEIGHT,
                        depth_or_array_layers: 1,
                    },
                );
                let commands = encoder.finish();
                // copy 命令先提交（staging 此時必須 Idle，不能同時有 pending map）。
                queue.submit([commands]);
                device
                    .poll(PollType::Wait {
                        submission_index: None,
                        timeout: None,
                    })
                    .map_err(|e| format!("poll failed: {e}"))?;
                // copy 完成後再 map → read → unmap。
                let (done_tx, done_rx) = mpsc::channel();
                staging
                    .slice(..)
                    .map_async(wgpu::MapMode::Read, move |_| {
                        let _ = done_tx.send(());
                    });
                device
                    .poll(PollType::Wait {
                        submission_index: None,
                        timeout: None,
                    })
                    .map_err(|e| format!("poll failed: {e}"))?;
                let rgba = {
                    let view = staging.slice(..).get_mapped_range();
                    view.to_vec()
                };
                staging.unmap();
                frames.push(Frame {
                    name: name.to_string(),
                    rgba,
                });
                let _ = done_rx.try_recv();
            }
            prim_idx += 1;
        }
    }

    if frames.is_empty() {
        return Err("no triangle primitives rendered".into());
    }
    Ok(frames)
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
        // 16/32-bit 貼圖在 spike 不處理；退回佔位灰。
        _ => {
            out = vec![200u8, 200, 200, 255];
            return (out, [1, 1]);
        }
    }
    (out, [w, h])
}

fn upload_albedo(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    albedo: Option<(Vec<u8>, [u32; 2])>,
) -> (wgpu::TextureView, wgpu::Texture) {
    let (rgba, [w, h]) = match albedo {
        Some(x) => x,
        None => (vec![200u8, 200, 200, 255], [1u32, 1u32]),
    };
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("albedo"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
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
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    println!("albedo: {w}x{h}");
    (tex.create_view(&wgpu::TextureViewDescriptor::default()), tex)
}

const CODE: &str = r#"
struct Uniforms {
    mvp: mat4x4<f32>,
    light: vec3<f32>,
};
@group(0) @binding(2)
var<uniform> u: Uniforms;

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
    let a = textureSample(albedo, smp, in.uv);
    let ndl = max(dot(normalize(in.n), normalize(u.light)), 0.0);
    return vec4<f32>(a.rgb * (0.18 + 0.95 * ndl), 1.0);
}
"#;
