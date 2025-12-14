use bevy::{platform::collections::HashMap, prelude::*, window::CursorOptions};
use mcre_core::Block;

use crate::{
    AppState,
    chunk::{
        Chunk, ChunkComponent,
        loader::{ChunkLoader, ChunkLoaderConfig},
        math::pos::BlockPosition,
        mesh::ChunkMeshBuilder,
    },
    textures::BlockTextures,
    ui::player::PlayerUi,
};

pub struct PlayerInteractionPlugin;

impl Plugin for PlayerInteractionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChunkUpdates>()
            .add_systems(Update, Self::handle_esc)
            .add_systems(
                Update,
                (ChunkUpdates::block_interaction, ChunkUpdates::block_updates)
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(OnEnter(AppState::InGame), Self::lock_cursor)
            .add_systems(OnExit(AppState::InGame), Self::unlock_cursor);
    }
}

impl PlayerInteractionPlugin {
    pub fn handle_esc(
        app_state: Res<State<AppState>>,
        mut next_app_state: ResMut<NextState<AppState>>,
        key: Res<ButtonInput<KeyCode>>,
        mut loader: ResMut<ChunkLoader>,
        config: Res<ChunkLoaderConfig>,
        chunks: Res<Assets<Chunk>>,
        server: Res<AssetServer>,
    ) {
        if key.just_released(KeyCode::Escape) {
            match app_state.get() {
                AppState::InGame => {
                    next_app_state.set(AppState::Paused);
                    loader.save_all_chunks(&chunks, &config, &server);
                }
                AppState::Paused => next_app_state.set(AppState::InGame),
                _ => {}
            }
        }
    }

    pub fn lock_cursor(mut commands: Commands, mut opts: Query<&mut CursorOptions, With<Window>>) {
        PlayerUi::spawn(&mut commands);
        let mut opts = opts.single_mut().expect("A window to be attached");
        opts.visible = false;
        opts.grab_mode = bevy::window::CursorGrabMode::Locked;
    }

    pub fn unlock_cursor(
        mut commands: Commands,
        mut opts: Query<&mut CursorOptions, With<Window>>,
        ui: Query<Entity, With<PlayerUi>>,
    ) {
        if let Ok(ui) = ui.single() {
            commands.entity(ui).despawn();
        }
        let mut opts = opts.single_mut().expect("A window to be attached");
        opts.visible = true;
        opts.grab_mode = bevy::window::CursorGrabMode::None;
    }
}

#[derive(Resource, Default)]
pub struct ChunkUpdates {
    updates: HashMap<BlockPosition, (Entity, Option<Block>)>,
}

impl ChunkUpdates {
    pub fn block_interaction(
        camera: Query<&Transform, With<Camera>>,
        mut ray_cast: MeshRayCast,
        components: Query<&Transform, With<ChunkComponent>>,
        mut updates: ResMut<ChunkUpdates>,
        mouse: Res<ButtonInput<MouseButton>>,
    ) {
        if mouse.just_pressed(MouseButton::Left) {
            let camera = camera.single().unwrap();
            if let Some((pos, entity)) = Self::cast_ray(camera, &mut ray_cast, &components, 1.) {
                updates.updates.insert(pos, (entity, None));
            }
        }
        if mouse.just_pressed(MouseButton::Right) {
            let camera = camera.single().unwrap();
            if let Some((pos, entity)) = Self::cast_ray(camera, &mut ray_cast, &components, -1.) {
                updates
                    .updates
                    .insert(pos, (entity, Some(Block::DIAMOND_ORE)));
            }
        }
    }

    pub fn block_updates(
        mut updates: ResMut<ChunkUpdates>,
        components: Query<(&ChunkComponent, &Mesh3d)>,
        mut meshes: ResMut<Assets<Mesh>>,
        mut chunks: ResMut<Assets<Chunk>>,
        textures: Res<BlockTextures>,
    ) {
        for (pos, (entity, val)) in updates.updates.drain() {
            let (component, mesh) = components.get(entity).unwrap();
            let chunk = chunks.get_mut(component.0.id()).unwrap();
            let mesh = meshes.get_mut(mesh.0.id()).unwrap();
            chunk.set(pos, val.unwrap_or(Block::AIR));
            let builder = ChunkMeshBuilder::new(chunk);
            builder.update_mesh(mesh, &textures);
        }
    }

    /// Returns the block position in the chunk that is associated with entity
    fn cast_ray(
        camera: &Transform,
        ray_cast: &mut MeshRayCast,
        components: &Query<&Transform, With<ChunkComponent>>,
        normal_cast: f32,
    ) -> Option<(BlockPosition, Entity)> {
        let v = camera.forward().normalize();
        let ray = Ray3d::new(camera.translation, Dir3::new_unchecked(v));
        if let Some((entity, hit)) = ray_cast
            .cast_ray(ray, &MeshRayCastSettings::default())
            .first()
            .filter(|(_, hit)| hit.distance < 5.)
            && let Ok(transform) = components.get(*entity)
        {
            let normal = Vec3::new(hit.normal.x, -hit.normal.y, -hit.normal.z) * normal_cast;
            let relative = (hit.point + normal / 2.) - transform.translation;
            let pos = BlockPosition {
                x: relative.x.floor() as u8,
                y: relative.y.floor() as i64,
                z: relative.z.floor() as u8,
            };
            return Some((pos, *entity));
        }
        None
    }
}
