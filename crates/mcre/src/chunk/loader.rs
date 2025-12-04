use bevy::{
    asset::LoadState,
    platform::collections::{HashMap, HashSet},
    prelude::*,
};

use crate::{
    AppState,
    chunk::{
        Chunk, ChunkComponent,
        asset::ChunkAssetLoader,
        generate::spawn_test_chunk,
        math::{pos::ChunkPosition, size::ChunkSize},
    },
    textures::BlockTextures,
};

const DEFAULT_CHUNK_RADIUS: usize = 7;
const DEFAULT_CHUNK_SIZE: ChunkSize = ChunkSize::new(16);

pub struct ChunkLoaderPlugin {
    pub config: ChunkLoaderConfig,
}

impl Plugin for ChunkLoaderPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<Chunk>()
            .init_resource::<ChunkLoader>()
            .init_asset_loader::<ChunkAssetLoader>()
            .insert_resource(self.config.clone())
            .insert_resource(Time::from_seconds(1. / 2.))
            .add_systems(
                FixedUpdate,
                (
                    ChunkLoader::load_chunks_system,
                    ChunkLoader::spawn_chunks,
                    ChunkLoader::despawn_chunks,
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

impl Default for ChunkLoaderPlugin {
    fn default() -> Self {
        Self {
            config: ChunkLoaderConfig {
                chunk_radius: DEFAULT_CHUNK_RADIUS,
                chunk_size: DEFAULT_CHUNK_SIZE,
            },
        }
    }
}

#[derive(Clone, Resource)]
pub struct ChunkLoaderConfig {
    /// Number of chunks rendered around the camera in the x, y, z directions
    pub chunk_radius: usize,
    pub chunk_size: ChunkSize,
}

type SpawnAssets<'a> = (
    Res<'a, AssetServer>,
    ResMut<'a, Assets<Chunk>>,
    ResMut<'a, Assets<Mesh>>,
);

#[derive(Resource, Default, Debug)]
pub struct ChunkLoader {
    //TODO: Convert to some faster lookup, possibly `Vec<Handle<Chunk>>`
    unloaded_chunks: HashMap<ChunkPosition, Handle<Chunk>>,
}

impl ChunkLoader {
    pub fn load_chunks_system(
        camera: Query<&Transform, With<Camera>>,
        assets: Res<AssetServer>,
        config: Res<ChunkLoaderConfig>,
        components: Query<&ChunkComponent>,
        chunks: Res<Assets<Chunk>>,
        mut loader: ResMut<ChunkLoader>,
    ) {
        let camera_loc = camera.single().unwrap().translation;
        let cur_chunk = config.chunk_size.chunk_coord(camera_loc);
        let current_components = components
            .iter()
            .filter_map(|c| chunks.get(c.0.id()))
            .map(|c| c.loc)
            //TODO: Optimize to a bitset?
            .collect::<HashSet<_>>();
        let mut insert_count = 0;
        for loc in cur_chunk.iter_around(config.chunk_radius as u64) {
            if !loader.unloaded_chunks.contains_key(&loc) && !current_components.contains(&loc) {
                insert_count += 1;
                loader.unloaded_chunks.insert(
                    loc,
                    assets.load(format!("chunks/{}_{}_{}.mcra", loc.x, loc.y, loc.z)),
                );
            }
        }
        if insert_count > 0 {
            info!("Loading Chunks: {insert_count}");
        }
    }

    /// Spawn chunks that are in the `UnloadedChunk` state
    pub fn spawn_chunks(
        mut commands: Commands,
        textures: Res<BlockTextures>,
        config: Res<ChunkLoaderConfig>,
        mut loader: ResMut<ChunkLoader>,
        (assets, mut chunks, mut meshes): SpawnAssets,
    ) {
        let mut new_chunks = Vec::new();
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
                    new_chunks.push(chunks.add(spawn_test_chunk(config.chunk_size, *loc)));
                    return false;
                }
                Some(LoadState::Loaded) => {
                    new_chunks.push(handle.clone());
                    return false;
                }
                _ => {
                    // waiting to finish loading
                }
            }
            true
        });
        if !new_chunks.is_empty() {
            let span = info_span!("chunk_spawning");
            span.in_scope(|| {
                info!("Spawning Chunks: {}", new_chunks.len());
                for new_chunk in new_chunks {
                    let chunk = chunks.get(new_chunk.id()).unwrap();
                    commands.spawn((
                        ChunkComponent(new_chunk),
                        chunk.transform(),
                        MeshMaterial3d(textures.texture().unwrap().clone()),
                        Mesh3d(meshes.add(chunk.generate_mesh(&textures))),
                    ));
                }
            });
        }
    }

    pub fn despawn_chunks(
        mut commands: Commands,
        camera: Query<&Transform, With<Camera>>,
        components: Query<(Entity, &ChunkComponent)>,
        mut chunks: ResMut<Assets<Chunk>>,
        config: Res<ChunkLoaderConfig>,
    ) {
        let camera_loc = camera.single().unwrap().translation;
        let cur_chunk = config.chunk_size.chunk_coord(camera_loc);
        let radius = config.chunk_radius as u64;
        if components.is_empty() {
            info!("Empty components");
        }
        let remove_chunks = components
            .iter()
            .filter_map(|(entity, chunk)| {
                let id = chunk.0.id();
                let chunk = chunks.get(id)?;

                (cur_chunk.x.abs_diff(chunk.loc.x) > radius
                    || cur_chunk.y.abs_diff(chunk.loc.y) > radius
                    || cur_chunk.z.abs_diff(chunk.loc.z) > radius)
                    .then_some((entity, id))
            })
            .collect::<Vec<_>>();
        if !remove_chunks.is_empty() {
            info!("Despawning Chunks: {}", remove_chunks.len());
        }
        for (entity, id) in remove_chunks {
            //TODO: Save to disk
            commands.entity(entity).despawn();
            chunks.remove(id);
        }
    }
}
