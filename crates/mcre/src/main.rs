mod camera;
mod chunk;
mod player;
mod textures;
mod ui;
mod utils;

use bevy::{
    color::palettes::css::WHITE,
    log::{DEFAULT_FILTER, LogPlugin},
    prelude::*,
    window::{CursorOptions, WindowMode},
};

use crate::{
    camera::FirstPersonPlugin,
    chunk::loader::ChunkLoaderPlugin,
    player::PlayerInteractionPlugin,
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
                })
                .set(LogPlugin {
                    filter: format!("{DEFAULT_FILTER},bevy_asset=off"),
                    ..Default::default()
                }),
            FirstPersonPlugin {
                transform: Transform::from_xyz(-2.0, 10.0, 10.0)
                    .looking_at(Vec3::new(4.0, 0.0, 0.0), Vec3::Y),
                camera_movement_speed: 0.2,
                camera_rotation_speed: 0.3,
            },
            DebugMenuPlugin,
            PlayerInteractionPlugin,
        ))
        .add_plugins(ChunkLoaderPlugin::default())
        .init_state::<AppState>()
        .add_sub_state::<LoadingState>()
        .add_systems(Startup, setup_light)
        .add_systems(OnEnter(AppState::Loading), LoadingUi::add_ui_system)
        .add_systems(
            OnEnter(LoadingState::Textures),
            BlockTextures::load_textures_system,
        )
        .add_systems(
            Update,
            BlockTextures::check_loaded_textures_system.run_if(in_state(LoadingState::Textures)),
        )
        .add_systems(
            Update,
            LoadingUi::update_ui_system.run_if(in_state(AppState::Loading)),
        )
        .add_systems(OnExit(AppState::Loading), LoadingUi::remove_ui_system)
        .run();
}

#[derive(Clone, PartialEq, Eq, Default, Hash, Debug, States)]
enum AppState {
    #[default]
    Loading,
    InGame,
    Paused,
}

#[derive(SubStates, Clone, PartialEq, Eq, Hash, Debug, Default)]
#[source(AppState = AppState::Loading)]
enum LoadingState {
    #[default]
    Camera,
    Textures,
    Chunks,
}

fn setup_light(mut commands: Commands) {
    commands.insert_resource(AmbientLight {
        color: WHITE.into(),
        brightness: 300.0,
        ..default()
    });
    commands.spawn((
        DirectionalLight {
            illuminance: 1000.0,
            ..Default::default()
        },
        Transform::from_xyz(20.0, 100.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}
