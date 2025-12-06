use crate::AppState;
use bevy::prelude::*;

#[derive(Component)]
pub struct Crosshair;

/// Plugin that manages crosshair rendering
pub struct CrosshairPlugin;

impl Plugin for CrosshairPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn_crosshair)
            .add_systems(OnExit(AppState::InGame), despawn_crosshair);
    }
}

fn spawn_crosshair(mut commands: Commands) {
    // Vertical line of crosshair
    commands.spawn((
        Crosshair,
        Node {
            width: Val::Px(2.0),
            height: Val::Px(16.0),
            position_type: PositionType::Absolute,
            left: Val::Percent(50.0),
            top: Val::Percent(50.0),
            // Center the crosshair
            margin: UiRect {
                left: Val::Px(-1.0),
                top: Val::Px(-8.0),
                ..default()
            },
            ..default()
        },
        BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.8)),
    ));

    // Horizontal line of crosshair
    commands.spawn((
        Crosshair,
        Node {
            width: Val::Px(16.0),
            height: Val::Px(2.0),
            position_type: PositionType::Absolute,
            left: Val::Percent(50.0),
            top: Val::Percent(50.0),
            margin: UiRect {
                left: Val::Px(-8.0),
                top: Val::Px(-1.0),
                ..default()
            },
            ..default()
        },
        BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.8)),
    ));
}

fn despawn_crosshair(mut commands: Commands, crosshair_query: Query<Entity, With<Crosshair>>) {
    for entity in crosshair_query.iter() {
        commands.entity(entity).despawn();
    }
}
