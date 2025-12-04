mod camera;
mod chunk;
mod textures;
mod ui;

use bevy::{
    color::palettes::css::WHITE,
    prelude::*,
    window::{CursorOptions, WindowMode},
};
use mcre_core::BlockId;

use crate::{
    camera::FirstPersonPlugin,
    chunk::{CHUNK_SIZE, Chunk},
    textures::BlockTextures,
    ui::{debug::DebugMenuPlugin, load::LoadingUi},
};

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        mode: WindowMode::Windowed,
                        title: "MCRE".to_string(),
                        ..Default::default()
                    }),
                    primary_cursor_options: Some(CursorOptions::default()),
                    ..Default::default()
                })
                .set(ImagePlugin::default_nearest())
                .set(AssetPlugin {
                    file_path: "../mcre_assets/assets".to_owned(),
                    ..Default::default()
                }),
            FirstPersonPlugin {
                transform: Transform::from_xyz(-2.0, 5.0, 10.0)
                    .looking_at(Vec3::new(0.0, 0.0, 0.0), Vec3::Y),
                camera_movement_speed: 0.2,
                camera_rotation_speed: 0.3,
            },
            DebugMenuPlugin,
        ))
        .init_state::<AppState>()
        .add_systems(Startup, (setup_light, BlockTextures::load_textures_system))
        .add_systems(Update, handle_esc)
        .add_systems(OnEnter(AppState::Loading), LoadingUi::add_ui_system)
        .add_systems(
            Update,
            (
                BlockTextures::check_loaded_textures_system,
                LoadingUi::update_ui_system,
            )
                .run_if(in_state(AppState::Loading)),
        )
        .add_systems(
            OnExit(AppState::Loading),
            (LoadingUi::remove_ui_system, spawn_chunk),
        )
        .add_systems(OnEnter(AppState::InGame), lock_cursor)
        .add_systems(OnExit(AppState::InGame), unlock_cursor)
        .run();
}

#[derive(Clone, PartialEq, Eq, Default, Hash, Debug, States)]
enum AppState {
    #[default]
    Loading,
    InGame,
    Paused,
}

fn setup_light(mut commands: Commands) {
    commands.insert_resource(AmbientLight {
        color: WHITE.into(),
        brightness: 100.0,
        ..default()
    });
    commands.spawn((
        DirectionalLight {
            illuminance: 5000.0,
            ..Default::default()
        },
        Transform::from_xyz(20.0, 10.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn handle_esc(
    app_state: Res<State<AppState>>,
    mut next_app_state: ResMut<NextState<AppState>>,
    key: Res<ButtonInput<KeyCode>>,
) {
    if key.just_released(KeyCode::Escape) {
        match app_state.get() {
            AppState::InGame => next_app_state.set(AppState::Paused),
            AppState::Paused => next_app_state.set(AppState::InGame),
            _ => {}
        }
    }
}

fn unlock_cursor(mut opts: Query<&mut CursorOptions, With<Window>>) {
    let mut opts = opts.single_mut().expect("A window to be attached");
    opts.visible = true;
    opts.grab_mode = bevy::window::CursorGrabMode::None;
}

fn lock_cursor(mut opts: Query<&mut CursorOptions, With<Window>>) {
    let mut opts = opts.single_mut().expect("A window to be attached");
    opts.visible = false;
    opts.grab_mode = bevy::window::CursorGrabMode::Locked;
}

fn spawn_chunk(
    mut commands: Commands,
    textures: Res<BlockTextures>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut chunk = Chunk::filled(UVec3::new(0, 0, 0), BlockId::STONE.default_state_id());
    chunk.set_block(UVec3::new(0, 0, 3), BlockId::AIR.default_state_id());
    chunk.set_block(UVec3::new(0, 1, 3), BlockId::AIR.default_state_id());
    chunk.set_block(UVec3::new(1, 0, 3), BlockId::AIR.default_state_id());
    chunk.set_block(UVec3::new(1, 1, 3), BlockId::AIR.default_state_id());

    chunk.set_block(UVec3::new(2, 1, 3), BlockId::DIAMOND_ORE.default_state_id());

    for x in 0..CHUNK_SIZE {
        for y in 0..CHUNK_SIZE {
            chunk.set_block(
                UVec3::new(x as u32, CHUNK_SIZE as u32 - 1, y as u32),
                BlockId::OAK_LEAVES.default_state_id(),
            );
            chunk.set_block(
                UVec3::new(x as u32, CHUNK_SIZE as u32 - 2, y as u32),
                BlockId::DIRT.default_state_id(),
            );
            chunk.set_block(
                UVec3::new(x as u32, 0, y as u32),
                BlockId::BEDROCK.default_state_id(),
            );
        }
    }

    commands.spawn(chunk.into_bundle(&textures, &mut meshes, &mut materials));
}
