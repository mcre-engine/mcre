use std::path::PathBuf;

use bevy::{
    asset::{AssetLoadError, AssetPath, LoadState, io::AssetReaderError},
    ecs::{system::SystemState, world::CommandQueue},
    platform::collections::HashMap,
    prelude::*,
    tasks::{AsyncComputeTaskPool, IoTaskPool, Task, TaskPool, futures},
};

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
            .add_systems(
                Update,
                (
                    ChunkLoader::read_chunks,
                    ChunkLoader::load_chunks,
                    |loader: Res<ChunkLoader>, mut next_state: ResMut<NextState<AppState>>| {
                        if loader.has_loaded() {
                            next_state.set(AppState::InGame);
                        }
                    },
                )
                    .chain()
                    .run_if(in_state(LoadingState::Chunks)),
            )
            .add_systems(
                Update,
                (
                    ChunkLoader::read_chunks,
                    ChunkLoader::load_chunks,
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
    chunk_states: HashMap<ChunkPosition, ChunkState>,
}

impl ChunkLoader {
    pub fn has_loaded(&self) -> bool {
        self.chunk_states
            .values()
            .all(|s| matches!(s, ChunkState::Loaded(_)))
    }

    pub fn unloaded_chunks(&self) -> usize {
        self.chunk_states
            .values()
            .filter(|s| matches!(s, ChunkState::Reading(_)))
            .count()
    }

    pub fn generating_chunks(&self) -> usize {
        self.chunk_states
            .values()
            .filter(|s| matches!(s, ChunkState::Generating(_)))
            .count()
    }

    pub fn rendering_chunks(&self) -> usize {
        self.chunk_states
            .values()
            .filter(|s| matches!(s, ChunkState::Render(_)))
            .count()
    }

    pub fn loaded_chunks(&self) -> usize {
        self.chunk_states
            .values()
            .filter(|s| matches!(s, ChunkState::Loaded(_)))
            .count()
    }

    pub fn saving_chunks(&self) -> usize {
        self.chunk_states
            .values()
            .filter(|s| matches!(s, ChunkState::Saving))
            .count()
    }

    pub fn read_chunks(
        camera: Query<&Transform, With<Camera>>,
        config: Res<ChunkLoaderConfig>,
        assets: Res<AssetServer>,
        mut loader: ResMut<ChunkLoader>,
    ) {
        let camera_loc = camera.single().unwrap().translation;
        let cur_chunk = ChunkPosition::from_world_coord(camera_loc, config.chunk_size);
        for loc in cur_chunk.iter_around(config.chunk_radius as u64) {
            if !loader.chunk_states.contains_key(&loc) {
                let handle = assets.load(format!("chunks/{}_{}.mcra", loc.x, loc.y));
                loader.chunk_states.insert(loc, ChunkState::Reading(handle));
            }
        }
    }

    pub fn load_chunks(
        mut commands: Commands,
        mut loader: ResMut<ChunkLoader>,
        mut chunks: ResMut<Assets<Chunk>>,
        assets: Res<AssetServer>,
        config: Res<ChunkLoaderConfig>,
        textures: Res<BlockTextures>,
        rng: Res<ChunkRng>,
    ) {
        let task_pool = AsyncComputeTaskPool::get();
        loader.chunk_states.retain(|pos, chunk| {
            let mut next_chunk = None;
            match chunk {
                ChunkState::Reading(handle) => {
                    match assets.get_load_state(handle.id()) {
                        None => {
                            if chunks.get(handle.id()).is_some() {
                                next_chunk =
                                    Some(ChunkState::render(task_pool, &chunks, handle, &textures));
                            } else {
                                warn!(
                                    "Attempting to load invalid chunk, reverting to regenerating"
                                );
                                next_chunk = Some(ChunkState::generate(
                                    task_pool,
                                    config.chunk_size,
                                    *pos,
                                    &rng,
                                ));
                            }
                        }
                        Some(LoadState::Failed(err)) => {
                            match &*err {
                                AssetLoadError::AssetReaderError(AssetReaderError::NotFound(_)) => {
                                }
                                err => {
                                    warn!("Error loading chunk ({}, {}): {err:?}", pos.x, pos.y);
                                }
                            }
                            // Chunk failed to load so we regenerate chunk
                            next_chunk = Some(ChunkState::generate(
                                task_pool,
                                config.chunk_size,
                                *pos,
                                &rng,
                            ));
                        }
                        Some(LoadState::Loaded) => {
                            next_chunk =
                                Some(ChunkState::render(task_pool, &chunks, handle, &textures));
                        }
                        _ => {
                            // waiting to finish loading
                        }
                    }
                }
                ChunkState::Generating(task) => {
                    if let Some(chunk) = futures::check_ready(task) {
                        let handle = chunks.add(chunk);
                        next_chunk =
                            Some(ChunkState::render(task_pool, &chunks, &handle, &textures));
                    }
                }
                ChunkState::Render(task) => {
                    if let Some((handle, mut queue)) = futures::check_ready(task) {
                        commands.append(&mut queue);
                        next_chunk = Some(ChunkState::Loaded(handle));
                    }
                }
                ChunkState::Saving => {
                    return false;
                }
                _ => {}
            }
            if let Some(next_chunk) = next_chunk {
                *chunk = next_chunk;
            }
            true
        });
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
            *loader.chunk_states.get_mut(&loc).unwrap() = ChunkState::Saving;
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
            .chunk_states
            .values()
            .filter_map(|s| match s {
                ChunkState::Loaded(h) => Some(h.id()),
                _ => None,
            })
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
            let batch_size = config.save_batch.min(data.len());
            let values = data.drain(..batch_size).collect::<Vec<_>>();
            if values.is_empty() {
                break;
            }
            let server = server.clone();
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
                })
                .detach();
        }
    }
}

#[derive(Debug)]
pub enum ChunkState {
    Reading(Handle<Chunk>),
    Generating(Task<Chunk>),
    Render(Task<(Handle<Chunk>, CommandQueue)>),
    Loaded(Handle<Chunk>),
    Saving,
}

impl ChunkState {
    fn generate(
        task_pool: &TaskPool,
        chunk_size: ChunkSize,
        pos: ChunkPosition,
        rng: &ChunkRng,
    ) -> Self {
        let rng = rng.clone();
        ChunkState::Generating(
            task_pool.spawn(async move { generate_chunk(chunk_size, pos, &rng) }),
        )
    }

    fn render(
        task_pool: &TaskPool,
        chunks: &Assets<Chunk>,
        handle: &Handle<Chunk>,
        block_textures: &BlockTextures,
    ) -> Self {
        let chunk = chunks
            .get(handle.id())
            .expect("Handle to correspond to chunk resource")
            .clone();
        let atlas = block_textures.atlas.clone();
        let handle = handle.clone();
        let task = task_pool.spawn(async move {
            let mesh = ChunkMeshBuilder::new(&chunk).build(&atlas);
            let mut commands = CommandQueue::default();

            let component = ChunkComponent(handle.clone());
            commands.push(move |world: &mut World| {
                let (mesh_handle, texture_handle) = {
                    let mut system_state =
                        SystemState::<(ResMut<Assets<Mesh>>, Res<BlockTextures>)>::new(world);
                    let (mut meshes, textures) = system_state.get_mut(world);
                    (meshes.add(mesh), textures.texture.clone())
                };
                world.spawn((
                    component,
                    chunk.transform(),
                    MeshMaterial3d(texture_handle),
                    Mesh3d(mesh_handle),
                ));
            });
            (handle, commands)
        });
        ChunkState::Render(task)
    }
}

#[derive(Clone, Resource)]
pub struct ChunkLoaderConfig {
    /// Number of chunks rendered around the camera in the x, y, z directions
    pub chunk_radius: usize,
    pub chunk_size: ChunkSize,
    pub enable_saving: bool,
    pub save_batch: usize,
}

impl Default for ChunkLoaderConfig {
    fn default() -> Self {
        ChunkLoaderConfig {
            chunk_radius: 10,
            enable_saving: false,
            chunk_size: ChunkSize::new(16),
            save_batch: 10,
        }
    }
}
