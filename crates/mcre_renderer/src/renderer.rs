use std::collections::HashMap;
use std::mem::size_of;
use std::sync::Arc;

use thiserror::Error;
use wgpu::util::DeviceExt;
use winit::window::Window;

use crate::atlas::Atlas;
use crate::clouds::{
    CELL_HEIGHT, CELL_SIZE, CLOUD_HEIGHT, CloudStatus, CloudTexture, CloudVertex,
    DEFAULT_CLOUD_RANGE, RelativeCameraPos, build_cloud_mesh, cloud_fog_end, cloud_origin,
    radius_cells,
};
use crate::lightmap::{LightmapParams, lightmap_rgba, sky_light_color, sky_light_factor};
use crate::mesh::SectionMesh;
use crate::region::{
    RegionKey, RenderRegion, SharedQuadIndexBuffer, chunk_section_slot, merge_visible_runs,
    section_world_origin, visible_faces,
};
use crate::vertex::{FormatFlags, TerrainPass};

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

/// Fixed section-table storage capacity (world origins, one per draw).
/// Pre-allocated so the bind group never needs rebuilding mid-frame.
const SECTION_TABLE_CAPACITY: usize = 1 << 16;

const TERRAIN_SHADER: &str = concat!(
    include_str!("terrain_vertex.wgsl"),
    "\n",
    include_str!("shader.wgsl"),
);

/// One chunk's packed sections for a frame, with an identity the renderer
/// uses to cache region allocations across frames. `version` must change
/// every time the section data for `origin` is replaced.
pub struct RenderChunk<'a> {
    pub origin: [i32; 2],
    pub version: u64,
    pub sections: &'a [Option<SectionMesh>; mcre_world::SECTIONS_PER_CHUNK],
    /// 24-bit mask of chunk-local sections to draw this frame (frustum).
    /// Uploads on version change still cover every section.
    pub visible_sections: u32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CloudUniforms {
    color: [f32; 4],
    offset: [f32; 4],
    cell_size: [f32; 4],
}

/// Identity of the generated cloud mesh: a new mesh is only built when the
/// camera crosses a cell boundary or the status/relative position changes.
type CloudMeshKey = (CloudStatus, i32, i32, RelativeCameraPos);

struct CloudMeshGpu {
    key: CloudMeshKey,
    vertex_buffer: Option<wgpu::Buffer>,
    vertex_count: u32,
}

struct CloudGpu {
    pipeline_fancy: wgpu::RenderPipeline,
    pipeline_flat: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    mesh: Option<CloudMeshGpu>,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    view_proj: [f32; 16],
    camera_pos: [f32; 4],
    /// Fog color in `rgb`, fog start distance in `a`.
    fog_color_start: [f32; 4],
    /// Fog end distance in `x`; cloud fog fade-out distance in `y`; the
    /// texcoord shrink epsilon in `z`.
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
    #[error("capture buffer map failed: {0}")]
    CaptureMap(#[from] wgpu::BufferAsyncError),
    #[error("capture device poll failed: {0}")]
    CapturePoll(#[from] wgpu::PollError),
    #[error("capture readback timed out")]
    CaptureTimeout,
    #[error("capture map range failed: {0}")]
    CaptureRange(String),
    #[error("capture map channel closed")]
    CaptureChannel,
}

/// Offscreen render target + readback buffer for [`Renderer::capture`].
/// Reused across captures; only recreated when the size changes.
struct CaptureTarget {
    color: wgpu::Texture,
    color_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    buffer: wgpu::Buffer,
    width: u32,
    height: u32,
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

struct TerrainDraw {
    key: RegionKey,
    base_vertex: u32,
    vertex_count: u32,
    table_index: u32,
}

struct TranslucentSectionDraw {
    key: RegionKey,
    slot: usize,
    table_index: u32,
    center: [f32; 3],
    base_vertex: u32,
    index_count: u32,
}

/// Last-frame terrain submission counters, for the viewer debug UI.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameStats {
    pub opaque_draws: u32,
    pub translucent_draws: u32,
    pub sections: u32,
    pub arena_used: u64,
    pub arena_capacity: u64,
}

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipelines: [wgpu::RenderPipeline; 4],
    translucent_pipelines: [wgpu::RenderPipeline; 4],
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
    /// Chunk origin → mesh version; the region arenas own the GPU data.
    mesh_cache: HashMap<[i32; 2], u64>,
    regions: HashMap<RegionKey, RenderRegion>,
    shared_indices: SharedQuadIndexBuffer,
    section_table: wgpu::Buffer,
    bind_group_layout: wgpu::BindGroupLayout,
    atlas_sampler: wgpu::Sampler,
    lightmap_sampler: wgpu::Sampler,
    lightmap_texture: wgpu::Texture,
    lightmap_view: wgpu::TextureView,
    /// The lightmap parameters the texture was baked for.
    lightmap_key: LightmapParams,
    /// Sodium `u_TexCoordShrink` (8-bit subtexel anti-bleed epsilon).
    tex_coord_shrink: f32,
    fog_color: [f32; 3],
    fog_start: f32,
    fog_end: f32,
    lightmap_params: LightmapParams,
    cloud_texture: Option<CloudTexture>,
    cloud_gpu: Option<CloudGpu>,
    cloud_status: CloudStatus,
    cloud_color: [f32; 4],
    cloud_height: f32,
    cloud_range_chunks: u32,
    cloud_time: (i64, f32),
    stats: FrameStats,
    capture: Option<CaptureTarget>,
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

        let section_table = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("section-table"),
            size: SECTION_TABLE_CAPACITY as u64 * size_of::<[f32; 4]>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let set = create_pipeline(&device, &queue, atlas, config.format, &section_table);
        let depth_view = create_depth_view(&device, config.width, config.height);
        let (border_vertices, border_vertex_count) = create_border_buffers(&device);
        let egui_renderer = egui_wgpu::Renderer::new(
            &device,
            config.format,
            egui_wgpu::RendererOptions::default(),
        );
        let shared_indices = SharedQuadIndexBuffer::new(&device);

        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipelines: set.mains,
            translucent_pipelines: set.translucent,
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
            lightmap_sampler: set.lightmap_sampler,
            lightmap_texture: set.lightmap_texture,
            lightmap_view: set.lightmap_view,
            lightmap_key: set.lightmap_key,
            uniform_buffer: set.uniform_buffer,
            depth_view,
            mesh_cache: HashMap::new(),
            regions: HashMap::new(),
            shared_indices,
            section_table,
            tex_coord_shrink: shrink_for_width(atlas.width),
            fog_color: [SKY.r as f32, SKY.g as f32, SKY.b as f32],
            fog_start: 64.0,
            fog_end: 160.0,
            lightmap_params: LightmapParams::default(),
            cloud_texture: None,
            cloud_gpu: set.clouds,
            cloud_status: CloudStatus::default(),
            cloud_color: [1.0, 1.0, 1.0, 0.8],
            cloud_height: CLOUD_HEIGHT,
            cloud_range_chunks: DEFAULT_CLOUD_RANGE,
            cloud_time: (0, 0.0),
            stats: FrameStats::default(),
            capture: None,
        })
    }

    /// Terrain submission counters from the most recent `render` call.
    pub fn frame_stats(&self) -> FrameStats {
        self.stats
    }

    /// Distance fog: color fades in between `start` and `end` world units
    /// from the camera.
    pub fn set_fog(&mut self, color: [f32; 3], start: f32, end: f32) {
        self.fog_color = color;
        self.fog_start = start;
        self.fog_end = end;
    }

    /// Vanilla `Timelines.OVERWORLD_DAY` environment: sets the sky light
    /// factor and color for `time_ticks` (0..24000, 0 = sunrise). Re-bakes
    /// the lightmap texture when the values change.
    pub fn set_time(&mut self, time_ticks: f32) {
        self.set_lightmap_params(LightmapParams {
            sky_factor: sky_light_factor(time_ticks),
            sky_light_color: sky_light_color(time_ticks),
            ..self.lightmap_params
        });
    }

    /// Multiplier applied to the sky-light channel (1.0 = full daylight).
    /// Re-bakes the lightmap texture when the value changes.
    pub fn set_sky_factor(&mut self, factor: f32) {
        self.set_lightmap_params(LightmapParams {
            sky_factor: factor,
            ..self.lightmap_params
        });
    }

    /// `LightmapInfo.SkyLightColor` tint (white by day, bluish moonlight at
    /// night). Re-bakes the lightmap texture when the value changes.
    pub fn set_sky_light_color(&mut self, color: [f32; 3]) {
        self.set_lightmap_params(LightmapParams {
            sky_light_color: color,
            ..self.lightmap_params
        });
    }

    /// Vanilla `Options.gamma` in `0.0..=1.0`. Re-bakes the lightmap
    /// texture when the value changes.
    pub fn set_brightness(&mut self, brightness: f32) {
        self.set_lightmap_params(LightmapParams {
            brightness: brightness.clamp(0.0, 1.0),
            ..self.lightmap_params
        });
    }

    fn set_lightmap_params(&mut self, params: LightmapParams) {
        if params != self.lightmap_params {
            self.lightmap_params = params;
            self.refresh_lightmap();
        }
    }

    fn refresh_lightmap(&mut self) {
        let key = self.lightmap_params;
        if key == self.lightmap_key {
            return;
        }
        self.lightmap_key = key;
        upload_lightmap(&self.queue, &self.lightmap_texture, &key);
    }

    /// Replaces the cloud cell texture (pack reload); `None` disables clouds
    /// until a texture is available again.
    pub fn set_cloud_texture(&mut self, texture: Option<CloudTexture>) {
        self.cloud_texture = texture;
        if let Some(clouds) = self.cloud_gpu.as_mut() {
            clouds.mesh = None;
        }
    }

    pub fn set_cloud_status(&mut self, status: CloudStatus) {
        self.cloud_status = status;
    }

    /// Vanilla `CLOUD_COLOR` (rgb with its alpha).
    pub fn set_cloud_color(&mut self, color: [f32; 4]) {
        self.cloud_color = color;
    }

    /// Vanilla `CLOUD_HEIGHT` (192.33 in the overworld).
    pub fn set_cloud_height(&mut self, height: f32) {
        self.cloud_height = height;
    }

    /// Cloud mesh radius in chunks; the layer also fades out by
    /// `min(chunks * 16, 2048)` blocks, so the mesh edge is invisible.
    pub fn set_cloud_range(&mut self, chunks: u32) {
        self.cloud_range_chunks = chunks;
        if let Some(clouds) = self.cloud_gpu.as_mut() {
            clouds.mesh = None;
        }
    }

    /// `game_ticks` drives the cloud drift; it advances regardless of the
    /// day/night cycle, like vanilla world time.
    pub fn set_cloud_time(&mut self, game_ticks: i64, partial_ticks: f32) {
        self.cloud_time = (game_ticks, partial_ticks);
    }

    /// Rebuilds the cell-anchored cloud mesh if the camera crossed a cell or
    /// the status changed, and refreshes the cloud uniform (color, offset,
    /// cell size).
    fn prepare_clouds(&mut self, camera_pos: [f32; 3]) {
        if self.cloud_status == CloudStatus::Off || self.cloud_texture.is_none() {
            return;
        }
        let (origin, key, vertices) = {
            let texture = self.cloud_texture.as_ref().expect("checked above");
            let origin = cloud_origin(
                texture.width,
                texture.height,
                self.cloud_height,
                camera_pos,
                self.cloud_time.0,
                self.cloud_time.1,
            );
            let key = (
                self.cloud_status,
                origin.cell_x,
                origin.cell_z,
                origin.relative_pos,
            );
            let stale = self
                .cloud_gpu
                .as_ref()
                .and_then(|clouds| clouds.mesh.as_ref())
                .is_none_or(|mesh| mesh.key != key);
            let vertices = stale.then(|| {
                build_cloud_mesh(
                    texture,
                    self.cloud_status,
                    radius_cells(self.cloud_range_chunks),
                    origin.cell_x,
                    origin.cell_z,
                    origin.relative_pos,
                )
            });
            (origin, key, vertices)
        };

        let Some(clouds) = self.cloud_gpu.as_mut() else {
            return;
        };
        if let Some(vertices) = vertices {
            clouds.mesh = Some(CloudMeshGpu {
                key,
                vertex_buffer: create_cloud_buffer(&self.device, &vertices),
                vertex_count: vertices.len() as u32,
            });
        }

        let uniforms = CloudUniforms {
            color: self.cloud_color,
            offset: [origin.offset[0], origin.offset[1], origin.offset[2], 0.0],
            cell_size: [CELL_SIZE, CELL_HEIGHT, CELL_SIZE, 0.0],
        };
        self.queue
            .write_buffer(&clouds.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
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

    /// Drops region allocations for chunk origins not in `keep`. The viewer
    /// calls this after pruning far-away chunk meshes so streaming over
    /// long distances cannot grow GPU memory without bound.
    pub fn prune_mesh_cache(&mut self, keep: &[[i32; 2]]) {
        let removed: Vec<[i32; 2]> = self
            .mesh_cache
            .keys()
            .filter(|origin| !keep.contains(origin))
            .copied()
            .collect();
        for origin in removed {
            self.free_chunk(origin);
            self.mesh_cache.remove(&origin);
        }
        self.regions.retain(|_, region| !region.is_empty());
    }

    fn free_chunk(&mut self, origin: [i32; 2]) {
        for section in 0..mcre_world::SECTIONS_PER_CHUNK {
            let (key, slot) = chunk_section_slot(origin, section);
            if let Some(region) = self.regions.get_mut(&key) {
                region.clear_section(slot);
            }
        }
    }

    fn upload_chunk(&mut self, origin: [i32; 2], sections: &[Option<SectionMesh>]) {
        for (index, section) in sections.iter().enumerate() {
            let (key, slot) = chunk_section_slot(origin, index);
            let region = self
                .regions
                .entry(key)
                .or_insert_with(|| RenderRegion::new(&self.device, key));
            match section {
                Some(mesh) => region.update_section(&self.device, &self.queue, slot, mesh),
                None => region.clear_section(slot),
            }
        }
    }

    /// Hot-swaps the texture atlas (pack reload): uploads new pixels and
    /// rebuilds the bind group. Pipelines and region caches are untouched;
    /// callers must remesh (UVs move) afterwards.
    pub fn set_atlas(&mut self, atlas: &Atlas) {
        let (_texture, view) = create_atlas_texture(&self.device, &self.queue, atlas);
        self.tex_coord_shrink = shrink_for_width(atlas.width);
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
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&self.lightmap_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: self.section_table.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(&self.lightmap_sampler),
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

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let depth_view = self.depth_view.clone();

        command_buffers.extend(self.record(
            &mut encoder,
            &view,
            &depth_view,
            egui_draw,
            view_proj,
            camera_pos,
            chunks,
        ));
        if let Some(egui) = egui_frame.as_mut() {
            for id in egui.textures_delta.free.drain() {
                self.egui_renderer.free_texture(&id);
            }
        }

        command_buffers.push(encoder.finish());
        self.queue.submit(command_buffers);
        self.queue.present(frame);
    }

    /// Encode the terrain/sky/clouds passes (plus optional egui) into an
    /// existing encoder targeting `view`. Returns extra command buffers from
    /// egui buffer updates; empty without egui.
    #[allow(clippy::too_many_arguments)]
    fn record<'egui>(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
        egui_draw: Option<(
            &'egui [egui::ClippedPrimitive],
            &'egui egui_wgpu::ScreenDescriptor,
        )>,
        view_proj: [f32; 16],
        camera_pos: [f32; 3],
        chunks: &[RenderChunk<'_>],
    ) -> Vec<wgpu::CommandBuffer> {
        let command_buffers: Vec<wgpu::CommandBuffer> = Vec::new();

        self.prepare_clouds(camera_pos);

        for region in self.regions.values_mut() {
            region.begin_frame();
        }
        for chunk in chunks {
            let stale = self
                .mesh_cache
                .get(&chunk.origin)
                .is_none_or(|version| *version != chunk.version);
            if stale {
                self.upload_chunk(chunk.origin, chunk.sections);
                self.mesh_cache.insert(chunk.origin, chunk.version);
            }
        }

        let mut section_table: Vec<[f32; 4]> = Vec::new();
        let mut opaque: Vec<TerrainDraw> = Vec::new();
        let mut translucent: Vec<TranslucentSectionDraw> = Vec::new();
        let mut sections_drawn = 0u32;
        'chunks: for chunk in chunks {
            for index in 0..mcre_world::SECTIONS_PER_CHUNK {
                if chunk.visible_sections & (1 << index) == 0 {
                    continue;
                }
                let (key, slot) = chunk_section_slot(chunk.origin, index);
                let Some(region) = self.regions.get(&key) else {
                    continue;
                };
                let Some(alloc) = region.section(slot) else {
                    continue;
                };
                let origin = section_world_origin(chunk.origin, index);
                let center = [
                    origin[0] as f32 + 8.0,
                    origin[1] as f32 + 8.0,
                    origin[2] as f32 + 8.0,
                ];
                sections_drawn += 1;
                for (pass_index, pass) in alloc.passes.iter().enumerate() {
                    if pass.is_empty() {
                        continue;
                    }
                    if section_table.len() >= SECTION_TABLE_CAPACITY {
                        break 'chunks;
                    }
                    let table_index = section_table.len() as u32;
                    section_table.push([origin[0] as f32, origin[1] as f32, origin[2] as f32, 0.0]);
                    if pass_index == TerrainPass::Translucent.index() {
                        translucent.push(TranslucentSectionDraw {
                            key,
                            slot,
                            table_index,
                            center,
                            base_vertex: pass.base_vertex,
                            index_count: 0,
                        });
                        continue;
                    }
                    let faces = visible_faces(camera_pos, origin);
                    let runs = merge_visible_runs(&pass.counts, pass.base_vertex, faces);
                    for (base_vertex, vertex_count) in runs {
                        opaque.push(TerrainDraw {
                            key,
                            base_vertex,
                            vertex_count,
                            table_index,
                        });
                    }
                }
            }
        }

        for draw in &mut translucent {
            let Some(region) = self.regions.get_mut(&draw.key) else {
                continue;
            };
            if let Some((base_vertex, index_count)) =
                region.sort_translucent(&self.device, &self.queue, draw.slot, camera_pos)
            {
                draw.base_vertex = base_vertex;
                draw.index_count = index_count;
            }
        }
        translucent.retain(|draw| draw.index_count > 0);
        translucent.sort_by(|a, b| {
            center_distance_sq(b.center, camera_pos)
                .total_cmp(&center_distance_sq(a.center, camera_pos))
        });

        self.stats = FrameStats {
            opaque_draws: opaque.len() as u32,
            translucent_draws: translucent.len() as u32,
            sections: sections_drawn,
            arena_used: self.regions.values().map(|r| r.arena_usage().0).sum(),
            arena_capacity: self.regions.values().map(|r| r.arena_usage().1).sum(),
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
            fog_end_sky: [
                self.fog_end,
                cloud_fog_end(self.cloud_range_chunks),
                self.tex_coord_shrink,
                0.0,
            ],
            flags: [debug_bits(&self.debug_flags), 0, 0, 0],
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
        if !section_table.is_empty() {
            self.queue
                .write_buffer(&self.section_table, 0, bytemuck::cast_slice(&section_table));
        }

        let max_quads = opaque
            .iter()
            .map(|draw| draw.vertex_count / 4)
            .max()
            .unwrap_or(0);
        self.shared_indices.ensure_quads(&self.device, max_quads);

        if chunks.is_empty() {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("clear-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(SKY),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth_view,
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
        } else {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(SKY),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth_view,
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
            pass.set_pipeline(&self.pipelines[pipeline_index(self.wireframe, self.cull_back)]);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_index_buffer(
                self.shared_indices.buffer().slice(..),
                wgpu::IndexFormat::Uint32,
            );
            for draw in &opaque {
                let region = &self.regions[&draw.key];
                pass.set_vertex_buffer(0, region.vertex_buffer().slice(..));
                pass.draw_indexed(
                    0..draw.vertex_count / 4 * 6,
                    draw.base_vertex as i32,
                    draw.table_index..draw.table_index + 1,
                );
            }
        }

        if !translucent.is_empty() {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("translucent-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth_view,
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
            pass.set_pipeline(
                &self.translucent_pipelines[pipeline_index(self.wireframe, self.cull_back)],
            );
            pass.set_bind_group(0, &self.bind_group, &[]);
            for draw in &translucent {
                let region = &self.regions[&draw.key];
                let Some(indices) = region.translucent_index_buffer(draw.slot) else {
                    continue;
                };
                pass.set_vertex_buffer(0, region.vertex_buffer().slice(..));
                pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(
                    0..draw.index_count,
                    draw.base_vertex as i32,
                    draw.table_index..draw.table_index + 1,
                );
            }
        }

        let cloud_draw = self.cloud_gpu.as_ref().and_then(|clouds| {
            clouds
                .mesh
                .as_ref()
                .filter(|mesh| self.cloud_status != CloudStatus::Off && mesh.vertex_count > 0)
                .and_then(|mesh| {
                    mesh.vertex_buffer
                        .as_ref()
                        .map(|buffer| (clouds, mesh, buffer))
                })
        });
        if let Some((clouds, mesh, vertex_buffer)) = cloud_draw {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("clouds-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth_view,
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
            pass.set_pipeline(if self.cloud_status == CloudStatus::Fancy {
                &clouds.pipeline_fancy
            } else {
                &clouds.pipeline_flat
            });
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_bind_group(1, &clouds.bind_group, &[]);
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            pass.draw(0..mesh.vertex_count, 0..1);
        }

        if self.chunk_borders {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("borders-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth_view,
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
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
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
            self.egui_renderer
                .render(&mut pass.forget_lifetime(), paint_jobs, screen_descriptor);
        }

        command_buffers
    }

    /// Render one frame offscreen at `width`x`height` and read it back as
    /// RGBA8 pixels (top-left origin). The window surface is not touched.
    pub fn capture(
        &mut self,
        width: u32,
        height: u32,
        view_proj: [f32; 16],
        camera_pos: [f32; 3],
        chunks: &[RenderChunk<'_>],
    ) -> Result<Vec<u8>, RendererError> {
        const ALIGN: u32 = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        const POLL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
        let bytes_per_row = width * 4;
        let aligned_bytes_per_row = bytes_per_row.div_ceil(ALIGN) * ALIGN;

        let stale = self
            .capture
            .as_ref()
            .is_none_or(|target| target.width != width || target.height != height);
        if stale {
            let color = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("capture-color"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.config.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
            let depth_view = create_depth_view(&self.device, width, height);
            let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("capture-buffer"),
                size: u64::from(aligned_bytes_per_row) * u64::from(height),
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            self.capture = Some(CaptureTarget {
                color,
                color_view,
                depth_view,
                buffer,
                width,
                height,
            });
        }
        let target = self.capture.as_ref().expect("capture target");
        let color = target.color.clone();
        let color_view = target.color_view.clone();
        let depth_view = target.depth_view.clone();
        let buffer = target.buffer.clone();

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("capture-encoder"),
            });
        self.record(
            &mut encoder,
            &color_view,
            &depth_view,
            None,
            view_proj,
            camera_pos,
            chunks,
        );
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &color,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(aligned_bytes_per_row),
                    rows_per_image: None,
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        let index = self.queue.submit([encoder.finish()]);
        self.device.poll(wgpu::PollType::Wait {
            submission_index: Some(index),
            timeout: Some(POLL_TIMEOUT),
        })?;

        let slice = buffer.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            sender.send(result).expect("capture map channel");
        });
        self.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(POLL_TIMEOUT),
        })?;
        receiver
            .recv()
            .map_err(|_| RendererError::CaptureChannel)??;

        let mapped = slice
            .get_mapped_range()
            .map_err(|err| RendererError::CaptureRange(err.to_string()))?;
        let mut pixels = Vec::with_capacity((bytes_per_row * height) as usize);
        for row in 0..height as usize {
            let start = row * aligned_bytes_per_row as usize;
            pixels.extend_from_slice(&mapped[start..start + bytes_per_row as usize]);
        }
        drop(mapped);
        buffer.unmap();
        Ok(pixels)
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
    /// Alpha-blended variants for translucent fluid quads.
    translucent: [wgpu::RenderPipeline; 4],
    border: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    bind_group_layout: wgpu::BindGroupLayout,
    atlas_sampler: wgpu::Sampler,
    lightmap_sampler: wgpu::Sampler,
    lightmap_texture: wgpu::Texture,
    lightmap_view: wgpu::TextureView,
    lightmap_key: LightmapParams,
    clouds: Option<CloudGpu>,
    uniform_buffer: wgpu::Buffer,
}

/// Creates and bakes the 16x16 lightmap texture (block level on `x`, sky
/// level on `y`).
fn create_lightmap_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    params: &LightmapParams,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("lightmap"),
        size: wgpu::Extent3d {
            width: 16,
            height: 16,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some("lightmap-view"),
        ..Default::default()
    });
    upload_lightmap(queue, &texture, params);
    (texture, view)
}

fn upload_lightmap(queue: &wgpu::Queue, texture: &wgpu::Texture, params: &LightmapParams) {
    let pixels = lightmap_rgba(params);
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        bytemuck::cast_slice(&pixels),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(16 * 4),
            rows_per_image: None,
        },
        wgpu::Extent3d {
            width: 16,
            height: 16,
            depth_or_array_layers: 1,
        },
    );
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
    section_table: &wgpu::Buffer,
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

    let lightmap_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("lightmap-sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
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
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 5,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });

    let (lightmap_texture, lightmap_view) =
        create_lightmap_texture(device, queue, &LightmapParams::default());

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
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&lightmap_view),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: section_table.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::Sampler(&lightmap_sampler),
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
        source: wgpu::ShaderSource::Wgsl(TERRAIN_SHADER.into()),
    });

    let mains = [
        build_main_pipeline(
            device,
            &pipeline_layout,
            &shader,
            surface_format,
            wgpu::PolygonMode::Fill,
            Some(wgpu::Face::Back),
            false,
        ),
        build_main_pipeline(
            device,
            &pipeline_layout,
            &shader,
            surface_format,
            wgpu::PolygonMode::Fill,
            None,
            false,
        ),
        build_main_pipeline(
            device,
            &pipeline_layout,
            &shader,
            surface_format,
            wgpu::PolygonMode::Line,
            Some(wgpu::Face::Back),
            false,
        ),
        build_main_pipeline(
            device,
            &pipeline_layout,
            &shader,
            surface_format,
            wgpu::PolygonMode::Line,
            None,
            false,
        ),
    ];

    let translucent = [
        build_main_pipeline(
            device,
            &pipeline_layout,
            &shader,
            surface_format,
            wgpu::PolygonMode::Fill,
            Some(wgpu::Face::Back),
            true,
        ),
        build_main_pipeline(
            device,
            &pipeline_layout,
            &shader,
            surface_format,
            wgpu::PolygonMode::Fill,
            None,
            true,
        ),
        build_main_pipeline(
            device,
            &pipeline_layout,
            &shader,
            surface_format,
            wgpu::PolygonMode::Line,
            Some(wgpu::Face::Back),
            true,
        ),
        build_main_pipeline(
            device,
            &pipeline_layout,
            &shader,
            surface_format,
            wgpu::PolygonMode::Line,
            None,
            true,
        ),
    ];

    let cloud_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("mcre-cloud-shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("clouds.wgsl").into()),
    });
    let cloud_bind_group_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mcre-cloud-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
    let cloud_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("cloud-info"),
        size: size_of::<CloudUniforms>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let cloud_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("mcre-cloud-bind-group"),
        layout: &cloud_bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: cloud_uniform_buffer.as_entire_binding(),
        }],
    });
    let cloud_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("mcre-cloud-pipeline-layout"),
        bind_group_layouts: &[Some(&bind_group_layout), Some(&cloud_bind_group_layout)],
        immediate_size: 0,
    });
    let clouds = Some(CloudGpu {
        pipeline_fancy: build_cloud_pipeline(
            device,
            &cloud_pipeline_layout,
            &cloud_shader,
            surface_format,
            Some(wgpu::Face::Back),
        ),
        pipeline_flat: build_cloud_pipeline(
            device,
            &cloud_pipeline_layout,
            &cloud_shader,
            surface_format,
            None,
        ),
        bind_group: cloud_bind_group,
        uniform_buffer: cloud_uniform_buffer,
        mesh: None,
    });

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
        translucent,
        border,
        bind_group,
        bind_group_layout,
        atlas_sampler: sampler,
        lightmap_sampler,
        lightmap_texture,
        lightmap_view,
        lightmap_key: LightmapParams::default(),
        clouds,
        uniform_buffer,
    }
}

fn build_cloud_pipeline(
    device: &wgpu::Device,
    pipeline_layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    surface_format: wgpu::TextureFormat,
    cull_mode: Option<wgpu::Face>,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("mcre-cloud-pipeline"),
        layout: Some(pipeline_layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_clouds"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: size_of::<CloudVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Uint32, 2 => Uint32],
            })],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_clouds"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: surface_format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn create_cloud_buffer(device: &wgpu::Device, vertices: &[CloudVertex]) -> Option<wgpu::Buffer> {
    if vertices.is_empty() {
        return None;
    }
    Some(
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cloud-vertices"),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        }),
    )
}

fn terrain_vertex_layout(offsets: FormatFlags) -> [wgpu::VertexAttribute; 8] {
    let offsets = offsets.offsets();
    [
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Uint32x2,
            offset: 0,
            shader_location: 0,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Unorm8x4,
            offset: 8,
            shader_location: 1,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Uint16x2,
            offset: 12,
            shader_location: 2,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Uint8x4,
            offset: 16,
            shader_location: 3,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Uint32,
            offset: offsets.block_id as u64,
            shader_location: 4,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Snorm8x4,
            offset: offsets.normal as u64,
            shader_location: 5,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Uint16x2,
            offset: offsets.mid_uv as u64,
            shader_location: 6,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Sint8x4,
            offset: offsets.mid_block as u64,
            shader_location: 7,
        },
    ]
}

fn build_main_pipeline(
    device: &wgpu::Device,
    pipeline_layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    surface_format: wgpu::TextureFormat,
    polygon_mode: wgpu::PolygonMode,
    cull_mode: Option<wgpu::Face>,
    translucent: bool,
) -> wgpu::RenderPipeline {
    let attributes = terrain_vertex_layout(FormatFlags::FULL);
    let stride = FormatFlags::FULL.offsets().stride as u64;
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("mcre-pipeline"),
        layout: Some(pipeline_layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: stride,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &attributes,
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
            depth_write_enabled: Some(!translucent),
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
            entry_point: Some(if translucent {
                "fs_translucent"
            } else {
                "fs_main"
            }),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: surface_format,
                blend: if translucent {
                    Some(wgpu::BlendState::ALPHA_BLENDING)
                } else {
                    None
                },
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn center_distance_sq(center: [f32; 3], camera_pos: [f32; 3]) -> f32 {
    let dx = center[0] - camera_pos[0];
    let dy = center[1] - camera_pos[1];
    let dz = center[2] - camera_pos[2];
    dx * dx + dy * dy + dz * dz
}

fn shrink_for_width(width: u32) -> f32 {
    1.0 / 32768.0 - 1.0 / (width as f32 * 256.0)
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
    fn egui_default_fonts_are_bundled() {
        let fonts = egui::FontDefinitions::default();
        for name in [
            "Ubuntu-Light",
            "Hack",
            "egui-icons",
            "NotoEmoji-Regular",
            "emoji-icon-font",
        ] {
            assert!(
                fonts.font_data.contains_key(name),
                "egui {name} font missing — enable the default_fonts feature"
            );
        }
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

    #[test]
    fn terrain_layout_matches_full_xhfp_offsets() {
        let offsets = FormatFlags::FULL.offsets();
        assert_eq!(offsets.stride, 36);
        assert_eq!(offsets.block_id, 20);
        assert_eq!(offsets.normal, 24);
        assert_eq!(offsets.mid_uv, 28);
        assert_eq!(offsets.mid_block, 32);
        let attributes = terrain_vertex_layout(FormatFlags::FULL);
        assert_eq!(attributes[4].offset, 20);
        assert_eq!(attributes[5].offset, 24);
        assert_eq!(attributes[6].offset, 28);
        assert_eq!(attributes[7].offset, 32);
    }

    #[test]
    fn texcoord_shrink_matches_sodium_formula() {
        assert!(
            (shrink_for_width(16384) - (1.0 / 32768.0 - 1.0 / (16384.0 * 256.0))).abs() < 1e-12
        );
        assert!(shrink_for_width(16384) > 0.0);
    }
}
