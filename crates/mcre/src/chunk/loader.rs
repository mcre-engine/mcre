use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicUsize},
};

use bevy::{
    asset::{AssetLoadError, AssetPath, LoadState, io::AssetReaderError},
    platform::collections::{HashMap, HashSet},
    prelude::*,
    tasks::{AsyncComputeTaskPool, IoTaskPool, Task},
};

use futures_lite::future;

use crate::{
    AppState, LoadingState,
    chunk::{
        Chunk, ChunkComponent,
        asset::ChunkAssetLoader,
        generate::{
            generate_chunk,
            rng::{ChunkRng, SeedRng},
        },
        math::{pos::ChunkPosition, size::ChunkSize},
        mesh::ChunkMeshBuilder,
    },
    textures::BlockTextures,
};

#[derive(Default)]
pub struct ChunkLoaderPlugin {
    pub config: ChunkLoaderConfig,
}

impl Plugin for ChunkLoaderPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<Chunk>()
            .init_resource::<ChunkLoader>()
            .init_resource::<SeedRng>()
            .init_asset_loader::<ChunkAssetLoader>()
            .insert_resource(self.config.clone())
            .insert_resource(Time::from_seconds(1. / 20.))
            .add_systems(
                FixedUpdate,
                (
                    ChunkLoader::read_chunks,
                    ChunkLoader::load_chunks,
                    ChunkLoader::queue_generation_tasks,
                    ChunkLoader::handle_generation_tasks,
                    ChunkLoader::queue_mesh_tasks,
                    ChunkLoader::handle_mesh_tasks,
                    |loader: Res<ChunkLoader>, mut next_state: ResMut<NextState<AppState>>| {
                        if loader.unloaded_chunks.is_empty() && loader.rendering_chunks.is_empty() {
                            next_state.set(AppState::InGame);
                        }
                    },
                )
                    .chain()
                    .run_if(in_state(LoadingState::Chunks)),
            )
            .add_systems(
                FixedUpdate,
                (
                    ChunkLoader::read_chunks,
                    ChunkLoader::load_chunks,
                    ChunkLoader::queue_generation_tasks,
                    ChunkLoader::handle_generation_tasks,
                    ChunkLoader::queue_mesh_tasks,
                    ChunkLoader::handle_mesh_tasks,
                    ChunkLoader::despawn_chunks,
                )
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            );
        let seed = app.world_mut().resource_mut::<SeedRng>().generate();
        app.insert_resource(ChunkRng::new(seed));
    }
}

#[derive(Resource, Default, Debug)]
pub struct ChunkLoader {
    //TODO: Convert to some faster lookups
    unloaded_chunks: HashSet<ChunkPosition>,
    reading_chunks: HashMap<ChunkPosition, Handle<Chunk>>,
    generating_chunks: HashSet<ChunkPosition>,
    rendering_chunks: HashMap<ChunkPosition, Handle<Chunk>>,
    meshing_chunks: HashSet<ChunkPosition>,
    loaded_chunks: HashMap<ChunkPosition, Handle<Chunk>>,
    saving_chunks: Arc<AtomicUsize>,
}

impl ChunkLoader {
    pub fn unloaded_chunks(&self) -> usize {
        self.unloaded_chunks.len()
    }

    pub fn generating_chunks(&self) -> usize {
        self.generating_chunks.len()
    }

    pub fn rendering_chunks(&self) -> usize {
        self.rendering_chunks.len()
    }

    #[allow(dead_code)]
    pub fn meshing_chunks(&self) -> usize {
        self.meshing_chunks.len()
    }

    pub fn loaded_chunks(&self) -> usize {
        self.loaded_chunks.len()
    }

    pub fn saving_chunks(&self) -> usize {
        self.saving_chunks
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    fn contains(&self, pos: &ChunkPosition) -> bool {
        self.unloaded_chunks.contains(pos)
            || self.reading_chunks.contains_key(pos)
            || self.generating_chunks.contains(pos)
            || self.rendering_chunks.contains_key(pos)
            || self.meshing_chunks.contains(pos)
            || self.loaded_chunks.contains_key(pos)
    }

    pub fn read_chunks(
        camera: Query<&Transform, With<Camera>>,
        config: Res<ChunkLoaderConfig>,
        mut loader: ResMut<ChunkLoader>,
    ) {
        let camera_loc = camera.single().unwrap().translation;
        let cur_chunk = ChunkPosition::from_world_coord(camera_loc, config.chunk_size);
        for loc in cur_chunk.iter_around(config.chunk_radius as u64) {
            if !loader.contains(&loc) {
                loader.unloaded_chunks.insert(loc);
            }
        }
    }

    pub fn load_chunks(
        mut loader: ResMut<ChunkLoader>,
        chunks: Res<Assets<Chunk>>,
        assets: Res<AssetServer>,
        config: Res<ChunkLoaderConfig>,
    ) {
        let mut generate_chunks = Vec::new();
        let mut file_chunks = Vec::new();
        loader.reading_chunks.retain(|loc, handle| {
            match assets.get_load_state(handle.id()) {
                None => {
                    if chunks.get(handle.id()).is_some() {
                        // Chunk is already loaded as an asset
                        return false;
                    }
                }
                Some(LoadState::Failed(err)) => {
                    match &*err {
                        AssetLoadError::AssetReaderError(AssetReaderError::NotFound(_)) => {}
                        err => {
                            warn!("Error loading chunk ({}, {}): {err:?}", loc.x, loc.y);
                        }
                    }
                    // Chunk failed to load so we regenerate chunk
                    generate_chunks.push(*loc);
                    // new_chunks.push((*loc, chunks.add(spawn_test_chunk(config.chunk_size, *loc))));
                    return false;
                }
                Some(LoadState::Loaded) => {
                    file_chunks.push((*loc, handle.clone()));
                    return false;
                }
                _ => {
                    // waiting to finish loading
                }
            }
            true
        });
        let batch_size = config
            .batching
            .reading
            .min(loader.unloaded_chunks.len())
            .saturating_sub(loader.reading_chunks.len());
        if batch_size > 0 {
            let iter = loader
                .unloaded_chunks
                .iter()
                .copied()
                .take(batch_size)
                .collect::<Vec<_>>();
            for loc in iter {
                loader.unloaded_chunks.remove(&loc);
                loader
                    .reading_chunks
                    .insert(loc, assets.load(format!("chunks/{}_{}.mcra", loc.x, loc.y)));
            }
        }
        if !file_chunks.is_empty() {
            loader.rendering_chunks.extend(file_chunks);
        }
        if !generate_chunks.is_empty() {
            loader.generating_chunks.extend(generate_chunks);
        }
    }

    pub fn queue_generation_tasks(
        mut commands: Commands,
        mut loader: ResMut<ChunkLoader>,
        config: Res<ChunkLoaderConfig>,
        state: Res<State<AppState>>,
        rng: Res<ChunkRng>,
    ) {
        if loader.generating_chunks.is_empty() {
            return;
        }

        let batch_size = match state.get() {
            AppState::Loading => config.batching.loading,
            _ => config.batching.generating,
        };

        let batch: Vec<ChunkPosition> = loader
            .generating_chunks
            .iter()
            .take(batch_size)
            .copied()
            .collect();

        let task_pool = AsyncComputeTaskPool::get();
        let chunk_size = config.chunk_size;
        let rng = rng.clone(); // ChunkRng is now Clone (cheap - just copies the Perlin seed)

        for loc in batch {
            loader.generating_chunks.remove(&loc);

            let rng = rng.clone();
            let task = task_pool.spawn(async move { generate_chunk(chunk_size, loc, &rng) });

            commands.spawn(GenerationTask { pos: loc, task });
        }
    }

    pub fn handle_generation_tasks(
        mut commands: Commands,
        mut chunks: ResMut<Assets<Chunk>>,
        mut loader: ResMut<ChunkLoader>,
        mut query: Query<(Entity, &mut GenerationTask)>,
    ) {
        for (entity, mut gen_task) in &mut query {
            if let Some(chunk) = future::block_on(future::poll_once(&mut gen_task.task)) {
                let loc = gen_task.pos;
                let handle = chunks.add(chunk);
                loader.rendering_chunks.insert(loc, handle);
                commands.entity(entity).despawn();
            }
        }
    }

    pub fn despawn_chunks(
        mut commands: Commands,
        camera: Query<&Transform, With<Camera>>,
        components: Query<(Entity, &ChunkComponent)>,
        mut chunks: ResMut<Assets<Chunk>>,
        config: Res<ChunkLoaderConfig>,
        mut loader: ResMut<ChunkLoader>,
        server: Res<AssetServer>,
    ) {
        if components.is_empty() {
            return;
        }
        let camera_loc = camera.single().unwrap().translation;
        let cur_chunk = ChunkPosition::from_world_coord(camera_loc, config.chunk_size);
        let radius = config.chunk_radius as u64;
        let remove_chunks = components
            .iter()
            .filter_map(|(entity, chunk)| {
                let id = chunk.0.id();
                let chunk = chunks.get(id)?;

                cur_chunk
                    .outside_radius(chunk.loc, radius)
                    .then_some((entity, chunk.loc, id))
            })
            .collect::<Vec<_>>();
        let mut file_batches = Vec::new();
        for (entity, loc, id) in remove_chunks {
            //TODO: Save to disk
            loader.loaded_chunks.remove(&loc);
            commands.entity(entity).despawn();
            let chunk = chunks.remove(id).unwrap();
            let loc = chunk.loc;
            let asset_loader = ChunkAssetLoader::default();
            let chunk = asset_loader.to_bytes(&chunk).unwrap();
            file_batches.push((loc, chunk))
        }
        loader.save_chunk_data(file_batches, &config, &server);
    }

    pub fn save_all_chunks(
        &mut self,
        chunks: &Assets<Chunk>,
        config: &ChunkLoaderConfig,
        server: &AssetServer,
    ) {
        let ids = self
            .loaded_chunks
            .values()
            .map(|v| v.id())
            .collect::<Vec<_>>();
        let mut file_batches = Vec::new();
        for id in ids {
            let chunk = chunks.get(id).unwrap();
            let loc = chunk.loc;
            let asset_loader = ChunkAssetLoader::default();
            let chunk = asset_loader.to_bytes(chunk).unwrap();
            file_batches.push((loc, chunk))
        }
        self.save_chunk_data(file_batches, config, server);
    }

    fn save_chunk_data(
        &mut self,
        mut data: Vec<(ChunkPosition, Vec<u8>)>,
        config: &ChunkLoaderConfig,
        server: &AssetServer,
    ) {
        if !config.enable_saving {
            return;
        }
        while !data.is_empty() {
            let batch_size = config.batching.saving.min(data.len());
            let values = data.drain(..batch_size).collect::<Vec<_>>();
            if values.is_empty() {
                break;
            }
            let server = server.clone();
            let size = values.len();
            let saving_chunks = self.saving_chunks.clone();
            IoTaskPool::get()
                .spawn(async move {
                    for (loc, chunk) in values {
                        let path = AssetPath::from_path_buf(PathBuf::from(format!(
                            "chunks/{}_{}.mcra",
                            loc.x, loc.y
                        )));

                        let source = server.get_source(path.source()).unwrap();
                        let writer = source.writer().unwrap();
                        writer.write_bytes(path.path(), &chunk).await.unwrap();
                    }
                    saving_chunks.fetch_sub(size, std::sync::atomic::Ordering::Relaxed);
                })
                .detach();
            self.saving_chunks
                .fetch_add(size, std::sync::atomic::Ordering::Relaxed);
        }
    }

    pub fn queue_mesh_tasks(
        mut commands: Commands,
        mut loader: ResMut<ChunkLoader>,
        chunks: Res<Assets<Chunk>>,
        textures: Res<BlockTextures>,
        config: Res<ChunkLoaderConfig>,
        state: Res<State<AppState>>,
    ) {
        if loader.rendering_chunks.is_empty() {
            return;
        }

        let Some(texture_lookup) = textures.lookup() else {
            return;
        };

        let length = loader.rendering_chunks.len();
        let batch_size = config.batching.rendering(state.get()).min(length);
        let batch_keys: Vec<ChunkPosition> = loader
            .rendering_chunks
            .keys()
            .take(batch_size)
            .copied()
            .collect();

        let task_pool = AsyncComputeTaskPool::get();

        for chunk_pos in batch_keys {
            let handle = loader.rendering_chunks.remove(&chunk_pos).unwrap();

            let Some(chunk) = chunks.get(&handle) else {
                loader.rendering_chunks.insert(chunk_pos, handle);
                continue;
            };

            let chunk_data = chunk.clone();
            let lookup = texture_lookup.clone();

            let task = task_pool.spawn(async move {
                let builder = ChunkMeshBuilder::new(&chunk_data);
                // TODO: Pass neighbors here later!
                builder.build(&lookup)
            });

            loader.meshing_chunks.insert(chunk_pos);

            commands.spawn((ChunkComponent(handle), chunk.transform(), MeshingTask(task)));
        }
    }

    pub fn handle_mesh_tasks(
        mut commands: Commands,
        mut meshes: ResMut<Assets<Mesh>>,
        textures: Res<BlockTextures>,
        mut loader: ResMut<ChunkLoader>,
        chunks: Res<Assets<Chunk>>,
        mut query: Query<(Entity, &ChunkComponent, &mut MeshingTask)>,
    ) {
        for (entity, chunk_component, mut task) in &mut query {
            if let Some(generated_mesh) = future::block_on(future::poll_once(&mut task.0)) {
                let mesh_handle = meshes.add(generated_mesh);

                // Move from meshing to loaded
                if let Some(chunk) = chunks.get(&chunk_component.0) {
                    loader.meshing_chunks.remove(&chunk.loc);
                    loader
                        .loaded_chunks
                        .insert(chunk.loc, chunk_component.0.clone());
                }

                commands.entity(entity).remove::<MeshingTask>().insert((
                    Mesh3d(mesh_handle),
                    MeshMaterial3d(textures.texture().unwrap().clone()),
                ));
            }
        }
    }
}

#[derive(Clone, Resource)]
pub struct ChunkLoaderConfig {
    /// Number of chunks rendered around the camera in the x, y, z directions
    pub chunk_radius: usize,
    pub chunk_size: ChunkSize,
    pub enable_saving: bool,
    pub batching: Batching,
}

impl Default for ChunkLoaderConfig {
    fn default() -> Self {
        ChunkLoaderConfig {
            chunk_radius: 10,
            enable_saving: false,
            chunk_size: ChunkSize::new(16),
            batching: Default::default(),
        }
    }
}

#[derive(Clone)]
pub struct Batching {
    reading: usize,
    loading: usize,
    generating: usize,
    rendering: usize,
    saving: usize,
}

impl Batching {
    fn rendering(&self, state: &AppState) -> usize {
        if let AppState::Loading = state {
            self.loading
        } else {
            self.rendering
        }
    }
}

impl Default for Batching {
    fn default() -> Self {
        Self {
            reading: 50,
            loading: 100,
            generating: 10,
            rendering: 50,
            saving: 10,
        }
    }
}

#[derive(Component)]
pub struct MeshingTask(Task<Mesh>);

#[derive(Component)]
pub struct GenerationTask {
    pos: ChunkPosition,
    task: Task<Chunk>,
}
