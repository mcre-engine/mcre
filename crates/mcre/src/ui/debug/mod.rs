mod camera;
mod targeted_block;
mod title;
use bevy::{diagnostic::FrameTimeDiagnosticsPlugin, prelude::*};

use crate::{
    AppState,
    ui::debug::{camera::PlayerText, targeted_block::TargetedBlockText, title::TitleText},
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
                    PlayerText::update_text_system.run_if(in_state(DebugState::On)),
                    TitleText::update_text_system.run_if(in_state(DebugState::On)),
                    TargetedBlockText::update_text_system.run_if(in_state(DebugState::On)),
                ),
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

    fn add_debug_ui(mut commands: Commands, camera: Query<&Transform, With<Camera>>) {
        let camera = camera.single().unwrap();
        commands
            .spawn(DebugUi.into_bundle())
            .with_children(|parent| {
                parent.spawn(TitleText.into_bundle());
                parent.spawn(PlayerText.into_bundle(camera));
                parent.spawn(TargetedBlockText.into_bundle());
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
