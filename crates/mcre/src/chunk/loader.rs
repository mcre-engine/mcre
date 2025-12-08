use bevy::{
    asset::LoadState,
    platform::collections::{HashMap, HashSet},
    prelude::*,
};

use crate::{
    AppState, LoadingState,
    chunk::{
        Chunk, ChunkComponent,
        asset::ChunkAssetLoader,
        generate::spawn_test_chunk,
        math::{pos::ChunkPosition, size::ChunkSize},
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
            .init_asset_loader::<ChunkAssetLoader>()
            .insert_resource(self.config.clone())
            .insert_resource(Time::from_seconds(1. / 20.))
            .add_systems(
                FixedUpdate,
                (
                    ChunkLoader::read_chunks,
                    ChunkLoader::load_chunks,
                    ChunkLoader::generate_chunks,
                    ChunkLoader::spawn_chunks,
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
                    ChunkLoader::generate_chunks,
                    ChunkLoader::spawn_chunks,
                    ChunkLoader::despawn_chunks,
                )
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

#[derive(Resource, Default, Debug)]
pub struct ChunkLoader {
    //TODO: Convert to some faster lookups
    unloaded_chunks: HashMap<ChunkPosition, Handle<Chunk>>,
    generating_chunks: HashSet<ChunkPosition>,
    rendering_chunks: HashMap<ChunkPosition, Handle<Chunk>>,
    loaded_chunks: HashMap<ChunkPosition, Handle<Chunk>>,
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

    pub fn loaded_chunks(&self) -> usize {
        self.loaded_chunks.len()
    }

    fn contains(&self, pos: &ChunkPosition) -> bool {
        self.unloaded_chunks.contains_key(pos)
            || self.generating_chunks.contains(pos)
            || self.rendering_chunks.contains_key(pos)
            || self.loaded_chunks.contains_key(pos)
    }

    pub fn read_chunks(
        camera: Query<&Transform, With<Camera>>,
        assets: Res<AssetServer>,
        config: Res<ChunkLoaderConfig>,
        mut loader: ResMut<ChunkLoader>,
    ) {
        let camera_loc = camera.single().unwrap().translation;
        let cur_chunk = config.chunk_size.chunk_coord(camera_loc);
        for loc in cur_chunk.iter_around(config.chunk_radius as u64) {
            if !loader.contains(&loc) {
                loader
                    .unloaded_chunks
                    .insert(loc, assets.load(format!("chunks/{}_{}.mcra", loc.x, loc.y)));
            }
        }
    }

    pub fn load_chunks(
        mut loader: ResMut<ChunkLoader>,
        chunks: Res<Assets<Chunk>>,
        assets: Res<AssetServer>,
    ) {
        let mut generate_chunks = Vec::new();
        let mut file_chunks = Vec::new();
        loader.unloaded_chunks.retain(|loc, handle| {
            match assets.get_load_state(handle.id()) {
                None => {
                    if chunks.get(handle.id()).is_some() {
                        // Chunk is already loaded as an asset
                        return false;
                    }
                }
                Some(LoadState::Failed(_)) => {
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
        if !file_chunks.is_empty() {
            loader.rendering_chunks.extend(file_chunks);
        }
        if !generate_chunks.is_empty() {
            loader.generating_chunks.extend(generate_chunks);
        }
    }

    pub fn generate_chunks(
        mut loader: ResMut<ChunkLoader>,
        mut chunks: ResMut<Assets<Chunk>>,
        config: Res<ChunkLoaderConfig>,
    ) {
        let batch = loader
            .generating_chunks
            .iter()
            .take(config.batching.generating)
            .copied()
            .collect::<Vec<_>>();
        for loc in batch {
            if loader.generating_chunks.remove(&loc) {
                let handle = chunks.add(spawn_test_chunk(config.chunk_size, loc));
                loader.rendering_chunks.insert(loc, handle);
            }
        }
    }

    /// Spawn chunks that are in the `UnloadedChunk` state
    pub fn spawn_chunks(
        mut commands: Commands,
        mut loader: ResMut<ChunkLoader>,
        mut meshes: ResMut<Assets<Mesh>>,
        state: Res<State<AppState>>,
        textures: Res<BlockTextures>,
        config: Res<ChunkLoaderConfig>,
        chunks: Res<Assets<Chunk>>,
    ) {
        if loader.rendering_chunks.is_empty() {
            return;
        }
        let length = loader.rendering_chunks.len();
        let batch_size = config.batching.rendering(state.get()).min(length);
        let batch = loader
            .rendering_chunks
            .keys()
            .copied()
            .take(batch_size)
            .collect::<Vec<_>>();
        if batch.is_empty() {
            return;
        }

        let span = info_span!("chunk_spawning");
        span.in_scope(|| {
            let batch = batch
                .into_iter()
                .filter_map(|i| loader.rendering_chunks.remove(&i))
                .collect::<Vec<_>>();
            for new_chunk in batch {
                let chunk = chunks.get(new_chunk.id()).unwrap();
                loader.loaded_chunks.insert(chunk.loc, new_chunk.clone());
                commands.spawn((
                    ChunkComponent(new_chunk),
                    chunk.transform(),
                    MeshMaterial3d(textures.texture().unwrap().clone()),
                    Mesh3d(meshes.add(chunk.generate_mesh(&textures))),
                ));
            }
        });
    }

    pub fn despawn_chunks(
        mut commands: Commands,
        camera: Query<&Transform, With<Camera>>,
        components: Query<(Entity, &ChunkComponent)>,
        mut chunks: ResMut<Assets<Chunk>>,
        config: Res<ChunkLoaderConfig>,
        mut loader: ResMut<ChunkLoader>,
    ) {
        if components.is_empty() {
            return;
        }
        let camera_loc = camera.single().unwrap().translation;
        let cur_chunk = config.chunk_size.chunk_coord(camera_loc);
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
        for (entity, loc, id) in remove_chunks {
            //TODO: Save to disk
            loader.loaded_chunks.remove(&loc);
            commands.entity(entity).despawn();
            chunks.remove(id);
        }
    }
}

#[derive(Clone, Resource)]
pub struct ChunkLoaderConfig {
    /// Number of chunks rendered around the camera in the x, y, z directions
    pub chunk_radius: usize,
    pub chunk_size: ChunkSize,
    pub batching: Batching,
}

impl Default for ChunkLoaderConfig {
    fn default() -> Self {
        ChunkLoaderConfig {
            chunk_radius: 10,
            chunk_size: ChunkSize::new(16),
            batching: Default::default(),
        }
    }
}

#[derive(Clone)]
pub struct Batching {
    loading: usize,
    generating: usize,
    rendering: usize,
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
            loading: 100,
            generating: 50,
            rendering: 50,
        }
    }
}
