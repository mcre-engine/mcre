mod camera;
mod chunk;
mod title;
use std::f32::consts::PI;

use bevy::{
    color::palettes::css::{BLACK, GREEN, RED},
    diagnostic::FrameTimeDiagnosticsPlugin,
    prelude::*,
};

use crate::{
    AppState,
    chunk::{
        ChunkComponent,
        loader::{ChunkLoader, ChunkLoaderConfig},
        math::pos::ChunkPosition,
    },
    player::ChunkUpdates,
    ui::debug::{camera::PlayerText, chunk::ChunkText, title::TitleText},
};

pub struct DebugMenuPlugin;

impl Plugin for DebugMenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<DebugState>()
            .add_plugins(FrameTimeDiagnosticsPlugin::default())
            .add_systems(
                Update,
                Self::check_for_debug.run_if(in_state(AppState::InGame)),
            )
            .add_systems(OnEnter(DebugState::On), Self::add_debug_ui)
            .add_systems(
                Update,
                (
                    PlayerText::update_text_system,
                    TitleText::update_text_system,
                    ChunkText::update_text_system,
                    draw_gizmos,
                )
                    .run_if(in_state(DebugState::On)),
            )
            .add_systems(OnExit(DebugState::On), Self::remove_debug_ui);
    }
}

impl DebugMenuPlugin {
    fn check_for_debug(
        state: Res<State<DebugState>>,
        mut next_state: ResMut<NextState<DebugState>>,
        key: Res<ButtonInput<KeyCode>>,
    ) {
        if key.just_released(KeyCode::F3) {
            match state.get() {
                DebugState::Off => next_state.set(DebugState::On),
                DebugState::On => next_state.set(DebugState::Off),
            }
        }
    }

    fn add_debug_ui(
        mut commands: Commands,
        camera: Query<&Transform, With<Camera>>,
        loader: Res<ChunkLoader>,
        config: Res<ChunkLoaderConfig>,
    ) {
        let camera = camera.single().unwrap();
        commands
            .spawn(DebugUi.into_bundle())
            .with_children(|parent| {
                parent.spawn(TitleText.into_bundle());
                parent.spawn(PlayerText.into_bundle(camera, config.chunk_size));
                parent.spawn(ChunkText.into_bundle(&loader));
            });
    }

    fn remove_debug_ui(mut commands: Commands, ui: Query<Entity, With<DebugUi>>) {
        if let Ok(entity) = ui.single() {
            commands.entity(entity).despawn();
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Hash, Default, States)]
enum DebugState {
    #[default]
    Off,
    On,
}

#[derive(Component)]
struct DebugUi;
const MARGIN: Val = Val::Px(12.);

impl DebugUi {
    fn into_bundle(self) -> impl Bundle {
        (
            self,
            Node {
                width: Val::Percent(100.),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::SpaceEvenly,
                align_items: AlignItems::FlexStart,
                padding: UiRect::all(MARGIN),
                row_gap: MARGIN,
                ..Default::default()
            },
        )
    }
}

fn draw_gizmos(
    mut gizmos: Gizmos,
    config: Res<ChunkLoaderConfig>,
    camera: Query<&Transform, With<Camera>>,
    components: Query<&Transform, With<ChunkComponent>>,
    mut ray_cast: MeshRayCast,
) {
    let camera = camera.single().unwrap();
    let pos = camera.translation;
    let chunk_coord = ChunkPosition::from_world_coord(pos, config.chunk_size);
    let size = config.chunk_size.as_f32();

    gizmos.grid(
        Isometry3d::from_translation(Vec3::new(
            chunk_coord.x as f32 * size + (size / 2.),
            size / 2.,
            chunk_coord.y as f32 * size,
        )),
        UVec2::new(16, 100),
        Vec2::splat(1.),
        LinearRgba::gray(0.65),
    );

    gizmos.grid(
        Isometry3d::new(
            Vec3::new(
                chunk_coord.x as f32 * size,
                size / 2.,
                chunk_coord.y as f32 * size + (size / 2.),
            ),
            Quat::from_rotation_y(PI / 2.),
        ),
        UVec2::new(16, 100),
        Vec2::splat(1.),
        LinearRgba::gray(0.65),
    );

    gizmos.grid(
        Isometry3d::new(
            Vec3::new(
                chunk_coord.x as f32 * size + (size / 2.),
                size / 2.,
                chunk_coord.y as f32 * size + size,
            ),
            Quat::from_rotation_y(0.),
        ),
        UVec2::new(16, 100),
        Vec2::splat(1.),
        LinearRgba::gray(0.65),
    );

    gizmos.grid(
        Isometry3d::new(
            Vec3::new(
                chunk_coord.x as f32 * size + size,
                size / 2.,
                chunk_coord.y as f32 * size + (size / 2.),
            ),
            Quat::from_rotation_y(PI / 2.),
        ),
        UVec2::new(16, 100),
        Vec2::splat(1.),
        LinearRgba::gray(0.65),
    );

    let v = camera.forward().normalize();
    let ray = Ray3d::new(camera.translation, Dir3::new_unchecked(v));
    if let Some((_entity, hit)) = ray_cast
        .cast_ray(ray, &MeshRayCastSettings::default())
        .first()
        .filter(|(_, hit)| hit.distance < 5.)
    {
        gizmos.sphere(hit.point, 0.2, RED);
        gizmos.line(
            hit.point - hit.normal / 2.,
            hit.point + hit.normal / 2.,
            GREEN,
        );
    }

    if let Some((pos, _entity)) = ChunkUpdates::cast_ray(camera, &mut ray_cast, &components, 1.) {
        let world = pos.into_world_coord(chunk_coord, config.chunk_size) + Vec3::splat(0.5);
        gizmos.cuboid(
            Transform::from_translation(world).with_scale(Vec3::splat(1.0)),
            BLACK,
        );
    }
}
