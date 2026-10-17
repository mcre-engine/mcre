use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::time::Instant;

use mcre_core::{Axis, Direction};
use mcre_fs::native::NativeFs;
use mcre_renderer::clouds::{CLOUD_HEIGHT, CloudStatus, CloudTexture};
use mcre_renderer::{
    Atlas, DebugFlags, EguiFrame, FlyCamera, Frustum, LightMode, Mat4, Mesh, MeshOptions,
    MeshResult, MeshWorker, RenderChunk, Renderer, StreamSource, build_atlas, mesh_chunk,
};
use mcre_resource_pack::{ResourcePackData, ResourcePackSet, download};
use mcre_world::fluid::{FluidKind, fluid_state, is_face_sturdy};
use mcre_world::{Block, CHUNK_WIDTH, Chunk, ChunkPos, PropVal, WORLD_HEIGHT, WORLD_MIN_Y, World};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

const FOG_COLOR: [f32; 3] = [0.53, 0.81, 0.92];
const NIGHT_FOG_COLOR: [f32; 3] = [0.03, 0.05, 0.12];
/// Vanilla `Timelines.NIGHT_CLOUD_COLOR_MULTIPLIER`.
const NIGHT_CLOUD_COLOR: [f32; 3] = [0.1, 0.1, 0.15];
const DAY_LENGTH_SECS: f32 = 600.0;
/// Vanilla `EnvironmentAttributes.WATER_FOG_COLOR` default (`#050533`).
const WATER_FOG_COLOR: [f32; 3] = [5.0 / 255.0, 5.0 / 255.0, 51.0 / 255.0];
/// Vanilla `LavaFogEnvironment.COLOR` (`#991900`).
const LAVA_FOG_COLOR: [f32; 3] = [153.0 / 255.0, 25.0 / 255.0, 0.0];
const FOV_SURFACE: f32 = 70.0 * std::f32::consts::PI / 180.0;
/// Vanilla `Camera.modifyFovBasedOnDeathOrFluid` at the default
/// `fovEffectScale` of 1.0 (`70 * 0.85714287 = 60`).
const FOV_UNDERWATER: f32 = 60.0 * std::f32::consts::PI / 180.0;

struct ChunkMesh {
    origin: [i32; 2],
    mesh: Mesh,
    version: u64,
}

fn fog_color_for(sky: f32) -> [f32; 3] {
    let t = sky.clamp(0.0, 1.0);
    [
        NIGHT_FOG_COLOR[0] + (FOG_COLOR[0] - NIGHT_FOG_COLOR[0]) * t,
        NIGHT_FOG_COLOR[1] + (FOG_COLOR[1] - NIGHT_FOG_COLOR[1]) * t,
        NIGHT_FOG_COLOR[2] + (FOG_COLOR[2] - NIGHT_FOG_COLOR[2]) * t,
    ]
}

fn fog_density_scale(sky: f32) -> f32 {
    0.5 + 0.5 * sky.clamp(0.0, 1.0)
}

/// Approximation of the vanilla `CLOUD_COLOR` timeline keyed on the same
/// day/night scalar as the fog: white at day, dark blue-grey at night, with
/// the overworld's 0.8 alpha.
fn cloud_color_for(sky: f32) -> [f32; 4] {
    let t = sky.clamp(0.0, 1.0);
    [
        NIGHT_CLOUD_COLOR[0] + (1.0 - NIGHT_CLOUD_COLOR[0]) * t,
        NIGHT_CLOUD_COLOR[1] + (1.0 - NIGHT_CLOUD_COLOR[1]) * t,
        NIGHT_CLOUD_COLOR[2] + (1.0 - NIGHT_CLOUD_COLOR[2]) * t,
        0.8,
    ]
}

fn chunk_aabb(origin: [i32; 2]) -> ([f32; 3], [f32; 3]) {
    (
        [origin[0] as f32, WORLD_MIN_Y as f32, origin[1] as f32],
        [
            (origin[0] + CHUNK_WIDTH) as f32,
            (WORLD_MIN_Y + WORLD_HEIGHT) as f32,
            (origin[1] + CHUNK_WIDTH) as f32,
        ],
    )
}

fn main() {
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    let args = ViewerArgs::parse();
    let reload_test = args.reload_test;
    let (data, atlas, cloud_texture, world, meshes, camera_pos, streamer, packs) =
        runtime.block_on(prepare(args));
    let total_chunks = meshes.len();

    let mut viewer = Viewer {
        camera: FlyCamera::new(camera_pos),
        atlas: atlas.clone(),
        world,
        meshes,
        worker: MeshWorker::new(data, atlas, streamer.as_ref().map(|s| s.source.clone())),
        streamer,
        packs,
        tokio_handle: runtime.handle().clone(),
        pending_reload: reload_test,
        inflight: 0,
        generation: 0,
        pending_stream_dirty: Vec::new(),
        light_mode: LightMode::Smooth,
        window: None,
        renderer: None,
        keys: HashSet::new(),
        looking: false,
        look_pos: None,
        warp_ok: true,
        last_frame: Instant::now(),
        egui_ctx: None,
        egui_state: None,
        menu_visible: true,
        frame_ms: 0.0,
        unlit: false,
        heatmap: false,
        fog_enabled: true,
        fog_start: 64.0,
        fog_end: 160.0,
        submerged: None,
        sky_factor: 1.0,
        brightness: mcre_renderer::lightmap::DEFAULT_BRIGHTNESS,
        cloud_texture,
        cloud_status: CloudStatus::Fancy,
        cloud_range: 32,
        cloud_ticks: 0.0,
        cycle_enabled: false,
        cycle_paused: false,
        cycle_speed: 1.0,
        scrubbing: false,
        double_sided_liquids: true,
        fast_leaves: false,
        visible_chunks: total_chunks,
        total_chunks,
        wireframe: false,
        backface_cull: true,
        chunk_borders: false,
    };

    let event_loop = EventLoop::new().expect("event loop");
    event_loop.set_control_flow(ControlFlow::Poll);

    event_loop.run_app(&mut viewer).expect("event loop");
}

async fn prepare(
    args: ViewerArgs,
) -> (
    Arc<ResourcePackData>,
    Arc<Atlas>,
    Option<CloudTexture>,
    World,
    Vec<ChunkMesh>,
    [f32; 3],
    Option<Streamer>,
    PackManager,
) {
    let fs = NativeFs::new_app_root("mcre").await.expect("app dir");
    let app_root = fs.root().clone();

    let mut packs = PackManager::new(fs, app_root, args.packs.clone()).await;
    let (atlas, cloud_texture, data) = packs.rebuild().await.expect("bake packs");
    for err in &packs.errors {
        eprintln!("viewer: {err}");
    }

    let mut world = match &args.world {
        Some(_) => World::new(args.radius),
        None => World::new(1),
    };
    let mut camera_pos = [8.0, 7.0, 26.0];
    let mut streamer = None;
    if let Some(save_dir) = args.world {
        let setup = load_save_into(&mut world, &save_dir, args.radius);
        camera_pos = setup.camera_pos;
        streamer = Some(setup.streamer);
    } else {
        for cx in -1..=1 {
            for cz in -1..=1 {
                world.add_chunk(demo_chunk_at(cx, cz));
            }
        }
    }
    world.compute_light();
    let options = MeshOptions::default();
    let mut meshes = Vec::new();
    for chunk in world.iter_chunks() {
        let origin = [chunk.pos().x * CHUNK_WIDTH, chunk.pos().z * CHUNK_WIDTH];
        let mesh = mesh_one(
            &data,
            &atlas,
            &world,
            chunk,
            origin,
            LightMode::Smooth,
            options,
        );
        log_mesh(
            &format!("initial mesh ({},{})", chunk.pos().x, chunk.pos().z),
            &mesh,
        );
        meshes.push(ChunkMesh {
            origin,
            mesh,
            version: 0,
        });
    }
    // Drop any dirty flags from world construction/import: the meshes above
    // are current, so the next edit must remesh only its own chunks.
    world.take_dirty();
    (
        Arc::new(data),
        Arc::new(atlas),
        cloud_texture,
        world,
        meshes,
        camera_pos,
        streamer,
        packs,
    )
}

struct ViewerArgs {
    world: Option<String>,
    radius: u32,
    packs: Vec<String>,
    reload_test: bool,
}

impl ViewerArgs {
    fn parse() -> Self {
        let mut world = None;
        let mut radius = 4;
        let mut packs = Vec::new();
        let mut reload_test = false;
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            if arg == "--world" {
                world = args.next();
            } else if arg == "--pack" {
                if let Some(path) = args.next() {
                    packs.push(path);
                }
            } else if arg == "--reload-test" {
                reload_test = true;
            } else if arg == "--radius" {
                radius = args
                    .next()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(4)
                    .clamp(1, 16);
            } else if arg == "--help" || arg == "-h" {
                eprintln!(
                    "usage: viewer [--pack <dir-or-zip> ...] [--world <save-dir>] [--radius N]"
                );
                eprintln!("  Packs stack bottom-to-top above the default pack, last wins.");
                eprintln!("  --pack adds session packs (also manageable in the Packs panel).");
                eprintln!("  --world loads the save around its spawn instead of the demo.");
                eprintln!("  --radius sets the loaded ring radius in chunks (default 4).");
                eprintln!("  User packs live in <app-dir>/resourcepacks (folders or .zip).");
                eprintln!("  --reload-test triggers one pack hot-reload on startup (testing).");
                std::process::exit(0);
            }
        }
        Self {
            world,
            radius,
            packs,
            reload_test,
        }
    }
}

struct Streamer {
    source: StreamSource,
    center: (i32, i32),
    radius: i32,
    pending: VecDeque<(i32, i32)>,
}

struct SaveSetup {
    camera_pos: [f32; 3],
    streamer: Streamer,
}

struct PackManager {
    fs: NativeFs,
    app_root: std::path::PathBuf,
    cache_dir: mcre_fs::path::FsPath,
    default_path: mcre_fs::path::FsPath,
    discovered: Vec<mcre_resource_pack::discover::DiscoveredPack>,
    enabled: Vec<String>,
    adhoc: Vec<String>,
    errors: Vec<String>,
}

impl PackManager {
    async fn new(fs: NativeFs, app_root: std::path::PathBuf, adhoc: Vec<String>) -> Self {
        use mcre_fs::Fs;
        let packs_dir = mcre_fs::path::FsPath::from("resourcepacks");
        let cache_dir = mcre_fs::path::FsPath::from("pack_cache");
        fs.create_dir_all(&packs_dir).await.ok();
        fs.create_dir_all(&cache_dir).await.ok();
        let default_path = download::ensure_default_pack(&fs)
            .await
            .expect("resource pack");
        let mut manager = Self {
            fs,
            app_root,
            cache_dir,
            default_path,
            discovered: Vec::new(),
            enabled: Vec::new(),
            adhoc,
            errors: Vec::new(),
        };
        manager.rescan().await;
        manager.load_selection();
        manager
    }

    fn config_path(&self) -> std::path::PathBuf {
        self.app_root.join("pack_stack.json")
    }

    fn load_selection(&mut self) {
        let Ok(bytes) = std::fs::read(self.config_path()) else {
            return;
        };
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            return;
        };
        if let Some(list) = value.get("enabled").and_then(|list| list.as_array()) {
            self.enabled = list
                .iter()
                .filter_map(|entry| entry.as_str())
                .filter(|id| self.discovered.iter().any(|pack| &pack.id == id))
                .map(str::to_string)
                .collect();
        }
    }

    fn save_selection(&self) {
        let value = serde_json::json!({ "enabled": self.enabled });
        if let Ok(bytes) = serde_json::to_vec_pretty(&value) {
            let _ = std::fs::write(self.config_path(), bytes);
        }
    }

    async fn rescan(&mut self) {
        self.discovered = mcre_resource_pack::discover::discover_packs(
            &self.fs,
            &mcre_fs::path::FsPath::from("resourcepacks"),
        )
        .await;
        self.enabled
            .retain(|id| self.discovered.iter().any(|pack| &pack.id == id));
    }

    fn set_enabled(&mut self, id: &str, enabled: bool) {
        if enabled {
            if !self.enabled.iter().any(|entry| entry == id) {
                self.enabled.push(id.to_string());
            }
        } else {
            self.enabled.retain(|entry| entry != id);
        }
        self.save_selection();
    }

    fn move_pack(&mut self, index: usize, direction: i32) {
        let other = index as i32 + direction;
        if other < 0 || other >= self.enabled.len() as i32 {
            return;
        }
        self.enabled.swap(index, other as usize);
        self.save_selection();
    }

    /// Rebuilds the whole stack (default + enabled + adhoc) with per-pack
    /// error isolation: broken packs are reported and skipped, never fatal
    /// unless even the default pack fails.
    async fn rebuild(
        &mut self,
    ) -> Result<
        (
            mcre_renderer::Atlas,
            Option<CloudTexture>,
            mcre_resource_pack::ResourcePackData,
        ),
        String,
    > {
        use mcre_fs::path::FsPath;
        self.errors.clear();
        let mut set = ResourcePackSet::new();
        if let Err(err) = set.add_pack(&self.fs, &self.default_path).await {
            return Err(format!("default pack failed to load: {err}"));
        }
        for id in self.enabled.clone() {
            let Some(pack) = self.discovered.iter().find(|pack| pack.id == id) else {
                continue;
            };
            if let Some(err) = &pack.error {
                self.errors.push(format!("{id}: skipped ({err})"));
                continue;
            }
            match mcre_resource_pack::discover::ensure_extracted(&self.fs, pack, &self.cache_dir)
                .await
            {
                Ok(path) => {
                    if let Err(err) = set.add_pack(&self.fs, &path).await {
                        self.errors.push(format!("{id}: skipped ({err})"));
                    }
                }
                Err(err) => self.errors.push(format!("{id}: skipped ({err})")),
            }
        }
        for raw in &self.adhoc {
            let path = FsPath::from(raw.as_str());
            if let Err(err) = set.add_pack(&self.fs, &path).await {
                self.errors.push(format!("{raw}: skipped ({err})"));
            }
        }
        for (i, pack) in set.packs().iter().enumerate() {
            if pack.compatible {
                eprintln!("viewer: pack {i}: {}", pack.path);
            } else {
                eprintln!("viewer: pack {i}: {} (INCOMPATIBLE pack format)", pack.path);
            }
        }
        let roots = set.texture_roots();
        let data = set.build(&self.fs).await.map_err(|err| err.to_string())?;
        let atlas = build_atlas(&self.fs, &roots, &data.texture_table)
            .await
            .map_err(|err| err.to_string())?;
        let cloud_texture = CloudTexture::load(&self.fs, &roots).await;
        if cloud_texture.is_none() {
            self.errors
                .push("clouds: textures/environment/clouds.png not found".to_string());
        }
        Ok((atlas, cloud_texture, data))
    }
}

fn load_save_into(world: &mut World, save_dir: &str, radius: u32) -> SaveSetup {
    use std::path::Path;
    use std::sync::Arc;
    let save_path = Path::new(save_dir);
    let spawn = match mcre_anvil::read_spawn(save_path) {
        Ok(spawn) => spawn,
        Err(err) => {
            eprintln!("viewer: cannot open save at {save_dir} ({err})");
            std::process::exit(1);
        }
    };
    eprintln!("viewer: save spawn {:?} in {}", spawn.pos, spawn.dimension);
    let paths = mcre_anvil::SavePaths::new(save_path);
    let region_dir = paths.resolve_region_dir(&spawn.dimension);
    let spawn_chunk = (
        spawn.pos.0.div_euclid(CHUNK_WIDTH),
        spawn.pos.2.div_euclid(CHUNK_WIDTH),
    );
    let blocks = Arc::new(mcre_anvil::BlockResolver::new());
    let biomes = Arc::new(mcre_anvil::BiomeResolver::new());
    let source = StreamSource {
        region_dir,
        blocks: blocks.clone(),
        biomes: biomes.clone(),
    };

    // Initial 3x3 around spawn, synchronously: instant scene, the streamer
    // fills the rest of the ring in the background.
    let radius = radius as i32;
    let mut stats = mcre_anvil::ImportStats::default();
    for dz in -1..=1 {
        for dx in -1..=1 {
            let pos = (spawn_chunk.0 + dx, spawn_chunk.1 + dz);
            let chunk = match mcre_anvil::load_chunk(
                &source.region_dir,
                pos.0,
                pos.1,
                &blocks,
                &biomes,
                &mut stats,
            ) {
                Ok(Some(chunk)) => chunk,
                Ok(None) => Chunk::new_at(ChunkPos::new(pos.0, pos.1)),
                Err(err) => {
                    eprintln!("viewer: skipping corrupt chunk {pos:?} ({err})");
                    stats.skipped_chunks += 1;
                    Chunk::new_at(ChunkPos::new(pos.0, pos.1))
                }
            };
            world.replace_chunk(chunk);
        }
    }
    eprintln!(
        "viewer: imported {} chunks, {} sections, {} blocks ({} unknown blocks, {} unknown biomes)",
        stats.chunks,
        stats.sections,
        stats.blocks_set,
        stats.unknown_blocks.len(),
        stats.unknown_biomes.len(),
    );
    for name in &stats.unknown_blocks {
        eprintln!("viewer: unknown block state: {name}");
    }
    for name in &stats.unknown_biomes {
        eprintln!("viewer: unknown biome: {name}");
    }

    let mut pending: Vec<((i32, i32), i32)> = Vec::new();
    for dz in -radius..=radius {
        for dx in -radius..=radius {
            if dx.abs() <= 1 && dz.abs() <= 1 {
                continue;
            }
            let pos = (spawn_chunk.0 + dx, spawn_chunk.1 + dz);
            pending.push((pos, dx.abs() + dz.abs()));
        }
    }
    pending.sort_by_key(|&(_, dist)| dist);
    let streamer = Streamer {
        source,
        center: spawn_chunk,
        radius,
        pending: pending.into_iter().map(|(pos, _)| pos).collect(),
    };
    eprintln!(
        "viewer: streaming {} chunks in background (radius {radius})",
        streamer.pending.len()
    );

    // Place the camera above its own column, not the spawn column: nearby
    // terrain can be much higher than the spawn point itself.
    let cam_column = (spawn.pos.0 + 6, spawn.pos.2 + 20);
    let mut cam_top = spawn.pos.1;
    for y in (WORLD_MIN_Y..WORLD_MIN_Y + WORLD_HEIGHT).rev() {
        if let Some(state) = world.get_block((cam_column.0, y, cam_column.1))
            && !state.is_air()
        {
            cam_top = y;
            break;
        }
    }
    eprintln!("viewer: camera column {cam_column:?} top={cam_top}");
    SaveSetup {
        camera_pos: [
            cam_column.0 as f32,
            cam_top as f32 + 6.0,
            cam_column.1 as f32,
        ],
        streamer,
    }
}

fn log_mesh(label: &str, mesh: &Mesh) {
    let shaded = mesh
        .vertices
        .iter()
        .filter(|v| v.color.iter().any(|c| *c < 0.999))
        .count();
    eprintln!(
        "viewer: {label} {} quads ({shaded} shaded verts: AO darkened or tinted)",
        (mesh.indices.len() + mesh.translucent_indices.len()) / 6,
    );
}

fn mesh_one(
    data: &ResourcePackData,
    atlas: &Atlas,
    world: &World,
    chunk: &Chunk,
    origin: [i32; 2],
    mode: LightMode,
    options: MeshOptions,
) -> Mesh {
    mesh_chunk(data, atlas, chunk, world, origin, mode, options)
}

fn demo_chunk_at(cx: i32, cz: i32) -> Chunk {
    if cx == 0 && cz == 0 {
        return demo_chunk();
    }
    let mut chunk = Chunk::new_at(ChunkPos::new(cx, cz));
    let height = 1 + (cx * 3 + cz * 5).rem_euclid(4);
    let surface = match (cx * 7 + cz * 11).rem_euclid(4) {
        0 => Block::SAND,
        1 => Block::COBBLESTONE,
        2 => Block::STONE,
        _ => Block::GRASS_BLOCK,
    };
    for x in 0..16 {
        for z in 0..16 {
            chunk.set_block(x, 0, z, Block::STONE.default_state());
            for y in 1..height {
                chunk.set_block(x, y, z, Block::STONE.default_state());
            }
            chunk.set_block(x, height, z, surface.default_state());
        }
    }
    if (cx + cz).rem_euclid(2) == 0 {
        let pillar = 2 + (cx * 5 - cz * 3).rem_euclid(4);
        for y in 0..pillar {
            chunk.set_block(8, height + 1 + y, 8, Block::OAK_LOG.default_state());
        }
        if (cx * cz).rem_euclid(3) == 0 {
            chunk.set_block(8, height + 1 + pillar, 8, Block::GLOWSTONE.default_state());
        }
    } else {
        for dx in 6..10 {
            for dz in 6..10 {
                chunk.set_block(dx, height + 1, dz, Block::OAK_LEAVES.default_state());
            }
        }
        chunk.set_block(7, height + 1, 7, Block::GLASS.default_state());
    }
    chunk
}

fn demo_chunk() -> Chunk {
    let mut chunk = Chunk::new();

    for x in 0..16 {
        for z in 0..16 {
            chunk.set_block(x, 0, z, Block::STONE.default_state());
            let surface = if x >= 12 && z >= 12 {
                Block::SAND
            } else if x >= 10 || z <= 1 {
                Block::COBBLESTONE
            } else {
                Block::GRASS_BLOCK
            };
            chunk.set_block(x, 1, z, surface.default_state());
        }
    }

    let furnace_facings = [
        Direction::North,
        Direction::South,
        Direction::East,
        Direction::West,
    ];
    for (index, facing) in furnace_facings.into_iter().enumerate() {
        let x = 3 + index as i32;
        chunk.set_block(
            x,
            2,
            1,
            Block::FURNACE
                .state(&[PropVal::Facing(facing)])
                .expect("furnace facing"),
        );
    }

    for step in 0..4_i32 {
        chunk.set_block(
            4 + step,
            2 + step,
            6,
            Block::OAK_STAIRS
                .state(&[PropVal::Facing(Direction::East)])
                .expect("oak stairs"),
        );
    }

    for x in 6..11 {
        for y in 2..5 {
            chunk.set_block(x, y, 13, Block::GLASS.default_state());
        }
    }

    for y in 2..5 {
        chunk.set_block(12, y, 8, Block::OAK_LOG.default_state());
    }
    for x in 10..15 {
        for z in 6..11 {
            if (x == 10 || x == 14) && (z == 6 || z == 10) {
                continue;
            }
            chunk.set_block(x, 5, z, Block::OAK_LEAVES.default_state());
            chunk.set_block(x, 6, z, Block::OAK_LEAVES.default_state());
        }
    }
    for x in 11..14 {
        for z in 7..10 {
            chunk.set_block(x, 7, z, Block::OAK_LEAVES.default_state());
        }
    }

    chunk.set_block(4, 2, 9, Block::POPPY.default_state());
    chunk.set_block(5, 2, 9, Block::DANDELION.default_state());
    chunk.set_block(6, 2, 9, Block::SHORT_GRASS.default_state());
    chunk.set_block(7, 2, 9, Block::OAK_SAPLING.default_state());
    chunk.set_block(8, 2, 8, Block::GLOWSTONE.default_state());

    for x in 12..15 {
        for z in 12..15 {
            chunk.set_block(x, 2, z, Block::WATER.default_state());
        }
    }

    for x in 10..13 {
        chunk.set_block(
            x,
            2,
            3,
            Block::OAK_LOG
                .state(&[PropVal::Axis(Axis::X)])
                .expect("oak log axis"),
        );
    }

    chunk
}

fn raycast(
    world: &World,
    origin: [f32; 3],
    dir: [f32; 3],
    max_dist: f32,
) -> Option<([i32; 3], [i32; 3])> {
    let mut cell = [
        origin[0].floor() as i32,
        origin[1].floor() as i32,
        origin[2].floor() as i32,
    ];
    let mut prev = cell;
    let mut step = [0_i32; 3];
    let mut t_max = [f32::INFINITY; 3];
    let mut t_delta = [f32::INFINITY; 3];
    for axis in 0..3 {
        if dir[axis] == 0.0 {
            continue;
        }
        step[axis] = if dir[axis] > 0.0 { 1 } else { -1 };
        t_delta[axis] = 1.0 / dir[axis].abs();
        let boundary = if step[axis] > 0 {
            cell[axis] as f32 + 1.0
        } else {
            cell[axis] as f32
        };
        t_max[axis] = (boundary - origin[axis]) / dir[axis];
    }

    let mut t = 0.0;
    while t <= max_dist {
        if let Some(state) = world.get_block((cell[0], cell[1], cell[2]))
            && !state.is_air()
        {
            return Some((cell, prev));
        }
        let axis = (0..3).min_by(|&a, &b| t_max[a].partial_cmp(&t_max[b]).unwrap())?;
        t = t_max[axis];
        if t > max_dist {
            break;
        }
        prev = cell;
        cell[axis] += step[axis];
        t_max[axis] += t_delta[axis];
    }
    None
}

struct Viewer {
    camera: FlyCamera,
    atlas: Arc<Atlas>,
    world: World,
    meshes: Vec<ChunkMesh>,
    worker: MeshWorker,
    streamer: Option<Streamer>,
    packs: PackManager,
    tokio_handle: tokio::runtime::Handle,
    pending_reload: bool,
    inflight: u32,
    generation: u64,
    /// Chunk positions whose streamed data was salvaged from a stale worker
    /// result and still need a remesh with the current world.
    pending_stream_dirty: Vec<ChunkPos>,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    keys: HashSet<KeyCode>,
    looking: bool,
    look_pos: Option<[f64; 2]>,
    warp_ok: bool,
    last_frame: Instant,
    light_mode: LightMode,
    egui_ctx: Option<egui::Context>,
    egui_state: Option<egui_winit::State>,
    menu_visible: bool,
    frame_ms: f32,
    unlit: bool,
    heatmap: bool,
    fog_enabled: bool,
    fog_start: f32,
    fog_end: f32,
    submerged: Option<FluidKind>,
    sky_factor: f32,
    brightness: f32,
    cloud_texture: Option<CloudTexture>,
    cloud_status: CloudStatus,
    cloud_range: u32,
    cloud_ticks: f64,
    cycle_enabled: bool,
    cycle_paused: bool,
    cycle_speed: f32,
    scrubbing: bool,
    double_sided_liquids: bool,
    fast_leaves: bool,
    visible_chunks: usize,
    total_chunks: usize,
    wireframe: bool,
    backface_cull: bool,
    chunk_borders: bool,
}

impl ApplicationHandler for Viewer {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("MCRE viewer")
                        .with_inner_size(LogicalSize::new(1280.0, 720.0)),
                )
                .expect("create window"),
        );
        let mut renderer = pollster::block_on(Renderer::new(window.clone(), self.atlas.as_ref()))
            .expect("create renderer");
        renderer.set_debug_flags(DebugFlags {
            unlit: self.unlit,
            heatmap: self.heatmap,
            fog_off: !self.fog_enabled,
        });
        renderer.set_wireframe(self.wireframe);
        renderer.set_backface_culling(self.backface_cull);
        renderer.set_chunk_borders(self.chunk_borders);
        renderer.set_fog(FOG_COLOR, self.fog_start, self.fog_end);
        renderer.set_sky_factor(self.sky_factor);
        renderer.set_brightness(self.brightness);
        renderer.set_cloud_texture(self.cloud_texture.clone());
        renderer.set_cloud_status(self.cloud_status);
        renderer.set_cloud_range(self.cloud_range);
        renderer.set_cloud_height(CLOUD_HEIGHT);
        renderer.set_cloud_color(cloud_color_for(self.sky_factor));

        let egui_ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            window.as_ref(),
            Some(window.scale_factor() as f32),
            None,
            None,
        );

        self.window = Some(window);
        self.renderer = Some(renderer);
        self.egui_ctx = Some(egui_ctx);
        self.egui_state = Some(egui_state);
        self.last_frame = Instant::now();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let consumed = match (self.window.as_ref(), self.egui_state.as_mut()) {
            (Some(window), Some(state)) => state.on_window_event(window, &event).consumed,
            _ => false,
        } && self.menu_visible;
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(size.width, size.height);
                }
            }
            WindowEvent::KeyboardInput {
                event: key_event, ..
            } => {
                let PhysicalKey::Code(code) = key_event.physical_key else {
                    return;
                };
                match key_event.state {
                    ElementState::Pressed => {
                        if code == KeyCode::Escape {
                            if self.looking {
                                self.release_pointer();
                            } else {
                                event_loop.exit();
                            }
                            return;
                        }
                        if code == KeyCode::F1 && !key_event.repeat {
                            self.menu_visible = !self.menu_visible;
                            eprintln!(
                                "viewer: debug menu {}",
                                if self.menu_visible {
                                    "visible"
                                } else {
                                    "hidden"
                                }
                            );
                            if self.menu_visible {
                                self.release_pointer();
                            }
                            return;
                        }
                        if consumed || self.egui_wants_keyboard() {
                            return;
                        }
                        if code == KeyCode::KeyL && !key_event.repeat {
                            self.light_mode = match self.light_mode {
                                LightMode::Smooth => LightMode::Flat,
                                LightMode::Flat => LightMode::Smooth,
                            };
                            self.remesh_all();
                            eprintln!("viewer: light mode {:?}", self.light_mode);
                        }
                        self.keys.insert(code);
                    }
                    ElementState::Released => {
                        self.keys.remove(&code);
                    }
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button,
                ..
            } => {
                let (using, over) = self
                    .egui_ctx
                    .as_ref()
                    .map(|ctx| (ctx.egui_is_using_pointer(), ctx.is_pointer_over_egui()))
                    .unwrap_or((false, false));
                eprintln!(
                    "viewer: click {button:?} consumed={consumed} ui={} looking={} egui_using={using} egui_over={over}",
                    self.egui_wants_pointer(),
                    self.looking
                );
                if consumed || self.egui_wants_pointer() {
                    return;
                }
                if self.looking {
                    match button {
                        MouseButton::Left => self.edit_block(false),
                        MouseButton::Right => self.edit_block(true),
                        _ => {}
                    }
                } else if button == MouseButton::Left
                    && let Some(window) = &self.window
                {
                    self.looking = true;
                    self.look_pos = None;
                    self.warp_ok = true;
                    window.set_cursor_visible(false);
                    let size = window.inner_size();
                    let center =
                        PhysicalPosition::new(size.width as f64 / 2.0, size.height as f64 / 2.0);
                    if window.set_cursor_position(center).is_ok() {
                        self.look_pos = Some([center.x, center.y]);
                    }
                    eprintln!("viewer: looking");
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if consumed || self.egui_wants_pointer() {
                    return;
                }
                self.look_move(position.x, position.y);
            }
            WindowEvent::RedrawRequested => self.frame(),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

impl Viewer {
    fn egui_wants_keyboard(&self) -> bool {
        self.menu_visible
            && self
                .egui_ctx
                .as_ref()
                .is_some_and(|ctx| ctx.egui_wants_keyboard_input())
    }

    fn egui_wants_pointer(&self) -> bool {
        self.menu_visible
            && self
                .egui_ctx
                .as_ref()
                .is_some_and(|ctx| ctx.egui_wants_pointer_input())
    }

    fn release_pointer(&mut self) {
        if !self.looking {
            return;
        }
        self.looking = false;
        self.look_pos = None;
        if let Some(window) = &self.window {
            window.set_cursor_visible(true);
        }
        eprintln!("viewer: pointer released");
    }

    fn look_move(&mut self, x: f64, y: f64) {
        if !self.looking {
            return;
        }
        if let Some([lx, ly]) = self.look_pos {
            let dx = x - lx;
            let dy = y - ly;
            if dx != 0.0 || dy != 0.0 {
                let sensitivity = self.camera.sensitivity;
                self.camera
                    .rotate(dx as f32 * sensitivity, -(dy as f32) * sensitivity);
            }
        }
        self.look_pos = Some([x, y]);
        if self.warp_ok
            && let Some(window) = &self.window
        {
            let size = window.inner_size();
            let center = PhysicalPosition::new(size.width as f64 / 2.0, size.height as f64 / 2.0);
            if window.set_cursor_position(center).is_ok() {
                self.look_pos = Some([center.x, center.y]);
            } else {
                self.warp_ok = false;
                eprintln!("viewer: cursor warp failed, look degrades at screen edges");
            }
        }
    }

    fn frame(&mut self) {
        let Some(window) = self.window.clone() else {
            return;
        };
        if self.renderer.is_none() {
            return;
        }

        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return;
        }

        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32().min(0.05);
        self.last_frame = now;
        self.frame_ms = dt * 1000.0;

        self.apply_results();
        self.tick_stream();

        if self.cycle_enabled && !self.cycle_paused && !self.scrubbing {
            self.sky_factor =
                (self.sky_factor + dt * self.cycle_speed / DAY_LENGTH_SECS).rem_euclid(1.0);
        }
        // Cloud drift follows real time, like vanilla world time.
        self.cloud_ticks += f64::from(dt) * 20.0;

        let mut forward = 0.0;
        if self.keys.contains(&KeyCode::KeyW) {
            forward += 1.0;
        }
        if self.keys.contains(&KeyCode::KeyS) {
            forward -= 1.0;
        }
        let mut strafe = 0.0;
        if self.keys.contains(&KeyCode::KeyD) {
            strafe += 1.0;
        }
        if self.keys.contains(&KeyCode::KeyA) {
            strafe -= 1.0;
        }
        let mut vertical = 0.0;
        if self.keys.contains(&KeyCode::Space) {
            vertical += 1.0;
        }
        if self.keys.contains(&KeyCode::ShiftLeft) {
            vertical -= 1.0;
        }

        let boost = if self.keys.contains(&KeyCode::ControlLeft) {
            3.0
        } else {
            1.0
        };
        let speed = self.camera.speed * boost * dt;
        let f = self.camera.forward();
        let r = self.camera.right();
        for axis in 0..3 {
            self.camera.position[axis] += (f[axis] * forward + r[axis] * strafe) * speed;
        }
        self.camera.position[1] += vertical * speed;

        let aspect = size.width as f32 / size.height as f32;
        let old_submerged = self.submerged;
        self.submerged = self.fog_fluid();
        if self.submerged != old_submerged {
            self.camera.fov = if self.submerged.is_some() {
                FOV_UNDERWATER
            } else {
                FOV_SURFACE
            };
        }
        let view_proj = self.camera.view_projection(aspect);
        let camera_pos = self.camera.position;

        if self.egui_ctx.is_none() || self.egui_state.is_none() {
            self.render_visible(view_proj.0, camera_pos, None);
            return;
        }

        let old_debug = (self.unlit, self.heatmap, self.fog_enabled);
        let old_geometry = (self.wireframe, self.backface_cull, self.chunk_borders);
        let old_fog = (self.fog_start, self.fog_end, self.sky_factor);
        let old_sky = self.sky_factor;
        let old_brightness = self.brightness;
        let old_clouds = (self.cloud_status, self.cloud_range);
        let old_cycle = (self.cycle_enabled, self.cycle_paused);
        let old_mesh_options = (self.double_sided_liquids, self.fast_leaves);

        let mut do_remesh = false;
        let mut do_recompute = false;
        let (paint_jobs, screen_descriptor, textures_delta) = {
            let ctx = self.egui_ctx.as_ref().expect("egui context");
            let state = self.egui_state.as_mut().expect("egui state");
            let input = state.take_egui_input(&window);
            ctx.begin_pass(input);
            if self.menu_visible {
                let mode_before = self.light_mode;
                egui::Window::new("Debug").show(ctx, |ui| {
                    ui.collapsing("Lighting", |ui| {
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut self.light_mode, LightMode::Smooth, "Smooth");
                            ui.selectable_value(&mut self.light_mode, LightMode::Flat, "Flat");
                        });
                        ui.checkbox(&mut self.unlit, "Unlit");
                        ui.checkbox(&mut self.heatmap, "Heatmap");
                        ui.checkbox(&mut self.fog_enabled, "Fog");
                        ui.add(
                            egui::Slider::new(&mut self.fog_start, 0.0..=400.0).text("Fog start"),
                        );
                        ui.add(egui::Slider::new(&mut self.fog_end, 0.0..=400.0).text("Fog end"));
                        let scrub = ui.add(
                            egui::Slider::new(&mut self.sky_factor, 0.0..=1.0).text("Day/night"),
                        );
                        self.scrubbing = scrub.dragged();
                        ui.add(
                            egui::Slider::new(&mut self.brightness, 0.0..=1.0).text("Brightness"),
                        );
                        ui.checkbox(&mut self.cycle_enabled, "Cycle");
                        ui.checkbox(&mut self.cycle_paused, "Pause");
                        ui.add(
                            egui::Slider::new(&mut self.cycle_speed, 0.1..=10.0)
                                .text("Cycle speed"),
                        );
                    });
                    ui.collapsing("Geometry", |ui| {
                        ui.checkbox(&mut self.wireframe, "Wireframe");
                        ui.checkbox(&mut self.backface_cull, "Backface cull");
                        ui.checkbox(&mut self.chunk_borders, "Chunk borders");
                    });
                    ui.collapsing("Mesh", |ui| {
                        ui.checkbox(&mut self.double_sided_liquids, "Double-sided liquids");
                        ui.checkbox(&mut self.fast_leaves, "Fast leaves");
                    });
                    ui.collapsing("Clouds", |ui| {
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut self.cloud_status, CloudStatus::Off, "Off");
                            ui.selectable_value(&mut self.cloud_status, CloudStatus::Fast, "Fast");
                            ui.selectable_value(
                                &mut self.cloud_status,
                                CloudStatus::Fancy,
                                "Fancy",
                            );
                        });
                        ui.add(
                            egui::Slider::new(&mut self.cloud_range, 2..=128)
                                .text("Range (chunks)"),
                        );
                    });
                    ui.collapsing("Packs", |ui| {
                        ui.label("Default pack (always active)");
                        ui.separator();
                        ui.heading("Active (bottom → top)");
                        let mut changed = false;
                        let mut move_dir: Option<(usize, i32)> = None;
                        let mut remove: Option<usize> = None;
                        for (index, id) in self.packs.enabled.clone().iter().enumerate() {
                            let name = self
                                .packs
                                .discovered
                                .iter()
                                .find(|pack| &pack.id == id)
                                .map(|pack| pack.display_name.clone())
                                .unwrap_or_else(|| id.clone());
                            ui.horizontal(|ui| {
                                ui.label(format!("{index}: {name}"));
                                if ui.small_button("▲").clicked() {
                                    move_dir = Some((index, -1));
                                }
                                if ui.small_button("▼").clicked() {
                                    move_dir = Some((index, 1));
                                }
                                if ui.small_button("✕").clicked() {
                                    remove = Some(index);
                                }
                            });
                        }
                        if !self.packs.adhoc.is_empty() {
                            ui.label("Session packs (--pack):");
                            for raw in &self.packs.adhoc {
                                ui.label(format!("  {raw}"));
                            }
                        }
                        ui.separator();
                        ui.heading("Available");
                        let mut enable: Option<String> = None;
                        for pack in self.packs.discovered.clone() {
                            if self.packs.enabled.iter().any(|id| id == &pack.id) {
                                continue;
                            }
                            ui.horizontal(|ui| {
                                let label = if pack.display_name == pack.id {
                                    pack.id.clone()
                                } else {
                                    format!("{} ({})", pack.display_name, pack.id)
                                };
                                ui.label(label);
                                if let Some(compatible) = pack.compatible
                                    && !compatible
                                {
                                    ui.label("⚠ format");
                                }
                                if let Some(err) = &pack.error {
                                    ui.label(format!("broken: {err}"));
                                } else if ui.small_button("+").clicked() {
                                    enable = Some(pack.id.clone());
                                }
                            });
                        }
                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui.button("Reload packs").clicked() {
                                self.pending_reload = true;
                            }
                            if ui.button("Rescan folder").clicked() {
                                let handle = self.tokio_handle.clone();
                                handle.block_on(self.packs.rescan());
                                self.pending_reload = true;
                            }
                        });
                        if !self.packs.errors.is_empty() {
                            ui.separator();
                            for err in self.packs.errors.clone() {
                                ui.label(format!("⚠ {err}"));
                            }
                        }
                        if let Some((index, direction)) = move_dir {
                            self.packs.move_pack(index, direction);
                            changed = true;
                        }
                        if let Some(index) = remove
                            && index < self.packs.enabled.len()
                        {
                            self.packs.enabled.remove(index);
                            self.packs.save_selection();
                            changed = true;
                        }
                        if let Some(id) = enable {
                            self.packs.set_enabled(&id, true);
                            changed = true;
                        }
                        if changed {
                            self.pending_reload = true;
                        }
                    });
                    ui.collapsing("Info", |ui| {
                        ui.label(format!(
                            "Chunks: {}/{}",
                            self.visible_chunks, self.total_chunks
                        ));
                        ui.label(format!(
                            "Quads: {} Verts: {}",
                            self.meshes
                                .iter()
                                .map(
                                    |m| (m.mesh.indices.len() + m.mesh.translucent_indices.len())
                                        / 6
                                )
                                .sum::<usize>(),
                            self.meshes
                                .iter()
                                .map(|m| m.mesh.vertices.len())
                                .sum::<usize>()
                        ));
                        ui.label(format!("Frame: {:.2} ms", self.frame_ms));
                        ui.label(format!(
                            "Camera: [{:.2}, {:.2}, {:.2}]",
                            self.camera.position[0],
                            self.camera.position[1],
                            self.camera.position[2]
                        ));
                        let eye_block = [
                            self.camera.position[0].floor() as i32,
                            self.camera.position[1].floor() as i32,
                            self.camera.position[2].floor() as i32,
                        ];
                        ui.label(format!(
                            "Block: [{}, {}, {}]",
                            eye_block[0], eye_block[1], eye_block[2]
                        ));
                        let eye_chunk = ChunkPos::from_block(eye_block[0], eye_block[2]);
                        ui.label(format!(
                            "Chunk: ({}, {}) loaded: {}",
                            eye_chunk.x,
                            eye_chunk.z,
                            self.world.is_chunk_loaded(eye_chunk)
                        ));
                        ui.label(format!("Submerged: {:?}", self.submerged));
                        if let Some(stream) = self.streamer.as_ref() {
                            ui.label(format!(
                                "Stream: center ({}, {}), pending {}, inflight {}",
                                stream.center.0,
                                stream.center.1,
                                stream.pending.len(),
                                self.inflight
                            ));
                        }
                        ui.label(format!("Light mode: {:?}", self.light_mode));
                        ui.separator();
                        ui.heading("Block inspector");
                        let dir = self.camera.forward();
                        let hit = raycast(&self.world, self.camera.position, dir, 8.0);
                        match hit {
                            Some((pos, _)) => {
                                match self.world.get_block((pos[0], pos[1], pos[2])) {
                                    Some(state) => {
                                        let (block_light, sky_light) =
                                            self.world.light_level((pos[0], pos[1], pos[2]));
                                        ui.label(format!(
                                            "Pos: [{}, {}, {}]",
                                            pos[0], pos[1], pos[2]
                                        ));
                                        ui.label(format!("Block: {}", state.block().name()));
                                        ui.label(format!("State id: {}", u16::from(state)));
                                        ui.label(format!(
                                            "Light: block {block_light} sky {sky_light}"
                                        ));
                                        ui.label(format!("self_skip: {}", state.self_skip()));
                                        ui.label(format!(
                                            "fluid: {}",
                                            state.fluid().unwrap_or("-")
                                        ));
                                        ui.label(format!(
                                            "liquid: {}",
                                            state.block().liquid_fluid().unwrap_or("-")
                                        ));
                                        let skip = state.block().skip_against_tags();
                                        ui.label(format!(
                                            "skip_against: {}",
                                            if skip.is_empty() {
                                                String::from("-")
                                            } else {
                                                skip.join(", ")
                                            }
                                        ));
                                        let culling = state.block().culling_tags();
                                        ui.label(format!(
                                            "culling: {}",
                                            if culling.is_empty() {
                                                String::from("-")
                                            } else {
                                                culling.join(", ")
                                            }
                                        ));
                                    }
                                    None => {
                                        ui.label("No block data");
                                    }
                                }
                            }
                            None => {
                                ui.label("No block in reach");
                            }
                        }
                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui.button("Remesh").clicked() {
                                do_remesh = true;
                            }
                            if ui.button("Recompute light").clicked() {
                                do_recompute = true;
                            }
                        });
                    });
                });
                if self.light_mode != mode_before {
                    do_remesh = true;
                    eprintln!("viewer: light mode {:?}", self.light_mode);
                }
                if (self.double_sided_liquids, self.fast_leaves) != old_mesh_options {
                    do_remesh = true;
                    eprintln!(
                        "viewer: mesh options double_sided_liquids={} fast_leaves={}",
                        self.double_sided_liquids, self.fast_leaves
                    );
                }
                if (self.cycle_enabled, self.cycle_paused) != old_cycle {
                    if self.cycle_enabled {
                        eprintln!("viewer: day/night cycle started");
                    } else {
                        eprintln!("viewer: day/night cycle stopped");
                    }
                    if self.cycle_paused {
                        eprintln!("viewer: day/night cycle paused");
                    }
                }
            }
            let output = ctx.end_pass();
            state.handle_platform_output(&window, output.platform_output);
            let pixels_per_point = window.scale_factor() as f32;
            let paint_jobs = ctx.tessellate(output.shapes, pixels_per_point);
            let screen_descriptor = egui_wgpu::ScreenDescriptor {
                size_in_pixels: [size.width, size.height],
                pixels_per_point,
            };
            (paint_jobs, screen_descriptor, output.textures_delta)
        };

        if do_recompute {
            self.world.mark_light_full();
            eprintln!("viewer: recompute requested");
        }
        if do_remesh || do_recompute {
            self.remesh_all();
        }
        if self.pending_reload {
            self.reload_packs();
        }

        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        if (self.unlit, self.heatmap, self.fog_enabled) != old_debug {
            renderer.set_debug_flags(DebugFlags {
                unlit: self.unlit,
                heatmap: self.heatmap,
                fog_off: !self.fog_enabled,
            });
        }
        if (self.wireframe, self.backface_cull, self.chunk_borders) != old_geometry {
            renderer.set_wireframe(self.wireframe);
            renderer.set_backface_culling(self.backface_cull);
            renderer.set_chunk_borders(self.chunk_borders);
        }
        if self.submerged != old_submerged
            || (self.fog_start, self.fog_end, self.sky_factor) != old_fog
        {
            let (color, start, end) = match self.submerged {
                Some(FluidKind::Water) => (WATER_FOG_COLOR, -8.0, 96.0),
                Some(FluidKind::Lava) => (LAVA_FOG_COLOR, 0.25, 1.0),
                None => {
                    let scale = fog_density_scale(self.sky_factor);
                    (
                        fog_color_for(self.sky_factor),
                        self.fog_start * scale,
                        self.fog_end.max(self.fog_start) * scale,
                    )
                }
            };
            renderer.set_fog(color, start, end);
        }
        if self.sky_factor != old_sky {
            renderer.set_sky_factor(self.sky_factor);
        }
        if self.brightness != old_brightness {
            renderer.set_brightness(self.brightness);
        }
        if (self.cloud_status, self.cloud_range) != old_clouds {
            renderer.set_cloud_status(self.cloud_status);
            renderer.set_cloud_range(self.cloud_range);
        }
        renderer.set_cloud_time(
            self.cloud_ticks.floor() as i64,
            self.cloud_ticks.fract() as f32,
        );
        renderer.set_cloud_color(cloud_color_for(self.sky_factor));
        self.render_visible(
            view_proj.0,
            camera_pos,
            Some(EguiFrame {
                paint_jobs: &paint_jobs,
                screen_descriptor,
                textures_delta,
            }),
        );
    }

    fn edit_block(&mut self, place: bool) {
        let origin = self.camera.position;
        let dir = self.camera.forward();
        let Some((hit, adjacent)) = raycast(&self.world, origin, dir, 8.0) else {
            return;
        };

        let target = if place { adjacent } else { hit };
        let Some(current) = self.world.get_block((target[0], target[1], target[2])) else {
            return;
        };

        if place {
            if !current.is_air() {
                return;
            }
            self.world.set_block(
                (target[0], target[1], target[2]),
                Block::STONE.default_state(),
            );
        } else {
            self.world.set_block(
                (target[0], target[1], target[2]),
                Block::AIR.default_state(),
            );
        }

        let dirty = self.world.take_dirty();
        if dirty.is_empty() {
            return;
        }
        self.submit_job(Vec::new(), dirty);
    }

    fn mesh_options(&self) -> MeshOptions {
        MeshOptions {
            double_sided_liquids: self.double_sided_liquids,
            fast_leaves: self.fast_leaves,
        }
    }

    /// Camera fog fluid per vanilla `Camera.calculateFogType` (water only; the
    /// lava near-plane probe is approximated by the eye block).
    fn fog_fluid(&self) -> Option<FluidKind> {
        let [x, y, z] = self.camera.position;
        let block_pos = (x.floor() as i32, y.floor() as i32, z.floor() as i32);
        if !self
            .world
            .is_chunk_loaded(ChunkPos::from_block(block_pos.0, block_pos.2))
        {
            // The eye chunk is still a placeholder: keep the previous state
            // instead of snapping out of the water fog mid-stream.
            return self.submerged;
        }
        let fluid = fluid_state(self.world.get_block(block_pos)?)?;
        let top = block_pos.1 as f32
            + if fluid.is_source()
                && self
                    .world
                    .get_block((block_pos.0, block_pos.1 + 1, block_pos.2))
                    .is_some_and(|above| is_face_sturdy(above, Direction::Down))
            {
                1.0
            } else {
                fluid.own_height()
            };
        (y < top).then_some(fluid.kind)
    }

    fn render_visible(
        &mut self,
        view_proj: [f32; 16],
        camera_pos: [f32; 3],
        egui: Option<EguiFrame<'_>>,
    ) {
        let frustum = Frustum::from_matrix(&Mat4(view_proj));
        let mut visible = Vec::new();
        for (index, chunk) in self.meshes.iter().enumerate() {
            let (min, max) = chunk_aabb(chunk.origin);
            if frustum.intersects_aabb(min, max) {
                visible.push(index);
            }
        }
        self.visible_chunks = visible.len();
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        let chunks: Vec<RenderChunk<'_>> = visible
            .iter()
            .map(|&index| {
                let entry = &self.meshes[index];
                RenderChunk {
                    origin: entry.origin,
                    version: entry.version,
                    mesh: &entry.mesh,
                }
            })
            .collect();
        renderer.render(view_proj, camera_pos, &chunks, egui);
    }

    fn submit_job(&mut self, stream: Vec<(i32, i32)>, dirty: Vec<ChunkPos>) {
        self.generation = self.worker.submit(
            self.world.clone(),
            stream,
            dirty,
            self.world.pending_light_edits().to_vec(),
            self.light_mode,
            self.mesh_options(),
        );
        self.inflight += 1;
    }

    fn remesh_all(&mut self) {
        self.world.mark_all_dirty();
        let dirty = self.world.take_dirty();
        self.submit_job(Vec::new(), dirty);
    }

    /// Hot-reloads the pack stack after a selection change: rebuilds data +
    /// atlas, swaps the GPU texture, recreates the worker on the new lookup
    /// table, and remeshes everything (UVs move). Old in-flight results die
    /// with the old worker; generations restart from zero.
    fn reload_packs(&mut self) {
        self.pending_reload = false;
        let result = self.tokio_handle.block_on(self.packs.rebuild());
        match result {
            Ok((atlas, cloud_texture, data)) => {
                let data = Arc::new(data);
                let atlas = Arc::new(atlas);
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.set_atlas(&atlas);
                    renderer.set_cloud_texture(cloud_texture.clone());
                }
                self.cloud_texture = cloud_texture;
                self.atlas = atlas.clone();
                let source = self.streamer.as_ref().map(|s| s.source.clone());
                self.worker = MeshWorker::new(data, atlas, source);
                self.generation = 0;
                self.inflight = 0;
                self.pending_stream_dirty.clear();
                for err in &self.packs.errors {
                    eprintln!("viewer: {err}");
                }
                self.remesh_all();
                eprintln!("viewer: packs reloaded");
            }
            Err(err) => eprintln!("viewer: pack reload failed ({err}), keeping current stack"),
        }
    }

    /// A stale result's meshes may predate newer edits, but any block data it
    /// streamed in is still the only copy of that chunk: apply it (with its
    /// light and provisional mesh) and queue a remesh of the chunk and its
    /// border neighbors so the current world's edits and light win.
    fn salvage_streamed(&mut self, result: MeshResult) {
        for chunk in result.chunks {
            let Some(data) = chunk.chunk else {
                continue;
            };
            let origin = [chunk.pos.x * CHUNK_WIDTH, chunk.pos.z * CHUNK_WIDTH];
            self.world.set_chunk(data);
            self.world.set_chunk_light(chunk.pos, chunk.light);
            match self.meshes.iter_mut().find(|entry| entry.origin == origin) {
                Some(entry) => {
                    entry.mesh = chunk.mesh;
                    entry.version += 1;
                }
                None => self.meshes.push(ChunkMesh {
                    origin,
                    mesh: chunk.mesh,
                    version: 0,
                }),
            }
            for dz in -1..=1 {
                for dx in -1..=1 {
                    let pos = ChunkPos::new(chunk.pos.x + dx, chunk.pos.z + dz);
                    if !self.pending_stream_dirty.contains(&pos) {
                        self.pending_stream_dirty.push(pos);
                    }
                }
            }
        }
        self.total_chunks = self.meshes.len();
    }

    fn apply_results(&mut self) {
        for result in self.worker.poll() {
            self.inflight = self.inflight.saturating_sub(1);
            if result.generation != self.generation {
                self.salvage_streamed(result);
                continue;
            }
            self.world.acknowledge_light_edits(&result.light_edits);
            let mut remeshed = 0;
            for chunk in result.chunks {
                if let Some(data) = chunk.chunk {
                    let origin = [chunk.pos.x * CHUNK_WIDTH, chunk.pos.z * CHUNK_WIDTH];
                    self.world.set_chunk(data);
                    self.world.set_chunk_light(chunk.pos, chunk.light);
                    match self.meshes.iter_mut().find(|entry| entry.origin == origin) {
                        Some(entry) => {
                            entry.mesh = chunk.mesh;
                            entry.version += 1;
                        }
                        None => self.meshes.push(ChunkMesh {
                            origin,
                            mesh: chunk.mesh,
                            version: 0,
                        }),
                    }
                    remeshed += 1;
                } else {
                    self.world.set_chunk_light(chunk.pos, chunk.light);
                    if let Some(entry) = self
                        .meshes
                        .iter_mut()
                        .find(|entry| entry.origin == chunk.origin)
                    {
                        entry.mesh = chunk.mesh;
                        entry.version += 1;
                        remeshed += 1;
                    }
                }
            }
            self.prune_far_meshes();
            self.total_chunks = self.meshes.len();
            eprintln!(
                "viewer: remeshed {remeshed} chunks (generation {})",
                result.generation
            );
        }
    }

    /// Drops chunk meshes farther than `radius + 2` from the stream center
    /// so long flights cannot grow CPU/GPU memory without bound. Demo mode
    /// (no streamer) keeps everything.
    fn prune_far_meshes(&mut self) {
        let Some(stream) = self.streamer.as_ref() else {
            return;
        };
        let keep_radius = stream.radius + 2;
        let before = self.meshes.len();
        self.meshes.retain(|entry| {
            let cx = entry.origin[0].div_euclid(CHUNK_WIDTH);
            let cz = entry.origin[1].div_euclid(CHUNK_WIDTH);
            (cx - stream.center.0).abs() <= keep_radius
                && (cz - stream.center.1).abs() <= keep_radius
        });
        if self.meshes.len() != before
            && let Some(renderer) = self.renderer.as_mut()
        {
            let keep: Vec<[i32; 2]> = self.meshes.iter().map(|entry| entry.origin).collect();
            renderer.prune_mesh_cache(&keep);
        }
    }

    /// Streams save chunks around the camera: rebases the ring when the
    /// camera chunk moves and submits one pending chunk per quiet frame.
    fn tick_stream(&mut self) {
        let Some(stream) = self.streamer.as_mut() else {
            return;
        };
        let center = (
            self.camera.position[0].div_euclid(CHUNK_WIDTH as f32) as i32,
            self.camera.position[2].div_euclid(CHUNK_WIDTH as f32) as i32,
        );
        if center != stream.center {
            stream.center = center;
            stream.pending.clear();
            let mut needed: Vec<((i32, i32), i32)> = Vec::new();
            for dz in -stream.radius..=stream.radius {
                for dx in -stream.radius..=stream.radius {
                    let pos = (center.0 + dx, center.1 + dz);
                    let loaded = self.world.is_chunk_loaded(ChunkPos::new(pos.0, pos.1));
                    if !loaded {
                        needed.push((pos, dx.abs() + dz.abs()));
                    }
                }
            }
            needed.sort_by_key(|&(_, dist)| dist);
            stream.pending.extend(needed.iter().map(|&(pos, _)| pos));
            if !stream.pending.is_empty() {
                eprintln!(
                    "viewer: stream center {center:?}, {} chunks pending",
                    stream.pending.len()
                );
            }
        }
        if self.inflight > 0 {
            return;
        }
        // Salvaged streamed chunks wait for a quiet frame too: their remesh
        // must see every later edit.
        if !self.pending_stream_dirty.is_empty() {
            let dirty = std::mem::take(&mut self.pending_stream_dirty);
            self.submit_job(Vec::new(), dirty);
            return;
        }
        // One chunk per quiet frame: keeps individual jobs short so an edit
        // landing mid-stream never waits long behind a big batch.
        while let Some(pos) = stream.pending.pop_front() {
            let loaded = self.world.is_chunk_loaded(ChunkPos::new(pos.0, pos.1));
            if loaded {
                continue;
            }
            self.submit_job(vec![pos], Vec::new());
            break;
        }
    }
}
