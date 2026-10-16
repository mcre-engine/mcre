use std::collections::HashMap;
use std::mem::size_of;
use std::sync::Arc;

use thiserror::Error;
use wgpu::util::DeviceExt;
use winit::window::Window;

use crate::atlas::Atlas;
use crate::mesh::{Mesh, Vertex};

const SKY: wgpu::Color = wgpu::Color {
    r: 0.53,
    g: 0.81,
    b: 0.92,
    a: 1.0,
};
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24Plus;
const ATLAS_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

const DEBUG_UNLIT: u32 = 1;
const DEBUG_HEATMAP: u32 = 2;
const DEBUG_FOG_OFF: u32 = 4;

const BORDER_CHUNK_RADIUS: i32 = 4;
const BORDER_CELL: f32 = 16.0;
const BORDER_Y: f32 = 0.06;

/// One chunk's mesh for a frame, with an identity the renderer uses to
/// cache GPU buffers across frames. `version` must change every time the
/// mesh data for `origin` is replaced; unchanged versions are never
/// re-uploaded.
pub struct RenderChunk<'a> {
    pub origin: [i32; 2],
    pub version: u64,
    pub mesh: &'a Mesh,
}

struct GpuMesh {
    version: u64,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    view_proj: [f32; 16],
    camera_pos: [f32; 4],
    /// Fog color in `rgb`, fog start distance in `a`.
    fog_color_start: [f32; 4],
    /// Fog end distance in `x`, sky-light factor in `y`.
    fog_end_sky: [f32; 4],
    /// Debug bits in `x` (`unlit`, `heatmap`, `fog_off`), `yzw` padding.
    flags: [u32; 4],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DebugFlags {
    pub unlit: bool,
    pub heatmap: bool,
    pub fog_off: bool,
}

pub struct EguiFrame<'a> {
    pub paint_jobs: &'a [egui::ClippedPrimitive],
    pub screen_descriptor: egui_wgpu::ScreenDescriptor,
    pub textures_delta: egui::TexturesDelta,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
struct BorderVertex {
    position: [f32; 3],
}

#[derive(Debug, Error)]
pub enum RendererError {
    #[error("no compatible GPU adapter found")]
    NoAdapter,
    #[error("surface is incompatible with the adapter (no supported formats)")]
    NoSurfaceFormats,
    #[error("failed to request adapter: {0}")]
    Adapter(#[from] wgpu::RequestAdapterError),
    #[error("failed to request device: {0}")]
    Device(#[from] wgpu::RequestDeviceError),
    #[error("failed to create surface: {0}")]
    Surface(#[from] wgpu::CreateSurfaceError),
}

/// Owned window handle wrapper: lets a `Surface<'static>` share the window
/// with the event loop via `Arc`.
struct SharedWindow(Arc<Window>);

impl wgpu::rwh::HasWindowHandle for SharedWindow {
    fn window_handle(&self) -> Result<wgpu::rwh::WindowHandle<'_>, wgpu::rwh::HandleError> {
        self.0.window_handle()
    }
}

impl wgpu::rwh::HasDisplayHandle for SharedWindow {
    fn display_handle(&self) -> Result<wgpu::rwh::DisplayHandle<'_>, wgpu::rwh::HandleError> {
        self.0.display_handle()
    }
}

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipelines: [wgpu::RenderPipeline; 4],
    wireframe: bool,
    cull_back: bool,
    debug_flags: DebugFlags,
    chunk_borders: bool,
    border_pipeline: wgpu::RenderPipeline,
    border_vertices: wgpu::Buffer,
    border_vertex_count: u32,
    egui_renderer: egui_wgpu::Renderer,
    bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    depth_view: wgpu::TextureView,
    mesh_cache: HashMap<[i32; 2], GpuMesh>,
    bind_group_layout: wgpu::BindGroupLayout,
    atlas_sampler: wgpu::Sampler,
    fog_color: [f32; 3],
    fog_start: f32,
    fog_end: f32,
    sky_factor: f32,
}

impl Renderer {
    pub async fn new(window: Arc<Window>, atlas: &Atlas) -> Result<Self, RendererError> {
        let size = window.inner_size();

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = instance.create_surface(SharedWindow(window.clone()))?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await?;

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .ok_or(RendererError::NoSurfaceFormats)?;

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("mcre-device"),
                required_features: wgpu::Features::POLYGON_MODE_LINE,
                ..Default::default()
            })
            .await?;

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        let set = create_pipeline(&device, &queue, atlas, config.format);
        let depth_view = create_depth_view(&device, config.width, config.height);
        let (border_vertices, border_vertex_count) = create_border_buffers(&device);
        let egui_renderer = egui_wgpu::Renderer::new(
            &device,
            config.format,
            egui_wgpu::RendererOptions::default(),
        );

        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipelines: set.mains,
            wireframe: false,
            cull_back: true,
            debug_flags: DebugFlags::default(),
            chunk_borders: false,
            border_pipeline: set.border,
            border_vertices,
            border_vertex_count,
            egui_renderer,
            bind_group: set.bind_group,
            bind_group_layout: set.bind_group_layout,
            atlas_sampler: set.atlas_sampler,
            uniform_buffer: set.uniform_buffer,
            depth_view,
            mesh_cache: HashMap::new(),
            fog_color: [SKY.r as f32, SKY.g as f32, SKY.b as f32],
            fog_start: 64.0,
            fog_end: 160.0,
            sky_factor: 1.0,
        })
    }

    /// Distance fog: color fades in between `start` and `end` world units
    /// from the camera.
    pub fn set_fog(&mut self, color: [f32; 3], start: f32, end: f32) {
        self.fog_color = color;
        self.fog_start = start;
        self.fog_end = end;
    }

    /// Multiplier applied to the sky-light channel (1.0 = full daylight).
    pub fn set_sky_factor(&mut self, factor: f32) {
        self.sky_factor = factor;
    }

    pub fn set_debug_flags(&mut self, flags: DebugFlags) {
        self.debug_flags = flags;
    }

    pub fn set_wireframe(&mut self, enabled: bool) {
        self.wireframe = enabled;
    }

    pub fn set_backface_culling(&mut self, enabled: bool) {
        self.cull_back = enabled;
    }

    pub fn set_chunk_borders(&mut self, enabled: bool) {
        self.chunk_borders = enabled;
    }

    /// Drops cached GPU buffers for chunk origins not in `keep`. The viewer
    /// calls this after pruning far-away chunk meshes so streaming over
    /// long distances cannot grow GPU memory without bound.
    pub fn prune_mesh_cache(&mut self, keep: &[[i32; 2]]) {
        self.mesh_cache.retain(|origin, _| keep.contains(origin));
    }

    /// Hot-swaps the texture atlas (pack reload): uploads new pixels and
    /// rebuilds the bind group. Pipelines and mesh caches are untouched;
    /// callers must remesh (UVs move) afterwards.
    pub fn set_atlas(&mut self, atlas: &Atlas) {
        let (_texture, view) = create_atlas_texture(&self.device, &self.queue, atlas);
        self.bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mcre-bind-group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.atlas_sampler),
                },
            ],
        });
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(&self.device, &self.config);
        self.depth_view = create_depth_view(&self.device, self.config.width, self.config.height);
    }

    pub fn render(
        &mut self,
        view_proj: [f32; 16],
        camera_pos: [f32; 3],
        chunks: &[RenderChunk<'_>],
        egui: Option<EguiFrame<'_>>,
    ) {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame-encoder"),
            });

        let mut command_buffers = Vec::new();
        let mut egui_frame = egui;
        let mut egui_draw = None;
        if let Some(egui) = egui_frame.as_mut() {
            for (id, deltas) in &egui.textures_delta.set {
                for delta in deltas {
                    self.egui_renderer
                        .update_texture(&self.device, &self.queue, *id, delta);
                }
            }
            egui.textures_delta.set.clear();
            command_buffers = self.egui_renderer.update_buffers(
                &self.device,
                &self.queue,
                &mut encoder,
                egui.paint_jobs,
                &egui.screen_descriptor,
            );
            egui_draw = Some((egui.paint_jobs, &egui.screen_descriptor));
        }

        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                if let Some(egui) = egui_frame.as_mut() {
                    for id in egui.textures_delta.free.drain() {
                        self.egui_renderer.free_texture(&id);
                    }
                }
                command_buffers.push(encoder.finish());
                self.queue.submit(command_buffers);
                return;
            }
            wgpu::CurrentSurfaceTexture::Outdated
            | wgpu::CurrentSurfaceTexture::Lost
            | wgpu::CurrentSurfaceTexture::Validation => {
                if let Some(egui) = egui_frame.as_mut() {
                    for id in egui.textures_delta.free.drain() {
                        self.egui_renderer.free_texture(&id);
                    }
                }
                command_buffers.push(encoder.finish());
                self.queue.submit(command_buffers);
                self.surface.configure(&self.device, &self.config);
                return;
            }
        };

        let uniforms = Uniforms {
            view_proj,
            camera_pos: [camera_pos[0], camera_pos[1], camera_pos[2], 0.0],
            fog_color_start: [
                self.fog_color[0],
                self.fog_color[1],
                self.fog_color[2],
                self.fog_start,
            ],
            fog_end_sky: [self.fog_end, self.sky_factor, 0.0, 0.0],
            flags: [debug_bits(&self.debug_flags), 0, 0, 0],
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut first = true;
        for chunk in chunks {
            let stale = self
                .mesh_cache
                .get(&chunk.origin)
                .is_none_or(|cached| cached.version != chunk.version);
            if stale {
                let (vertex_buffer, index_buffer, index_count) =
                    create_mesh_buffers(&self.device, chunk.mesh);
                self.mesh_cache.insert(
                    chunk.origin,
                    GpuMesh {
                        version: chunk.version,
                        vertex_buffer,
                        index_buffer,
                        index_count,
                    },
                );
            }
            let cached = &self.mesh_cache[&chunk.origin];
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: if first {
                            wgpu::LoadOp::Clear(SKY)
                        } else {
                            wgpu::LoadOp::Load
                        },
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: if first {
                            wgpu::LoadOp::Clear(1.0)
                        } else {
                            wgpu::LoadOp::Load
                        },
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            pass.set_pipeline(&self.pipelines[pipeline_index(self.wireframe, self.cull_back)]);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, cached.vertex_buffer.slice(..));
            pass.set_index_buffer(cached.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..cached.index_count, 0, 0..1);
            first = false;
        }
        if first {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("clear-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(SKY),
                        store: wgpu::StoreOp::Store,
                    },
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
                multiview_mask: None,
            });
            drop(pass);
        }

        if self.chunk_borders {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("borders-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            pass.set_pipeline(&self.border_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, self.border_vertices.slice(..));
            pass.draw(0..self.border_vertex_count, 0..1);
        }

        if let Some((paint_jobs, screen_descriptor)) = egui_draw {
            {
                let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("egui-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                self.egui_renderer.render(
                    &mut pass.forget_lifetime(),
                    paint_jobs,
                    screen_descriptor,
                );
            }
            if let Some(egui) = egui_frame.as_mut() {
                for id in egui.textures_delta.free.drain() {
                    self.egui_renderer.free_texture(&id);
                }
            }
        }

        command_buffers.push(encoder.finish());
        self.queue.submit(command_buffers);
        self.queue.present(frame);
    }
}

fn pipeline_index(wireframe: bool, cull_back: bool) -> usize {
    ((wireframe as usize) << 1) | ((!cull_back) as usize)
}

fn debug_bits(flags: &DebugFlags) -> u32 {
    let mut bits = 0;
    if flags.unlit {
        bits |= DEBUG_UNLIT;
    }
    if flags.heatmap {
        bits |= DEBUG_HEATMAP;
    }
    if flags.fog_off {
        bits |= DEBUG_FOG_OFF;
    }
    bits
}

fn border_vertices() -> Vec<BorderVertex> {
    let mut vertices = Vec::new();
    let extent = BORDER_CHUNK_RADIUS as f32 * BORDER_CELL;
    for line in -BORDER_CHUNK_RADIUS..=BORDER_CHUNK_RADIUS {
        let coord = line as f32 * BORDER_CELL;
        vertices.push(BorderVertex {
            position: [coord, BORDER_Y, -extent],
        });
        vertices.push(BorderVertex {
            position: [coord, BORDER_Y, extent],
        });
        vertices.push(BorderVertex {
            position: [-extent, BORDER_Y, coord],
        });
        vertices.push(BorderVertex {
            position: [extent, BORDER_Y, coord],
        });
    }
    vertices
}

struct PipelineSet {
    mains: [wgpu::RenderPipeline; 4],
    border: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    bind_group_layout: wgpu::BindGroupLayout,
    atlas_sampler: wgpu::Sampler,
    uniform_buffer: wgpu::Buffer,
}

/// Uploads atlas pixels into a fresh texture + view. Split out so hot reload
/// can replace the atlas without rebuilding pipelines.
fn create_atlas_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    atlas: &Atlas,
) -> (wgpu::Texture, wgpu::TextureView) {
    let atlas_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("atlas"),
        size: wgpu::Extent3d {
            width: atlas.width,
            height: atlas.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: ATLAS_FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    if !atlas.pixels.is_empty() {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &atlas_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &atlas.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(atlas.width * 4),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: atlas.width,
                height: atlas.height,
                depth_or_array_layers: 1,
            },
        );
    }

    let atlas_view = atlas_texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some("atlas-view"),
        format: None,
        dimension: None,
        usage: None,
        aspect: wgpu::TextureAspect::All,
        base_mip_level: 0,
        mip_level_count: None,
        base_array_layer: 0,
        array_layer_count: None,
    });
    (atlas_texture, atlas_view)
}

fn create_pipeline(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    atlas: &Atlas,
    surface_format: wgpu::TextureFormat,
) -> PipelineSet {
    let (_atlas_texture, atlas_view) = create_atlas_texture(device, queue, atlas);

    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("atlas-sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        ..Default::default()
    });

    let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("frame-params"),
        size: size_of::<Uniforms>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("mcre-layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
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
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("mcre-bind-group"),
        layout: &bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&atlas_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("mcre-pipeline-layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("mcre-shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
    });

    let mains = [
        build_main_pipeline(
            device,
            &pipeline_layout,
            &shader,
            surface_format,
            wgpu::PolygonMode::Fill,
            Some(wgpu::Face::Back),
        ),
        build_main_pipeline(
            device,
            &pipeline_layout,
            &shader,
            surface_format,
            wgpu::PolygonMode::Fill,
            None,
        ),
        build_main_pipeline(
            device,
            &pipeline_layout,
            &shader,
            surface_format,
            wgpu::PolygonMode::Line,
            Some(wgpu::Face::Back),
        ),
        build_main_pipeline(
            device,
            &pipeline_layout,
            &shader,
            surface_format,
            wgpu::PolygonMode::Line,
            None,
        ),
    ];

    let border = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("mcre-border-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_line"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: size_of::<BorderVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3],
            })],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::LineList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_line"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: surface_format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });

    PipelineSet {
        mains,
        border,
        bind_group,
        bind_group_layout,
        atlas_sampler: sampler,
        uniform_buffer,
    }
}

fn build_main_pipeline(
    device: &wgpu::Device,
    pipeline_layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    surface_format: wgpu::TextureFormat,
    polygon_mode: wgpu::PolygonMode,
    cull_mode: Option<wgpu::Face>,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("mcre-pipeline"),
        layout: Some(pipeline_layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: size_of::<Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![
                    0 => Float32x3,
                    1 => Float32x2,
                    2 => Float32x3,
                    3 => Float32x2
                ],
            })],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode,
            unclipped_depth: false,
            polygon_mode,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            // Less-equal (vanilla terrain uses greater-equal with its
            // reverse-Z depth): coplanar overlay faces (grass/mycelium side
            // overlays) must resolve instead of losing to the base cube.
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: surface_format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn create_depth_view(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

fn create_mesh_buffers(device: &wgpu::Device, mesh: &Mesh) -> (wgpu::Buffer, wgpu::Buffer, u32) {
    let vertex_contents: &[u8] = if mesh.vertices.is_empty() {
        &[0u8; 4]
    } else {
        bytemuck::cast_slice(&mesh.vertices)
    };
    let index_contents: &[u8] = if mesh.indices.is_empty() {
        &[0u8; 4]
    } else {
        bytemuck::cast_slice(&mesh.indices)
    };

    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("mesh-vertices"),
        contents: vertex_contents,
        usage: wgpu::BufferUsages::VERTEX,
    });
    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("mesh-indices"),
        contents: index_contents,
        usage: wgpu::BufferUsages::INDEX,
    });

    (vertex_buffer, index_buffer, mesh.indices.len() as u32)
}

fn create_border_buffers(device: &wgpu::Device) -> (wgpu::Buffer, u32) {
    let vertices = border_vertices();
    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("border-lines"),
        contents: bytemuck::cast_slice(&vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });

    (buffer, vertices.len() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_defaults_to_fill_back_cull() {
        assert_eq!(pipeline_index(false, true), 0);
    }

    #[test]
    fn pipeline_covers_all_wireframe_cull_combos() {
        let indices = [
            pipeline_index(false, true),
            pipeline_index(false, false),
            pipeline_index(true, true),
            pipeline_index(true, false),
        ];
        let mut sorted = indices;
        sorted.sort_unstable();
        assert_eq!(sorted, [0, 1, 2, 3]);
    }

    #[test]
    fn debug_flags_default_to_no_bits() {
        assert_eq!(
            DebugFlags::default(),
            DebugFlags {
                unlit: false,
                heatmap: false,
                fog_off: false,
            }
        );
        assert_eq!(debug_bits(&DebugFlags::default()), 0);
    }

    #[test]
    fn debug_flags_pack_to_bits() {
        assert_eq!(
            debug_bits(&DebugFlags {
                unlit: true,
                ..Default::default()
            }),
            DEBUG_UNLIT
        );
        assert_eq!(
            debug_bits(&DebugFlags {
                heatmap: true,
                ..Default::default()
            }),
            DEBUG_HEATMAP
        );
        assert_eq!(
            debug_bits(&DebugFlags {
                fog_off: true,
                ..Default::default()
            }),
            DEBUG_FOG_OFF
        );
        assert_eq!(
            debug_bits(&DebugFlags {
                unlit: true,
                heatmap: true,
                fog_off: true,
            }),
            DEBUG_UNLIT | DEBUG_HEATMAP | DEBUG_FOG_OFF
        );
    }

    #[test]
    fn uniforms_stay_16_byte_aligned() {
        assert_eq!(size_of::<Uniforms>(), 128);
        assert_eq!(size_of::<Uniforms>() % 16, 0);
        assert_eq!(std::mem::offset_of!(Uniforms, flags), 112);
    }

    #[test]
    fn border_grid_uses_16_block_spacing() {
        let vertices = border_vertices();
        let lines_per_axis = (BORDER_CHUNK_RADIUS * 2 + 1) as usize;
        assert_eq!(vertices.len(), lines_per_axis * 4);
        let extent = BORDER_CHUNK_RADIUS as f32 * BORDER_CELL;
        for vertex in &vertices {
            assert!((vertex.position[1] - BORDER_Y).abs() < 1e-6);
            assert_eq!(vertex.position[0].rem_euclid(BORDER_CELL), 0.0);
            assert_eq!(vertex.position[2].rem_euclid(BORDER_CELL), 0.0);
            assert!(vertex.position[0].abs() <= extent);
            assert!(vertex.position[2].abs() <= extent);
        }
        assert!(vertices.contains(&BorderVertex {
            position: [extent, BORDER_Y, -extent],
        }));
        assert!(vertices.contains(&BorderVertex {
            position: [-extent, BORDER_Y, extent],
        }));
    }
}
